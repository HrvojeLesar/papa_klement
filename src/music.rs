use std::{
    collections::{HashMap, HashSet},
    env,
    fmt::Display,
    sync::Arc,
    time::Duration,
};

use anyhow::{anyhow, Result};
use chrono::{DateTime, Utc};
use log::{error, info, warn};
use mongodb::{bson::doc, Collection, Database};
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use serenity::{
    all::{
        ActivityData, CommandInteraction, CommandOptionType, CreateCommand, CreateCommandOption,
    },
    async_trait,
    model::{
        prelude::{ChannelId, ChannelType, GuildId},
        user::OnlineStatus,
    },
    prelude::{Context, Mutex, RwLock, TypeMapKey},
    utils::MessageBuilder,
};
use sha2::{Digest, Sha256};
use songbird::{
    input::{AuxMetadata, Input, YoutubeDl},
    Call, CoreEvent, Event, EventContext, EventHandler,
};
use tokio::{process::Command, task::JoinHandle};

use crate::{
    commands::slash_commands::SlashCommands,
    util::{defer_response, retrieve_save_handler, CommandRunner, MakeCommandResponse},
    CommandResponse, ReqwestClient,
};

const QUERY: &str = "search";
static HOME: Lazy<String> =
    Lazy::new(|| env::var("HOME").expect("HOME environment variable is required!"));

const CACHED_AUDIO_COLLECTION: &str = "cached_audio";
const DISCONNECT_AFTER: u64 = 5 * 60;

type InvalidCommandUsage = CommandResponse;

#[derive(Debug, Serialize, Deserialize)]
struct CachedAudioRecord {
    #[serde(rename = "_id")]
    id: String,
    possible_queries: Vec<String>,
    url: String,
    title: Option<String>,
    date: DateTime<Utc>,
}

struct AuxMetadataExt;
impl TypeMapKey for AuxMetadataExt {
    type Value = AuxMetadata;
}

struct TrackStartEventHandler {
    context: Context,
}

pub(crate) struct QueuedDisconnect {
    queue: HashMap<GuildId, JoinHandle<()>>,
}

impl QueuedDisconnect {
    pub(crate) fn new() -> Self {
        Self {
            queue: HashMap::new(),
        }
    }

    pub(crate) fn insert_handle(
        &mut self,
        guild_id: GuildId,
        songbird_handler: Arc<Mutex<Call>>,
        disconnect_after_secs: Option<u64>,
    ) {
        println!("INSERTING DISCONNECT HANDLE");
        if self.queue.get(&guild_id).is_some() {
            println!("{:#?}", self.queue);
            self.remove_handle(&guild_id);
        }
        self.queue.insert(
            guild_id,
            self.make_disconnect_queue_handle(
                disconnect_after_secs.unwrap_or(DISCONNECT_AFTER),
                songbird_handler,
            ),
        );
    }

    // WARN: self.queue should shrink in some cases to release memory
    pub(crate) fn remove_handle(&mut self, guild_id: &GuildId) {
        if let Some(handle) = self.queue.remove(guild_id) {
            println!("REMOVED DISCONNECT HANDLE");
            handle.abort();
        }
    }

    fn make_disconnect_queue_handle(
        &self,
        disconnect_after_secs: u64,
        songbird_handler: Arc<Mutex<Call>>,
    ) -> JoinHandle<()> {
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_secs(disconnect_after_secs)).await;
            {
                let mut lock = songbird_handler.lock().await;
                if let Err(e) = lock.leave().await {
                    error!("Disconnect failed: {:?}", e);
                }
            }
        })
    }
}

impl TypeMapKey for QueuedDisconnect {
    type Value = Arc<RwLock<Self>>;
}

#[async_trait]
impl EventHandler for TrackStartEventHandler {
    async fn act(&self, ctx: &EventContext<'_>) -> Option<Event> {
        if let EventContext::Track(track) = ctx {
            if let Some((_track_state, track_handle)) = track.first() {
                let (title, metadata) = {
                    let handle_lock = track_handle.typemap().read().await;
                    let metadata = handle_lock.get::<AuxMetadataExt>().cloned();
                    let title = if let Some(metadata) = metadata.as_ref() {
                        metadata
                            .title
                            .clone()
                            .unwrap_or_else(|| "TITLE NOT FOUND".to_string())
                    } else {
                        "TITLE NOT FOUND".to_string()
                    };

                    (title, metadata)
                };
                if title == "TITLE NOT FOUND" {
                    warn!("Set TITLE NOT FOUND for track: {:?}", metadata);
                }
                self.context
                    .set_activity(Some(ActivityData::playing(title)));
                self.context
                    .set_activity(Some(ActivityData::playing("TITLE NOT FOUND")));
            }
        }
        None
    }
}

struct TrackEndEventHandler {
    guild_id: GuildId,
    call_handler: Arc<Mutex<Call>>,
    context: Context,
}

#[async_trait]
impl EventHandler for TrackEndEventHandler {
    async fn act(&self, ctx: &EventContext<'_>) -> Option<Event> {
        if let EventContext::Track(_track) = ctx {
            let is_empty = {
                let lock = self.call_handler.lock().await;
                lock.queue().is_empty()
            };
            if is_empty {
                self.context.set_presence(None, OnlineStatus::Online);
                let queued_disconnects = self
                    .context
                    .data
                    .read()
                    .await
                    .get::<QueuedDisconnect>()
                    .expect("QueuedDisconnect must be present!")
                    .clone();
                {
                    let mut lock = queued_disconnects.write().await;
                    lock.insert_handle(self.guild_id, self.call_handler.clone(), None);
                }
            }
        }
        None
    }
}

struct DriverDisconnectHandler {
    guild_id: GuildId,
    context: Context,
    call_handler: Arc<Mutex<Call>>,
}

#[async_trait]
impl EventHandler for DriverDisconnectHandler {
    async fn act(&self, ctx: &EventContext<'_>) -> Option<Event> {
        if let EventContext::DriverDisconnect(dd) = ctx {
            if dd.reason.is_some() {
                let lock = self.call_handler.lock().await;
                if lock.current_channel().is_none() {
                    lock.queue().stop();
                    self.context.set_presence(None, OnlineStatus::Online);
                    let queued_disconnects = self
                        .context
                        .data
                        .read()
                        .await
                        .get::<QueuedDisconnect>()
                        .expect("QueuedDisconnect must be present!")
                        .clone();
                    {
                        let mut lock = queued_disconnects.write().await;
                        lock.remove_handle(&self.guild_id);
                    }
                }
            }
        }
        None
    }
}

pub(crate) struct SaveHandler {
    save_queue: RwLock<HashSet<String>>,
    db_handle: Database,
    hasher: RwLock<Sha256>,
}

impl SaveHandler {
    pub(crate) fn new(db_handle: Database) -> Self {
        Self {
            save_queue: RwLock::new(HashSet::new()),
            db_handle,
            hasher: RwLock::new(Sha256::new()),
        }
    }

    fn get_collection(&self) -> Collection<CachedAudioRecord> {
        self.db_handle.collection(CACHED_AUDIO_COLLECTION)
    }

    async fn get_hash(&self, input: &str) -> Result<String> {
        let mut hasher = self.hasher.write().await;
        hasher.update(input);
        Ok(format!("{:x}", hasher.finalize_reset()))
    }

    async fn get_saved_file(&self, query: &str) -> Result<Option<CachedAudioRecord>> {
        let hash = self.get_hash(query).await?;
        let collection = self.get_collection();
        if let Some(saved_file) = collection.find_one(doc! {"_id": &hash}, None).await? {
            Ok(Some(saved_file))
        } else if let Some(saved_file) = collection
            .find_one(doc! {"possible_queries": query}, None)
            .await?
        {
            Ok(Some(saved_file))
        } else {
            Ok(None)
        }
    }

    async fn is_url_saved(&self, url: &str) -> Result<bool> {
        let hash = self.get_hash(url).await?;
        let collection = self.get_collection();
        Ok(collection
            .find_one(doc! {"_id": &hash}, None)
            .await?
            .is_some())
    }

    async fn write_to_db(
        &self,
        hash: &str,
        query: &str,
        url: &str,
        title: Option<&String>,
    ) -> Result<()> {
        let collection = self
            .db_handle
            .collection::<CachedAudioRecord>(CACHED_AUDIO_COLLECTION);
        collection
            .insert_one(
                CachedAudioRecord {
                    id: hash.to_string(),
                    url: url.to_string(),
                    possible_queries: vec![query.to_string()],
                    title: title.cloned(),
                    date: Utc::now(),
                },
                None,
            )
            .await?;
        info!("Saved track to database");
        info!("Id: {} | url: {} | title: {:?}", hash, url, title);
        Ok(())
    }

    async fn try_append_new_query_to_saved(&self, url: &str, query: &str) -> Result<()> {
        let hash = self.get_hash(url).await?;
        let collection = self.get_collection();
        collection
            .update_one(
                doc! {
                    "_id": hash,
                    "possible_queries": {"$ne": query}
                },
                doc! {"$push": {"possible_queries": query}},
                None,
            )
            .await?;
        Ok(())
    }

    async fn init_save(&self, url: &str, query: &str, title: Option<&String>) -> Result<()> {
        if self.is_url_saved(url).await? {
            println!("SKIPPING already saved file!");
            self.try_append_new_query_to_saved(url, query).await?;
            return Ok(());
        }
        let hash = self.get_hash(url).await?;
        let contains_url = {
            let lock = self.save_queue.read().await;
            lock.contains(&hash)
        };
        if contains_url {
            return Err(anyhow!("URL already in save queue!"));
        } else {
            info!(
                "Downloading and saving track from url: {} with title: {:?}",
                url, title
            );
            {
                let mut lock = self.save_queue.write().await;
                lock.insert(hash.to_string());
            }
            let ytdl_args = [
                "-f",
                "webm[abr>0]/bestaudio/best",
                "--no-playlist",
                "--ignore-config",
                "--no-warnings",
                url,
                "-o",
                &format!("{}/songbird_cache/{}", *HOME, &hash),
            ];

            let command_status = match Command::new("yt-dlp").args(ytdl_args).spawn() {
                Ok(mut child) => child.wait().await,
                Err(e) => Err(e),
            };

            let removed = {
                let mut lock = self.save_queue.write().await;
                lock.remove(&hash)
            };

            match command_status {
                Ok(_) => {
                    if removed {
                        self.write_to_db(&hash, query, url, title).await?;
                    }
                }
                Err(e) => return Err(anyhow!(e)),
            }
        }

        Ok(())
    }
}

pub(crate) struct PlayCommand;
impl MakeCommandResponse for PlayCommand {}

impl PlayCommand {
    fn get_members_voice_channel(
        &self,
        ctx: &Context,
        command: &CommandInteraction,
    ) -> Result<Option<ChannelId>> {
        if let (Some(guild_id), Some(member)) = (command.guild_id.as_ref(), command.member.as_ref())
        {
            let guild = match guild_id.to_guild_cached(&ctx.cache) {
                Some(g) => g,
                None => return Err(anyhow::anyhow!("Message not sent in guild")),
            };
            Ok(guild
                .voice_states
                .get(&member.user.id)
                .and_then(|voice_state| voice_state.channel_id))
        } else {
            Ok(None)
        }
    }

    async fn handle_connection(
        &self,
        ctx: &Context,
        command: &CommandInteraction,
    ) -> Result<(Option<InvalidCommandUsage>, Option<Arc<Mutex<Call>>>)> {
        if let (Some(channel_id), Some(guild_id)) = (
            self.get_members_voice_channel(ctx, command)?,
            command.guild_id,
        ) {
            let manager = songbird::get(ctx)
                .await
                .expect("Songbird must be registered in client")
                .clone();

            let handler = match manager.get(guild_id) {
                Some(handler) => {
                    info!("Re-using Call handler");
                    {
                        let mut lock = handler.lock().await;
                        if lock.current_channel().is_none() {
                            lock.join(channel_id).await?;
                        }
                    }
                    handler
                }
                None => {
                    info!("Registering new Call handler and event handlers");
                    let join_result = manager.join(guild_id, channel_id).await?;
                    {
                        let mut lock = join_result.lock().await;
                        // WARN: Very inefficient
                        let context = ctx.clone();
                        lock.add_global_event(
                            Event::Track(songbird::TrackEvent::Play),
                            TrackStartEventHandler {
                                context: context.clone(),
                            },
                        );
                        lock.add_global_event(
                            Event::Track(songbird::TrackEvent::End),
                            TrackEndEventHandler {
                                guild_id,
                                call_handler: join_result.clone(),
                                context: context.clone(),
                            },
                        );
                        lock.add_global_event(
                            Event::Core(CoreEvent::DriverDisconnect),
                            DriverDisconnectHandler {
                                guild_id,
                                call_handler: join_result.clone(),
                                context,
                            },
                        );
                    }
                    join_result
                }
            };
            Ok((None, Some(handler)))
        } else {
            Ok((
                Some(self.make_response("Not connected to voice channel", true)),
                None,
            ))
        }
    }

    fn get_query(&self, command: &CommandInteraction) -> Result<String> {
        let query_string = command
            .data
            .options
            .iter()
            .find(|opt| opt.name == QUERY)
            .ok_or_else(|| anyhow!("Missing query option"))?
            .value
            .as_str()
            .ok_or_else(|| anyhow!("Search is empty"))?
            .to_string();
        // query_string.remove(query_string.len() - 1);
        // query_string.remove(0);
        Ok(query_string)
    }
}

#[async_trait]
impl CommandRunner for PlayCommand {
    fn register(&self) -> CreateCommand {
        info!("Command registered: {}", SlashCommands::Play.as_str());
        let command = CreateCommand::new(SlashCommands::Play.as_str());
        command
            .dm_permission(false)
            .add_option(
                CreateCommandOption::new(
                    CommandOptionType::String,
                    QUERY,
                    "Search youtube or use direct URL",
                )
                .required(true)
                .channel_types(vec![ChannelType::Text]),
            )
            .description("Plays a track from youtube")
    }

    async fn run(&self, ctx: &Context, command: &CommandInteraction) -> Result<CommandResponse> {
        defer_response(ctx, command).await?;
        let query = self.get_query(command)?;

        let (early_response, handler) = self.handle_connection(ctx, command).await?;
        if let Some(r) = early_response {
            return Ok(r);
        }
        let handler = handler.ok_or_else(|| anyhow!("Failed to retrieve handler!\nThis value should always be Some if handled correctly!"))?;

        let save_handler = retrieve_save_handler(ctx.data.clone()).await?;
        let saved_file = save_handler.get_saved_file(&query).await?;

        // WARN: Still does not check for file actully existing
        // BUG:  Still does not check for file actully existing
        let (source, metadata) = if let Some(saved) = saved_file {
            info!("Reading file from disk!");
            let mut source: Input =
                songbird::input::File::new(format!("{}/songbird_cache/{}", *HOME, saved.id)).into();
            let mut metadata = source.aux_metadata().await?;
            metadata.source_url = Some(saved.url);
            metadata.title = saved.title;
            (source, metadata)
        } else {
            info!("Searching youtube for: {}", query);
            let client = {
                let lock = ctx.data.read().await;
                lock.get::<ReqwestClient>()
                    .ok_or_else(|| anyhow!("Failed to get reqwest client"))?
                    .clone()
            };
            // WARN: cannot be sure if query is actually url
            let mut source: Input = if query.starts_with("http") {
                YoutubeDl::new(client, query.clone()).into()
            } else {
                YoutubeDl::new_search(client, query.clone()).into()
            };

            let url = match source.aux_metadata().await?.source_url.as_ref() {
                Some(url) => url.to_string(),
                None => {
                    error!("Failed to retrieve url from input!");
                    return Ok(self.make_response("Failed to retrieve url from input!", true));
                }
            };

            let metadata = source.aux_metadata().await?;
            let title = metadata.title.clone();
            let data = ctx.data.clone();
            tokio::spawn(async move {
                let save_handler = match retrieve_save_handler(data).await {
                    Ok(sh) => sh,
                    Err(e) => {
                        error!(
                            "Error retrieving save handler in save init thread: {:#?}",
                            e
                        );
                        return;
                    }
                };
                if let Err(e) = save_handler.init_save(&url, &query, title.as_ref()).await {
                    error!("Error while saving: {:#?}", e);
                }
            });
            (source, metadata)
        };

        let title = metadata
            .title
            .clone()
            .unwrap_or_else(|| "TITLE NOT FOUND".to_string());
        let mut handle = handler.lock().await;
        let track_handle = handle.enqueue(source.into()).await;
        {
            let mut track_handle_lock = track_handle.typemap().write().await;
            track_handle_lock.insert::<AuxMetadataExt>(metadata);
        }

        if handle.queue().len() == 1 {
            let queued_disconnects = ctx
                .data
                .read()
                .await
                .get::<QueuedDisconnect>()
                .expect("QueuedDisconnect must be present!")
                .clone();
            {
                let mut lock = queued_disconnects.write().await;
                if let Some(guild_id) = command.guild_id {
                    lock.remove_handle(&guild_id);
                }
            }
            ctx.set_activity(Some(ActivityData::playing(&title)));
            Ok(self.make_response(format!("Now playing: {}", title), false))
        } else {
            Ok(self.make_response(format!("Added to queue: {}", title), false))
        }
    }

    fn has_deferred_response(&self) -> bool {
        true
    }
}

pub(crate) struct SkipCommand;
impl MakeCommandResponse for SkipCommand {}

#[async_trait]
impl CommandRunner for SkipCommand {
    fn register(&self) -> CreateCommand {
        info!("Command registered: {}", SlashCommands::Skip.as_str());
        let command = CreateCommand::new(SlashCommands::Skip.as_str());
        command
            .dm_permission(false)
            .description("Skip current track")
    }

    async fn run(&self, ctx: &Context, command: &CommandInteraction) -> Result<CommandResponse> {
        let guild_id = match command.guild_id {
            Some(g) => g,
            None => {
                return Ok(self.make_response("Command must be run in a guild!", true));
            }
        };
        info!("Skip in guild: {}", guild_id.get());
        let manager = songbird::get(ctx)
            .await
            .expect("Songbird must be registered in client")
            .clone();
        if let Some(handler_lock) = manager.get(guild_id) {
            let handler = handler_lock.lock().await;
            let queue = handler.queue();
            if !queue.is_empty() {
                let current = match queue.current() {
                    Some(track) => track,
                    None => return Err(anyhow!("Failed to retrieve current track")),
                };
                let title = {
                    let handle_lock = current.typemap().read().await;
                    let metadata = handle_lock.get::<AuxMetadataExt>();
                    if let Some(metadata) = metadata {
                        metadata
                            .title
                            .clone()
                            .unwrap_or_else(|| "[TITLE NOT FOUND]".to_string())
                    } else {
                        "[TITLE NOT FOUND]".to_string()
                    }
                };
                let _ = queue.skip()?;
                Ok(self.make_response(format!("Skipped: {}", title), false))
            } else {
                Ok(self.make_response("There is nothing to skip!", true))
            }
        } else {
            Ok(self.make_response("Failed to skip", true))
        }
    }
}

pub(crate) struct StopCommand;
impl MakeCommandResponse for StopCommand {}

#[async_trait]
impl CommandRunner for StopCommand {
    fn register(&self) -> CreateCommand {
        info!("Command registered: {}", SlashCommands::Stop.as_str());
        let command = CreateCommand::new(SlashCommands::Stop.as_str());
        command
            .dm_permission(false)
            .description("Stops the bot playing tracks and disconnects it")
    }

    async fn run(&self, ctx: &Context, command: &CommandInteraction) -> Result<CommandResponse> {
        let guild_id = match command.guild_id {
            Some(g) => g,
            None => {
                return Ok(self.make_response("Command must be run in a guild!", true));
            }
        };
        info!("Stop in guild: {}", guild_id.get());
        let manager = songbird::get(ctx)
            .await
            .expect("Songbird must be registered in client")
            .clone();
        if let Some(handler_lock) = manager.get(guild_id) {
            let mut handler = handler_lock.lock().await;
            // WARN: queue is cleared in event handler
            // let queue = handler.queue();
            // queue.stop();
            ctx.set_presence(None, OnlineStatus::Online);
            handler.leave().await?;
            Ok(self.make_response("Stopping", false))
        } else {
            Ok(self.make_response("Failed to stop", true))
        }
    }
}

pub(crate) struct QueueCommand;
impl MakeCommandResponse for QueueCommand {}

struct MinutesDisplay(String);

impl From<Duration> for MinutesDisplay {
    fn from(duration: Duration) -> Self {
        let seconds = duration.as_secs();
        let minutes = seconds / 60;
        Self(format!("{:02}:{:02}", minutes, seconds - minutes * 60))
    }
}

impl Display for MinutesDisplay {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[async_trait]
impl CommandRunner for QueueCommand {
    fn register(&self) -> CreateCommand {
        info!("Command registered: {}", SlashCommands::Queue.as_str());
        let command = CreateCommand::new(SlashCommands::Queue.as_str());
        command
            .dm_permission(false)
            .description("Fetches current track queue.")
    }

    async fn run(&self, ctx: &Context, command: &CommandInteraction) -> Result<CommandResponse> {
        let guild_id = match command.guild_id {
            Some(g) => g,
            None => {
                return Ok(self.make_response("Command must be run in a guild!", true));
            }
        };
        info!("Stop in guild: {}", guild_id.get());
        let manager = songbird::get(ctx)
            .await
            .expect("Songbird must be registered in client")
            .clone();
        if let Some(handler_lock) = manager.get(guild_id) {
            let queue = {
                let handler = handler_lock.lock().await;
                handler.queue().current_queue()
            };

            if queue.is_empty() {
                return Ok(self.make_response("Queue is empty", false));
            }
            let mut builder = MessageBuilder::new();
            let (current_track_position, current_track_length, title) = {
                let track = queue.first().ok_or_else(|| anyhow!("Queue is empty"))?;
                let (title, duration) = {
                    let handle_lock = track.typemap().read().await;
                    let metadata = handle_lock.get::<AuxMetadataExt>();
                    let (title, duration) = if let Some(metadata) = metadata {
                        let title = metadata.title.clone();
                        let duration = metadata.duration;
                        (title, duration)
                    } else {
                        (None, None)
                    };
                    (title, duration)
                };
                (
                    track.get_info().await?.position,
                    duration.unwrap_or(Duration::from_secs(0)),
                    title.unwrap_or_else(|| "TITLE NOT FOUND!".to_string()),
                )
            };
            builder
                .push_bold("Currently playing: ")
                .push(title)
                .push_bold_line(format!(
                    " | {} / {}",
                    MinutesDisplay::from(current_track_position),
                    MinutesDisplay::from(current_track_length)
                ));
            let mut time_until = Some(current_track_length - current_track_position);
            for (i, track) in queue.iter().skip(1).enumerate() {
                if builder.0.len() >= 2000 {
                    break;
                }
                builder.push_bold(format!("{}. ", i + 1));
                let metadata = {
                    let handle_lock = track.typemap().read().await;
                    let metadata = handle_lock.get::<AuxMetadataExt>().cloned();
                    match metadata {
                        Some(m) => m,
                        None => AuxMetadata::default(),
                    }
                };
                match metadata.title.as_ref() {
                    Some(title) => builder.push(title),
                    None => builder.push("NO TITLE FOUND"),
                };
                if let (Some(track_duration), Some(time_until)) =
                    (metadata.duration.as_ref(), time_until.as_mut())
                {
                    builder.push_bold_line(format!(" | {}", MinutesDisplay::from(*time_until)));
                    *time_until += *track_duration;
                } else {
                    builder.push_bold_line("????");
                    time_until = None;
                }
            }
            let queue_response = if builder.0.len() >= 2000 {
                const TOO_LONG: &str = "...\n**Queue is too long to display**";
                let mut queue = builder.build();
                queue.replace_range(2000 - TOO_LONG.len()..2000, TOO_LONG);
                queue.truncate(2000);

                queue
            } else {
                builder.build()
            };
            return Ok(self.make_response(queue_response, false));
        }
        Ok(self.make_response("Failed retrieving queue", false))
    }
}

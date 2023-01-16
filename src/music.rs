use std::{
    collections::{HashMap, HashSet},
    env,
    sync::Arc,
};

use anyhow::{anyhow, Result};
use chrono::{DateTime, Utc};
use log::{error, info, warn};
use mongodb::{bson::doc, Collection, Database};
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use serenity::{
    async_trait,
    builder::CreateApplicationCommand,
    model::{
        prelude::{
            command::CommandOptionType,
            interaction::application_command::ApplicationCommandInteraction,
            interaction::InteractionResponseType, Activity, ChannelId, ChannelType, GuildId,
        },
        user::OnlineStatus,
    },
    prelude::{Context, Mutex, RwLock, TypeMapKey},
};
use sha2::{Digest, Sha256};
use songbird::{
    input::{Input, Restartable},
    Call, CoreEvent, Event, EventContext, EventHandler,
};
use tokio::{process::Command, task::JoinHandle};

use crate::{
    util::{deferr_response, retrieve_save_handler, CommandRunner},
    CommandResponse, SlashCommands,
};

const QUERY: &str = "query";
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

struct TrackStartEventHandler {
    context: Arc<Context>,
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
                if let Some(title) = track_handle.metadata().title.as_ref() {
                    self.context.set_activity(Activity::playing(title)).await;
                } else {
                    warn!(
                        "Set TITLE NOT FOUND for track: {:?}",
                        track_handle.metadata()
                    );
                    self.context
                        .set_activity(Activity::playing("TITLE NOT FOUND"))
                        .await;
                }
            }
        }
        None
    }
}

struct TrackEndEventHandler {
    guild_id: GuildId,
    call_handler: Arc<Mutex<Call>>,
    context: Arc<Context>,
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
                self.context.set_presence(None, OnlineStatus::Online).await;
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
    context: Arc<Context>,
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
                    self.context.set_presence(None, OnlineStatus::Online).await;
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
    db_handle: Arc<Database>,
    hasher: RwLock<Sha256>,
}

impl SaveHandler {
    pub(crate) fn new(db_handle: Arc<Database>) -> Self {
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

impl PlayCommand {
    fn get_members_voice_channel(
        ctx: &Context,
        command: &ApplicationCommandInteraction,
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
        ctx: &Context,
        command: &ApplicationCommandInteraction,
    ) -> Result<(Option<InvalidCommandUsage>, Option<Arc<Mutex<Call>>>)> {
        if let (Some(channel_id), Some(guild_id)) = (
            Self::get_members_voice_channel(ctx, command)?,
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
                    let (handler, join_result) = manager.join(guild_id, channel_id).await;
                    join_result?;
                    {
                        let mut lock = handler.lock().await;
                        // WARN: Very inefficient
                        let context_arc = Arc::new(ctx.clone());
                        lock.add_global_event(
                            Event::Track(songbird::TrackEvent::Play),
                            TrackStartEventHandler {
                                context: context_arc.clone(),
                            },
                        );
                        lock.add_global_event(
                            Event::Track(songbird::TrackEvent::End),
                            TrackEndEventHandler {
                                guild_id,
                                call_handler: handler.clone(),
                                context: context_arc.clone(),
                            },
                        );
                        lock.add_global_event(
                            Event::Core(CoreEvent::DriverDisconnect),
                            DriverDisconnectHandler {
                                guild_id,
                                call_handler: handler.clone(),
                                context: context_arc,
                            },
                        );
                    }
                    handler
                }
            };
            Ok((None, Some(handler)))
        } else {
            Ok((
                Some(Self::make_response(
                    "Not connected to voice channel".to_string(),
                    true,
                    None,
                )),
                None,
            ))
        }
    }

    fn get_query(command: &ApplicationCommandInteraction) -> Result<String> {
        let mut query_string = command
            .data
            .options
            .iter()
            .find(|opt| opt.name == QUERY)
            .ok_or_else(|| anyhow!("Missing query option"))?
            .value
            .as_ref()
            .ok_or_else(|| anyhow!("Query option is empty"))?
            .to_string();
        query_string.remove(query_string.len() - 1);
        query_string.remove(0);
        Ok(query_string)
    }
}

#[async_trait]
impl CommandRunner for PlayCommand {
    fn register(command: &mut CreateApplicationCommand) -> &mut CreateApplicationCommand {
        info!("Command registered: {}", SlashCommands::Play.as_str());
        command
            .name(SlashCommands::Play.as_str())
            .dm_permission(false)
            .create_option(|opt| {
                opt.required(true)
                    .name(QUERY)
                    .kind(CommandOptionType::String)
                    .description("Search query or youtube URL")
                    .channel_types(&[ChannelType::Text])
            })
            .description("Plays a track from youtube")
    }

    async fn run(
        ctx: &Context,
        command: &ApplicationCommandInteraction,
    ) -> Result<CommandResponse> {
        deferr_response(ctx, command).await?;
        let query = Self::get_query(command)?;

        let (early_response, handler) = Self::handle_connection(ctx, command).await?;
        if let Some(r) = early_response {
            return Ok(r);
        }
        let handler = handler.ok_or_else(|| anyhow!("Failed to retrieve handler!\nThis value should always be Some if handled correctly!"))?;

        let save_handler = retrieve_save_handler(ctx.data.clone()).await?;
        let saved_file = save_handler.get_saved_file(&query).await?;

        // WARN: Still does not check for file actully existing
        // BUG:  Still does not check for file actully existing
        let source = if let Some(saved) = saved_file {
            info!("Reading file from disk!");
            let mut source: Input =
                Restartable::ffmpeg(format!("{}/songbird_cache/{}", *HOME, saved.id), true)
                    .await?
                    .into();
            source.metadata.source_url = Some(saved.url);
            source.metadata.title = saved.title;
            source
        } else {
            info!("Searching youtube for: {}", query);
            // WARN: cannot be sure if query is actually url
            let source: Input = if query.starts_with("http") {
                Restartable::ytdl(query.clone(), true).await?.into()
            } else {
                Restartable::ytdl_search(query.clone(), true).await?.into()
            };

            let url = match source.metadata.source_url.as_ref() {
                Some(url) => url.to_string(),
                None => {
                    error!("Failed to retrieve url from input!");
                    return Ok(Self::make_response(
                        "Failed response".to_string(),
                        true,
                        None,
                    ));
                }
            };

            let title = source.metadata.title.clone();
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
            source
        };

        let title = source
            .metadata
            .title
            .clone()
            .unwrap_or_else(|| "TITLE NOT FOUND".to_string());
        let mut handle = handler.lock().await;
        handle.enqueue_source(source);

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
            ctx.set_activity(Activity::playing(&title)).await;
            Ok(Self::make_response(
                format!("Now playing: {}", title),
                false,
                Some(InteractionResponseType::DeferredUpdateMessage),
            ))
        } else {
            Ok(Self::make_response(
                format!("Added to queue: {}", title),
                false,
                Some(InteractionResponseType::DeferredUpdateMessage),
            ))
        }
    }

    fn deferr() -> bool {
        true
    }
}

pub(crate) struct SkipCommand;

#[async_trait]
impl CommandRunner for SkipCommand {
    fn register(command: &mut CreateApplicationCommand) -> &mut CreateApplicationCommand {
        info!("Command registered: {}", SlashCommands::Skip.as_str());
        command
            .name(SlashCommands::Skip.as_str())
            .dm_permission(false)
            .description("Skip current track")
    }

    async fn run(
        ctx: &Context,
        command: &ApplicationCommandInteraction,
    ) -> Result<CommandResponse> {
        let guild_id = match command.guild_id {
            Some(g) => g,
            None => {
                return Ok(Self::make_response(
                    "Command must be run in a guild!".to_string(),
                    true,
                    None,
                ));
            }
        };
        info!("Skip in guild: {}", guild_id.0);
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
                let title = current
                    .metadata()
                    .title
                    .clone()
                    .unwrap_or_else(|| "[TITLE NOT FOUND]".to_string());
                let _ = queue.skip()?;
                Ok(Self::make_response(
                    format!("Skipped: {}", title),
                    false,
                    None,
                ))
            } else {
                Ok(Self::make_response(
                    "There is nothing to skip!".to_string(),
                    true,
                    None,
                ))
            }
        } else {
            Ok(Self::make_response(
                "Failed to skip".to_string(),
                true,
                None,
            ))
        }
    }
}

pub(crate) struct StopCommand;

#[async_trait]
impl CommandRunner for StopCommand {
    fn register(command: &mut CreateApplicationCommand) -> &mut CreateApplicationCommand {
        info!("Command registered: {}", SlashCommands::Stop.as_str());
        command
            .name(SlashCommands::Stop.as_str())
            .dm_permission(false)
            .description("Stops the bot playing tracks and disconnects it")
    }

    async fn run(
        ctx: &Context,
        command: &ApplicationCommandInteraction,
    ) -> Result<CommandResponse> {
        let guild_id = match command.guild_id {
            Some(g) => g,
            None => {
                return Ok(Self::make_response(
                    "Command must be run in a guild!".to_string(),
                    true,
                    None,
                ));
            }
        };
        info!("Stop in guild: {}", guild_id.0);
        let manager = songbird::get(ctx)
            .await
            .expect("Songbird must be registered in client")
            .clone();
        if let Some(handler_lock) = manager.get(guild_id) {
            let mut handler = handler_lock.lock().await;
            // WARN: queue is cleared in event handler
            // let queue = handler.queue();
            // queue.stop();
            ctx.set_presence(None, OnlineStatus::Online).await;
            handler.leave().await?;
            Ok(Self::make_response("Stopping".to_string(), false, None))
        } else {
            Ok(Self::make_response(
                "Failed to stop".to_string(),
                true,
                None,
            ))
        }
    }
}

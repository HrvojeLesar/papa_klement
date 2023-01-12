use std::{collections::HashSet, env, sync::Arc};

use anyhow::{anyhow, Result};
use chrono::{DateTime, Utc};
use log::{error, info};
use mongodb::{bson::doc, Collection, Database};
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use serenity::{
    async_trait,
    builder::CreateApplicationCommand,
    model::prelude::{
        command::CommandOptionType,
        interaction::application_command::ApplicationCommandInteraction,
        interaction::InteractionResponseType, ChannelId, ChannelType,
    },
    prelude::{Context, Mutex, RwLock},
};
use sha2::{Digest, Sha256};
use songbird::{
    ffmpeg,
    input::{cached::Compressed, Input, Restartable},
    Call,
};
use tokio::process::Command;

use crate::{
    util::{retrieve_save_handler, CommandRunner},
    CommandResponse, SlashCommands,
};

const QUERY: &str = "query";
static HOME: Lazy<String> =
    Lazy::new(|| env::var("HOME").expect("HOME environment variable is required!"));

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
        self.db_handle.collection("cached_audio")
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
            .collection::<CachedAudioRecord>("cached_audio");
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

            let (handler, join_result) = manager.join(guild_id, channel_id).await;
            join_result?;
            // handler;
            // let handler = match manager.get(guild_id) {
            //     Some(_h) => {
            //         let (handler, join_result) = manager.join(guild_id, channel_id).await;
            //         join_result?;
            //         handler
            //     }
            //     None => {
            //         let (handler, join_result) = manager.join(guild_id, channel_id).await;
            //         join_result?;
            //         handler
            //     }
            // };
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

    async fn deferr_response(ctx: &Context, command: &ApplicationCommandInteraction) -> Result<()> {
        Ok(command
            .create_interaction_response(&ctx.http, |response| {
                response.kind(InteractionResponseType::DeferredChannelMessageWithSource)
            })
            .await?)
    }
}

#[async_trait]
impl CommandRunner for PlayCommand {
    fn register(command: &mut CreateApplicationCommand) -> &mut CreateApplicationCommand {
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
            .description("Test description")
    }

    async fn run(
        ctx: &Context,
        command: &ApplicationCommandInteraction,
    ) -> Result<CommandResponse> {
        Self::deferr_response(ctx, command).await?;
        let query = Self::get_query(command)?;

        let (early_response, handler) = Self::handle_connection(ctx, command).await?;
        if let Some(r) = early_response {
            return Ok(r);
        }
        let handler = handler.ok_or_else(|| anyhow!("Failed to retrieve handler!\nThis value should always be Some if handled correctly!"))?;

        let save_handler = retrieve_save_handler(ctx.data.clone()).await?;
        let saved_file = save_handler.get_saved_file(&query).await?;

        let source = if let Some(saved) = saved_file {
            info!("Reading from disk!");
            let mut source: Input = Compressed::new(
                // BUG: Does not check if file actually exists
                ffmpeg(format!("{}/songbird_cache/{}", *HOME, saved.id)).await?,
                songbird::driver::Bitrate::BitsPerSecond(128_000),
            )?
            .into();
            source.metadata.source_url = Some(saved.url);
            source.metadata.title = saved.title;
            source
        } else {
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

        {
            let mut handle = handler.lock().await;
            handle.enqueue_source(source);
        }

        Ok(Self::make_response(
            "Response".to_string(),
            true,
            Some(InteractionResponseType::DeferredUpdateMessage),
        ))
    }

    fn deferr() -> bool {
        true
    }

    fn make_response(
        content: String,
        ephemeral: bool,
        response_type: Option<InteractionResponseType>,
    ) -> CommandResponse {
        CommandResponse::new(content, ephemeral, response_type, Self::deferr())
    }
}

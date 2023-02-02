use anyhow::Result;
use aoc::start_aoc_auto_fetch;
use banaj_matijosa::{BAN_COOLDOWN_TIME, MATT_BAN_COLLECTION};
use bantop::BanTopCommand;
use music::{PlayCommand, QueuedDisconnect, SaveHandler, SkipCommand, StopCommand};
use serde::{Deserialize, Serialize};
use songbird::SerenityInit;
use std::{env, str::FromStr, sync::Arc};
use util::CommandRunner;

use log::{error, info};
use mongodb::{bson::doc, options::ClientOptions, Database};
use serenity::{
    async_trait,
    model::{
        prelude::{
            interaction::{
                application_command::ApplicationCommandInteraction, Interaction,
                InteractionResponseType,
            },
            GuildId, Member, Message, Ready,
        },
        user::User,
    },
    prelude::{Context, EventHandler, GatewayIntents, RwLock, TypeMapKey},
    Client,
};

use crate::{
    aoc::{AddPrivateLeaderboardCommand, SetSessionCookieCommand, SpeedrunCommand},
    music::QueueCommand,
};

mod aoc;
mod banaj_matijosa;
mod bantop;
mod music;
mod roles;
mod unban;
mod util;

pub const UNDERSCOREBANS: &str = "_bans";
const MONGODB_NAME: &str = "papa_klement";

pub(crate) enum SlashCommands {
    BanTop,
    Play,
    Skip,
    Stop,
    Queue,
    Speedrun,
    AddPrivateLeaderboard,
    SetSessionCookie,
}

impl SlashCommands {
    pub(crate) const fn as_str(&self) -> &'static str {
        match self {
            Self::BanTop => "bantop",
            Self::Play => "play",
            Self::Skip => "skip",
            Self::Stop => "stop",
            Self::Queue => "queue",
            Self::Speedrun => "speedrun",
            Self::AddPrivateLeaderboard => "addprivateleaderboard",
            Self::SetSessionCookie => "setsessioncookie",
        }
    }
}

impl FromStr for SlashCommands {
    type Err = anyhow::Error;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        match input {
            "bantop" => Ok(Self::BanTop),
            "play" => Ok(Self::Play),
            "skip" => Ok(Self::Skip),
            "stop" => Ok(Self::Stop),
            "queue" => Ok(Self::Queue),
            "speedrun" => Ok(Self::Speedrun),
            "addprivateleaderboard" => Ok(Self::AddPrivateLeaderboard),
            "setsessioncookie" => Ok(Self::SetSessionCookie),
            _ => Err(anyhow::anyhow!("Failed to convert string to SlashCommand")),
        }
    }
}

pub(crate) struct MongoDatabaseHandle;
impl TypeMapKey for MongoDatabaseHandle {
    type Value = Arc<Database>;
}

pub(crate) struct MongoClientHandle;
impl TypeMapKey for MongoClientHandle {
    type Value = Arc<mongodb::Client>;
}

#[derive(Serialize, Deserialize)]
pub(crate) struct MattBanCooldown {
    pub(crate) cooldown: i64,
    pub(crate) last_ban_timestamp: i64,
}

impl TypeMapKey for MattBanCooldown {
    type Value = Arc<RwLock<Self>>;
}

pub(crate) struct SaveHandlerHandle;
impl TypeMapKey for SaveHandlerHandle {
    type Value = Arc<SaveHandler>;
}

#[derive(Clone, Debug)]
pub(crate) struct CommandResponse {
    content: String,
    ephemeral: bool,
    response_type: InteractionResponseType,
    has_deferred_response: bool,
}

impl CommandResponse {
    pub(crate) fn new(
        content: String,
        ephemeral: bool,
        response_type: Option<InteractionResponseType>,
        has_deferred_response: bool,
    ) -> Self {
        Self {
            content,
            ephemeral,
            response_type: response_type
                .unwrap_or(InteractionResponseType::ChannelMessageWithSource),
            has_deferred_response,
        }
    }
}

async fn register_slash_commands(ctx: &Context, ready: &Ready) -> Result<()> {
    for guild in ready.guilds.iter() {
        guild
            .id
            .set_application_commands(&ctx.http, |commands| {
                commands
                    .create_application_command(|command| BanTopCommand::register(command))
                    .create_application_command(|command| PlayCommand::register(command))
                    .create_application_command(|command| SkipCommand::register(command))
                    .create_application_command(|command| StopCommand::register(command))
                    .create_application_command(|command| QueueCommand::register(command))
                    .create_application_command(|command| SpeedrunCommand::register(command))
                    .create_application_command(|command| {
                        AddPrivateLeaderboardCommand::register(command)
                    })
                    .create_application_command(|command| {
                        SetSessionCookieCommand::register(command)
                    })
            })
            .await?;
    }
    info!("Successfully registered slash commands");
    Ok(())
}

async fn handle_application_command(
    ctx: &Context,
    command: ApplicationCommandInteraction,
) -> Result<()> {
    let command_response = match command.data.name.as_str().parse()? {
        SlashCommands::BanTop => BanTopCommand::run(ctx, &command).await,
        SlashCommands::Play => PlayCommand::run(ctx, &command).await,
        SlashCommands::Skip => SkipCommand::run(ctx, &command).await,
        SlashCommands::Stop => StopCommand::run(ctx, &command).await,
        SlashCommands::Queue => QueueCommand::run(ctx, &command).await,
        SlashCommands::Speedrun => SpeedrunCommand::run(ctx, &command).await,
        SlashCommands::AddPrivateLeaderboard => {
            AddPrivateLeaderboardCommand::run(ctx, &command).await
        }
        SlashCommands::SetSessionCookie => SetSessionCookieCommand::run(ctx, &command).await,
    };

    let command_response = match command_response {
        Ok(c) => c,
        Err(err) => {
            error!("Error handling slash command: {:#?}", err);
            // BUG: Fails to respond if message is deferred
            CommandResponse::new(format!("Error happend: {:#?}", err), false, None, false)
        }
    };

    if !command_response.has_deferred_response {
        command
            .create_interaction_response(&ctx.http, |response| {
                response
                    .kind(command_response.response_type)
                    .interaction_response_data(|message| {
                        message
                            .ephemeral(command_response.ephemeral)
                            .content(command_response.content)
                    })
            })
            .await?;
    } else {
        command
            .create_followup_message(&ctx.http, |response| {
                response
                    .ephemeral(command_response.ephemeral)
                    .content(command_response.content)
            })
            .await?;
    }
    Ok(())
}

struct Handler;
#[async_trait]
impl EventHandler for Handler {
    async fn interaction_create(&self, ctx: Context, interaction: Interaction) {
        if let Interaction::ApplicationCommand(command) = interaction {
            match handle_application_command(&ctx, command).await {
                Ok(_) => {}
                Err(e) => error!("Application command error: {}", e),
            }
        }
    }

    async fn guild_member_update(&self, ctx: Context, _old: Option<Member>, new: Member) {
        match self.save_member_roles_on_update(&ctx, &new).await {
            Ok(_) => (),
            Err(e) => error!("Guild member update error: {}", e),
        };
    }
    async fn guild_member_addition(&self, ctx: Context, mut new: Member) {
        match self.grant_roles_and_nickname(&ctx, &mut new).await {
            Ok(_) => (),
            Err(e) => error!("Guild member addition error: {}", e),
        };
    }
    async fn guild_ban_addition(&self, ctx: Context, guild_id: GuildId, banned_user: User) {
        match self.unban(&ctx, &guild_id, &banned_user).await {
            Ok(_) => (),
            Err(e) => error!("Guild ban addition error: {}", e),
        };
    }

    async fn cache_ready(&self, ctx: Context, _guilds: Vec<GuildId>) {
        log::info!("Cache ready");
        match self.save_roles_on_startup(&ctx).await {
            Ok(_) => (),
            Err(e) => error!("Save roles on startup error: {}", e),
        };
    }

    async fn message(&self, ctx: Context, message: Message) {
        match self.banaj_matijosa(&ctx, &message).await {
            Ok(_) => (),
            Err(e) => error!("Banaj Matijoša error: {}", e),
        };
    }

    async fn ready(&self, ctx: Context, ready: Ready) {
        log::info!("Ready");
        match register_slash_commands(&ctx, &ready).await {
            Ok(_) => {}
            Err(e) => {
                error!("Ready error: {:#?}", e);
            }
        };
    }
}

// (songbird::remove...)
// TODO: all AoC stuff

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().expect(".env file not found");

    pretty_env_logger::env_logger::init_from_env(
        pretty_env_logger::env_logger::Env::new().default_filter_or("warn"),
    );

    let mut mongo_client_options =
        ClientOptions::parse(env::var("MONGO_URL").expect("MONGO_URL is required!"))
            .await
            .unwrap();
    mongo_client_options.app_name = Some("Papa_Klement".to_string());
    let mongo_client = mongodb::Client::with_options(mongo_client_options).unwrap();

    let mongo_database = mongo_client.database(MONGODB_NAME);

    let token = env::var("DISCORD_TOKEN").expect("DISCORD_TOKEN env variable is not defined!");
    let gateway = GatewayIntents::GUILD_MEMBERS
        | GatewayIntents::MESSAGE_CONTENT
        | GatewayIntents::GUILDS
        | GatewayIntents::GUILD_BANS
        | GatewayIntents::GUILD_MESSAGES
        | GatewayIntents::DIRECT_MESSAGES
        | GatewayIntents::non_privileged();
    let mut client = Client::builder(token, gateway)
        .event_handler(Handler)
        .register_songbird()
        .await
        .expect("Error creating client");

    {
        let last_ban = {
            let collection = mongo_database.collection::<MattBanCooldown>(MATT_BAN_COLLECTION);
            collection.find_one(doc! {"_id": "COOLDOWN"}, None).await?
        };
        let mut lock = client.data.write().await;
        let db_handle = Arc::new(mongo_database);
        lock.insert::<MongoDatabaseHandle>(db_handle.clone());
        lock.insert::<MongoClientHandle>(Arc::new(mongo_client));
        match last_ban {
            Some(lb) => {
                lock.insert::<MattBanCooldown>(Arc::new(RwLock::new(lb)));
            }
            None => {
                lock.insert::<MattBanCooldown>(Arc::new(RwLock::new(MattBanCooldown {
                    cooldown: BAN_COOLDOWN_TIME,
                    last_ban_timestamp: 0,
                })));
            }
        }
        lock.insert::<SaveHandlerHandle>(Arc::new(SaveHandler::new(db_handle.clone())));
        lock.insert::<QueuedDisconnect>(Arc::new(RwLock::new(QueuedDisconnect::new())));

        tokio::spawn(start_aoc_auto_fetch(db_handle));
    }

    if let Err(err) = client.start().await {
        println!("Client error: {:?}", err);
    }

    Ok(())
}

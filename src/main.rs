use anyhow::Result;
use aoc::{start_aoc_auto_fetch, RollCommand};
use banaj_matijosa::{BAN_COOLDOWN_TIME, MATT_BAN_COLLECTION};
use bantop::BanTopCommand;
use client::init_serenity_client;
use database::init_database;
use event_handlers::mr_handler::MrHandler;
use music::{PlayCommand, QueuedDisconnect, SaveHandler, SkipCommand, StopCommand};
use serde::{Deserialize, Serialize};
use songbird::SerenityInit;
use std::{env, str::FromStr, sync::Arc};
use util::CommandRunner;

use log::{error, info, warn};
use mongodb::{bson::doc, options::ClientOptions, Database};
use serenity::{
    all::{
        CommandInteraction, CreateInteractionResponse, CreateInteractionResponseMessage,
        GuildMemberUpdateEvent,
    },
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
mod client;
mod commands;
mod database;
mod event_handlers;
mod music;
mod roles;
mod unban;
mod util;

pub const UNDERSCOREBANS: &str = "_bans";

pub(crate) enum SlashCommands {
    BanTop,
    Play,
    Skip,
    Stop,
    Queue,
    Speedrun,
    AddPrivateLeaderboard,
    SetSessionCookie,
    Roll,
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
            Self::Roll => "roll",
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
            "roll" => Ok(Self::Roll),
            _ => Err(anyhow::anyhow!("Failed to convert string to SlashCommand")),
        }
    }
}

pub(crate) struct MongoDatabaseHandle;
impl TypeMapKey for MongoDatabaseHandle {
    type Value = Database;
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
    response_type: CreateInteractionResponse,
    has_deferred_response: bool,
}

impl CommandResponse {
    pub(crate) fn new(
        content: String,
        ephemeral: bool,
        response_type: Option<CreateInteractionResponse>,
        has_deferred_response: bool,
    ) -> Self {
        Self {
            content,
            ephemeral,
            response_type: response_type.unwrap_or(CreateInteractionResponse::Defer(
                CreateInteractionResponseMessage::new(),
            )),
            has_deferred_response,
        }
    }
}

async fn register_slash_commands(ctx: &Context, ready: &Ready) -> Result<()> {
    for guild in ready.guilds.iter() {
        guild
            .id
            .set_commands(&ctx.http, commands::create_commands::registered_commands())
            .await?;
    }
    info!("Successfully registered slash commands");
    Ok(())
}

async fn handle_application_command(ctx: &Context, command: CommandInteraction) -> Result<()> {
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
        SlashCommands::Roll => RollCommand::run(ctx, &command).await,
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
            .create_response(
                &ctx.http,
                command_response.response_type | response | {
                    response
                        .kind(command_response.response_type)
                        .interaction_response_data(|message| {
                            message
                                .ephemeral(command_response.ephemeral)
                                .content(command_response.content)
                        })
                },
            )
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

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().expect(".env file not found");

    pretty_env_logger::env_logger::init_from_env(
        pretty_env_logger::env_logger::Env::new().default_filter_or("warn"),
    );

    let mongo_database = init_database().await;

    let client = init_serenity_client(vec![MrHandler]).await;

    {
        let mut lock = client.data.write().await;
        lock.insert::<MongoDatabaseHandle>(mongo_database.clone());
        lock.insert::<QueuedDisconnect>(Arc::new(RwLock::new(QueuedDisconnect::new())));

        tokio::spawn(start_aoc_auto_fetch(mongo_database));
    }

    if let Err(err) = client.start().await {
        println!("Client error: {:?}", err);
    }

    Ok(())
}

use std::sync::Arc;

use anyhow::Result;
use aoc::start_aoc_auto_fetch;
use client::init_serenity_client;
use database::{init_database, MongoDatabaseHandle};
use event_handlers::mr_handler::MrHandler;
use music::{QueuedDisconnect, SaveHandler};
use songbird::typemap::TypeMapKey;
use tokio::sync::RwLock;

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
const DISCORD_MESSAGE_MAX_LENGTH: usize = 2000;

pub(crate) struct SaveHandlerHandle;
impl TypeMapKey for SaveHandlerHandle {
    type Value = Arc<SaveHandler>;
}

#[derive(Clone, Debug)]
pub(crate) struct CommandResponse {
    content: String,
    ephemeral: bool,
    is_deferred: bool,
}

impl CommandResponse {
    pub(crate) fn new(mut content: String, ephemeral: bool, is_deferred: bool) -> Self {
        if content.len() > DISCORD_MESSAGE_MAX_LENGTH {
            content.truncate(DISCORD_MESSAGE_MAX_LENGTH);
        }
        Self {
            content,
            ephemeral,
            is_deferred,
        }
    }
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
        lock.insert::<SaveHandlerHandle>(Arc::new(SaveHandler::new(mongo_database.clone())));
        lock.insert::<MongoDatabaseHandle>(mongo_database.clone());
        lock.insert::<QueuedDisconnect>(Arc::new(RwLock::new(QueuedDisconnect::new())));

        tokio::spawn(start_aoc_auto_fetch(mongo_database));
    }

    if let Err(err) = client.start().await {
        println!("Client error: {:?}", err);
    }

    Ok(())
}

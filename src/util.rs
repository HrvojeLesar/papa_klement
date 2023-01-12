use std::sync::Arc;

use anyhow::Result;
use mongodb::Database;
use serenity::{
    async_trait,
    builder::CreateApplicationCommand,
    model::prelude::interaction::application_command::ApplicationCommandInteraction,
    prelude::{Context, RwLock, TypeMap},
};

use crate::{music::SaveHandler, CommandResponse, MongoDatabaseHandle, SaveHandlerHandle};

pub(crate) async fn retrieve_db_handle(data: Arc<RwLock<TypeMap>>) -> Result<Arc<Database>> {
    let database_handle = {
        data.read()
            .await
            .get::<MongoDatabaseHandle>()
            .ok_or_else(|| anyhow::anyhow!("Failed to retrieve MongoDatabaseHandle from data"))?
            .clone()
    };
    Ok(database_handle)
}

pub(crate) async fn retrieve_save_handler(data: Arc<RwLock<TypeMap>>) -> Result<Arc<SaveHandler>> {
    Ok(data
        .read()
        .await
        .get::<SaveHandlerHandle>()
        .ok_or_else(|| anyhow::anyhow!("Failed to retrieve SaveHandlerHandle from data"))?
        .clone())
}

#[async_trait]
pub(crate) trait CommandRunner {
    async fn run(ctx: &Context, command: &ApplicationCommandInteraction)
        -> Result<CommandResponse>;
    fn register(command: &mut CreateApplicationCommand) -> &mut CreateApplicationCommand;
}

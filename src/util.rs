use std::sync::Arc;

use anyhow::Result;
use mongodb::Database;
use serenity::{
    all::{
        CommandInteraction, CreateCommand, CreateInteractionResponse,
        CreateInteractionResponseMessage,
    },
    async_trait,
    prelude::{Context, RwLock, TypeMap},
};

use crate::{music::SaveHandler, CommandResponse, MongoDatabaseHandle, SaveHandlerHandle};

pub(crate) async fn retrieve_db_handle(data: Arc<RwLock<TypeMap>>) -> Result<Database> {
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

pub(crate) async fn defer_response(ctx: &Context, command: &CommandInteraction) -> Result<()> {
    Ok(command
        .create_response(
            &ctx.http,
            CreateInteractionResponse::Defer(CreateInteractionResponseMessage::new()),
        )
        .await?)
}

#[async_trait()]
pub(crate) trait CommandRunner: Send {
    async fn run(&self, ctx: &Context, command: &CommandInteraction) -> Result<CommandResponse>;
    fn register(&self) -> CreateCommand;
    fn has_deferred_response(&self) -> bool {
        false
    }
}

pub(crate) trait MakeCommandResponse
where
    Self: CommandRunner + Send,
{
    fn make_response(&self, content: impl Into<String>, ephemeral: bool) -> CommandResponse {
        CommandResponse::new(content.into(), ephemeral, self.has_deferred_response())
    }
}

use std::sync::Arc;

use anyhow::Result;
use mongodb::Database;
use serenity::{
    async_trait,
    builder::CreateApplicationCommand,
    model::prelude::interaction::{
        application_command::ApplicationCommandInteraction, InteractionResponseType,
    },
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

pub(crate) async fn deferr_response(
    ctx: &Context,
    command: &ApplicationCommandInteraction,
) -> Result<()> {
    Ok(command
        .create_interaction_response(&ctx.http, |response| {
            response.kind(InteractionResponseType::DeferredChannelMessageWithSource)
        })
        .await?)
}

#[async_trait]
pub(crate) trait CommandRunner {
    async fn run(ctx: &Context, command: &ApplicationCommandInteraction)
        -> Result<CommandResponse>;
    fn register(command: &mut CreateApplicationCommand) -> &mut CreateApplicationCommand;
    fn deferr() -> bool {
        false
    }
    fn make_response(
        content: String,
        ephemeral: bool,
        response_type: Option<InteractionResponseType>,
    ) -> CommandResponse {
        CommandResponse::new(content, ephemeral, response_type, Self::deferr())
    }
}

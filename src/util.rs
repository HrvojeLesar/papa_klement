use std::sync::Arc;

use anyhow::Result;
use mongodb::Database;
use serenity::prelude::Context;

use crate::MongoDatabaseHandle;

pub async fn retrieve_db_handle(ctx: &Context) -> Result<Arc<Database>> {
    let database_handle = {
        ctx.data
            .read()
            .await
            .get::<MongoDatabaseHandle>()
            .ok_or_else(|| anyhow::anyhow!("Failed to retrieve MongoDatabaseHandle from data"))?
            .clone()
    };
    Ok(database_handle)
}

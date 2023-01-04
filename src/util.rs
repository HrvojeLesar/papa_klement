use std::sync::Arc;

use anyhow::Result;
use mongodb::Database;
use serenity::prelude::{RwLock, TypeMap};

use crate::MongoDatabaseHandle;

pub async fn retrieve_db_handle(data: Arc<RwLock<TypeMap>>) -> Result<Arc<Database>> {
    let database_handle = {
        data.read()
            .await
            .get::<MongoDatabaseHandle>()
            .ok_or_else(|| anyhow::anyhow!("Failed to retrieve MongoDatabaseHandle from data"))?
            .clone()
    };
    Ok(database_handle)
}

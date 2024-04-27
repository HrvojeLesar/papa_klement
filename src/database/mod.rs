use std::env;

use mongodb::{options::ClientOptions, Database};
use songbird::typemap::TypeMapKey;

pub(crate) const MONGODB_NAME: &str = "papa_klement";

pub(crate) struct MongoDatabaseHandle;
impl TypeMapKey for MongoDatabaseHandle {
    type Value = Database;
}

pub(crate) async fn init_database() -> Database {
    let mut mongo_client_options =
        ClientOptions::parse(env::var("MONGO_URL").expect("MONGO_URL is required!"))
            .await
            .unwrap();
    if mongo_client_options.app_name.is_none() {
        mongo_client_options.app_name = Some("Papa_Klement".to_string());
    }
    let mongo_client = mongodb::Client::with_options(mongo_client_options).expect("Mongo client");

    mongo_client.database(
        &env::var("MONGODB_NAME")
            .ok()
            .unwrap_or(MONGODB_NAME.to_string()),
    )
}

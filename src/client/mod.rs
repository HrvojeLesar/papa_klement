use std::env;

use serenity::all::{EventHandler, GatewayIntents};
use songbird::SerenityInit;

pub(crate) async fn init_serenity_client<H>(event_handlers: Vec<H>) -> serenity::Client
where
    H: EventHandler + 'static,
{
    let token = env::var("DISCORD_TOKEN").expect("DISCORD_TOKEN env variable is not defined!");
    let gateway = GatewayIntents::GUILD_MEMBERS
        | GatewayIntents::MESSAGE_CONTENT
        | GatewayIntents::GUILDS
        | GatewayIntents::GUILD_MODERATION
        | GatewayIntents::GUILD_MESSAGES
        | GatewayIntents::DIRECT_MESSAGES
        | GatewayIntents::non_privileged();
    let mut builder = serenity::Client::builder(token, gateway).register_songbird();

    for handler in event_handlers {
        builder = builder.event_handler(handler);
    }

    builder.await.expect("Error registering event handler")
}

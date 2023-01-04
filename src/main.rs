use std::{env, sync::Arc};

use log::error;
use mongodb::{options::ClientOptions, Database};
use serenity::{
    async_trait,
    model::{
        prelude::{
            interaction::{Interaction, InteractionResponseType},
            GuildId, Member, Message, Ready,
        },
        user::User,
    },
    prelude::{Context, EventHandler, GatewayIntents, RwLock, TypeMapKey},
    Client,
};

mod banaj_matijosa;
mod roles;
mod unban;
mod util;

pub(crate) struct MongoDatabaseHandle;
impl TypeMapKey for MongoDatabaseHandle {
    type Value = Arc<Database>;
}

pub(crate) struct MongoClientHandle;
impl TypeMapKey for MongoClientHandle {
    type Value = Arc<mongodb::Client>;
}

pub(crate) struct MattBanCooldown {
    pub(crate) cooldown: i64,
    pub(crate) last_ban_timestamp: i64,
}

impl TypeMapKey for MattBanCooldown {
    type Value = Arc<RwLock<Self>>;
}

struct Handler;

#[async_trait]
impl EventHandler for Handler {
    async fn interaction_create(&self, ctx: Context, interaction: Interaction) {
        println!("Interaction");
        // TODO: Top bans command
        // TODO: Top banee command
        if let Interaction::ApplicationCommand(command) = interaction {
            let content = match command.data.name.as_str() {
                "me" => "lemejo".to_string(),
                _ => "not implemented :(".to_string(),
            };

            if let Err(why) = command
                .create_interaction_response(&ctx.http, |response| {
                    response
                        .kind(InteractionResponseType::ChannelMessageWithSource)
                        .interaction_response_data(|message| {
                            message.ephemeral(true).content(content)
                        })
                })
                .await
            {
                println!("Cannot respond to slash command: {}", why);
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
        match self.save_roles_on_startup(&ctx).await {
            Ok(_) => (),
            Err(e) => error!("Save roles on startup error: {}", e),
        };
    }

    async fn message(&self, ctx: Context, message: Message) {
        println!("Mes");
        match self.banaj_matijosa(&ctx, &message).await {
            Ok(_) => (),
            Err(e) => error!("Banaj Matijoša error: {}", e),
        };
    }

    async fn ready(&self, ctx: Context, ready: Ready) {
        let guild_id = ready.guilds.first().unwrap().id;

        let _commands = match guild_id
            .set_application_commands(&ctx.http, |commands| {
                commands.create_application_command(|command| {
                    command
                        .name("me")
                        .description("Checking for command collision")
                })
            })
            .await
        {
            Ok(c) => c,
            Err(e) => {
                println!("Ready error: {:#?}", e);
                return;
            }
        };
    }
}

#[tokio::main]
async fn main() {
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

    let mongo_database = mongo_client.database("papa_klement");

    let token = env::var("DISCORD_TOKEN").expect("DISCORD_TOKEN env variable is not defined!");
    let gateway = GatewayIntents::GUILD_MEMBERS
        | GatewayIntents::MESSAGE_CONTENT
        | GatewayIntents::GUILDS
        | GatewayIntents::GUILD_BANS
        | GatewayIntents::GUILD_MESSAGES
        | GatewayIntents::DIRECT_MESSAGES;
    let mut client = Client::builder(token, gateway)
        .event_handler(Handler)
        .await
        .expect("Error creating client");

    {
        let mut lock = client.data.write().await;
        lock.insert::<MongoDatabaseHandle>(Arc::new(mongo_database));
        lock.insert::<MongoClientHandle>(Arc::new(mongo_client));
        lock.insert::<MattBanCooldown>(Arc::new(RwLock::new(MattBanCooldown {
            cooldown: 3600,
            last_ban_timestamp: 0,
        })));
    }

    if let Err(err) = client.start().await {
        println!("Client error: {:?}", err);
    }
}

use log::{error, warn};
use serenity::{
    all::{
        Context, EventHandler, GuildId, GuildMemberUpdateEvent, Interaction, Member, Message, Ready, User
    },
    async_trait,
};

use crate::{handle_application_command, register_slash_commands};

// Name credits to Fabian Benc
pub(crate) struct MrHandler;
#[async_trait]
impl EventHandler for MrHandler {
    async fn interaction_create(&self, ctx: Context, interaction: Interaction) {
        if let Interaction::Command(command) = interaction {
            match handle_application_command(&ctx, command).await {
                Ok(_) => {}
                Err(e) => error!("Application command error: {}", e),
            }
        }
    }

    async fn guild_member_update(
        &self,
        ctx: Context,
        _old: Option<Member>,
        new: Option<Member>,
        event: GuildMemberUpdateEvent,
    ) {
        if let Some(new) = new {
            match self.save_member_roles_on_update(&ctx, &new).await {
                Ok(_) => (),
                Err(e) => error!("Guild member update error: {}", e),
            };
        } else {
            warn!("Guild Member Update called without new member in update");
        }
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
        log::info!("Cache ready");
        match self.save_roles_on_startup(&ctx).await {
            Ok(_) => (),
            Err(e) => error!("Save roles on startup error: {}", e),
        };
    }

    async fn message(&self, ctx: Context, message: Message) {
        match self.banaj_matijosa(&ctx, &message).await {
            Ok(_) => (),
            Err(e) => error!("Banaj Matijoša error: {}", e),
        };
    }

    async fn ready(&self, ctx: Context, ready: Ready) {
        log::info!("Ready");
        println!("Bot started");
        match register_slash_commands(&ctx, &ready).await {
            Ok(_) => {}
            Err(e) => {
                error!("Ready error: {:#?}", e);
            }
        };
    }
}

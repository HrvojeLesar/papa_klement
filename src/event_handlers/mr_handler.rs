use anyhow::Result;
use log::{error, warn};
use serenity::{
    all::{
        CommandInteraction, Context, CreateInteractionResponse, CreateInteractionResponseFollowup,
        CreateInteractionResponseMessage, EventHandler, GuildId, GuildMemberUpdateEvent,
        Interaction, Member, Message, Ready, User,
    },
    async_trait,
};

use crate::{
    commands::{create_commands::register_slash_commands, slash_commands::SlashCommands},
    CommandResponse,
};

// Name credits to Fabian Benc
pub(crate) struct MrHandler;

impl MrHandler {
    async fn handle_application_command(
        &self,
        ctx: &Context,
        command: CommandInteraction,
    ) -> Result<()> {
        let slash_command = command
            .data
            .name
            .as_str()
            .parse::<SlashCommands>()?
            .get_command();

        let response = match slash_command.run(ctx, &command).await {
            Ok(c) => c,
            Err(err) => {
                error!("Error handling slash command: {:#?}", err);
                CommandResponse::new(format!("Error: {:#?}", err), false, false)
            }
        };

        if slash_command.has_deferred_response() {
            command
                .create_followup(
                    &ctx.http,
                    CreateInteractionResponseFollowup::new().content(response.content),
                )
                .await?;
        } else {
            let message = CreateInteractionResponseMessage::new()
                .content(response.content)
                .ephemeral(response.ephemeral);
            let interaction_response = if response.is_deferred {
                CreateInteractionResponse::Defer(message)
            } else {
                CreateInteractionResponse::Message(message)
            };
            command
                .create_response(&ctx.http, interaction_response)
                .await?;
        }
        Ok(())
    }
}

#[async_trait]
impl EventHandler for MrHandler {
    async fn interaction_create(&self, ctx: Context, interaction: Interaction) {
        if let Interaction::Command(command) = interaction {
            match self.handle_application_command(&ctx, command).await {
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
        _event: GuildMemberUpdateEvent,
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

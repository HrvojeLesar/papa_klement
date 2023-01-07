use anyhow::Result;
use serenity::{
    async_trait,
    builder::CreateApplicationCommand,
    model::prelude::{interaction::application_command::ApplicationCommandInteraction, ChannelId},
    prelude::Context,
};

use crate::{util::CommandRunner, CommandResponse, SlashCommands};

pub(crate) struct PlayCommand;

impl PlayCommand {
    async fn get_members_voice_channel(
        ctx: &Context,
        command: &ApplicationCommandInteraction,
    ) -> Result<Option<ChannelId>> {
        if let (Some(guild_id), Some(member)) = (command.guild_id.as_ref(), command.member.as_ref())
        {
            let guild = match guild_id.to_guild_cached(&ctx.cache) {
                Some(g) => g,
                None => return Err(anyhow::anyhow!("Message not sent in guild")),
            };
            Ok(guild
                .voice_states
                .get(&member.user.id)
                .and_then(|voice_state| voice_state.channel_id))
        } else {
            Ok(None)
        }
    }
}

#[async_trait]
impl CommandRunner for PlayCommand {
    fn register(command: &mut CreateApplicationCommand) -> &mut CreateApplicationCommand {
        command
            .name(SlashCommands::Play.as_str())
            .description("Test description")
    }

    async fn run(
        ctx: &Context,
        command: &ApplicationCommandInteraction,
    ) -> Result<CommandResponse> {
        todo!()
    }
}

use anyhow::Result;
use serenity::{
    builder::CreateApplicationCommand,
    model::prelude::interaction::application_command::CommandDataOption,
};

use crate::SlashCommands;

pub(crate) struct BanTopCommand;

impl BanTopCommand {
    pub(crate) async fn run(options: &[CommandDataOption]) -> Result<String> {
        todo!()
    }

    pub(crate) fn register(
        command: &mut CreateApplicationCommand,
    ) -> &mut CreateApplicationCommand {
        command
            .name(SlashCommands::BanTop.as_str())
            .description("Test description")
    }
}

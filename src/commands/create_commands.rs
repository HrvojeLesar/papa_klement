use anyhow::Result;
use log::info;
use serenity::all::{Context, CreateCommand, Ready};

use crate::{
    aoc::{AddPrivateLeaderboardCommand, RollCommand, SetSessionCookieCommand, SpeedrunCommand},
    bantop::BanTopCommand,
    music::{PlayCommand, QueueCommand, SkipCommand, StopCommand},
    util::CommandRunner,
};

pub(crate) async fn register_slash_commands(ctx: &Context, ready: &Ready) -> Result<()> {
    for guild in ready.guilds.iter() {
        guild
            .id
            .set_commands(&ctx.http, registered_commands())
            .await?;
    }
    info!("Successfully registered slash commands");
    Ok(())
}

fn registered_commands() -> Vec<CreateCommand> {
    return vec![
        BanTopCommand {}.register(),
        PlayCommand {}.register(),
        SkipCommand {}.register(),
        StopCommand {}.register(),
        QueueCommand {}.register(),
        SpeedrunCommand {}.register(),
        AddPrivateLeaderboardCommand {}.register(),
        SetSessionCookieCommand {}.register(),
        RollCommand {}.register(),
    ];
}

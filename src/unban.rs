use crate::Handler;

use anyhow::Result;
use serenity::{
    model::{prelude::GuildId, user::User},
    prelude::Context,
};

impl Handler {
    pub async fn unban(&self, ctx: &Context, guild: &GuildId, banned_user: &User) -> Result<()> {
        guild.unban(&ctx.http, banned_user.id).await?;
        let mut invite = None;
        for (channel_id, channel) in guild.channels(&ctx.http).await?.iter() {
            if channel.is_text_based() {
                invite = Some(
                    channel_id
                        .create_invite(&ctx.http, |f| f.max_uses(10))
                        .await?,
                );
                break;
            }
        }
        match invite {
            Some(inv) => {
                banned_user.dm(&ctx.http, |f| f.content(inv.url())).await?;
            }
            None => return Err(anyhow::anyhow!("Failed to create invite!")),
        }
        Ok(())
    }
}

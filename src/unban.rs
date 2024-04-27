use crate::{event_handlers::mr_handler::MrHandler, util::retrieve_db_handle, UNDERSCOREBANS};

use anyhow::Result;
use chrono::{DateTime, Utc};
use log::{error, info};
use mongodb::{bson::doc, IndexModel};
use serde::{Deserialize, Serialize};
use serenity::{
    all::{CreateInvite, CreateMessage},
    model::{
        guild::audit_log::Action,
        prelude::{GuildId, MemberAction},
        user::User,
    },
    prelude::Context,
};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct BanRecordUser(pub(crate) i64);

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct BanRecord {
    pub(crate) banned_by: BanRecordUser,
    pub(crate) banned_user: BanRecordUser,
    pub(crate) reason: Option<String>,
    pub(crate) timestamp: DateTime<Utc>,
}

impl MrHandler {
    async fn record_ban(&self, ctx: &Context, guild: &GuildId, banned_user: &User) -> Result<()> {
        let ban_logs = guild
            .audit_logs(
                &ctx.http,
                Some(Action::Member(MemberAction::BanAdd)),
                None,
                None,
                None,
            )
            .await?;
        let latest_ban = ban_logs.entries.iter().find(|log| {
            if let Some(target) = log.target_id {
                if target.get() == banned_user.id.get() {
                    return true;
                }
            }
            false
        });
        if let Some(latest_ban) = latest_ban {
            let target_id = if let Some(generic_id) = latest_ban.target_id {
                generic_id.get()
            } else {
                0
            };
            let banned_by = BanRecordUser(latest_ban.user_id.get() as i64);
            let banned_user = BanRecordUser(target_id as i64);
            let record = BanRecord {
                banned_by,
                banned_user,
                reason: latest_ban.reason.clone(),
                timestamp: Utc::now(),
            };
            let bans_collection = retrieve_db_handle(ctx.data.clone())
                .await?
                .collection::<BanRecord>(&format!("{}{UNDERSCOREBANS}", guild.get()));
            bans_collection
                .create_indexes(
                    [
                        IndexModel::builder().keys(doc! {"banned_by": 1}).build(),
                        IndexModel::builder().keys(doc! {"banned_user": 1}).build(),
                    ],
                    None,
                )
                .await?;
            bans_collection.insert_one(&record, None).await?;
            info!("Recorded new ban: {:#?}", record);
        }
        Ok(())
    }

    pub(crate) async fn unban(
        &self,
        ctx: &Context,
        guild: &GuildId,
        banned_user: &User,
    ) -> Result<()> {
        guild.unban(&ctx.http, banned_user.id).await?;
        info!("Unbanned {}", banned_user.id);
        if let Err(e) = self.record_ban(ctx, guild, banned_user).await {
            error!("Record ban error: {:#?}", e);
        }
        let mut invite = None;
        for (channel_id, channel) in guild.channels(&ctx.http).await?.iter() {
            if channel.is_text_based() {
                invite = Some(
                    channel_id
                        .create_invite(&ctx.http, CreateInvite::new().max_uses(10))
                        .await?,
                );
                break;
            }
        }
        match invite {
            Some(inv) => {
                banned_user
                    .dm(&ctx.http, CreateMessage::new().content(inv.url()))
                    .await?;
            }
            None => return Err(anyhow::anyhow!("Failed to create invite!")),
        }
        info!(
            "Successfully unbaned and sent invite to user {}",
            banned_user.id
        );
        Ok(())
    }
}

use std::time::Duration;

use anyhow::Result;
use chrono::{DateTime, Utc};
use mongodb::{bson::doc, options::FindOneAndUpdateOptions};
use serde::{Deserialize, Serialize};
use serenity::{all::CreateMessage, futures::StreamExt, model::prelude::Message, prelude::Context};

use crate::{
    database::MongoDatabaseHandle, event_handlers::mr_handler::MrHandler, unban::BanRecordUser,
};

const MATTID: u64 = 252114544485335051;
const SERVER: u64 = 173766075484340234;
const EMOJIID: &str = "<:banajmatijosa:621685158600245248>";
const ALLOWEDMEMBERS: &[u64] = &[
    // Jo
    132286945031094272,
    // Qubacc
    170561008786604034,
    // Fico
    245956125713760258,
    // Znidaric
    268420122090274816,
    // Fabac
    344472419085582347,
    // Sebek
    344121954124431360,
    // Domi
    302763402944839680,
    // Seba
    155013213811900416,
];
pub const MATT_BAN_COLLECTION: &str = "matt_ban";
pub const BAN_COOLDOWN_TIME: i64 = 3600;

#[derive(Serialize, Deserialize)]
pub(crate) struct MattBanCooldown {
    pub(crate) cooldown: i64,
    pub(crate) last_ban_timestamp: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct MattBan {
    banned_by: BanRecordUser,
    timestamp: DateTime<Utc>,
    success: bool,
}

impl MattBan {
    fn new(banned_by: u64, success: bool) -> Self {
        Self {
            banned_by: BanRecordUser(banned_by as i64),
            timestamp: Utc::now(),
            success,
        }
    }
}

impl MrHandler {
    pub(crate) async fn banaj_matijosa(&self, ctx: &Context, message: &Message) -> Result<()> {
        if message
            .guild_id
            .ok_or_else(|| anyhow::anyhow!("Message is missing guild id."))?
            == SERVER
            && ALLOWEDMEMBERS.contains(&message.author.id.get())
            && message.content.contains(EMOJIID)
        {
            let author_id = message.author.id.get();
            let handle = ctx
                .data
                .read()
                .await
                .get::<MongoDatabaseHandle>()
                .ok_or_else(|| anyhow::anyhow!("Failed to retrieve MongoDatabaseHandle from data"))?
                .clone();
            let time_now = Utc::now().timestamp();
            let last_ban = {
                let collection = handle.collection::<MattBanCooldown>(MATT_BAN_COLLECTION);
                let last_ban = collection.find_one(doc! {"_id": "COOLDOWN"}, None).await?;
                match last_ban {
                    Some(lb) => lb,
                    None => MattBanCooldown {
                        cooldown: 0,
                        last_ban_timestamp: 0,
                    },
                }
            };

            if time_now - last_ban.cooldown > last_ban.last_ban_timestamp {
                if let Some(guild) = ctx
                    .cache
                    .guilds()
                    .iter()
                    .find(|guild| guild.get() == SERVER)
                {
                    let mut members_stream = guild.members_iter(&ctx.http).boxed();
                    while let Some(member_result) = members_stream.next().await {
                        let member = member_result?;
                        if member.user.id.get() == MATTID {
                            message
                                .channel(&ctx.http)
                                .await?
                                .id()
                                .send_message(
                                    &ctx.http,
                                    CreateMessage::new().content("Ajde bok Matijoš!").tts(true),
                                )
                                .await?;
                            tokio::time::sleep(Duration::from_secs(4)).await;
                            member.ban(&ctx.http, 0).await?;
                            handle
                                .collection::<MattBan>(MATT_BAN_COLLECTION)
                                .insert_one(MattBan::new(author_id, true), None)
                                .await?;
                            handle
                                .collection::<MattBanCooldown>(MATT_BAN_COLLECTION)
                                .find_one_and_update(
                                    doc! {"_id": "COOLDOWN"},
                                    doc! {"$set": {"cooldown": BAN_COOLDOWN_TIME, "last_ban_timestamp": time_now}},
                                    Some(FindOneAndUpdateOptions::builder().upsert(true).build()),
                                )
                                .await?;
                            break;
                        }
                    }
                }
            } else {
                message
                    .channel(&ctx.http)
                    .await?
                    .id()
                    .send_message(
                        &ctx.http,
                        CreateMessage::new().content(&format!(
                            "Nečem ga još banati! ({} s)",
                            last_ban.cooldown - (time_now - last_ban.last_ban_timestamp)
                        )),
                    )
                    .await?;
                let collection = handle.collection::<MattBan>(MATT_BAN_COLLECTION);
                collection
                    .insert_one(MattBan::new(author_id, false), None)
                    .await?;
            }
        }
        Ok(())
    }
}

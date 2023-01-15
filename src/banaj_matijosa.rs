use std::time::Duration;

use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serenity::{futures::StreamExt, model::prelude::Message, prelude::Context};

use crate::{unban::BanRecordUser, util::retrieve_db_handle, Handler, MattBanCooldown};

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

#[derive(Clone, Debug, Serialize, Deserialize)]
struct MattBan {
    banned_by: BanRecordUser,
    timestamp: DateTime<Utc>,
    success: bool,
}

const MATT_BAN_COLLECTION: &str = "matt_ban";

impl MattBan {
    fn new(banned_by: u64, success: bool) -> Self {
        Self {
            banned_by: BanRecordUser(banned_by as i64),
            timestamp: Utc::now(),
            success,
        }
    }
}

impl Handler {
    pub(crate) async fn banaj_matijosa(&self, ctx: &Context, message: &Message) -> Result<()> {
        if message
            .guild_id
            .ok_or_else(|| anyhow::anyhow!("Message is missing guild id."))?
            == SERVER
            && ALLOWEDMEMBERS.contains(message.author.id.as_u64())
            && message.content.contains(EMOJIID)
        {
            let time_now = Utc::now().timestamp();
            let cooldown_data_lock = {
                ctx.data
                    .read()
                    .await
                    .get::<MattBanCooldown>()
                    .ok_or_else(|| anyhow::anyhow!("Failed to retrieve MattBanCooldown!"))?
                    .clone()
            };
            let (cooldown, last_ban_timestamp) = {
                let cooldown_data = cooldown_data_lock.read().await;
                (cooldown_data.cooldown, cooldown_data.last_ban_timestamp)
            };
            let handle = retrieve_db_handle(ctx.data.clone()).await?;
            let author_id = *message.author.id.as_u64();

            if time_now - cooldown > last_ban_timestamp {
                if let Some(guild) = ctx
                    .cache
                    .guilds()
                    .iter()
                    .find(|guild| *guild.as_u64() == SERVER)
                {
                    let mut members_stream = guild.members_iter(&ctx.http).boxed();
                    while let Some(member_result) = members_stream.next().await {
                        let member = member_result?;
                        if *member.user.id.as_u64() == MATTID {
                            message
                                .channel(&ctx.http)
                                .await?
                                .id()
                                .send_message(&ctx.http, |f| {
                                    f.content("Ajde bok Matijoš!").tts(true)
                                })
                                .await?;
                            tokio::time::sleep(Duration::from_secs(4)).await;
                            member.ban(&ctx.http, 0).await?;
                            {
                                let mut cooldown_data = cooldown_data_lock.write().await;
                                cooldown_data.last_ban_timestamp = time_now;
                            }
                            handle
                                .collection::<MattBan>(MATT_BAN_COLLECTION)
                                .insert_one(MattBan::new(author_id, true), None)
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
                    .send_message(&ctx.http, |f| {
                        f.content(&format!(
                            "Nečem ga još banati! ({} s)",
                            cooldown - (time_now - last_ban_timestamp)
                        ))
                    })
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

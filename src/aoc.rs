use std::{collections::HashMap, sync::Arc, time::Duration};

use chrono::{DateTime, Datelike, NaiveDate, NaiveDateTime, Utc};
use log::{error, info, warn};
use mongodb::{
    bson::{doc, to_bson},
    options::FindOneAndUpdateOptions,
    Database,
};
use serde::{Deserialize, Serialize};
use serenity::{
    async_trait,
    builder::CreateApplicationCommand,
    futures::StreamExt,
    model::prelude::{
        command::CommandOptionType,
        interaction::application_command::ApplicationCommandInteraction, ChannelType, GuildId,
    },
    prelude::Context,
};
use tokio::time::{interval, Interval};
use tokio_stream::wrappers::IntervalStream;

use crate::{
    util::{retrieve_db_handle, CommandRunner},
    CommandResponse, MongoDatabaseHandle, SlashCommands,
};
use anyhow::Result;

const PRIVATE_LEADERBOARDS_COLLECTION: &str = "private_leaderboards";

#[derive(Clone, Debug, Serialize, Deserialize)]
struct CompletionDayLevel {
    star_index: i64,
    get_star_ts: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct Member {
    id: i64,
    name: String,
    stars: i64,
    global_score: i64,
    local_score: i64,
    last_star_ts: i64,
    completion_day_level: HashMap<String, HashMap<String, CompletionDayLevel>>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PrivateLeaderboard {
    members: HashMap<String, Member>,
    owner_id: i64,
    event: String,
    #[serde(default = "i64::default")]
    last_update_timestamp: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct Session {
    cookie: Option<String>,
    added_timestamp: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PrivateLeaderboardDatabaseDoc {
    guild_id: GuildId,
    private_leaderboard_id: i64,
    // WARN: store in a safe manner, or at least safe-ish
    session_cookie: Session,
    // Key represents year
    leaderboard: HashMap<String, PrivateLeaderboard>,
}

// TODO: aoc leaderboard fetch
// 1. check db for last fetched timestamp
// 1a. if below set 15 minutes do nothing
// 2. try fetching leaderboard for guild with set cookie and leaderboard url
// 2a. if month is not december fetch only once a week ???
// 3. write results to db ???
pub async fn start_aoc_auto_fetch(db_handle: Arc<Database>) {
    let interval = interval(Duration::from_secs(15 * 60));
    let db_handle = db_handle.clone();
    IntervalStream::new(interval)
        .for_each_concurrent(None, |_| {
            println!("starting");
            let db_handle = db_handle.clone();
            async move {
                let collection = db_handle
                    .collection::<PrivateLeaderboardDatabaseDoc>(PRIVATE_LEADERBOARDS_COLLECTION);
                let mut leaderboards = match collection.find(None, None).await {
                    Ok(lb) => lb,
                    Err(e) => {
                        error!(
                            "Failed to fetch cursor for collection {}: {:#?}",
                            PRIVATE_LEADERBOARDS_COLLECTION, e
                        );
                        return;
                    }
                };
                let client = reqwest::Client::new();
                // WARN: year must be year - 1 unless it is december
                let year = Utc::now().year() - 1;
                while let Some(leaderboard_doc) = leaderboards.next().await {
                    let leaderboard_doc = match leaderboard_doc {
                        Ok(lb) => lb,
                        Err(e) => {
                            error!("Failed to fetch leaderboard: {:#?}", e);
                            continue;
                        }
                    };
                    let session_cookie = match leaderboard_doc.session_cookie.cookie.as_ref() {
                        Some(c) => c,
                        None => {
                            warn!(
                                "Session cookie for guild {} is not set",
                                leaderboard_doc.guild_id
                            );
                            continue;
                        }
                    };
                    let response = match client
                        .get(format!(
                            "https://adventofcode.com/{}/leaderboard/private/view/{}.json",
                            year, leaderboard_doc.private_leaderboard_id
                        ))
                        .header("Cookie", format!("session={}", session_cookie))
                        .send()
                        .await
                    {
                        Ok(r) => match r.json::<PrivateLeaderboard>().await {
                            Ok(r) => r,
                            Err(e) => {
                                error!("Error parsing fetched leaderboards json: {:#?}", e);
                                continue;
                            }
                        },
                        Err(e) => {
                            error!("Error fetching leaderboard: {:#?}", e);
                            continue;
                        }
                    };
                    let response = match to_bson(&response) {
                        Ok(r) => r,
                        Err(e) => {
                            error!("Failed to convert response to bson: {:#?}", e);
                            continue;
                        }
                    };
                    let lb_year = format!("leaderboard.{}", year);
                    if let Err(e) = collection
                        .find_one_and_update(
                            doc! {
                                "guild_id": leaderboard_doc.guild_id.0 as i64,
                                "leaderboard.year": year.to_string()
                            },
                            doc! {
                                "$set": { &lb_year: &response },
                            },
                            Some(FindOneAndUpdateOptions::builder().upsert(true).build()),
                        )
                        .await
                    {
                        error!("Failed to find and update leaderboard: {:#?}", e);
                    }
                }
            }
        })
        .await;
}

pub struct SpeedrunCommand;

#[async_trait]
impl CommandRunner for SpeedrunCommand {
    fn register(command: &mut CreateApplicationCommand) -> &mut CreateApplicationCommand {
        info!("Command registered: {}", SlashCommands::Speedrun.as_str());
        command
            .name(SlashCommands::Speedrun.as_str())
            .dm_permission(false)
            .create_option(|opt| {
                opt.required(false)
                    .name("day")
                    .kind(CommandOptionType::Number)
                    .description("Speedrun for selected day")
                    .channel_types(&[ChannelType::Text])
            })
            .description("AoC Speedrun")
    }

    async fn run(
        ctx: &Context,
        command: &ApplicationCommandInteraction,
    ) -> Result<CommandResponse> {
        todo!()
    }
}

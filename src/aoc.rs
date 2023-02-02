use std::{collections::HashMap, sync::Arc, time::Duration};

use chrono::{Datelike, Utc};
use log::{error, info, warn};
use mongodb::{
    bson::{doc, to_bson},
    options::FindOneAndUpdateOptions,
    Collection, Database,
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
    utils::MessageBuilder,
};
use tokio::time::interval;
use tokio_stream::wrappers::IntervalStream;

use crate::{
    util::{retrieve_db_handle, CommandRunner},
    CommandResponse, SlashCommands,
};
use anyhow::{anyhow, Result};

const PRIVATE_LEADERBOARDS_COLLECTION: &str = "private_leaderboards";

const PRIVATE_LEADERBOARD_ID_OPTION: &str = "leaderboard_id";
const DAY_OPTION: &str = "day";
const YEAR_OPTION: &str = "year";
const SESSION_COOKIE_OPTION: &str = "session_cookie";

const INTERVAL_TIME: i64 = 15 * 60;
const THIRTY_DAYS_TIME: i64 = 60 * 60 * 24 * 30;

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
    #[serde(default = "generate_current_timestamp")]
    last_update_timestamp: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct Session {
    cookie: Option<String>,
    added_timestamp: Option<i64>,
}

impl Session {
    fn new(cookie: Option<String>, mut added_timestamp: Option<i64>) -> Self {
        if added_timestamp.is_none() {
            added_timestamp = Some(Utc::now().timestamp());
        }
        Self {
            cookie,
            added_timestamp,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PrivateLeaderboardDatabaseDoc {
    guild_id: i64,
    private_leaderboard_id: i64,
    // WARN: store in a safe manner, or at least safe-ish
    session_cookie: Session,
    // Key represents year
    leaderboards: HashMap<String, PrivateLeaderboard>,
}

impl PrivateLeaderboardDatabaseDoc {
    fn new(guild_id: GuildId, private_leaderboard_id: i64, session_cookie: Session) -> Self {
        Self {
            guild_id: guild_id.0 as i64,
            private_leaderboard_id,
            session_cookie,
            leaderboards: HashMap::new(),
        }
    }
}

fn generate_current_timestamp() -> i64 {
    Utc::now().timestamp()
}

async fn fetch_leaderboard(
    year: &String,
    private_leaderboard_id: i64,
    session_cookie: &String,
    client: reqwest::Client,
) -> Result<PrivateLeaderboard> {
    Ok(client
        .get(format!(
            "https://adventofcode.com/{}/leaderboard/private/view/{}.json",
            year, private_leaderboard_id
        ))
        .header("Cookie", format!("session={}", session_cookie))
        .send()
        .await?
        .json()
        .await?)
}

async fn fetch_leaderboards(
    leaderboard_doc: PrivateLeaderboardDatabaseDoc,
    client: reqwest::Client,
    collection: &Collection<PrivateLeaderboardDatabaseDoc>,
) {
    let session_cookie = match leaderboard_doc.session_cookie.cookie.as_ref() {
        Some(c) => c,
        None => {
            warn!(
                "Session cookie for guild {} is not set. Skipping.",
                leaderboard_doc.guild_id
            );
            return;
        }
    };
    tokio_stream::iter(leaderboard_doc.leaderboards.iter())
        .for_each_concurrent(None, |(year, leaderboard)| {
            let client = client.clone();
            async move {
                if Utc::now().timestamp() - leaderboard.last_update_timestamp
                    <= INTERVAL_TIME as i64
                {
                    warn!("Tried to fetch too recently. Skipping.");
                    return;
                }
                let response = match fetch_leaderboard(
                    year,
                    leaderboard_doc.private_leaderboard_id,
                    session_cookie,
                    client,
                )
                .await
                {
                    Ok(resp) => resp,
                    Err(e) => {
                        error!("Error fetching leaderboard: {:#?}", e);
                        return;
                    }
                };
                let response = match to_bson(&response) {
                    Ok(r) => r,
                    Err(e) => {
                        error!("Failed to convert response to bson: {:#?}", e);
                        return;
                    }
                };
                if let Err(e) = collection
                    .find_one_and_update(
                        doc! {
                            "guild_id": leaderboard_doc.guild_id,
                            "private_leaderboard_id": leaderboard_doc.private_leaderboard_id,
                        },
                        doc! {
                            "$set": { format!("leaderboards.{}", year): response },
                        },
                        Some(FindOneAndUpdateOptions::builder().upsert(true).build()),
                    )
                    .await
                {
                    error!("Failed to find and update leaderboard: {:#?}", e);
                }
            }
        })
        .await;
}

pub async fn start_aoc_auto_fetch(db_handle: Arc<Database>) {
    let interval = interval(Duration::from_secs(INTERVAL_TIME as u64 + 5));
    let db_handle = db_handle.clone();
    IntervalStream::new(interval)
        .for_each(|_| {
            info!("Running AoC autofetch");
            let db_handle = db_handle.clone();
            async move {
                let collection = db_handle
                    .collection::<PrivateLeaderboardDatabaseDoc>(PRIVATE_LEADERBOARDS_COLLECTION);
                let leaderboards = match collection.find(None, None).await {
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
                leaderboards
                    .for_each_concurrent(None, |leaderboard_doc| {
                        let client = client.clone();
                        let collection = collection.clone();
                        async move {
                            let leaderboard_doc = match leaderboard_doc {
                                Ok(lb) => lb,
                                Err(e) => {
                                    error!("Failed to fetch leaderboard: {:#?}", e);
                                    return;
                                }
                            };
                            // Safe unwrap because we skip if is none
                            if leaderboard_doc.session_cookie.added_timestamp.is_none()
                                || Utc::now().timestamp()
                                    - leaderboard_doc.session_cookie.added_timestamp.unwrap()
                                    > THIRTY_DAYS_TIME
                            {
                                warn!("Skipped fetching leaderboard for guild {} and leaderboard {} cookie is possibly expired!", leaderboard_doc.guild_id, leaderboard_doc.private_leaderboard_id);
                                return;
                            }
                            fetch_leaderboards(leaderboard_doc, client, &collection).await;
                        }
                    })
                    .await;
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
                    .name(DAY_OPTION)
                    .kind(CommandOptionType::Number)
                    .description("Speedrun for selected day")
                    .channel_types(&[ChannelType::Text])
            })
            .create_option(|opt| {
                opt.required(false)
                    .name(YEAR_OPTION)
                    .kind(CommandOptionType::Number)
                    .description("Speedrun for selected year")
                    .channel_types(&[ChannelType::Text])
            })
            .create_option(|opt| {
                opt.required(false)
                    .name(PRIVATE_LEADERBOARD_ID_OPTION)
                    .kind(CommandOptionType::Number)
                    .description("Speedrun for selected leaderboard")
                    .channel_types(&[ChannelType::Text])
            })
            .description("AoC Speedrun")
    }

    // TODO: Handle options
    async fn run(
        ctx: &Context,
        command: &ApplicationCommandInteraction,
    ) -> Result<CommandResponse> {
        let guild_id = command
            .guild_id
            .ok_or_else(|| anyhow!("Command must be run in guild"))?;
        let db_handle = retrieve_db_handle(ctx.data.clone()).await?;
        let collection =
            db_handle.collection::<PrivateLeaderboardDatabaseDoc>(PRIVATE_LEADERBOARDS_COLLECTION);
        if let Some(leaderboard_doc) = collection
            .find_one(doc! {"guild_id": guild_id.0 as i64}, None)
            .await?
        {
            let now = Utc::now();
            let month = now.month();
            let year = if month == 12 {
                now.year()
            } else {
                now.year() - 1
            };
            let day = now.day();
            let mut results = leaderboard_doc
                .leaderboards
                .get(&year.to_string())
                .ok_or_else(|| anyhow!("No leaderboard for selected year"))?
                .members
                .values()
                .filter_map(|member| {
                    if let Some(day_result) = member.completion_day_level.get(&day.to_string()) {
                        if let (Some(first), Some(second)) =
                            (day_result.get("1"), day_result.get("2"))
                        {
                            return Some((&member.name, second.get_star_ts - first.get_star_ts));
                        }
                    }
                    None
                })
                .collect::<Vec<(&String, i64)>>();
            results.sort_by(|a, b| a.1.cmp(&b.1));
            let mut message_builder = MessageBuilder::new();
            results.iter().for_each(|result| {
                message_builder.push_line(format!(
                    "{}: {:#?}",
                    result.0,
                    Duration::from_secs(result.1 as u64)
                ));
            });
            Ok(Self::make_response(message_builder.build(), false, None))
        } else {
            Ok(Self::make_response(
                "There are not speedruns".to_string(),
                false,
                None,
            ))
        }
    }
}

pub struct AddPrivateLeaderboardCommand;

#[async_trait]
impl CommandRunner for AddPrivateLeaderboardCommand {
    fn register(command: &mut CreateApplicationCommand) -> &mut CreateApplicationCommand {
        info!(
            "Command registered: {}",
            SlashCommands::AddPrivateLeaderboard.as_str()
        );
        command
            .name(SlashCommands::AddPrivateLeaderboard.as_str())
            .dm_permission(false)
            .create_option(|opt| {
                opt.required(true)
                    .name(PRIVATE_LEADERBOARD_ID_OPTION)
                    .kind(CommandOptionType::Number)
                    .description("Private leaderboard ID")
                    .channel_types(&[ChannelType::Text])
            })
            .create_option(|opt| {
                opt.required(true)
                    .name(YEAR_OPTION)
                    .kind(CommandOptionType::String)
                    .description("Year")
                    .channel_types(&[ChannelType::Text])
            })
            .create_option(|opt| {
                opt.required(false)
                    .name(SESSION_COOKIE_OPTION)
                    .kind(CommandOptionType::String)
                    .description("Session cookie string")
                    .channel_types(&[ChannelType::Text])
            })
            .description("AoC add private leaderboard")
    }

    async fn run(
        ctx: &Context,
        command: &ApplicationCommandInteraction,
    ) -> Result<CommandResponse> {
        let guild_id = command
            .guild_id
            .ok_or_else(|| anyhow!("Command must be run in guild"))?;

        // WARN: Inefficient, but should be ran rarely
        let leaderboard_id = command
            .data
            .options
            .iter()
            .find(|opt| opt.name == PRIVATE_LEADERBOARD_ID_OPTION)
            .ok_or_else(|| anyhow!("Leaderboard ID is required"))?
            .value
            .clone()
            .ok_or_else(|| anyhow!("Leaderboard ID value is missing"))?
            .as_f64()
            .ok_or_else(|| anyhow!("Leaderboard ID is not a number"))?
            as i64;
        let mut year = command
            .data
            .options
            .iter()
            .find(|opt| opt.name == YEAR_OPTION)
            .ok_or_else(|| anyhow!("Year is required"))?
            .value
            .as_ref()
            .ok_or_else(|| anyhow!("Year value is missing"))?
            .to_string();
        year.remove(year.len() - 1);
        year.remove(0);
        let session_cookie = match command
            .data
            .options
            .iter()
            .find(|opt| opt.name == SESSION_COOKIE_OPTION)
        {
            Some(sc) => {
                let mut cookie = sc
                    .value
                    .clone()
                    .ok_or_else(|| anyhow!("Session cookie is missing"))?
                    .to_string();
                cookie.remove(cookie.len() - 1);
                cookie.remove(0);
                Some(cookie)
            }
            None => None,
        };
        let db_handle = retrieve_db_handle(ctx.data.clone()).await?;
        let collection =
            db_handle.collection::<PrivateLeaderboardDatabaseDoc>(PRIVATE_LEADERBOARDS_COLLECTION);
        if collection
            .find_one(
                doc! {
                    "guild_id": guild_id.0 as i64,
                    "private_leaderboard_id": leaderboard_id,
                },
                None,
            )
            .await?
            .is_none()
        {
            collection
                .insert_one(
                    PrivateLeaderboardDatabaseDoc::new(
                        guild_id,
                        leaderboard_id,
                        Session::new(session_cookie.clone(), None),
                    ),
                    None,
                )
                .await?;
        }
        // WARN: Can be spammed and bypass minimum recommended 15 minutes between requests
        if let Some(session_cookie) = session_cookie {
            let client = reqwest::Client::new();
            let response =
                fetch_leaderboard(&year, leaderboard_id, &session_cookie, client).await?;
            let response = to_bson(&response)?;
            collection
                .find_one_and_update(
                    doc! {
                        "guild_id": guild_id.0 as i64,
                        "private_leaderboard_id": leaderboard_id,
                    },
                    doc! {
                        "$set": { format!("leaderboards.{}", year): response },
                    },
                    Some(FindOneAndUpdateOptions::builder().upsert(true).build()),
                )
                .await?;
        }
        Ok(Self::make_response(
            "Leaderboard has been added".to_string(),
            false,
            None,
        ))
    }
}

pub struct SetSessionCookieCommand;

#[async_trait]
impl CommandRunner for SetSessionCookieCommand {
    fn register(command: &mut CreateApplicationCommand) -> &mut CreateApplicationCommand {
        info!(
            "Command registered: {}",
            SlashCommands::SetSessionCookie.as_str()
        );
        command
            .name(SlashCommands::SetSessionCookie.as_str())
            .dm_permission(false)
            .create_option(|opt| {
                opt.required(true)
                    .name(PRIVATE_LEADERBOARD_ID_OPTION)
                    .kind(CommandOptionType::Number)
                    .description("ID of a leaderboard to update the session cookie for")
                    .channel_types(&[ChannelType::Text])
            })
            .create_option(|opt| {
                opt.required(true)
                    .name(SESSION_COOKIE_OPTION)
                    .kind(CommandOptionType::String)
                    .description("Session cookie string")
                    .channel_types(&[ChannelType::Text])
            })
            .description("Adds a session cookie for fetching AoC private leaderboards")
    }

    async fn run(
        ctx: &Context,
        command: &ApplicationCommandInteraction,
    ) -> Result<CommandResponse> {
        let guild_id = command
            .guild_id
            .ok_or_else(|| anyhow!("Command must be run in guild"))?;
        let leaderboard_id = command
            .data
            .options
            .iter()
            .find(|opt| opt.name == PRIVATE_LEADERBOARD_ID_OPTION)
            .ok_or_else(|| anyhow!("Leaderboard ID is required"))?
            .value
            .clone()
            .ok_or_else(|| anyhow!("Leaderboard ID value is missing"))?
            .as_f64()
            .ok_or_else(|| anyhow!("Leaderboard ID is not a number"))?
            as i64;
        let mut session_cookie = command
            .data
            .options
            .iter()
            .find(|opt| opt.name == SESSION_COOKIE_OPTION)
            .ok_or_else(|| anyhow!("Session cookie is required"))?
            .value
            .as_ref()
            .ok_or_else(|| anyhow!("Session cookie value is missing"))?
            .to_string();
        session_cookie.remove(session_cookie.len() - 1);
        session_cookie.remove(0);

        let db_handle = retrieve_db_handle(ctx.data.clone()).await?;
        let collection =
            db_handle.collection::<PrivateLeaderboardDatabaseDoc>(PRIVATE_LEADERBOARDS_COLLECTION);
        if collection
            .find_one_and_update(
                doc! {
                    "guild_id": guild_id.0 as i64,
                    "private_leaderboard_id": leaderboard_id
                },
                doc! {
                    "$set": {
                        "session_cookie.cookie": session_cookie,
                        "session_cookie.added_timestamp": Utc::now().timestamp(),
                    }
                },
                None,
            )
            .await?
            .is_some()
        {
            Ok(Self::make_response(
                "Successfully set session".to_string(),
                false,
                None,
            ))
        } else {
            Ok(Self::make_response(
                "Leaderboard not found".to_string(),
                false,
                None,
            ))
        }
    }
}

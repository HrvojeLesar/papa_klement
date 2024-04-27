use anyhow::Result;
use log::info;
use mongodb::{bson::doc, Collection, Cursor};
use serde::{Deserialize, Serialize};
use serenity::{
    all::{CommandInteraction, CreateCommand},
    async_trait,
    prelude::Context,
    utils::MessageBuilder,
};

use crate::{
    util::{retrieve_db_handle, CommandRunner},
    CommandResponse, SlashCommands, UNDERSCOREBANS,
};

#[derive(Clone, Debug, Serialize, Deserialize)]
struct BanCountRecord {
    #[serde(rename = "_id")]
    user_id: i64,
    count: i64,
    display_name: String,
    nickname: Option<String>,
}

pub(crate) struct BanTopCommand;

impl BanTopCommand {
    async fn get_users(
        collection: &Collection<BanCountRecord>,
        field: &str,
        guild_id: &str,
    ) -> Result<Vec<BanCountRecord>> {
        let cursor = collection
            .aggregate(
                [
                    doc! {"$group": {"_id": field, "count": {"$count": {}}}},
                    doc! {"$sort": {"count": -1}},
                    doc! {"$limit": 5},
                    doc! {"$lookup": {
                    "from": guild_id,
                    "localField": "_id",
                    "foreignField": "_id",
                    "as": "user"}
                    },
                    doc! {"$unwind": "$user"},
                    doc! {"$project": {
                        "_id": 1,
                        "count": 1,
                        "display_name": "$user.display_name",
                        "nickname": "$user.nickname"
                    }},
                ],
                None,
            )
            .await?
            .with_type::<BanCountRecord>();
        Self::ban_count_cursor_to_vec(cursor).await
    }

    async fn ban_count_cursor_to_vec(
        mut cursor: Cursor<BanCountRecord>,
    ) -> Result<Vec<BanCountRecord>> {
        let mut collection = Vec::new();
        while cursor.advance().await? {
            collection.push(cursor.deserialize_current()?);
        }
        Ok(collection)
    }
}

#[async_trait]
impl CommandRunner for BanTopCommand {
    fn register(&self) -> CreateCommand {
        info!("Command registered: {}", SlashCommands::BanTop.as_str());
        CreateCommand::new(SlashCommands::BanTop.as_str()).description("Ban leaderboard")
    }

    async fn run(
        ctx: &Context,
        command: &ApplicationCommandInteraction,
    ) -> Result<CommandResponse> {
        info!("BanTop command called");
        let guild_id = command
            .guild_id
            .ok_or_else(|| anyhow::anyhow!("Command is not called from a guild!"))?;
        let db_handle = retrieve_db_handle(ctx.data.clone()).await?;
        let collection = db_handle
            .collection::<BanCountRecord>(&format!("{}{UNDERSCOREBANS}", guild_id.as_u64()));
        let guild_id_string = guild_id.as_u64().to_string();
        let most_bans_issued = Self::get_users(&collection, "$banned_by", &guild_id_string).await?;
        let most_banned_users =
            Self::get_users(&collection, "$banned_user", &guild_id_string).await?;
        let mut builder = MessageBuilder::new();
        builder.push_bold_line("Top Banned:");
        most_banned_users
            .iter()
            .enumerate()
            .for_each(|(idx, user)| {
                builder
                    .push_quote("")
                    .push_bold(format!("{}. ", idx + 1))
                    .push_line(format!(
                        "{}: {}",
                        user.nickname.as_ref().unwrap_or(&user.display_name),
                        user.count
                    ));
            });
        builder.push_bold_line("Top Bans:");
        most_bans_issued.iter().enumerate().for_each(|(idx, user)| {
            builder
                .push_quote("")
                .push_bold(format!("{}. ", idx + 1))
                .push_line(format!(
                    "{}: {}",
                    user.nickname.as_ref().unwrap_or(&user.display_name),
                    user.count
                ));
        });
        Ok(Self::make_response(builder.build(), false, None))
    }
}

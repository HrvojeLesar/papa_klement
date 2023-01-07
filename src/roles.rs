use anyhow::Result;

use log::info;
use mongodb::{bson::doc, Collection};
use serde::{Deserialize, Serialize};
use serenity::{
    futures::StreamExt,
    model::prelude::{Member, RoleId},
    prelude::Context,
};

use crate::{util::retrieve_db_handle, Handler};

#[derive(Clone, Debug, Serialize, Deserialize)]
struct SavedUser {
    #[serde(rename = "_id")]
    user_id: i64,
    display_name: String,
    nickname: Option<String>,
    roles: Vec<i64>,
}

impl SavedUser {
    fn new(user_id: i64, display_name: String, nickname: Option<String>, roles: Vec<i64>) -> Self {
        Self {
            user_id,
            display_name,
            nickname,
            roles,
        }
    }
}

impl Handler {
    async fn record_roles(
        &self,
        collection: &Collection<SavedUser>,
        user_id: i64,
        display_name: &String,
        nickname: &Option<String>,
        roles: &[i64],
    ) -> Result<()> {
        match collection.find_one(doc! {"_id": user_id}, None).await? {
            Some(saved_user) => {
                collection
                    .find_one_and_update(
                        doc! {"_id": user_id},
                        doc! {"$set": {"nickname": nickname, "roles": roles, "display_name": display_name}},
                        None,
                    )
                    .await?;
                info!("Saved user: {:#?}", saved_user);
            }
            None => {
                let new_user = SavedUser::new(
                    user_id,
                    display_name.clone(),
                    nickname.clone(),
                    roles.to_vec(),
                );
                collection.insert_one(&new_user, None).await?;
                info!("Saved new user: {:#?}", new_user);
            }
        }

        Ok(())
    }

    fn get_roles(&self, member: &Member, ctx: &Context) -> Option<Vec<i64>> {
        member.roles(&ctx.cache).map(|member_roles| {
            member_roles
                .iter()
                .map(|role| *role.id.as_u64() as i64)
                .collect::<Vec<i64>>()
        })
    }

    pub async fn save_roles_on_startup(&self, ctx: &Context) -> Result<()> {
        let database_handle = retrieve_db_handle(ctx.data.clone()).await?;
        for guild in ctx.cache.guilds() {
            info!("Saving members for guild: {}", guild.as_u64());
            let saved_users_collection =
                database_handle.collection::<SavedUser>(&guild.as_u64().to_string());
            let mut members_stream = guild.members_iter(&ctx.http).boxed();
            while let Some(member_result) = members_stream.next().await {
                let member = member_result?;
                let roles = self.get_roles(&member, ctx);
                if let Some(roles) = roles {
                    let id = *member.user.id.as_u64() as i64;
                    let member_nick = &member.nick;
                    let display_name = member.display_name();
                    self.record_roles(
                        &saved_users_collection,
                        id,
                        &display_name,
                        member_nick,
                        &roles,
                    )
                    .await?;
                }
            }
        }
        info!("Startup save done!");
        Ok(())
    }

    // TODO: check what changed
    pub async fn save_member_roles_on_update(&self, ctx: &Context, member: &Member) -> Result<()> {
        let database_handle = retrieve_db_handle(ctx.data.clone()).await?;
        let guild_id = *member.guild_id.as_u64() as i64;
        let saved_users_collection = database_handle.collection::<SavedUser>(&guild_id.to_string());
        let roles = self.get_roles(member, ctx);
        if let Some(roles) = roles {
            let user_id = *member.user.id.as_u64() as i64;
            let member_nick = &member.nick;
            let display_name = member.display_name();
            self.record_roles(
                &saved_users_collection,
                user_id,
                &display_name,
                member_nick,
                &roles,
            )
            .await?;
        }
        Ok(())
    }

    pub async fn grant_roles_and_nickname(&self, ctx: &Context, member: &mut Member) -> Result<()> {
        let database_handle = retrieve_db_handle(ctx.data.clone()).await?;
        let guild_id = *member.guild_id.as_u64() as i64;
        let saved_users_collection = database_handle.collection::<SavedUser>(&guild_id.to_string());
        let member_id = *member.user.id.as_u64() as i64;
        if let Some(saved_user) = saved_users_collection
            .find_one(doc! {"_id": member_id}, None)
            .await?
        {
            let roles = saved_user
                .roles
                .iter()
                .map(|id| RoleId(*id as u64))
                .collect::<Vec<RoleId>>();
            if !roles.is_empty() {
                member.add_roles(&ctx.http, &roles).await?;
                info!("Added roles to member: {}", member.user.id.as_u64());
            }
            if let Some(nickname) = saved_user.nickname {
                member.edit(&ctx.http, |m| m.nickname(nickname)).await?;
                info!("Updated nickname for member: {}", member.user.id.as_u64());
            }
        }
        Ok(())
    }
}

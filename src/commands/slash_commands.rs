use anyhow::Result;
use std::{pin::Pin, str::FromStr};

use crate::{
    aoc::{AddPrivateLeaderboardCommand, RollCommand, SetSessionCookieCommand, SpeedrunCommand},
    bantop::BanTopCommand,
    music::{PlayCommand, QueueCommand, SkipCommand, StopCommand},
    util::CommandRunner,
};

pub(crate) enum SlashCommands {
    BanTop,
    Play,
    Skip,
    Stop,
    Queue,
    Speedrun,
    AddPrivateLeaderboard,
    SetSessionCookie,
    Roll,
}

impl SlashCommands {
    pub(crate) const fn as_str(&self) -> &'static str {
        match self {
            Self::BanTop => "bantop",
            Self::Play => "play",
            Self::Skip => "skip",
            Self::Stop => "stop",
            Self::Queue => "queue",
            Self::Speedrun => "speedrun",
            Self::AddPrivateLeaderboard => "addprivateleaderboard",
            Self::SetSessionCookie => "setsessioncookie",
            Self::Roll => "roll",
        }
    }

    pub fn get_command(&self) -> Pin<Box<dyn CommandRunner>> {
        match self {
            Self::BanTop => Box::pin(BanTopCommand {}),
            Self::Play => Box::pin(PlayCommand {}),
            Self::Skip => Box::pin(SkipCommand {}),
            Self::Stop => Box::pin(StopCommand {}),
            Self::Queue => Box::pin(QueueCommand {}),
            Self::Speedrun => Box::pin(SpeedrunCommand {}),
            Self::AddPrivateLeaderboard => Box::pin(AddPrivateLeaderboardCommand {}),
            Self::SetSessionCookie => Box::pin(SetSessionCookieCommand {}),
            Self::Roll => Box::pin(RollCommand {}),
        }
    }
}

impl FromStr for SlashCommands {
    type Err = anyhow::Error;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        match input {
            "bantop" => Ok(Self::BanTop),
            "play" => Ok(Self::Play),
            "skip" => Ok(Self::Skip),
            "stop" => Ok(Self::Stop),
            "queue" => Ok(Self::Queue),
            "speedrun" => Ok(Self::Speedrun),
            "addprivateleaderboard" => Ok(Self::AddPrivateLeaderboard),
            "setsessioncookie" => Ok(Self::SetSessionCookie),
            "roll" => Ok(Self::Roll),
            _ => Err(anyhow::anyhow!("Failed to convert string to SlashCommand")),
        }
    }
}

use anyhow::Result;
use std::str::FromStr;

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

    pub fn get_command(&self) -> Box<dyn CommandRunner> {
        match self {
            Self::BanTop => Box::new(BanTopCommand {}),
            Self::Play => Box::new(PlayCommand {}),
            Self::Skip => Box::new(SkipCommand {}),
            Self::Stop => Box::new(StopCommand {}),
            Self::Queue => Box::new(QueueCommand {}),
            Self::Speedrun => Box::new(SpeedrunCommand {}),
            Self::AddPrivateLeaderboard => Box::new(AddPrivateLeaderboardCommand {}),
            Self::SetSessionCookie => Box::new(SetSessionCookieCommand {}),
            Self::Roll => Box::new(RollCommand {}),
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

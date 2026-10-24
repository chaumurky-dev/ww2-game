use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::game::{GameId, Power, UserRef};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChatChannel {
    All,
    Team,
    Power,
    Lobby,
}

impl ChatChannel {
    pub fn as_str(self) -> &'static str {
        match self {
            ChatChannel::All => "all",
            ChatChannel::Team => "team",
            ChatChannel::Power => "power",
            ChatChannel::Lobby => "lobby",
        }
    }
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "all" => ChatChannel::All,
            "team" => ChatChannel::Team,
            "power" => ChatChannel::Power,
            "lobby" => ChatChannel::Lobby,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ChatSender {
    User(UserRef),
    Bot { name: String },
    System,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChatMessage {
    pub id: Uuid,
    pub game_id: GameId,
    pub channel: ChatChannel,
    /// `Team` channel: `"axis"`/`"allies"`; `Power` channel: the power name; otherwise `None`.
    pub scope: Option<String>,
    pub sender: ChatSender,
    pub body: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SendChatRequest {
    pub channel: ChatChannel,
    pub body: String,
    /// Required for `power` channel in 2-player games where a seat controls several powers;
    /// also selects the team for `team` (defaults to the sender's own).
    #[serde(default)]
    pub power: Option<Power>,
}

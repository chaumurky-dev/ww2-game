use serde::{Deserialize, Serialize};

use crate::chat::ChatMessage;
use crate::game::{GameId, Power};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientMessage {
    Subscribe { game_id: GameId },
    Unsubscribe { game_id: GameId },
    Ping,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerMessage {
    GameUpdated {
        game_id: GameId,
        seq: i64,
    },
    ChatReceived {
        message: ChatMessage,
    },
    YourTurn {
        game_id: GameId,
        power: Option<Power>,
    },
    MatchFound {
        game_id: GameId,
    },
    Subscribed {
        game_id: GameId,
    },
    Pong,
    Error {
        code: String,
        message: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tagged_roundtrip() {
        let m = ClientMessage::Subscribe {
            game_id: GameId(uuid::Uuid::nil()),
        };
        let s = serde_json::to_string(&m).unwrap();
        assert!(s.contains("\"type\":\"subscribe\""));
        let _: ClientMessage = serde_json::from_str(&s).unwrap();
    }
}

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct GameId(pub Uuid);

impl std::fmt::Display for GameId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UserRef {
    pub id: Uuid,
    pub username: String,
    pub display_name: String,
    pub avatar: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Power {
    Ussr,
    Germany,
    Uk,
    Japan,
    Usa,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Team {
    Axis,
    Allies,
}

impl Power {
    pub const ALL: [Power; 5] = [
        Power::Ussr,
        Power::Germany,
        Power::Uk,
        Power::Japan,
        Power::Usa,
    ];

    pub fn team(self) -> Team {
        match self {
            Power::Germany | Power::Japan => Team::Axis,
            _ => Team::Allies,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Power::Ussr => "ussr",
            Power::Germany => "germany",
            Power::Uk => "uk",
            Power::Japan => "japan",
            Power::Usa => "usa",
        }
    }

    pub fn parse(s: &str) -> Option<Power> {
        Power::ALL.into_iter().find(|p| p.as_str() == s)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GameKind {
    Private,
    Public,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GameMode {
    /// Axis vs Allies; each human controls all powers on a side.
    TwoPlayer,
    /// One human (or bot) per power.
    FivePlayer,
}

impl GameMode {
    pub fn seat_count(self) -> usize {
        match self {
            GameMode::TwoPlayer => 2,
            GameMode::FivePlayer => 5,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GameSettings {
    #[serde(default = "default_timeout")]
    pub turn_timeout_secs: u32,
    #[serde(default = "default_true")]
    pub weapons_development: bool,
    #[serde(default)]
    pub economic_victory: bool,
    #[serde(default = "default_threshold")]
    pub economic_victory_threshold: u32,
}

fn default_timeout() -> u32 {
    172_800
}
fn default_true() -> bool {
    true
}
fn default_threshold() -> u32 {
    84
}

impl Default for GameSettings {
    fn default() -> Self {
        Self {
            turn_timeout_secs: default_timeout(),
            weapons_development: true,
            economic_victory: false,
            economic_victory_threshold: default_threshold(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GameStatus {
    Lobby,
    InProgress,
    Finished,
    Abandoned,
}

impl GameStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            GameStatus::Lobby => "lobby",
            GameStatus::InProgress => "in_progress",
            GameStatus::Finished => "finished",
            GameStatus::Abandoned => "abandoned",
        }
    }
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "lobby" => GameStatus::Lobby,
            "in_progress" => GameStatus::InProgress,
            "finished" => GameStatus::Finished,
            "abandoned" => GameStatus::Abandoned,
            _ => return None,
        })
    }
}

/// One seat of a game. In 2-player games a seat controls a whole team (several powers).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SeatAssignment {
    pub seat: u8,
    pub powers: Vec<Power>,
    pub user: Option<UserRef>,
    pub is_bot: bool,
    /// Persona id from the catalog (`GET /api/bots/personas`), bot seats only.
    pub bot_persona: Option<String>,
    /// Display name of the persona (falls back to the stored id for unknown/legacy values).
    #[serde(default)]
    pub bot_name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GameSummary {
    pub id: GameId,
    /// Human-friendly title, e.g. `Private game #A1B2` or `Ranked 2p #1234`.
    #[serde(default)]
    pub name: String,
    pub kind: GameKind,
    pub mode: GameMode,
    pub status: GameStatus,
    pub settings: GameSettings,
    pub seats: Vec<SeatAssignment>,
    pub round: i32,
    pub current_power: Option<Power>,
    pub turn_deadline: Option<DateTime<Utc>>,
    pub group_id: Option<Uuid>,
    pub host_id: Option<Uuid>,
    /// Invite code, visible to the host while the game is in the lobby.
    pub invite_code: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    /// Winning team once the game is finished (None for unfinished, abandoned or drawn games).
    #[serde(default)]
    pub winner: Option<Team>,
    /// Seat index of the caller, if seated.
    pub my_seat: Option<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreatePrivateGameRequest {
    pub mode: GameMode,
    /// Game title (1-60 chars). Defaults to `Private game #<short id>`.
    #[serde(default)]
    pub name: Option<String>,
    /// Persona ids per seat index (0-based); `null`/missing = default for that seat. Entries are
    /// only allowed for seats listed in `bot_seats`.
    #[serde(default)]
    pub bot_personas: Vec<Option<String>>,
    /// Number of human seats (including the host). Remaining seats are bots or must be
    /// listed in `bot_seats`. Defaults to `seat_count - bot_seats.len()`.
    #[serde(default)]
    pub human_seats: Option<u8>,
    /// Seat indexes (0-based) that are bots.
    #[serde(default)]
    pub bot_seats: Vec<u8>,
    #[serde(default)]
    pub settings: GameSettings,
    #[serde(default)]
    pub group_id: Option<Uuid>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JoinByInviteRequest {
    pub code: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatchmakingRequest {
    pub mode: GameMode,
}

/// Placeholder; replaced by the engine's view type later.
pub type GameStateView = serde_json::Value;

/// Placeholder; engine action/event types will replace the JSON values later.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GameActionEnvelope {
    pub seq: i64,
    pub actor: Option<u8>,
    pub action: serde_json::Value,
    pub events: serde_json::Value,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubmitActionRequest {
    pub action: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameStateResponse {
    pub seq: i64,
    pub state: GameStateView,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn teams() {
        assert_eq!(Power::Japan.team(), Team::Axis);
        assert_eq!(Power::Uk.team(), Team::Allies);
    }

    #[test]
    fn settings_defaults_from_empty_json() {
        let s: GameSettings = serde_json::from_str("{}").unwrap();
        assert_eq!(s, GameSettings::default());
        assert_eq!(s.turn_timeout_secs, 172_800);
        assert_eq!(s.economic_victory_threshold, 84);
    }

    #[test]
    fn create_request_new_fields_are_optional() {
        let r: CreatePrivateGameRequest = serde_json::from_str(r#"{"mode":"two_player"}"#).unwrap();
        assert!(r.name.is_none() && r.bot_personas.is_empty());
        let r: CreatePrivateGameRequest = serde_json::from_str(
            r#"{"mode":"five_player","name":"X","bot_personas":[null,"frost"]}"#,
        )
        .unwrap();
        assert_eq!(r.bot_personas, vec![None, Some("frost".to_string())]);
    }

    #[test]
    fn game_id_is_transparent() {
        let id = GameId(Uuid::nil());
        assert_eq!(
            serde_json::to_string(&id).unwrap(),
            "\"00000000-0000-0000-0000-000000000000\""
        );
    }
}

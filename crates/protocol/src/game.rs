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
    /// Any other private seat layout (1-5 seats, powers freely partitioned). Never rated.
    Custom,
}

impl GameMode {
    /// Seats of the fixed layouts; `0` for `Custom` (the layout is stored per game).
    pub fn seat_count(self) -> usize {
        match self {
            GameMode::TwoPlayer => 2,
            GameMode::FivePlayer => 5,
            GameMode::Custom => 0,
        }
    }

    /// Mode of a seat layout: `two_player` for exactly two seats split by team, `five_player`
    /// for five single-power seats, otherwise `custom`. Only the first two are ever rated.
    pub fn derive(layout: &[Vec<Power>]) -> GameMode {
        let one_team = |s: &Vec<Power>| s.iter().all(|p| p.team() == s[0].team());
        match layout.len() {
            2 if layout.iter().all(|s| !s.is_empty() && one_team(s))
                && layout[0][0].team() != layout[1][0].team() =>
            {
                GameMode::TwoPlayer
            }
            5 if layout.iter().all(|s| s.len() == 1) => GameMode::FivePlayer,
            _ => GameMode::Custom,
        }
    }
}

/// Who plays a seat of a private game.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SeatKind {
    /// The creator ("me"); exactly one per game.
    Host,
    /// A friend joins with the invite code.
    Open,
    /// A computer player.
    Bot,
}

/// One seat of a private game being created.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SeatSpec {
    /// Powers this seat controls (non-empty; the seats partition the five powers).
    pub powers: Vec<Power>,
    pub kind: SeatKind,
    /// Persona id (`GET /api/bots/personas`), bot seats only; default chosen by seat otherwise.
    #[serde(default)]
    pub persona: Option<String>,
}

/// Why a layout is not a valid partition of the five powers into 1-5 seats.
pub fn check_partition(layout: &[Vec<Power>]) -> Result<(), &'static str> {
    if layout.is_empty() || layout.len() > 5 {
        return Err("a game needs between 1 and 5 seats");
    }
    let mut seen = Vec::new();
    for powers in layout {
        if powers.is_empty() {
            return Err("every seat needs at least one power");
        }
        for p in powers {
            if seen.contains(p) {
                return Err("a power is assigned to more than one seat");
            }
            seen.push(*p);
        }
    }
    if seen.len() != 5 {
        return Err("every power must be assigned to a seat");
    }
    Ok(())
}

/// The classic suggested split for `players` seats (1-5), in seat (turn) order.
pub fn preset_layout(players: usize) -> Vec<Vec<Power>> {
    use Power::*;
    match players {
        1 => vec![vec![Ussr, Germany, Uk, Japan, Usa]],
        2 => vec![vec![Germany, Japan], vec![Ussr, Uk, Usa]],
        3 => vec![vec![Germany, Japan], vec![Ussr], vec![Uk, Usa]],
        4 => vec![vec![Germany], vec![Japan], vec![Ussr], vec![Uk, Usa]],
        _ => Power::ALL.iter().map(|p| vec![*p]).collect(),
    }
}

/// Per-game options. Mirrors `wartide_engine::GameSettings` (plus the turn timeout); every field
/// has a serde default so partial JSON (and rows stored by older versions) keep working.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GameSettings {
    #[serde(default = "default_timeout")]
    pub turn_timeout_secs: u32,
    /// Research phase enabled.
    #[serde(default = "default_true")]
    pub weapons_development: bool,
    /// Axis economic victory threshold (combined production); `None` disables it.
    #[serde(default = "default_axis_economic")]
    pub axis_economic_victory: Option<u32>,
    /// Optional Allied economic victory threshold.
    #[serde(default)]
    pub allied_economic_victory: Option<u32>,
    /// USSR may not attack on its first turn.
    #[serde(default)]
    pub soviet_first_turn_restriction: bool,
    /// No new industrial complexes may be bought.
    #[serde(default)]
    pub original_complexes_only: bool,
    /// Military victory also requires holding all of the winners' own capitals.
    #[serde(default)]
    pub total_victory: bool,
    /// Balance option: Germany starts with jet power.
    #[serde(default)]
    pub germany_jet_power: bool,
    /// Balance option: Japan starts with super submarines.
    #[serde(default)]
    pub japan_super_subs: bool,
}

fn default_mode() -> GameMode {
    GameMode::FivePlayer
}
fn default_timeout() -> u32 {
    172_800
}
fn default_true() -> bool {
    true
}
fn default_axis_economic() -> Option<u32> {
    Some(84)
}

impl Default for GameSettings {
    fn default() -> Self {
        Self {
            turn_timeout_secs: default_timeout(),
            weapons_development: true,
            axis_economic_victory: default_axis_economic(),
            allied_economic_victory: None,
            soviet_first_turn_restriction: false,
            original_complexes_only: false,
            total_victory: false,
            germany_jet_power: false,
            japan_super_subs: false,
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
    /// The human left (resigned, or timed out three times in a row); a bot plays the seat.
    #[serde(default)]
    pub resigned: bool,
    /// Turns this seat missed because of the timeout.
    #[serde(default)]
    pub timeouts: u32,
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
    /// Power whose turn it is.
    pub current_power: Option<Power>,
    /// Engine phase of the current turn (`develop_weapons`, `purchase`, `combat_move`, `combat`,
    /// `non_combat_move`, `mobilize`, ...).
    #[serde(default)]
    pub phase: Option<String>,
    /// Power that must act right now; differs from `current_power` while the defender chooses
    /// casualties or withdraws submarines.
    #[serde(default)]
    pub pending_power: Option<Power>,
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
    /// Legacy layout selector (`two_player`/`five_player`); ignored when `seats` is given.
    #[serde(default = "default_mode")]
    pub mode: GameMode,
    /// General seat model: 1-5 seats partitioning the five powers, exactly one `host`. When
    /// present it supersedes `mode`, `human_seats`, `bot_seats` and `bot_personas`. The game's
    /// mode is derived from the layout (see `GameMode::derive`).
    #[serde(default)]
    pub seats: Option<Vec<SeatSpec>>,
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

/// The engine's `GameState` as JSON (all information is public in this game, so there is no
/// per-viewer projection). See `crates/engine/README.md` for the shape.
pub type GameStateView = serde_json::Value;

/// One log entry. `action` is a `wartide_engine::Action` and `events` a `Vec<wartide_engine::Event>`
/// (both as JSON); `actor` is the seat that controlled the acting power.
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
        assert_eq!(s.axis_economic_victory, Some(84));
        assert_eq!(s.allied_economic_victory, None);
        assert!(s.weapons_development);
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
    fn mode_derivation_and_presets() {
        for n in 1..=5 {
            let l = preset_layout(n);
            assert_eq!(l.len(), n);
            assert!(check_partition(&l).is_ok());
        }
        assert_eq!(GameMode::derive(&preset_layout(2)), GameMode::TwoPlayer);
        assert_eq!(GameMode::derive(&preset_layout(3)), GameMode::Custom);
        assert_eq!(GameMode::derive(&preset_layout(5)), GameMode::FivePlayer);
        use Power::*;
        let mixed = vec![vec![Germany, Usa], vec![Ussr, Uk, Japan]];
        assert_eq!(GameMode::derive(&mixed), GameMode::Custom);
        assert!(check_partition(&[vec![Germany], vec![Germany]]).is_err());
        assert!(check_partition(&[vec![Germany], vec![]]).is_err());
        assert!(check_partition(&[vec![Germany, Japan]]).is_err());
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

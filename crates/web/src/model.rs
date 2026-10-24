//! UI-side domain helpers layered on top of `wartide-protocol` types (names, colours, phases,
//! unit kinds). The engine crate will eventually own unit/phase tables; until its state view lands
//! these small tables live here.
pub use wartide_protocol::game::{Power, Team};

pub trait PowerExt {
    fn id(self) -> &'static str;
    fn name(self) -> &'static str;
    /// CSS value (a custom property reference) holding the power colour.
    fn color(self) -> String;
}

impl PowerExt for Power {
    fn id(self) -> &'static str {
        self.as_str()
    }
    fn name(self) -> &'static str {
        match self {
            Power::Ussr => "Soviet Union",
            Power::Germany => "Germany",
            Power::Uk => "United Kingdom",
            Power::Japan => "Japan",
            Power::Usa => "United States",
        }
    }
    fn color(self) -> String {
        format!("var(--{})", self.as_str())
    }
}

pub fn team_name(t: Team) -> &'static str {
    match t {
        Team::Axis => "Axis",
        Team::Allies => "Allies",
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum UnitKind {
    Infantry,
    Armor,
    Fighter,
    Bomber,
    Battleship,
    Carrier,
    Transport,
    Submarine,
    AaGun,
    IndustrialComplex,
}

impl UnitKind {
    pub const ALL: [UnitKind; 10] = [
        UnitKind::Infantry,
        UnitKind::Armor,
        UnitKind::Fighter,
        UnitKind::Bomber,
        UnitKind::Battleship,
        UnitKind::Carrier,
        UnitKind::Transport,
        UnitKind::Submarine,
        UnitKind::AaGun,
        UnitKind::IndustrialComplex,
    ];
    /// Id used by the engine data files and the icon sprite (`unit-<id>`).
    pub fn id(self) -> &'static str {
        match self {
            UnitKind::Infantry => "infantry",
            UnitKind::Armor => "armor",
            UnitKind::Fighter => "fighter",
            UnitKind::Bomber => "bomber",
            UnitKind::Battleship => "battleship",
            UnitKind::Carrier => "carrier",
            UnitKind::Transport => "transport",
            UnitKind::Submarine => "submarine",
            UnitKind::AaGun => "aa_gun",
            UnitKind::IndustrialComplex => "industrial_complex",
        }
    }
    pub fn from_id(s: &str) -> Option<UnitKind> {
        UnitKind::ALL.into_iter().find(|k| k.id() == s)
    }
    pub fn name(self) -> &'static str {
        match self {
            UnitKind::Infantry => "Infantry",
            UnitKind::Armor => "Armor",
            UnitKind::Fighter => "Fighter",
            UnitKind::Bomber => "Bomber",
            UnitKind::Battleship => "Battleship",
            UnitKind::Carrier => "Carrier",
            UnitKind::Transport => "Transport",
            UnitKind::Submarine => "Submarine",
            UnitKind::AaGun => "AA Gun",
            UnitKind::IndustrialComplex => "Industrial Complex",
        }
    }
    /// Display-only costs; real values come from the engine's unit table.
    pub fn cost(self) -> u32 {
        match self {
            UnitKind::Infantry => 3,
            UnitKind::Armor => 5,
            UnitKind::Fighter => 12,
            UnitKind::Bomber => 15,
            UnitKind::Battleship => 24,
            UnitKind::Carrier => 18,
            UnitKind::Transport => 8,
            UnitKind::Submarine => 8,
            UnitKind::AaGun => 5,
            UnitKind::IndustrialComplex => 15,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    DevelopWeapons,
    Purchase,
    CombatMove,
    Combat,
    NonCombatMove,
    Mobilize,
    CollectIncome,
}

impl Phase {
    pub const ALL: [Phase; 7] = [
        Phase::DevelopWeapons,
        Phase::Purchase,
        Phase::CombatMove,
        Phase::Combat,
        Phase::NonCombatMove,
        Phase::Mobilize,
        Phase::CollectIncome,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Phase::DevelopWeapons => "Research",
            Phase::Purchase => "Purchase",
            Phase::CombatMove => "Combat Move",
            Phase::Combat => "Combat",
            Phase::NonCombatMove => "Non-Combat",
            Phase::Mobilize => "Place",
            Phase::CollectIncome => "Income",
        }
    }
    pub fn index(self) -> usize {
        Phase::ALL.iter().position(|p| *p == self).unwrap_or(0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Treasury {
    pub power: Power,
    pub ipc: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogEntry {
    pub round: u32,
    pub text: String,
}

// ---- helpers over protocol game types ----------------------------------------------------

use wartide_protocol::game::{GameMode, GameStatus, GameSummary, SeatAssignment};

/// Games have no name in the protocol yet (TODO(protocol)); derive a stable title from the id.
pub fn game_title(g: &GameSummary) -> String {
    let id = g.id.to_string();
    let kind = match g.kind {
        wartide_protocol::game::GameKind::Public => "Public",
        wartide_protocol::game::GameKind::Private => "Private",
    };
    format!("{kind} game {}", &id[..4])
}

pub fn mode_label(m: GameMode) -> &'static str {
    match m {
        GameMode::TwoPlayer => "2-player",
        GameMode::FivePlayer => "5-player",
    }
}

pub fn status_label(s: GameStatus) -> &'static str {
    match s {
        GameStatus::Lobby => "Waiting for players",
        GameStatus::InProgress => "In progress",
        GameStatus::Finished => "Finished",
        GameStatus::Abandoned => "Abandoned",
    }
}

pub fn my_seat(g: &GameSummary) -> Option<&SeatAssignment> {
    let i = g.my_seat?;
    g.seats.iter().find(|s| s.seat == i)
}

/// True when the current power belongs to the caller's seat.
pub fn my_turn(g: &GameSummary) -> bool {
    g.status == GameStatus::InProgress
        && match (my_seat(g), g.current_power) {
            (Some(s), Some(p)) => s.powers.contains(&p),
            _ => false,
        }
}

pub fn deadline_ms(g: &GameSummary) -> Option<f64> {
    g.turn_deadline.map(|d| d.timestamp_millis() as f64)
}

pub fn seat_label(s: &SeatAssignment) -> String {
    if s.is_bot {
        match &s.bot_persona {
            Some(p) => format!("bot ({p})"),
            None => "bot".into(),
        }
    } else if let Some(u) = &s.user {
        u.display_name.clone()
    } else {
        "open".into()
    }
}

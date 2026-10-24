//! Mock domain types (shape-compatible with what we expect from the protocol crate).
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Power {
    Ussr,
    Germany,
    Uk,
    Japan,
    Usa,
}

impl Power {
    pub const ALL: [Power; 5] = [
        Power::Ussr,
        Power::Germany,
        Power::Uk,
        Power::Japan,
        Power::Usa,
    ];
    pub fn id(self) -> &'static str {
        match self {
            Power::Ussr => "ussr",
            Power::Germany => "germany",
            Power::Uk => "uk",
            Power::Japan => "japan",
            Power::Usa => "usa",
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Power::Ussr => "Soviet Union",
            Power::Germany => "Germany",
            Power::Uk => "United Kingdom",
            Power::Japan => "Japan",
            Power::Usa => "United States",
        }
    }
    pub fn color(self) -> &'static str {
        match self {
            Power::Ussr => "#b5443c",
            Power::Germany => "#6e6e6e",
            Power::Uk => "#c2a266",
            Power::Japan => "#e08a2e",
            Power::Usa => "#6b8e4e",
        }
    }
    pub fn is_axis(self) -> bool {
        matches!(self, Power::Germany | Power::Japan)
    }
    pub fn from_id(s: &str) -> Option<Power> {
        Power::ALL.into_iter().find(|p| p.id() == s)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
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
    /// Asset id: `/assets/units/<id>.png`.
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
    /// Display-only mock costs; real values come from the engine's unit table.
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct User {
    pub username: String,
    pub display_name: String,
    pub email: String,
    pub bio: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GameSummary {
    pub id: String,
    pub name: String,
    pub mode: &'static str,
    pub round: u32,
    pub current_power: Power,
    pub my_powers: Vec<Power>,
    pub deadline_ms: f64,
    pub opponents: Vec<String>,
}

impl GameSummary {
    pub fn my_turn(&self) -> bool {
        self.my_powers.contains(&self.current_power)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum SeatKind {
    Me,
    Open,
    Bot(Persona),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Persona {
    Balanced,
    Aggressive,
    Cautious,
    Economist,
}

impl Persona {
    pub const ALL: [Persona; 4] = [
        Persona::Balanced,
        Persona::Aggressive,
        Persona::Cautious,
        Persona::Economist,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Persona::Balanced => "Balanced",
            Persona::Aggressive => "Aggressive",
            Persona::Cautious => "Cautious",
            Persona::Economist => "Economist",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct NewGame {
    pub name: String,
    pub seats: Vec<(Power, SeatKind)>,
    pub timeout_secs: u32,
    pub weapons_development: bool,
    pub economic_victory: bool,
    pub economic_threshold: u32,
    pub group: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Rating {
    pub ladder: &'static str,
    pub rating: u32,
    pub wins: u32,
    pub losses: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RecentGame {
    pub id: String,
    pub name: String,
    pub result: &'static str,
    pub power: Power,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Profile {
    pub user: User,
    pub ratings: Vec<Rating>,
    pub recent: Vec<RecentGame>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LeaderRow {
    pub rank: u32,
    pub username: String,
    pub rating: u32,
    pub wins: u32,
    pub losses: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Group {
    pub slug: String,
    pub name: String,
    pub description: String,
    pub members: Vec<String>,
    pub joined: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Channel {
    All,
    Team,
    Power,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ChatMsg {
    pub channel: Channel,
    pub author: String,
    pub power: Option<Power>,
    pub bot: bool,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Treasury {
    pub power: Power,
    pub ipc: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LogEntry {
    pub round: u32,
    pub text: String,
}

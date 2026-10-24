//! Unit types, their stats (from `data/units.json`) and the unit record.
use crate::map::LocationId;
use crate::power::Power;
use serde::{Deserialize, Serialize};
use std::sync::OnceLock;

const UNITS_JSON: &str = include_str!("../data/units.json");

/// Unique id of a unit within a game.
pub type UnitId = u32;

/// All ten piece types.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UnitType {
    /// Infantry.
    Infantry,
    /// Armor (tank).
    Armor,
    /// Fighter.
    Fighter,
    /// Bomber.
    Bomber,
    /// Battleship.
    Battleship,
    /// Aircraft carrier.
    Carrier,
    /// Transport.
    Transport,
    /// Submarine.
    Submarine,
    /// Antiaircraft gun.
    AaGun,
    /// Industrial complex.
    IndustrialComplex,
}

/// Where a unit operates.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Domain {
    /// Land.
    Land,
    /// Air.
    Air,
    /// Sea.
    Sea,
}

/// Static stats of a unit type.
#[derive(Clone, Debug)]
pub struct UnitStats {
    /// Purchase cost (IPC).
    pub cost: u32,
    /// Attack hit number.
    pub attack: u8,
    /// Defence hit number.
    pub defense: u8,
    /// Base movement.
    pub movement: u8,
    /// Domain.
    pub domain: Domain,
}

#[derive(Deserialize)]
struct RawUnit {
    id: String,
    cost: u32,
    attack: u8,
    defense: u8,
    #[serde(rename = "move")]
    mv: u8,
    domain: Domain,
}

static STATS: OnceLock<Vec<(String, UnitStats)>> = OnceLock::new();

fn table() -> &'static Vec<(String, UnitStats)> {
    STATS.get_or_init(|| {
        let raw: Vec<RawUnit> = serde_json::from_str(UNITS_JSON).expect("units.json valid");
        raw.into_iter()
            .map(|r| {
                (
                    r.id,
                    UnitStats {
                        cost: r.cost,
                        attack: r.attack,
                        defense: r.defense,
                        movement: r.mv,
                        domain: r.domain,
                    },
                )
            })
            .collect()
    })
}

impl UnitType {
    /// Every unit type.
    pub const ALL: [UnitType; 10] = [
        UnitType::Infantry,
        UnitType::Armor,
        UnitType::Fighter,
        UnitType::Bomber,
        UnitType::Battleship,
        UnitType::Carrier,
        UnitType::Transport,
        UnitType::Submarine,
        UnitType::AaGun,
        UnitType::IndustrialComplex,
    ];
    /// Snake-case id.
    pub fn id(self) -> &'static str {
        match self {
            UnitType::Infantry => "infantry",
            UnitType::Armor => "armor",
            UnitType::Fighter => "fighter",
            UnitType::Bomber => "bomber",
            UnitType::Battleship => "battleship",
            UnitType::Carrier => "carrier",
            UnitType::Transport => "transport",
            UnitType::Submarine => "submarine",
            UnitType::AaGun => "aa_gun",
            UnitType::IndustrialComplex => "industrial_complex",
        }
    }
    /// Parse a snake-case id.
    pub fn from_id(s: &str) -> Option<UnitType> {
        UnitType::ALL.into_iter().find(|u| u.id() == s)
    }
    /// Stats from `units.json`.
    pub fn stats(self) -> &'static UnitStats {
        &table()
            .iter()
            .find(|(id, _)| id == self.id())
            .expect("unit in table")
            .1
    }
    /// Domain shortcut.
    pub fn domain(self) -> Domain {
        self.stats().domain
    }
    /// Fighter or bomber.
    pub fn is_air(self) -> bool {
        self.domain() == Domain::Air
    }
    /// Any ship.
    pub fn is_ship(self) -> bool {
        self.domain() == Domain::Sea
    }
    /// Infantry, armor, AA gun, complex.
    pub fn is_land(self) -> bool {
        self.domain() == Domain::Land
    }
    /// Units that fight on land (infantry, armor, aircraft) and make a territory "occupied".
    pub fn is_fighting_land_unit(self) -> bool {
        matches!(
            self,
            UnitType::Infantry | UnitType::Armor | UnitType::Fighter | UnitType::Bomber
        )
    }
    /// Slots taken aboard a transport (None = cannot be carried).
    pub fn transport_slots(self) -> Option<u8> {
        match self {
            UnitType::Infantry => Some(1),
            UnitType::Armor | UnitType::AaGun => Some(2),
            _ => None,
        }
    }
}

fn is_false(b: &bool) -> bool {
    !*b
}
fn is_zero(n: &u8) -> bool {
    *n == 0
}

/// A piece on the board.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Unit {
    /// Unique id.
    pub id: UnitId,
    /// Type.
    pub kind: UnitType,
    /// Owning power.
    pub owner: Power,
    /// Current location. For cargo/aircraft on carriers this is the carrier's zone.
    pub location: LocationId,
    /// Movement points spent this turn of its owner.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub moves_used: u8,
    /// Transport / carrier carrying this unit.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub loaded_on: Option<UnitId>,
    /// Turn counter value when it was created (for placement restrictions).
    #[serde(default)]
    pub born_turn: u32,
    /// Moved during combat movement (cannot move again in non-combat, air excepted).
    #[serde(default, skip_serializing_if = "is_false")]
    pub moved_cm: bool,
    /// Cannot move for the rest of the turn.
    #[serde(default, skip_serializing_if = "is_false")]
    pub frozen: bool,
    /// Entered a battle space / neutral / captured as last step: may not move on.
    #[serde(default, skip_serializing_if = "is_false")]
    pub stopped: bool,
    /// Already took part in a battle this turn.
    #[serde(default, skip_serializing_if = "is_false")]
    pub fought: bool,
    /// Declared in a strategic bombing raid.
    #[serde(default, skip_serializing_if = "is_false")]
    pub bombing: bool,
    /// Where it came from when it entered a battle space (retreat target).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub combat_from: Option<LocationId>,
    /// Cargo awaiting an amphibious landing at this territory.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub assault_to: Option<LocationId>,
    /// Transport: territory it unloaded into this turn.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unloaded_to: Option<LocationId>,
    /// Turn counter value when this unit was loaded onto its current transport.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub loaded_turn: Option<u32>,
}

impl Unit {
    /// Fresh unit with all flags cleared.
    pub fn new(
        id: UnitId,
        kind: UnitType,
        owner: Power,
        location: LocationId,
        born_turn: u32,
    ) -> Unit {
        Unit {
            id,
            kind,
            owner,
            location,
            moves_used: 0,
            loaded_on: None,
            born_turn,
            moved_cm: false,
            frozen: false,
            stopped: false,
            fought: false,
            bombing: false,
            combat_from: None,
            assault_to: None,
            unloaded_to: None,
            loaded_turn: None,
        }
    }
    /// Clear all per-turn flags (called at the start of the owner's turn).
    pub fn reset_turn(&mut self) {
        self.moves_used = 0;
        self.moved_cm = false;
        self.frozen = false;
        self.stopped = false;
        self.fought = false;
        self.bombing = false;
        self.combat_from = None;
        self.assault_to = None;
        self.unloaded_to = None;
        self.loaded_turn = None;
    }
}

/// Short unit description used in events.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnitRef {
    /// Id.
    pub id: UnitId,
    /// Type.
    pub kind: UnitType,
    /// Owner.
    pub owner: Power,
}

impl From<&Unit> for UnitRef {
    fn from(u: &Unit) -> UnitRef {
        UnitRef {
            id: u.id,
            kind: u.kind,
            owner: u.owner,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn stats_load() {
        assert_eq!(UnitType::Armor.stats().cost, 5);
        assert_eq!(UnitType::Bomber.stats().movement, 6);
        assert_eq!(UnitType::Submarine.stats().attack, 2);
        assert_eq!(UnitType::Fighter.domain(), Domain::Air);
        assert_eq!(UnitType::from_id("aa_gun"), Some(UnitType::AaGun));
        assert_eq!(UnitType::Armor.transport_slots(), Some(2));
    }
}

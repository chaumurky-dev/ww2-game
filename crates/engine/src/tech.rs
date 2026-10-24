//! Weapons development: technologies and their numeric effects.
use crate::power::Power;
use crate::state::GameState;
use crate::units::{Unit, UnitType};
use serde::{Deserialize, Serialize};

/// The six technologies (die roll 1..=6 in this order).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Tech {
    /// Fighters defend at 5.
    JetPower,
    /// AA guns may launch rockets.
    Rockets,
    /// Submarines attack at 3.
    SuperSubmarines,
    /// Fighters move 6, bombers 8.
    LongRangeAircraft,
    /// Purchases cost 1 less.
    IndustrialTechnology,
    /// Bombers roll 3 dice.
    HeavyBombers,
}

impl Tech {
    /// All techs in die order.
    pub const ALL: [Tech; 6] = [
        Tech::JetPower,
        Tech::Rockets,
        Tech::SuperSubmarines,
        Tech::LongRangeAircraft,
        Tech::IndustrialTechnology,
        Tech::HeavyBombers,
    ];
    /// Tech for a die roll 1..=6.
    pub fn from_roll(r: u8) -> Tech {
        Tech::ALL[(r.clamp(1, 6) - 1) as usize]
    }
}

/// Price of one research die.
pub const RESEARCH_DIE_COST: u32 = 5;

impl GameState {
    /// Does `p` own `t`?
    pub fn has_tech(&self, p: Power, t: Tech) -> bool {
        self.techs[p].contains(&t)
    }
}

/// Purchase cost of a unit type for a power (industrial technology applies).
pub fn unit_cost(state: &GameState, p: Power, kind: UnitType) -> u32 {
    let base = kind.stats().cost;
    if state.has_tech(p, Tech::IndustrialTechnology) {
        base - 1
    } else {
        base
    }
}

/// Movement allowance of a unit including technology.
pub fn move_allowance(state: &GameState, u: &Unit) -> u8 {
    let base = u.kind.stats().movement;
    if state.has_tech(u.owner, Tech::LongRangeAircraft) {
        match u.kind {
            UnitType::Fighter => return base + 2,
            UnitType::Bomber => return base + 2,
            _ => {}
        }
    }
    base
}

/// Hit number when attacking (0 = cannot attack).
pub fn attack_value(state: &GameState, u: &Unit) -> u8 {
    if u.kind == UnitType::Submarine && state.has_tech(u.owner, Tech::SuperSubmarines) {
        return 3;
    }
    u.kind.stats().attack
}

/// Hit number when defending (0 = cannot defend).
pub fn defense_value(state: &GameState, u: &Unit) -> u8 {
    if u.kind == UnitType::Fighter && state.has_tech(u.owner, Tech::JetPower) {
        return 5;
    }
    u.kind.stats().defense
}

/// Dice rolled when attacking (heavy bombers roll 3).
pub fn attack_dice(state: &GameState, u: &Unit) -> u32 {
    if attack_value(state, u) == 0 {
        return 0;
    }
    if u.kind == UnitType::Bomber && state.has_tech(u.owner, Tech::HeavyBombers) {
        3
    } else {
        1
    }
}

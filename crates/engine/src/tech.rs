//! Weapons development: technologies and their numeric effects.
use crate::event::plain_die;
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

use crate::dice::{d6, Rng};
use crate::error::{illegal, Result, RuleError};
use crate::event::{DiceContext, Event};
use crate::state::Phase;

/// Pay for `dice` research dice and roll them (phase DevelopWeapons, once per turn).
pub fn research(
    state: &mut GameState,
    p: Power,
    dice: u32,
    rng: &mut Rng,
    ev: &mut Vec<Event>,
) -> Result<()> {
    if state.phase != Phase::DevelopWeapons || !state.settings.weapons_development {
        return illegal("weapons development is not available now");
    }
    if state.turn.researched {
        return illegal("research dice were already bought this turn");
    }
    let cost = dice.saturating_mul(RESEARCH_DIE_COST);
    if cost > state.treasury[p] {
        return Err(RuleError::InsufficientFunds {
            need: cost,
            have: state.treasury[p],
        });
    }
    state.treasury[p] -= cost;
    state.turn.researched = true;
    ev.push(Event::ResearchBought {
        power: p,
        dice,
        cost,
    });
    let rolls: Vec<u8> = (0..dice).map(|_| d6(rng)).collect();
    ev.push(Event::DiceRolled {
        power: p,
        context: DiceContext::Research,
        rolls: rolls.clone(),
        details: rolls
            .iter()
            .map(|&r| {
                let mut d = plain_die(p, 6, r);
                d.hit = r == 6;
                d
            })
            .collect(),
    });
    for _ in rolls.iter().filter(|&&r| r == 6) {
        if state.techs[p].len() >= Tech::ALL.len() {
            break;
        }
        let mut tried = vec![];
        loop {
            let r = d6(rng);
            tried.push(r);
            let t = Tech::from_roll(r);
            if !state.techs[p].contains(&t) {
                ev.push(Event::DiceRolled {
                    power: p,
                    context: DiceContext::ResearchTech,
                    details: tried.iter().map(|&r| plain_die(p, 0, r)).collect(),
                    rolls: tried,
                });
                state.techs[p].push(t);
                ev.push(Event::TechGained { power: p, tech: t });
                break;
            }
        }
    }
    Ok(())
}

//! Monte-Carlo battle odds using the real combat code.
use crate::combat::auto_resolve;
use crate::dice::rng_from_seed;
use crate::event::BattleOutcome;
use crate::map::LocationId;
use crate::state::{BattleKind, GameState, Phase};
use serde::{Deserialize, Serialize};

/// Summary of a simulation.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BattleOdds {
    /// Simulated battles.
    pub trials: u32,
    /// Fraction won by the attacker.
    pub attacker_win: f64,
    /// Fraction won by the defender (including mutual destruction).
    pub defender_win: f64,
    /// Fraction ending without a winner (stalemate).
    pub stalemate: f64,
    /// Average surviving attacking units in attacker wins.
    pub avg_attackers_left: f64,
    /// Average surviving defending units in defender wins.
    pub avg_defenders_left: f64,
}

/// Simulate the battle at `loc` as if it began now (Combat phase, battle not yet started),
/// always pressing the attack to the end and choosing casualties cheapest-first.
pub fn battle_odds(
    state: &GameState,
    loc: LocationId,
    trials: u32,
    seed: u64,
) -> Option<BattleOdds> {
    let mut base = state.clone();
    base.phase = Phase::Combat;
    base.battle = None;
    let mut ev = vec![];
    crate::battle::begin_battle(&mut base, loc, &mut ev).ok()?;
    simulate(&base, loc, trials, seed)
}

/// Simulate the remainder of the battle currently in progress (`state.battle`), starting from its
/// exact state (pending casualties are resolved cheapest-first, then the attacker presses every round).
/// `None` when no battle is in progress.
pub fn current_battle_odds(state: &GameState, trials: u32, seed: u64) -> Option<BattleOdds> {
    let b = state.battle.as_ref()?;
    simulate(state, b.location, trials, seed)
}

fn simulate(base: &GameState, loc: LocationId, trials: u32, seed: u64) -> Option<BattleOdds> {
    let attacker = base.battle.as_ref().map(|b| b.attacker);
    let (mut aw, mut dw, mut st) = (0u32, 0u32, 0u32);
    let (mut al, mut dl) = (0u64, 0u64);
    let trials = trials.max(1);
    for i in 0..trials {
        let mut s = base.clone();
        let mut rng = rng_from_seed(seed.wrapping_add(u64::from(i)));
        let mut ev = vec![];
        crate::combat::advance(&mut s, &mut rng, &mut ev);
        auto_resolve(&mut s, &mut rng, &mut ev);
        let outcome = ev.iter().rev().find_map(|e| match e {
            crate::event::Event::BattleEnded { outcome, .. } => Some(*outcome),
            _ => None,
        });
        let left = |pred: &dyn Fn(&crate::units::Unit) -> bool| {
            s.units_at(loc)
                .filter(|u| {
                    pred(u)
                        && u.loaded_on.is_none()
                        && u.kind != crate::units::UnitType::AaGun
                        && u.kind != crate::units::UnitType::IndustrialComplex
                })
                .count() as u64
        };
        let is_att = |u: &crate::units::Unit| Some(u.owner) == attacker;
        match outcome {
            Some(BattleOutcome::AttackerWins) => {
                aw += 1;
                al += left(&is_att);
            }
            Some(BattleOutcome::Stalemate) => st += 1,
            _ => {
                dw += 1;
                dl += left(&|u| !u.owner.is_ally(attacker.unwrap()));
            }
        }
    }
    let _ = BattleKind::Land;
    let t = f64::from(trials);
    Some(BattleOdds {
        trials,
        attacker_win: f64::from(aw) / t,
        defender_win: f64::from(dw) / t,
        stalemate: f64::from(st) / t,
        avg_attackers_left: if aw > 0 {
            al as f64 / f64::from(aw)
        } else {
            0.0
        },
        avg_defenders_left: if dw > 0 {
            dl as f64 / f64::from(dw)
        } else {
            0.0
        },
    })
}

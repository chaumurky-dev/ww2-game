//! Read-only helpers for UIs and bots: reachable destinations, placements, affordability.
use crate::battle::{pending_battles, retreat_options};
use crate::economy::{purchase_cost, validate_place};
use crate::map::{LocationId, MapData};
use crate::movement::{check_unit_ready, remaining_moves, unit_step, validate_move};
use crate::power::Power;
use crate::state::{GameState, Phase};
use crate::tech::unit_cost;
use crate::transport::{validate_load, validate_unload};
use crate::units::{UnitId, UnitType};
use serde::{Deserialize, Serialize};

pub use crate::bombing::{bombing_targets, rocket_targets};
pub use crate::decision::{pending_decision, Decision};
pub use crate::economy::{income, production_level};
pub use crate::odds::{battle_odds, BattleOdds};

/// A reachable destination with the path to take.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Reach {
    /// Destination.
    pub to: LocationId,
    /// Locations entered, excluding the start.
    pub path: Vec<LocationId>,
    /// Ending here means a battle (combat move).
    pub battle: bool,
    /// For aircraft: a legal landing spot right now.
    pub landing: bool,
}

/// All destinations a single unit may legally move to with a `Move` action in the current phase.
pub fn reachable(state: &GameState, unit: UnitId) -> Vec<Reach> {
    let Some(u) = state.unit(unit) else {
        return vec![];
    };
    let p = u.owner;
    if !matches!(state.phase, Phase::CombatMove | Phase::NonCombatMove)
        || p != state.current_power
        || state.battle.is_some()
    {
        return vec![];
    }
    if check_unit_ready(state, u, p).is_err() {
        return vec![];
    }
    let map = MapData::get();
    let rem = remaining_moves(state, u) as usize;
    let mut best: Vec<Option<Vec<LocationId>>> = vec![None; map.len()];
    best[u.location.idx()] = Some(vec![]);
    let mut frontier = vec![u.location];
    let mut out = vec![];
    for depth in 0..rem {
        let mut next = vec![];
        for cur in frontier {
            let cur_path = best[cur.idx()].clone().unwrap();
            for &n in map.neighbours(cur) {
                if best[n.idx()].is_some() {
                    continue;
                }
                let first = u.moves_used as usize + depth == 0;
                let Ok(info) = unit_step(state, u, state.phase, first, cur, n) else {
                    continue;
                };
                let mut path = cur_path.clone();
                path.push(n);
                best[n.idx()] = Some(path.clone());
                if !info.terminal {
                    next.push(n);
                }
                if validate_move(state, p, &[unit], &path).is_ok() {
                    out.push(Reach {
                        to: n,
                        path,
                        battle: info.battle,
                        landing: u.kind.is_air() && crate::movement::is_landing_spot(state, u, n),
                    });
                }
            }
        }
        frontier = next;
    }
    out
}

/// Transports adjacent to a land unit that it could board right now.
pub fn load_options(state: &GameState, unit: UnitId) -> Vec<UnitId> {
    let Some(u) = state.unit(unit) else {
        return vec![];
    };
    state
        .units
        .iter()
        .filter(|t| {
            t.kind == UnitType::Transport && validate_load(state, u.owner, &[unit], t.id).is_ok()
        })
        .map(|t| t.id)
        .collect()
}

/// Territories where `transport`'s cargo (of the current power) could be unloaded.
pub fn unload_options(state: &GameState, transport: UnitId) -> Vec<LocationId> {
    let Some(t) = state.unit(transport) else {
        return vec![];
    };
    let p = state.current_power;
    let cargo: Vec<UnitId> = state
        .cargo_of(transport)
        .filter(|c| c.owner == p)
        .map(|c| c.id)
        .collect();
    if cargo.is_empty() {
        return vec![];
    }
    MapData::get()
        .neighbours(t.location)
        .iter()
        .copied()
        .filter(|&l| validate_unload(state, p, transport, &cargo, l).is_ok())
        .collect()
}

/// Places where `count` units of `kind` (from the purchase list) can be placed now.
pub fn legal_placements(state: &GameState, kind: UnitType) -> Vec<LocationId> {
    let p = state.current_power;
    MapData::get()
        .locations()
        .filter(|&l| validate_place(state, p, kind, l, 1, None).is_ok())
        .collect()
}

/// Something that can be bought.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PurchaseOption {
    /// Unit.
    pub unit: UnitType,
    /// Price for the power (with industrial technology).
    pub cost: u32,
    /// How many more can be afforded given the current list.
    pub max_more: u32,
}

/// Purchase menu with affordability for the current power.
pub fn purchase_options(state: &GameState) -> Vec<PurchaseOption> {
    let p = state.current_power;
    let spent = purchase_cost(state, p, &state.purchases);
    let have = state.treasury[p] + spent;
    let left = have.saturating_sub(spent);
    UnitType::ALL
        .into_iter()
        .map(|unit| {
            let cost = unit_cost(state, p, unit);
            let mut max_more = left / cost;
            if unit == UnitType::IndustrialComplex {
                let bought = state
                    .purchases
                    .iter()
                    .find(|i| i.unit == unit)
                    .map_or(0, |i| i.count);
                let cap = if state.settings.original_complexes_only {
                    0
                } else {
                    crate::economy::NEW_COMPLEX_CAP
                        .saturating_sub(state.new_complexes_placed + bought)
                };
                max_more = max_more.min(cap);
            }
            PurchaseOption {
                unit,
                cost,
                max_more,
            }
        })
        .collect()
}

/// Units of the current power that could still move.
pub fn movable_units(state: &GameState, p: Power) -> Vec<UnitId> {
    state
        .units
        .iter()
        .filter(|u| u.owner == p && !reachable(state, u.id).is_empty())
        .map(|u| u.id)
        .collect()
}

/// Battles still to fight.
pub fn battles_pending(state: &GameState) -> Vec<LocationId> {
    pending_battles(state)
}

/// Retreat destinations in the current battle.
pub fn retreat_destinations(state: &GameState) -> Vec<LocationId> {
    retreat_options(state)
}

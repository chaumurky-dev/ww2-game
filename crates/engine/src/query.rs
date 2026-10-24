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
pub use crate::odds::{battle_odds, current_battle_odds, BattleOdds};

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

/// Units of `p` with moves left and at least one reachable destination in the current phase.
/// Computed in one pass (cheap pre-filters before the path search); empty unless `p` is the
/// active power in a movement phase.
pub fn movable_units(state: &GameState, p: Power) -> Vec<UnitId> {
    if !matches!(state.phase, Phase::CombatMove | Phase::NonCombatMove)
        || p != state.current_power
        || state.battle.is_some()
    {
        return vec![];
    }
    state
        .units
        .iter()
        .filter(|u| {
            u.owner == p
                && check_unit_ready(state, u, p).is_ok()
                && remaining_moves(state, u) > 0
                && !reachable(state, u.id).is_empty()
        })
        .map(|u| u.id)
        .collect()
}

/// Destinations (with one path) legal for the whole group moving together in the current phase.
/// All units must be the current power's, start in the same place and share a domain (all land,
/// all air or all sea); a mixed group yields nothing. The group moves at its slowest unit's speed.
pub fn reachable_group(state: &GameState, units: &[UnitId]) -> Vec<Reach> {
    let Some(first) = units.first().and_then(|&i| state.unit(i)) else {
        return vec![];
    };
    let p = first.owner;
    if !matches!(state.phase, Phase::CombatMove | Phase::NonCombatMove)
        || p != state.current_power
        || state.battle.is_some()
    {
        return vec![];
    }
    let mut group = vec![];
    for &id in units {
        let Some(u) = state.unit(id) else {
            return vec![];
        };
        if u.owner != p
            || u.location != first.location
            || u.kind.domain() != first.kind.domain()
            || group.iter().any(|g: &&crate::units::Unit| g.id == id)
            || check_unit_ready(state, u, p).is_err()
        {
            return vec![];
        }
        group.push(u);
    }
    let map = MapData::get();
    let rem = group
        .iter()
        .map(|u| remaining_moves(state, u))
        .min()
        .unwrap_or(0) as usize;
    let mut seen = vec![false; map.len()];
    seen[first.location.idx()] = true;
    let mut paths: Vec<Option<Vec<LocationId>>> = vec![None; map.len()];
    paths[first.location.idx()] = Some(vec![]);
    let mut frontier = vec![first.location];
    let mut out = vec![];
    for depth in 0..rem {
        let mut next = vec![];
        for cur in frontier {
            let cur_path = paths[cur.idx()].clone().unwrap_or_default();
            for &n in map.neighbours(cur) {
                if seen[n.idx()] {
                    continue;
                }
                let mut terminal = false;
                let mut battle = false;
                let mut ok = true;
                for u in &group {
                    let f = u.moves_used as usize + depth == 0;
                    match unit_step(state, u, state.phase, f, cur, n) {
                        Ok(i) => {
                            terminal |= i.terminal;
                            battle |= i.battle;
                        }
                        Err(_) => {
                            ok = false;
                            break;
                        }
                    }
                }
                if !ok {
                    continue;
                }
                seen[n.idx()] = true;
                let mut path = cur_path.clone();
                path.push(n);
                paths[n.idx()] = Some(path.clone());
                if !terminal {
                    next.push(n);
                }
                if validate_move(state, p, units, &path).is_ok() {
                    out.push(Reach {
                        to: n,
                        path,
                        battle,
                        landing: group.iter().all(|u| {
                            u.kind.is_air() && crate::movement::is_landing_spot(state, u, n)
                        }),
                    });
                }
            }
        }
        frontier = next;
    }
    out
}

/// A legal place to put units of one type in the Mobilize phase.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Placement {
    /// Where the units appear (territory, or sea zone for ships).
    pub location: LocationId,
    /// Complex territory whose allowance is used (the `from` of a `Place` action for ships).
    /// None for new industrial complexes, which use no complex allowance.
    pub from_complex: Option<LocationId>,
    /// Units that complex can still produce this turn, before this placement
    /// (`u32::MAX` = unlimited, `NEW_COMPLEX_CAP` minus those placed for a new complex).
    pub capacity_left: u32,
}

/// Every legal way to place `count` units of `kind` now (must be in the purchase list). Honours
/// per-complex capacity, one AA gun per territory, the four-new-complex cap and, for ships,
/// one entry per supplying complex adjacent to the sea zone.
pub fn legal_placements_for(state: &GameState, kind: UnitType, count: u32) -> Vec<Placement> {
    let p = state.current_power;
    let map = MapData::get();
    let mut out = vec![];
    for l in map.locations() {
        match kind {
            UnitType::IndustrialComplex => {
                if validate_place(state, p, kind, l, count, None).is_ok() {
                    out.push(Placement {
                        location: l,
                        from_complex: None,
                        capacity_left: crate::economy::NEW_COMPLEX_CAP
                            .saturating_sub(state.new_complexes_placed),
                    });
                }
            }
            k if k.is_ship() => {
                for &t in map.neighbours(l) {
                    if map.is_sea(t) {
                        continue;
                    }
                    if let Ok(src) = validate_place(state, p, kind, l, count, Some(t)) {
                        out.push(Placement {
                            location: l,
                            from_complex: Some(src),
                            capacity_left: capped(crate::economy::complex_capacity_left(
                                state, p, src,
                            )),
                        });
                    }
                }
            }
            _ => {
                if let Ok(src) = validate_place(state, p, kind, l, count, None) {
                    out.push(Placement {
                        location: l,
                        from_complex: Some(src),
                        capacity_left: capped(crate::economy::complex_capacity_left(state, p, src)),
                    });
                }
            }
        }
    }
    out
}

fn capped(n: u32) -> u32 {
    if n >= u32::MAX / 2 {
        u32::MAX
    } else {
        n
    }
}

/// Where an aircraft stands with respect to having to land.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum LandingStatus {
    /// Not an aircraft, or already on a legal landing spot.
    Safe,
    /// Must still reach one of these landing spots with its remaining moves.
    NeedsLanding {
        /// Legal landing spots in range.
        options: Vec<LocationId>,
    },
    /// No landing spot is in range: the aircraft will be lost.
    Stranded,
}

/// Landing situation of aircraft `unit` (both movement phases; `Safe` for non-aircraft).
pub fn landing_status(state: &GameState, unit: UnitId) -> LandingStatus {
    let Some(u) = state.unit(unit) else {
        return LandingStatus::Safe;
    };
    if !u.kind.is_air() || crate::movement::is_landing_spot(state, u, u.location) {
        return LandingStatus::Safe;
    }
    let map = MapData::get();
    let p = u.owner;
    let range = if u.loaded_on.is_some() {
        0
    } else {
        remaining_moves(state, u) as usize
    };
    let mut seen = vec![false; map.len()];
    seen[u.location.idx()] = true;
    let mut frontier = vec![u.location];
    let mut options = vec![];
    for _ in 0..range {
        let mut next = vec![];
        for cur in frontier {
            for &n in map.neighbours(cur) {
                if seen[n.idx()]
                    || state.is_untouched_neutral(n)
                    || (map.is_sea(n) && state.hostile_sea(p, n))
                {
                    continue;
                }
                seen[n.idx()] = true;
                next.push(n);
                if crate::movement::is_landing_spot(state, u, n) {
                    options.push(n);
                }
            }
        }
        frontier = next;
    }
    if options.is_empty() {
        LandingStatus::Stranded
    } else {
        LandingStatus::NeedsLanding { options }
    }
}

/// Battles still to fight.
pub fn battles_pending(state: &GameState) -> Vec<LocationId> {
    pending_battles(state)
}

/// Retreat destinations in the current battle.
pub fn retreat_destinations(state: &GameState) -> Vec<LocationId> {
    retreat_options(state)
}

//! Loading and unloading transports, including deferred amphibious landings.
use crate::economy::capture_territory;
use crate::error::{illegal, Result, RuleError};
use crate::event::Event;
use crate::map::{LocationId, MapData};
use crate::movement::NEUTRAL_FEE;
use crate::power::Power;
use crate::state::{GameState, Phase};
use crate::tech::move_allowance;
use crate::units::{UnitId, UnitRef, UnitType};

fn refs(state: &GameState, ids: &[UnitId]) -> Vec<UnitRef> {
    ids.iter()
        .filter_map(|&i| state.unit(i))
        .map(UnitRef::from)
        .collect()
}

/// Slots in use on a transport.
pub fn used_slots(state: &GameState, transport: UnitId) -> u8 {
    state
        .cargo_of(transport)
        .filter_map(|c| c.kind.transport_slots())
        .sum()
}

/// Validate `Load`.
pub fn validate_load(state: &GameState, p: Power, ids: &[UnitId], transport: UnitId) -> Result<()> {
    if !matches!(state.phase, Phase::CombatMove | Phase::NonCombatMove) {
        return illegal("loading is only allowed during movement");
    }
    let t = state
        .unit(transport)
        .ok_or(RuleError::UnknownUnit(transport))?;
    if t.kind != UnitType::Transport {
        return illegal("only transports carry land units");
    }
    if !t.owner.is_ally(p) {
        return illegal("that transport belongs to an enemy");
    }
    if ids.is_empty() {
        return illegal("nothing to load");
    }
    let map = MapData::get();
    let mut slots = used_slots(state, transport);
    let mut seen = ids.to_vec();
    seen.sort_unstable();
    seen.dedup();
    if seen.len() != ids.len() {
        return illegal("duplicate unit");
    }
    for &id in ids {
        let u = state.unit(id).ok_or(RuleError::UnknownUnit(id))?;
        if u.owner != p {
            return Err(RuleError::NotOwner(id));
        }
        let Some(s) = u.kind.transport_slots() else {
            return illegal(format!("{:?} cannot be carried", u.kind));
        };
        if u.loaded_on.is_some() || map.is_sea(u.location) {
            return illegal("unit is already aboard");
        }
        if !map.adjacent(u.location, t.location) {
            return illegal("the transport must be in an adjacent sea zone");
        }
        if u.moves_used > 0 || u.moved_cm || u.frozen {
            return illegal("a unit that has moved cannot board");
        }
        slots += s;
    }
    // Armor/AA fill the whole transport; two infantry may share one.
    if slots > 2 {
        return illegal("transport capacity exceeded (1 armor, 1 AA gun, or 2 infantry)");
    }
    Ok(())
}

/// Execute `Load`.
pub fn execute_load(
    state: &mut GameState,
    p: Power,
    ids: &[UnitId],
    transport: UnitId,
    ev: &mut Vec<Event>,
) {
    let at = state.unit(transport).unwrap().location;
    let cm = state.phase == Phase::CombatMove;
    let turn = state.turn.number;
    let r = refs(state, ids);
    for &id in ids {
        let allow = move_allowance(state, state.unit(id).unwrap());
        let u = state.unit_mut(id).unwrap();
        u.location = at;
        u.loaded_on = Some(transport);
        u.moves_used = allow;
        u.moved_cm = cm;
        u.loaded_turn = Some(turn);
    }
    ev.push(Event::UnitsLoaded {
        power: p,
        units: r,
        transport,
        at,
    });
}

/// How an unload will resolve.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnloadMode {
    /// Units go ashore now.
    Immediate,
    /// Units wait aboard and land when the battle at `to` begins.
    Deferred,
}

/// Validate `Unload` and say how it will resolve.
pub fn validate_unload(
    state: &GameState,
    p: Power,
    transport: UnitId,
    ids: &[UnitId],
    to: LocationId,
) -> Result<UnloadMode> {
    let cm = match state.phase {
        Phase::CombatMove => true,
        Phase::NonCombatMove => false,
        _ => return illegal("unloading is only allowed during movement"),
    };
    let t = state
        .unit(transport)
        .ok_or(RuleError::UnknownUnit(transport))?;
    if t.kind != UnitType::Transport {
        return illegal("not a transport");
    }
    let map = MapData::get();
    if ids.is_empty() {
        return illegal("nothing to unload");
    }
    if map.is_sea(to) || !map.adjacent(t.location, to) {
        return illegal("destination must be an adjacent territory");
    }
    for &id in ids {
        let u = state.unit(id).ok_or(RuleError::UnknownUnit(id))?;
        if u.owner != p {
            return Err(RuleError::NotOwner(id));
        }
        if u.loaded_on != Some(transport) {
            return illegal("unit is not aboard that transport");
        }
        if t.owner != p && u.loaded_turn == Some(state.turn.number) {
            return illegal("allied transports cannot load and unload in the same turn");
        }
        if u.assault_to.is_some() {
            return illegal("unit already committed to an assault");
        }
    }
    let occupied = state.enemy_occupied_land(p, to);
    let hostile_target =
        occupied || state.is_enemy_controlled(p, to) || state.is_untouched_neutral(to);
    if !cm {
        if hostile_target || !state.is_friendly_land(p, to) {
            return illegal("non-combat unloading is only into friendly territory");
        }
    } else {
        if let Some(prev) = t.unloaded_to {
            if prev != to {
                return illegal(
                    "all cargo of one transport must go to one territory during combat movement",
                );
            }
        }
        if state.soviet_restricted() && hostile_target {
            return illegal("the Soviet Union may not attack on its first turn");
        }
    }
    if ids
        .iter()
        .any(|&i| state.unit(i).unwrap().kind == UnitType::AaGun)
    {
        if hostile_target {
            return illegal("AA guns cannot make assaults");
        }
        if state.units_at(to).any(|u| u.kind == UnitType::AaGun) {
            return illegal("only one AA gun per territory");
        }
    }
    if hostile_target && state.is_untouched_neutral(to) && state.treasury[p] < NEUTRAL_FEE {
        return Err(RuleError::InsufficientFunds {
            need: NEUTRAL_FEE,
            have: state.treasury[p],
        });
    }
    if cm && hostile_target && (occupied || state.hostile_sea(p, t.location)) {
        return Ok(UnloadMode::Deferred);
    }
    Ok(UnloadMode::Immediate)
}

/// Put units ashore at `to` (violating a neutral / capturing as needed).
pub fn land_units(
    state: &mut GameState,
    p: Power,
    ids: &[UnitId],
    to: LocationId,
    ev: &mut Vec<Event>,
) {
    if state.is_untouched_neutral(to) {
        let pay = NEUTRAL_FEE.min(state.treasury[p]);
        state.treasury[p] -= pay;
        state.control[to] = Some(p);
        ev.push(Event::NeutralViolated {
            power: p,
            location: to,
            fee: pay,
        });
    }
    let combat_move = state.phase == Phase::CombatMove || state.phase == Phase::Combat;
    for &id in ids {
        if let Some(u) = state.unit_mut(id) {
            u.location = to;
            u.loaded_on = None;
            u.assault_to = None;
            u.stopped = combat_move;
        }
    }
    if state.is_enemy_controlled(p, to) && !state.enemy_occupied_land(p, to) {
        capture_territory(state, p, to, ev);
    }
}

/// Execute `Unload`.
pub fn execute_unload(
    state: &mut GameState,
    p: Power,
    transport: UnitId,
    ids: &[UnitId],
    to: LocationId,
    mode: UnloadMode,
    ev: &mut Vec<Event>,
) {
    let cm = state.phase == Phase::CombatMove;
    ev.push(Event::UnitsUnloaded {
        power: p,
        units: refs(state, ids),
        transport,
        to,
        assault: mode == UnloadMode::Deferred,
    });
    for &id in ids {
        let allow = move_allowance(state, state.unit(id).unwrap());
        let u = state.unit_mut(id).unwrap();
        u.moves_used = allow;
        u.moved_cm = cm;
        if mode == UnloadMode::Deferred {
            u.assault_to = Some(to);
        }
    }
    if mode == UnloadMode::Immediate {
        land_units(state, p, ids, to, ev);
    }
    let t = state.unit_mut(transport).unwrap();
    t.frozen = true;
    t.unloaded_to = Some(to);
}

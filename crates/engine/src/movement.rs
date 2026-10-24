//! Movement rules: per-step legality shared by `apply` and the query helpers, and execution.
use crate::dice::{d6, Rng};
use crate::economy::capture_territory;
use crate::error::{illegal, Result};
use crate::event::{DiceContext, Event};
use crate::map::{LocationId, MapData};
use crate::power::Power;
use crate::state::{GameState, Phase};
use crate::tech::move_allowance;
use crate::units::{Domain, Unit, UnitId, UnitRef, UnitType};

/// Fee for violating a neutral.
pub const NEUTRAL_FEE: u32 = 3;

/// Properties of a legal single step.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct StepInfo {
    /// The unit must stop here.
    pub terminal: bool,
    /// Entering enemy-occupied space (a battle will be fought if the unit ends here).
    pub battle: bool,
    /// A land unit entering captures an enemy-controlled, unoccupied territory.
    pub capture: bool,
    /// Step violates an untouched neutral.
    pub neutral: bool,
}

/// Result of a step check.
pub type StepResult = std::result::Result<StepInfo, &'static str>;

/// Legality of a land unit stepping `from -> to`. `first_space` is true if it is the unit's first space this turn.
pub fn land_step(
    state: &GameState,
    p: Power,
    phase: Phase,
    kind: UnitType,
    first_space: bool,
    from: LocationId,
    to: LocationId,
) -> StepResult {
    let map = MapData::get();
    if map.is_sea(to) || !map.adjacent(from, to) {
        return Err("land units move to adjacent land territories");
    }
    let cm = phase == Phase::CombatMove;
    if state.enemy_occupied_land(p, to) {
        if !cm || kind == UnitType::AaGun {
            return Err("cannot enter an enemy-occupied territory now");
        }
        return Ok(StepInfo {
            terminal: true,
            battle: true,
            ..Default::default()
        });
    }
    if state.is_untouched_neutral(to) {
        if !cm || kind == UnitType::AaGun {
            return Err("neutral territories can only be entered by combat movement");
        }
        return Ok(StepInfo {
            terminal: true,
            neutral: true,
            ..Default::default()
        });
    }
    if state.is_friendly_land(p, to) {
        return Ok(StepInfo::default());
    }
    // enemy-controlled, unoccupied
    if !cm || kind == UnitType::AaGun {
        return Err("cannot enter enemy territory in non-combat movement");
    }
    Ok(StepInfo {
        terminal: !(kind == UnitType::Armor && first_space),
        capture: true,
        ..Default::default()
    })
}

/// Is the canal between two zones usable by `p` this turn?
pub fn canal_open(state: &GameState, p: Power, a: LocationId, b: LocationId) -> bool {
    match MapData::get().canal_between(a, b) {
        None => true,
        Some(c) => c
            .land_territories
            .iter()
            .all(|&t| matches!(state.turn.start_control[t], Some(o) if o.is_ally(p))),
    }
}

/// Legality of a ship stepping `from -> to`.
pub fn sea_step(
    state: &GameState,
    p: Power,
    phase: Phase,
    from: LocationId,
    to: LocationId,
) -> StepResult {
    let map = MapData::get();
    if !map.is_sea(to) || !map.adjacent(from, to) {
        return Err("ships move to adjacent sea zones");
    }
    if !canal_open(state, p, from, to) {
        return Err("the canal is closed to you");
    }
    if state.hostile_sea(p, to) {
        if phase != Phase::CombatMove {
            return Err("cannot enter a hostile sea zone in non-combat movement");
        }
        return Ok(StepInfo {
            terminal: true,
            battle: true,
            ..Default::default()
        });
    }
    Ok(StepInfo::default())
}

/// Legality of an aircraft stepping `from -> to`.
pub fn air_step(
    state: &GameState,
    p: Power,
    phase: Phase,
    from: LocationId,
    to: LocationId,
) -> StepResult {
    let map = MapData::get();
    if !map.adjacent(from, to) {
        return Err("not adjacent");
    }
    let cm = phase == Phase::CombatMove;
    if map.is_sea(to) {
        return Ok(StepInfo {
            battle: cm && state.hostile_sea(p, to),
            ..Default::default()
        });
    }
    if state.is_untouched_neutral(to) {
        if !cm {
            return Err("aircraft cannot cross neutral territory in non-combat movement");
        }
        return Ok(StepInfo {
            neutral: true,
            ..Default::default()
        });
    }
    Ok(StepInfo {
        battle: cm && state.enemy_occupied_land(p, to),
        ..Default::default()
    })
}

/// Step check for any unit type.
pub fn unit_step(
    state: &GameState,
    u: &Unit,
    phase: Phase,
    first_space: bool,
    from: LocationId,
    to: LocationId,
) -> StepResult {
    match u.kind.domain() {
        Domain::Land => land_step(state, u.owner, phase, u.kind, first_space, from, to),
        Domain::Sea => sea_step(state, u.owner, phase, from, to),
        Domain::Air => air_step(state, u.owner, phase, from, to),
    }
}

/// Movement points left to a unit this turn.
pub fn remaining_moves(state: &GameState, u: &Unit) -> u8 {
    move_allowance(state, u).saturating_sub(u.moves_used)
}

/// Can aircraft `u` (owned by `p`) land at `loc` right now?
pub fn is_landing_spot(state: &GameState, u: &Unit, loc: LocationId) -> bool {
    let p = u.owner;
    if MapData::get().is_sea(loc) {
        if u.kind != UnitType::Fighter {
            return false;
        }
        return state.units_at(loc).any(|c| {
            c.kind == UnitType::Carrier
                && c.owner.is_ally(p)
                && (state.carrier_free_slots(c.id) > 0 || u.loaded_on == Some(c.id))
        });
    }
    let friendly_now = state.is_friendly_land(p, loc);
    let friendly_start = matches!(state.turn.start_control[loc], Some(o) if o.is_ally(p));
    friendly_now && friendly_start && !state.enemy_occupied_land(p, loc)
}

/// Is a landing spot reachable from `from` with `range` more moves (neutrals cannot be crossed)?
pub fn landing_reachable(state: &GameState, u: &Unit, from: LocationId, range: u8) -> bool {
    let map = MapData::get();
    let mut best = vec![u8::MAX; map.len()];
    let mut queue = std::collections::VecDeque::new();
    best[from.idx()] = 0;
    queue.push_back(from);
    while let Some(cur) = queue.pop_front() {
        if is_landing_spot(state, u, cur) {
            return true;
        }
        let d = best[cur.idx()];
        if d >= range {
            continue;
        }
        for &n in map.neighbours(cur) {
            if best[n.idx()] != u8::MAX || state.is_untouched_neutral(n) {
                continue;
            }
            best[n.idx()] = d + 1;
            queue.push_back(n);
        }
    }
    false
}

/// Unit-level preconditions for moving in the current movement phase.
pub fn check_unit_ready(state: &GameState, u: &Unit, p: Power) -> Result<()> {
    let cm = state.phase == Phase::CombatMove;
    if u.frozen {
        return illegal(format!("{:?} cannot move any more this turn", u.kind));
    }
    if u.kind == UnitType::IndustrialComplex {
        return illegal("industrial complexes cannot move");
    }
    if let Some(c) = u.loaded_on {
        if u.kind != UnitType::Fighter {
            return illegal("cargo must be unloaded, not moved");
        }
        let carrier = state
            .unit(c)
            .ok_or(crate::error::RuleError::UnknownUnit(c))?;
        if carrier.owner == p && carrier.moves_used > 0 {
            return illegal("fighters must launch before their carrier moves");
        }
    }
    if cm && (u.stopped || u.combat_from.is_some()) {
        return illegal("unit already ended its move in a battle space");
    }
    if !cm && !u.kind.is_air() && u.moved_cm {
        return illegal("units that moved in combat movement may not move again");
    }
    if cm && u.kind == UnitType::Bomber && u.bombing {
        return illegal("bomber already raided");
    }
    Ok(())
}

/// A validated group move.
#[derive(Debug)]
pub struct MovePlan {
    /// Starting location.
    pub from: LocationId,
    /// Per-step info for the longest-travelling unit (union of flags).
    pub steps: Vec<StepInfo>,
    /// Untouched neutrals entered (fees due).
    pub neutrals: Vec<LocationId>,
}

/// Validate a `Move` action for `p` without changing the state.
pub fn validate_move(
    state: &GameState,
    p: Power,
    ids: &[UnitId],
    path: &[LocationId],
) -> Result<MovePlan> {
    let phase = state.phase;
    if phase != Phase::CombatMove && phase != Phase::NonCombatMove {
        return illegal("movement is only allowed in the movement phases");
    }
    if ids.is_empty() || path.is_empty() {
        return illegal("empty move");
    }
    let mut seen = ids.to_vec();
    seen.sort_unstable();
    seen.dedup();
    if seen.len() != ids.len() {
        return illegal("duplicate unit in move");
    }
    let cm = phase == Phase::CombatMove;
    let first = state
        .unit(ids[0])
        .ok_or(crate::error::RuleError::UnknownUnit(ids[0]))?;
    let from = first.location;
    let map = MapData::get();
    let mut steps = vec![StepInfo::default(); path.len()];
    let mut neutrals: Vec<LocationId> = vec![];
    for &id in ids {
        let u = state
            .unit(id)
            .ok_or(crate::error::RuleError::UnknownUnit(id))?;
        if u.owner != p {
            return Err(crate::error::RuleError::NotOwner(id));
        }
        if u.location != from {
            return illegal("units in a group must start in the same place");
        }
        check_unit_ready(state, u, p)?;
        let rem = remaining_moves(state, u) as usize;
        if path.len() > rem {
            return illegal(format!("{:?} has only {} moves left", u.kind, rem));
        }
        let mut cur = from;
        for (i, &to) in path.iter().enumerate() {
            let first_space = u.moves_used as usize + i == 0;
            let info = unit_step(state, u, phase, first_space, cur, to)
                .map_err(crate::error::RuleError::illegal)?;
            if info.terminal && i + 1 != path.len() {
                return illegal(format!("{:?} must stop upon entering {}", u.kind, to.id()));
            }
            if (info.capture || info.battle) && cm && state.soviet_restricted() {
                return illegal("the Soviet Union may not attack on its first turn");
            }
            if info.neutral && !neutrals.contains(&to) {
                neutrals.push(to);
            }
            steps[i].terminal |= info.terminal;
            steps[i].battle |= info.battle;
            steps[i].capture |= info.capture;
            steps[i].neutral |= info.neutral;
            cur = to;
        }
        let last = *path.last().unwrap();
        if u.kind == UnitType::AaGun
            && state
                .units_at(last)
                .any(|o| o.kind == UnitType::AaGun && o.id != u.id)
        {
            return illegal("only one AA gun per territory");
        }
        if u.kind.is_air() && cm {
            let left = rem - path.len();
            if !landing_reachable(state, u, last, left as u8) {
                return illegal("no legal landing place would be within range (no kamikaze)");
            }
        }
        if map.is_sea(last) && u.kind.is_land() {
            return illegal("land units cannot end at sea");
        }
    }
    let fee = neutrals.len() as u32 * NEUTRAL_FEE;
    if fee > state.treasury[p] {
        return Err(crate::error::RuleError::InsufficientFunds {
            need: fee,
            have: state.treasury[p],
        });
    }
    Ok(MovePlan {
        from,
        steps,
        neutrals,
    })
}

fn refs(state: &GameState, ids: &[UnitId]) -> Vec<UnitRef> {
    ids.iter()
        .filter_map(|&i| state.unit(i))
        .map(UnitRef::from)
        .collect()
}

/// Antiaircraft fire at aircraft that enter `loc`. Returns survivors' ids.
fn aa_fire(
    state: &mut GameState,
    p: Power,
    loc: LocationId,
    air: &mut Vec<UnitId>,
    rng: &mut Rng,
    ev: &mut Vec<Event>,
) {
    let Some(gun) = state.enemy_aa_at(p, loc).map(|g| g.owner) else {
        return;
    };
    if air.is_empty() {
        return;
    }
    let rolls: Vec<u8> = air.iter().map(|_| d6(rng)).collect();
    let gun_id = state.enemy_aa_at(p, loc).map(|g| g.id);
    let details: Vec<crate::event::DieDetail> = rolls
        .iter()
        .map(|&r| crate::event::DieDetail {
            unit: gun_id,
            unit_type: Some(UnitType::AaGun),
            owner: gun,
            hit_on: 1,
            roll: r,
            hit: r == 1,
        })
        .collect();
    ev.push(Event::DiceRolled {
        power: gun,
        context: DiceContext::AaFire { location: loc },
        rolls: rolls.clone(),
        details,
    });
    let hits = rolls.iter().filter(|&&r| r == 1).count();
    if hits == 0 {
        return;
    }
    // The attacker chooses; we sacrifice the cheapest aircraft first (documented in DECISIONS.md).
    let mut order = air.clone();
    order.sort_by_key(|&i| {
        let u = state.unit(i).unwrap();
        (u.kind.stats().cost, u.id)
    });
    let dead: Vec<UnitId> = order.into_iter().take(hits).collect();
    let destroyed = refs(state, &dead);
    for d in &dead {
        state.remove_unit(*d);
        air.retain(|x| x != d);
    }
    ev.push(Event::AaFire {
        gun_owner: gun,
        location: loc,
        destroyed,
    });
}

/// Execute a validated move.
pub fn execute_move(
    state: &mut GameState,
    p: Power,
    ids: &[UnitId],
    path: &[LocationId],
    plan: &MovePlan,
    rng: &mut Rng,
    ev: &mut Vec<Event>,
) {
    let cm = state.phase == Phase::CombatMove;
    let mut alive: Vec<UnitId> = ids.to_vec();
    let has_land = ids
        .iter()
        .any(|&i| state.unit(i).is_some_and(|u| u.kind.is_land()));
    let moved_refs = refs(state, ids);
    ev.push(Event::UnitsMoved {
        power: p,
        units: moved_refs,
        from: plan.from,
        path: path.to_vec(),
    });
    // launching aircraft leave their carrier
    for &id in ids {
        if let Some(u) = state.unit_mut(id) {
            u.loaded_on = None;
        }
    }
    for (i, &to) in path.iter().enumerate() {
        if plan.steps[i].neutral && state.is_untouched_neutral(to) {
            state.treasury[p] -= NEUTRAL_FEE;
            state.control[to] = Some(p);
            ev.push(Event::NeutralViolated {
                power: p,
                location: to,
                fee: NEUTRAL_FEE,
            });
        }
        if cm {
            let air: Vec<UnitId> = alive
                .iter()
                .copied()
                .filter(|&a| state.unit(a).is_some_and(|u| u.kind.is_air()))
                .collect();
            if !air.is_empty() {
                let mut survivors = air.clone();
                aa_fire(state, p, to, &mut survivors, rng, ev);
                alive.retain(|a| !air.contains(a) || survivors.contains(a));
            }
        }
        if has_land
            && plan.steps[i].capture
            && state.is_enemy_controlled(p, to)
            && !state.enemy_occupied_land(p, to)
        {
            capture_territory(state, p, to, ev);
        }
    }
    let last = *path.last().unwrap();
    let prev = if path.len() >= 2 {
        path[path.len() - 2]
    } else {
        plan.from
    };
    let info = *plan.steps.last().unwrap();
    for &id in &alive {
        let (cargo_ids, is_ship) = {
            let u = state.unit(id).unwrap();
            (
                state.cargo_of(id).map(|c| c.id).collect::<Vec<_>>(),
                u.kind.is_ship(),
            )
        };
        let u = state.unit_mut(id).unwrap();
        u.location = last;
        u.moves_used += path.len() as u8;
        if cm && !u.kind.is_air() {
            u.moved_cm = true;
        }
        if cm && (info.terminal || info.battle || info.neutral) {
            u.stopped = true;
        }
        if cm && info.battle {
            u.combat_from = Some(prev);
        }
        if is_ship {
            for c in cargo_ids {
                if let Some(cu) = state.unit_mut(c) {
                    cu.location = last;
                }
            }
        }
    }
    if !cm {
        land_aircraft_on_carriers(state, &alive, ev);
    }
}

/// Fighters that finish a non-combat move in a zone with a friendly carrier land on it.
fn land_aircraft_on_carriers(state: &mut GameState, ids: &[UnitId], ev: &mut Vec<Event>) {
    for &id in ids {
        let Some(u) = state.unit(id).cloned() else {
            continue;
        };
        if u.kind != UnitType::Fighter
            || !MapData::get().is_sea(u.location)
            || u.loaded_on.is_some()
        {
            continue;
        }
        let mut carriers: Vec<&Unit> = state
            .units_at(u.location)
            .filter(|c| {
                c.kind == UnitType::Carrier
                    && c.owner.is_ally(u.owner)
                    && state.carrier_free_slots(c.id) > 0
            })
            .collect();
        carriers.sort_by_key(|c| (c.owner != u.owner, c.id));
        if let Some(c) = carriers.first().map(|c| c.id) {
            state.unit_mut(id).unwrap().loaded_on = Some(c);
            state.unit_mut(c).unwrap().frozen = true;
            ev.push(Event::AircraftLanded {
                unit: UnitRef::from(&u),
                carrier: c,
            });
        }
    }
}

/// At the end of non-combat movement: aircraft of `p` that are not on a legal landing spot are lost.
pub fn resolve_landings(state: &mut GameState, p: Power, ev: &mut Vec<Event>) {
    let air: Vec<UnitId> = state
        .units
        .iter()
        .filter(|u| u.owner == p && u.kind.is_air() && u.loaded_on.is_none())
        .map(|u| u.id)
        .collect();
    land_aircraft_on_carriers(state, &air, ev);
    let mut lost = vec![];
    for id in air {
        let u = state.unit(id).unwrap().clone();
        if u.loaded_on.is_some() {
            continue;
        }
        if !is_landing_spot(state, &u, u.location) {
            lost.push(UnitRef::from(&u));
            state.remove_unit(id);
        }
    }
    if !lost.is_empty() {
        ev.push(Event::UnitsLost {
            power: p,
            units: lost,
            reason: "aircraft could not land".into(),
        });
    }
}

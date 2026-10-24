//! Battle discovery and set-up: which battles are pending, who takes part, retreat options.
use crate::combat;
use crate::economy::capture_territory;
use crate::error::{illegal, Result};
use crate::event::{BattleOutcome, Event};
use crate::map::{LocationId, MapData};
use crate::movement::canal_open;
use crate::power::Power;
use crate::state::{Battle, BattleKind, BattleStep, GameState, Phase};
use crate::transport::land_units;
use crate::units::{UnitId, UnitRef, UnitType};

/// Battles the active power may still begin (sea battles first).
pub fn pending_battles(state: &GameState) -> Vec<LocationId> {
    if !matches!(state.phase, Phase::CombatMove | Phase::Combat) {
        return vec![];
    }
    let p = state.current_power;
    let map = MapData::get();
    let mut sea = vec![];
    let mut land = vec![];
    for u in state.units.iter().filter(|u| u.owner == p) {
        if u.combat_from.is_some() && !u.fought && !u.bombing {
            let l = u.location;
            if map.is_sea(l) {
                if state.hostile_sea(p, l) && !sea.contains(&l) {
                    sea.push(l);
                }
            } else if state.enemy_occupied_land(p, l) && !land.contains(&l) {
                land.push(l);
            }
        }
    }
    for u in state
        .units
        .iter()
        .filter(|u| u.owner == p && u.assault_to.is_some())
    {
        let t = u.assault_to.unwrap();
        let Some(tr) = u.loaded_on.and_then(|i| state.unit(i)) else {
            continue;
        };
        let z = tr.location;
        let zone_pending = sea.contains(&z);
        if (!state.hostile_sea(p, z) || zone_pending) && map.adjacent(z, t) && !land.contains(&t) {
            land.push(t);
        }
    }
    sea.sort();
    land.sort();
    sea.into_iter()
        .chain(land)
        .filter(|l| !state.turn.resolved.contains(l))
        .collect()
}

/// Everything fought over a given site, for starting a battle.
fn participants(
    state: &GameState,
    p: Power,
    loc: LocationId,
    kind: BattleKind,
) -> (Vec<UnitId>, Vec<UnitId>) {
    let att: Vec<UnitId> = state
        .units_at(loc)
        .filter(|u| {
            u.owner == p
                && u.combat_from.is_some()
                && !u.fought
                && !u.bombing
                && u.loaded_on.is_none()
        })
        .filter(|u| match kind {
            BattleKind::Land => u.kind.is_fighting_land_unit(),
            BattleKind::Sea => u.kind.is_ship() || u.kind.is_air(),
        })
        .map(|u| u.id)
        .collect();
    let def: Vec<UnitId> = state
        .units_at(loc)
        .filter(|u| !u.owner.is_ally(p))
        .filter(|u| match kind {
            BattleKind::Land => u.kind.is_fighting_land_unit(),
            BattleKind::Sea => {
                u.kind.is_ship() || (u.kind == UnitType::Fighter && u.loaded_on.is_some())
            }
        })
        .map(|u| u.id)
        .collect();
    (att, def)
}

fn refs(state: &GameState, ids: &[UnitId]) -> Vec<UnitRef> {
    ids.iter()
        .filter_map(|&i| state.unit(i))
        .map(UnitRef::from)
        .collect()
}

/// Start the battle at `loc`.
pub fn begin_battle(state: &mut GameState, loc: LocationId, ev: &mut Vec<Event>) -> Result<()> {
    let p = state.current_power;
    if state.phase != Phase::Combat {
        return illegal("battles are fought in the Combat phase");
    }
    if state.battle.is_some() {
        return illegal("a battle is already in progress");
    }
    let pending = pending_battles(state);
    if !pending.contains(&loc) {
        return illegal("no battle to fight there");
    }
    let map = MapData::get();
    let kind = if map.is_sea(loc) {
        BattleKind::Sea
    } else {
        BattleKind::Land
    };
    let mut amphibious = false;
    let mut assault_zones = vec![];
    if kind == BattleKind::Land {
        let assault: Vec<UnitId> = state
            .units
            .iter()
            .filter(|u| u.owner == p && u.assault_to == Some(loc))
            .map(|u| u.id)
            .collect();
        for &id in &assault {
            let z = state
                .unit(id)
                .and_then(|u| u.loaded_on)
                .and_then(|t| state.unit(t))
                .map(|t| t.location);
            if let Some(z) = z {
                if pending.contains(&z) {
                    return illegal("resolve the sea battle next to the landing first");
                }
            }
        }
        let mut landing = vec![];
        for id in assault {
            let z = state
                .unit(id)
                .and_then(|u| u.loaded_on)
                .and_then(|t| state.unit(t))
                .map(|t| t.location);
            match z {
                Some(z) if !state.hostile_sea(p, z) => {
                    if !assault_zones.contains(&z) {
                        assault_zones.push(z);
                    }
                    landing.push((id, z));
                }
                _ => {
                    if let Some(u) = state.unit_mut(id) {
                        u.assault_to = None;
                    }
                }
            }
        }
        if landing.is_empty()
            && !state
                .units_at(loc)
                .any(|u| u.owner == p && u.combat_from.is_some() && !u.fought)
        {
            ev.push(Event::AssaultCancelled { location: loc });
            state.turn.resolved.push(loc);
            return Ok(());
        }
        if !landing.is_empty() {
            amphibious = true;
            let ids: Vec<UnitId> = landing.iter().map(|x| x.0).collect();
            if state.enemy_occupied_land(p, loc) {
                // Troops wade ashore but the territory is contested.
                if state.is_untouched_neutral(loc) {
                    unreachable!("neutral territories are never occupied");
                }
                for (id, z) in &landing {
                    let u = state.unit_mut(*id).unwrap();
                    u.location = loc;
                    u.loaded_on = None;
                    u.assault_to = None;
                    u.combat_from = Some(*z);
                }
            } else {
                land_units(state, p, &ids, loc, ev);
                state.turn.resolved.push(loc);
                for id in ids {
                    if let Some(u) = state.unit_mut(id) {
                        u.fought = true;
                    }
                }
                ev.push(Event::BattleEnded {
                    location: loc,
                    outcome: BattleOutcome::AttackerWins,
                });
                return Ok(());
            }
        }
    }
    let (attackers, defenders) = participants(state, p, loc, kind);
    if attackers.is_empty() {
        return illegal("no attackers");
    }
    if defenders.is_empty() {
        // Nothing to fight (e.g. defenders vanished): conclude at once.
        state.battle = Some(Battle {
            location: loc,
            kind,
            attacker: p,
            attackers,
            defenders,
            amphibious,
            round: 1,
            step: BattleStep::Resolve,
            pending: None,
            queued: None,
            marked_att: vec![],
            marked_def: vec![],
            bombarded: true,
            assault_zones,
        });
        combat::finish_battle(state, BattleOutcome::AttackerWins, ev);
        return Ok(());
    }
    ev.push(Event::BattleStarted {
        location: loc,
        kind,
        attacker: p,
        attackers: refs(state, &attackers),
        defenders: refs(state, &defenders),
        amphibious,
    });
    state.battle = Some(Battle {
        location: loc,
        kind,
        attacker: p,
        attackers,
        defenders,
        amphibious,
        round: 1,
        step: BattleStep::RoundStart,
        pending: None,
        queued: None,
        marked_att: vec![],
        marked_def: vec![],
        bombarded: false,
        assault_zones,
    });
    Ok(())
}

/// The power that assigns casualties for the defending side.
pub fn defending_controller(state: &GameState, b: &Battle) -> Power {
    let alive: Vec<_> = b.defenders.iter().filter_map(|&i| state.unit(i)).collect();
    if b.kind == BattleKind::Land {
        if let Some(c) = state.control[b.location] {
            if alive.iter().any(|u| u.owner == c) {
                return c;
            }
        }
    }
    let mut best: Option<(usize, Power)> = None;
    for pw in Power::ALL {
        let n = alive.iter().filter(|u| u.owner == pw).count();
        if n > 0 && best.is_none_or(|(bn, _)| n > bn) {
            best = Some((n, pw));
        }
    }
    best.map(|b| b.1).unwrap_or(b.attacker.next())
}

/// Legal retreat destinations for the attacker of the current battle.
pub fn retreat_options(state: &GameState) -> Vec<LocationId> {
    let Some(b) = &state.battle else {
        return vec![];
    };
    if b.amphibious {
        return vec![];
    }
    let p = b.attacker;
    let movers: Vec<_> = b
        .attackers
        .iter()
        .filter_map(|&i| state.unit(i))
        .filter(|u| !u.kind.is_air())
        .collect();
    if movers.is_empty() {
        return vec![b.location];
    }
    let mut out: Vec<LocationId> = vec![];
    let pend = pending_battles(state);
    for u in movers {
        let Some(c) = u.combat_from else { continue };
        if out.contains(&c) {
            continue;
        }
        let ok = match b.kind {
            BattleKind::Land => state.is_friendly_land(p, c),
            BattleKind::Sea => sea_destination_ok(state, p, b.location, c, &pend),
        };
        if ok {
            out.push(c);
        }
    }
    out
}

/// May ships of `p` retreat from `from` into sea zone `to`?
fn sea_destination_ok(
    state: &GameState,
    p: Power,
    from: LocationId,
    to: LocationId,
    pend: &[LocationId],
) -> bool {
    MapData::get().is_sea(to)
        && !state.hostile_sea(p, to)
        && !state.turn.resolved.contains(&to)
        && !pend.contains(&to)
        && canal_open(state, p, from, to)
}

/// Legal destinations for withdrawing attacking submarines.
pub fn sub_withdraw_options(state: &GameState) -> Vec<LocationId> {
    let Some(b) = &state.battle else {
        return vec![];
    };
    if b.kind != BattleKind::Sea {
        return vec![];
    }
    let pend = pending_battles(state);
    let mut out = vec![];
    for u in b
        .attackers
        .iter()
        .filter_map(|&i| state.unit(i))
        .filter(|u| u.kind.is_ship())
    {
        if let Some(c) = u.combat_from {
            if !out.contains(&c) && sea_destination_ok(state, b.attacker, b.location, c, &pend) {
                out.push(c);
            }
        }
    }
    out
}

/// Legal destinations for a defending submarine withdrawing.
pub fn defender_withdraw_options(state: &GameState, defender: Power) -> Vec<LocationId> {
    let Some(b) = &state.battle else {
        return vec![];
    };
    let map = MapData::get();
    map.neighbours(b.location)
        .iter()
        .copied()
        .filter(|&z| map.is_sea(z) && canal_open(state, defender, b.location, z))
        .filter(|&z| {
            !state
                .units_at(z)
                .any(|u| !u.owner.is_ally(defender) && (u.kind.is_ship() || u.kind.is_air()))
        })
        .collect()
}

/// Capture after a won land battle.
pub fn capture_after_win(state: &mut GameState, b: &Battle, ev: &mut Vec<Event>) {
    let p = b.attacker;
    let land_survivor = b
        .attackers
        .iter()
        .filter_map(|&i| state.unit(i))
        .any(|u| u.kind.is_land() && u.location == b.location);
    if land_survivor
        && state.control[b.location] != Some(p)
        && !state.is_friendly_land(p, b.location)
    {
        capture_territory(state, p, b.location, ev);
    }
}

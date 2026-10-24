//! Strategic bombing raids and rocket attacks.
use crate::dice::{d6, Rng};
use crate::error::{illegal, Result, RuleError};
use crate::event::{DiceContext, Event};
use crate::map::{LocationId, MapData};
use crate::power::Power;
use crate::state::{GameState, Phase};
use crate::tech::{attack_dice, Tech};
use crate::units::{UnitId, UnitType};

/// Territories with an enemy complex where `p` has bombers able to raid, with those bombers.
pub fn bombing_targets(state: &GameState, p: Power) -> Vec<(LocationId, Vec<UnitId>)> {
    let mut out: Vec<(LocationId, Vec<UnitId>)> = vec![];
    for u in state.units.iter().filter(|u| {
        u.owner == p
            && u.kind == UnitType::Bomber
            && !u.bombing
            && !u.fought
            && u.loaded_on.is_none()
    }) {
        let l = u.location;
        if MapData::get().is_sea(l)
            || state.turn.resolved.contains(&l)
            || !has_enemy_complex(state, p, l)
        {
            continue;
        }
        match out.iter_mut().find(|x| x.0 == l) {
            Some(x) => x.1.push(u.id),
            None => out.push((l, vec![u.id])),
        }
    }
    out
}

fn has_enemy_complex(state: &GameState, p: Power, l: LocationId) -> bool {
    state.is_enemy_controlled(p, l) && state.complex_at(l).is_some_and(|c| !c.owner.is_ally(p))
}

/// Resolve a strategic bombing raid.
pub fn declare(
    state: &mut GameState,
    p: Power,
    loc: LocationId,
    bombers: &[UnitId],
    rng: &mut Rng,
    ev: &mut Vec<Event>,
) -> Result<()> {
    if state.phase != Phase::Combat || state.battle.is_some() {
        return illegal("raids are declared in the Combat phase between battles");
    }
    if state.soviet_restricted() {
        return illegal("the Soviet Union may not attack on its first turn");
    }
    if state.turn.resolved.contains(&loc) {
        return illegal("raids must be resolved before the battle at that location");
    }
    if !has_enemy_complex(state, p, loc) {
        return illegal("no enemy industrial complex there");
    }
    if bombers.is_empty() {
        return illegal("no bombers named");
    }
    let mut seen = bombers.to_vec();
    seen.sort_unstable();
    seen.dedup();
    if seen.len() != bombers.len() {
        return illegal("duplicate bomber");
    }
    for &id in bombers {
        let u = state.unit(id).ok_or(RuleError::UnknownUnit(id))?;
        if u.owner != p {
            return Err(RuleError::NotOwner(id));
        }
        if u.kind != UnitType::Bomber || u.location != loc || u.bombing || u.fought {
            return illegal("only bombers in the target territory can raid");
        }
    }
    let victim = state.complex_at(loc).unwrap().owner;
    let mut dice = 0;
    for &id in bombers {
        dice += attack_dice_for_raid(state, id);
        let u = state.unit_mut(id).unwrap();
        u.bombing = true;
        u.fought = true;
        u.combat_from = None;
    }
    let rolls: Vec<u8> = (0..dice).map(|_| d6(rng)).collect();
    let total: u32 = rolls.iter().map(|&r| u32::from(r)).sum();
    ev.push(Event::DiceRolled {
        power: p,
        context: DiceContext::StrategicBombing { location: loc },
        rolls,
    });
    let damage = total.min(state.treasury[victim]);
    state.treasury[victim] -= damage;
    ev.push(Event::StrategicBombing {
        power: p,
        location: loc,
        victim,
        damage,
    });
    Ok(())
}

fn attack_dice_for_raid(state: &GameState, id: UnitId) -> u32 {
    state.unit(id).map_or(0, |u| attack_dice(state, u).max(1))
}

/// Territories with enemy complexes within 3 spaces of the gun.
pub fn rocket_targets(state: &GameState, gun: UnitId) -> Vec<LocationId> {
    let Some(g) = state.unit(gun) else {
        return vec![];
    };
    let p = g.owner;
    if g.kind != UnitType::AaGun
        || g.loaded_on.is_some()
        || MapData::get().is_sea(g.location)
        || !state.has_tech(p, Tech::Rockets)
    {
        return vec![];
    }
    let map = MapData::get();
    let mut dist = vec![u8::MAX; map.len()];
    let mut q = std::collections::VecDeque::new();
    dist[g.location.idx()] = 0;
    q.push_back(g.location);
    let mut out = vec![];
    while let Some(c) = q.pop_front() {
        let d = dist[c.idx()];
        if !map.is_sea(c) && has_enemy_complex(state, p, c) {
            out.push(c);
        }
        if d == 3 {
            continue;
        }
        for &n in map.neighbours(c) {
            if dist[n.idx()] == u8::MAX {
                dist[n.idx()] = d + 1;
                q.push_back(n);
            }
        }
    }
    out
}

/// Can `p` still launch a rocket at anything this turn?
pub fn rocket_available(state: &GameState, p: Power) -> bool {
    !state.turn.rocket_fired
        && state.has_tech(p, Tech::Rockets)
        && state.units.iter().any(|u| {
            u.owner == p && u.kind == UnitType::AaGun && !rocket_targets(state, u.id).is_empty()
        })
}

/// Fire a rocket.
pub fn fire_rocket(
    state: &mut GameState,
    p: Power,
    gun: UnitId,
    target: LocationId,
    rng: &mut Rng,
    ev: &mut Vec<Event>,
) -> Result<()> {
    if state.phase != Phase::Combat || state.battle.is_some() {
        return illegal("rockets are fired in the Combat phase between battles");
    }
    if state.soviet_restricted() {
        return illegal("the Soviet Union may not attack on its first turn");
    }
    if !state.has_tech(p, Tech::Rockets) || state.turn.rocket_fired {
        return illegal("no rocket available");
    }
    let g = state.unit(gun).ok_or(RuleError::UnknownUnit(gun))?;
    if g.owner != p {
        return Err(RuleError::NotOwner(gun));
    }
    if !rocket_targets(state, gun).contains(&target) {
        return illegal("target out of range or without an enemy complex");
    }
    let victim = state.complex_at(target).unwrap().owner;
    let r = d6(rng);
    ev.push(Event::DiceRolled {
        power: p,
        context: DiceContext::Rocket { location: target },
        rolls: vec![r],
    });
    let damage = u32::from(r).min(state.treasury[victim]);
    state.treasury[victim] -= damage;
    state.turn.rocket_fired = true;
    state.unit_mut(gun).unwrap().frozen = true;
    ev.push(Event::RocketFired {
        power: p,
        location: target,
        victim,
        damage,
    });
    Ok(())
}

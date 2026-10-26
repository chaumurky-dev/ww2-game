//! Casualty selection: who must lose what, forced-choice detection, removal.
use crate::battle::defending_controller;
use crate::casualty_policy::{choose_casualties, CasualtyPolicy};
use crate::combat::advance;
use crate::dice::Rng;
use crate::error::{illegal, Result, RuleError};
use crate::event::Event;
use crate::power::Power;
use crate::state::{Battle, GameState, PendingHits, Side};
use crate::units::{UnitId, UnitRef, UnitType};

pub(crate) fn refs(state: &GameState, ids: &[UnitId]) -> Vec<UnitRef> {
    ids.iter()
        .filter_map(|&i| state.unit(i))
        .map(UnitRef::from)
        .collect()
}

pub(crate) fn alive(state: &GameState, ids: &[UnitId]) -> Vec<UnitId> {
    ids.iter()
        .copied()
        .filter(|&i| state.unit(i).is_some())
        .collect()
}

/// Units of one side that can still be chosen as casualties.
pub fn candidates(state: &GameState, b: &Battle, side: Side) -> Vec<UnitId> {
    let (list, marked) = match side {
        Side::Attacker => (&b.attackers, &b.marked_att),
        Side::Defender => (&b.defenders, &b.marked_def),
    };
    list.iter()
        .copied()
        .filter(|i| state.unit(*i).is_some() && !marked.contains(i))
        .collect()
}

/// Number of casualties required for `normal`/`sub` hits against `cand`.
pub fn required_casualties(
    state: &GameState,
    cand: &[UnitId],
    normal: u32,
    sub: u32,
) -> (usize, usize) {
    let air = cand
        .iter()
        .filter(|&&i| state.unit(i).is_some_and(|u| u.kind.is_air()))
        .count();
    let nonair = cand.len() - air;
    let ms = (sub as usize).min(nonair);
    let cap = (normal as usize).min(nonair - ms + air);
    (ms + cap, ms)
}

/// Who chooses casualties for a side.
pub fn chooser(state: &GameState, b: &Battle, side: Side) -> Power {
    match side {
        Side::Attacker => b.attacker,
        Side::Defender => defending_controller(state, b),
    }
}

/// Remove units from play (detaching aircraft from sunk carriers), returning what died.
pub fn kill_units(state: &mut GameState, ids: &[UnitId]) -> Vec<UnitRef> {
    let mut out = vec![];
    for &id in ids {
        let Some(u) = state.unit(id) else { continue };
        out.push(UnitRef::from(u));
        if u.kind == UnitType::Carrier {
            let air: Vec<UnitId> = state
                .cargo_of(id)
                .filter(|c| c.kind.is_air())
                .map(|c| c.id)
                .collect();
            for a in air {
                if let Some(au) = state.unit_mut(a) {
                    au.loaded_on = None;
                }
            }
        }
        state.remove_unit(id);
    }
    out
}

pub(crate) fn remove_now(
    state: &mut GameState,
    b: &mut Battle,
    ids: &[UnitId],
    ev: &mut Vec<Event>,
) {
    let r = kill_units(state, ids);
    b.attackers = alive(state, &b.attackers);
    b.defenders = alive(state, &b.defenders);
    if !r.is_empty() {
        ev.push(Event::CasualtiesRemoved {
            location: b.location,
            units: r,
        });
    }
}

/// Cheapest-first valid casualty choice (used for auto-resolution and as an AI fallback).
/// This is [`choose_casualties`](crate::casualty_policy::choose_casualties) under the default
/// policy.
pub fn default_pick(state: &GameState, cand: &[UnitId], normal: u32, sub: u32) -> Vec<UnitId> {
    choose_casualties(state, CasualtyPolicy::default(), normal + sub, sub, cand)
}

/// Assign the casualties now if the choice is forced; otherwise record a pending selection.
pub(crate) fn request(
    state: &mut GameState,
    b: &mut Battle,
    side: Side,
    normal: u32,
    sub: u32,
    immediate: bool,
    ev: &mut Vec<Event>,
) {
    let cand = candidates(state, b, side);
    let (t, _) = required_casualties(state, &cand, normal, sub);
    if t == 0 {
        return;
    }
    let same = cand
        .iter()
        .filter_map(|&i| state.unit(i))
        .map(|u| (u.kind, u.owner))
        .collect::<std::collections::BTreeSet<_>>()
        .len()
        == 1;
    let nonair: Vec<UnitId> = cand
        .iter()
        .copied()
        .filter(|&i| state.unit(i).is_some_and(|u| !u.kind.is_air()))
        .collect();
    let only_subs_forced = normal == 0 && t == nonair.len();
    if t == cand.len() || same || only_subs_forced {
        let ids = if t == cand.len() {
            cand
        } else if only_subs_forced {
            nonair
        } else {
            default_pick(state, &cand, normal, sub)
        };
        assign(state, b, side, &ids, immediate, ev);
    } else {
        b.pending = Some(PendingHits {
            side,
            normal,
            sub,
            immediate,
        });
    }
}

pub(crate) fn assign(
    state: &mut GameState,
    b: &mut Battle,
    side: Side,
    ids: &[UnitId],
    immediate: bool,
    ev: &mut Vec<Event>,
) {
    if immediate {
        remove_now(state, b, ids, ev);
    } else {
        match side {
            Side::Attacker => b.marked_att.extend_from_slice(ids),
            Side::Defender => b.marked_def.extend_from_slice(ids),
        }
    }
}

pub(crate) fn after_selection(state: &mut GameState, b: &mut Battle, ev: &mut Vec<Event>) {
    b.pending = None;
    if let Some(q) = b.queued.take() {
        request(state, b, q.side, q.normal, q.sub, q.immediate, ev);
    }
}

/// Handle `SelectCasualties`.
pub fn select_casualties(
    state: &mut GameState,
    actor: Power,
    ids: &[UnitId],
    rng: &mut Rng,
    ev: &mut Vec<Event>,
) -> Result<()> {
    let b = state
        .battle
        .as_ref()
        .ok_or_else(|| RuleError::illegal("no battle in progress"))?;
    let ph = b
        .pending
        .clone()
        .ok_or_else(|| RuleError::illegal("no casualties to select"))?;
    let who = chooser(state, b, ph.side);
    if who != actor {
        return Err(RuleError::NotYourTurn {
            actor,
            expected: who,
        });
    }
    let cand = candidates(state, b, ph.side);
    let (t, ms) = required_casualties(state, &cand, ph.normal, ph.sub);
    let mut seen = ids.to_vec();
    seen.sort_unstable();
    seen.dedup();
    if seen.len() != ids.len() || ids.len() != t {
        return illegal(format!("you must choose exactly {t} distinct casualties"));
    }
    if ids.iter().any(|i| !cand.contains(i)) {
        return illegal("a chosen unit is not eligible");
    }
    let air_chosen = ids
        .iter()
        .filter(|&&i| state.unit(i).is_some_and(|u| u.kind.is_air()))
        .count();
    if air_chosen > (ph.normal as usize).min(t - ms) {
        return illegal("submarine hits cannot be assigned to aircraft");
    }
    let mut b = state.battle.take().unwrap();
    assign(state, &mut b, ph.side, ids, ph.immediate, ev);
    after_selection(state, &mut b, ev);
    state.battle = Some(b);
    advance(state, rng, ev);
    Ok(())
}

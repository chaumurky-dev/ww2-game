//! Taking back movement within a movement phase.
//!
//! When a Combat Move or Non-Combat Move phase begins the engine saves a [`MoveSnapshot`] and
//! journals every successful `Move`, `Load` and `Unload`. `UndoMoves` restores the snapshot and
//! replays the journal entries that were not selected. Replay rolls no dice: a phase in which any
//! move drew antiaircraft fire cannot be undone at all, so nobody gets to re-roll those dice.
use crate::action::{Action, UndoScope};
use crate::dice::rng_from_seed;
use crate::error::{illegal, Result};
use crate::event::Event;
use crate::map::LocationId;
use crate::power::Power;
use crate::state::{GameState, JournalEntry, MoveSnapshot, Phase};
use crate::units::{UnitId, UnitRef};

/// Message returned when the phase contains a move that was shot at.
pub const AA_MESSAGE: &str =
    "units that were fired on by anti-aircraft guns cannot take back their move";

/// Did the events of an action include any die roll (only antiaircraft fire can roll in a move)?
pub(crate) fn rolled_dice(ev: &[Event]) -> bool {
    ev.iter().any(|e| matches!(e, Event::DiceRolled { .. }))
}

/// Called whenever the phase changes: snapshot on entering a movement phase, clear otherwise.
pub(crate) fn phase_entered(state: &mut GameState) {
    state.turn.move_journal.clear();
    state.turn.phase_start = match state.phase {
        Phase::CombatMove | Phase::NonCombatMove => Some(Box::new(MoveSnapshot {
            units: state.units.clone(),
            next_unit_id: state.next_unit_id,
            control: state.control.clone(),
            treasury: state.treasury.clone(),
        })),
        _ => None,
    };
}

fn entry_units(a: &Action) -> &[UnitId] {
    match a {
        Action::Move { units, .. } | Action::Load { units, .. } | Action::Unload { units, .. } => {
            units
        }
        _ => &[],
    }
}

fn entry_dest(state: &GameState, a: &Action) -> Option<LocationId> {
    match a {
        Action::Move { path, .. } => path.last().copied(),
        Action::Unload { to, .. } => Some(*to),
        Action::Load { transport, .. } => state.unit(*transport).map(|t| t.location),
        _ => None,
    }
}

fn with_units(a: &Action, keep: Vec<UnitId>) -> Action {
    match a {
        Action::Move { path, .. } => Action::Move {
            units: keep,
            path: path.clone(),
        },
        Action::Load { transport, .. } => Action::Load {
            units: keep,
            transport: *transport,
        },
        Action::Unload { transport, to, .. } => Action::Unload {
            transport: *transport,
            units: keep,
            to: *to,
        },
        other => other.clone(),
    }
}

fn refs(state: &GameState, ids: &[UnitId]) -> Vec<UnitRef> {
    ids.iter()
        .filter_map(|&i| state.unit(i))
        .map(UnitRef::from)
        .collect()
}

/// Outcome of planning an undo: the resulting state and who was put back.
struct Plan {
    after: GameState,
    removed: Vec<UnitRef>,
    cascaded: Vec<UnitRef>,
}

fn plan(state: &GameState, p: Power, scope: &UndoScope) -> Result<Plan> {
    if !matches!(state.phase, Phase::CombatMove | Phase::NonCombatMove) {
        return illegal("moves can only be taken back during a movement phase");
    }
    let Some(snap) = state.turn.phase_start.as_deref() else {
        return illegal("there is nothing to take back");
    };
    let journal = &state.turn.move_journal;
    if journal.is_empty() {
        return illegal("there is nothing to take back");
    }
    if journal.iter().any(|e| e.aa_fired) {
        return illegal(AA_MESSAGE);
    }
    // Split the journal into entries to keep (possibly with fewer units) and removed units.
    let mut kept: Vec<Action> = vec![];
    let mut removed: Vec<UnitId> = vec![];
    for e in journal {
        let units = entry_units(&e.action);
        let selected: Vec<UnitId> = match scope {
            UndoScope::All => units.to_vec(),
            UndoScope::Units { units: want } => {
                units.iter().copied().filter(|u| want.contains(u)).collect()
            }
            UndoScope::Into { location } => {
                if entry_dest(state, &e.action) == Some(*location) {
                    units.to_vec()
                } else {
                    vec![]
                }
            }
        };
        if selected.is_empty() {
            kept.push(e.action.clone());
        } else {
            let rest: Vec<UnitId> = units
                .iter()
                .copied()
                .filter(|u| !selected.contains(u))
                .collect();
            if !rest.is_empty() {
                kept.push(with_units(&e.action, rest));
            }
            removed.extend(selected);
        }
    }
    if removed.is_empty() {
        return illegal("none of that movement was made this phase");
    }
    removed.sort_unstable();
    removed.dedup();
    let mut after = state.clone();
    after.units = snap.units.clone();
    after.next_unit_id = snap.next_unit_id;
    after.control = snap.control.clone();
    after.treasury = snap.treasury.clone();
    after.turn.move_journal.clear();
    let mut cascaded: Vec<UnitId> = vec![];
    for action in kept {
        let mut trial = after.clone();
        let mut rng = rng_from_seed(0);
        let mut ev = vec![];
        let ok = crate::apply::exec_movement(&mut trial, p, &action, &mut rng, &mut ev).is_ok()
            && !rolled_dice(&ev);
        if ok {
            after = trial;
            after.turn.move_journal.push(JournalEntry {
                action,
                aa_fired: false,
            });
        } else {
            cascaded.extend(entry_units(&action).iter().copied());
        }
    }
    cascaded.sort_unstable();
    cascaded.dedup();
    cascaded.retain(|c| !removed.contains(c));
    Ok(Plan {
        after,
        removed: refs(state, &removed),
        cascaded: refs(state, &cascaded),
    })
}

/// Apply `UndoMoves` for the acting power.
pub(crate) fn undo_moves(
    state: &mut GameState,
    p: Power,
    scope: &UndoScope,
    ev: &mut Vec<Event>,
) -> Result<()> {
    let plan = plan(state, p, scope)?;
    *state = plan.after;
    ev.push(Event::MovesUndone {
        power: p,
        units: plan.removed,
        cascaded: plan.cascaded,
    });
    Ok(())
}

/// Would `UndoMoves` with this scope succeed for the active power right now?
pub fn undoable(state: &GameState, scope: &UndoScope) -> bool {
    state.winner.is_none()
        && state.battle.is_none()
        && plan(state, state.current_power, scope).is_ok()
}

/// Units that stand somewhere other than where this movement phase began, as
/// `(unit, from, to)` sorted by unit id (cargo that rode a ship is included).
pub fn moved_this_phase(state: &GameState) -> Vec<(UnitId, LocationId, LocationId)> {
    let Some(snap) = state.turn.phase_start.as_deref() else {
        return vec![];
    };
    snap.units
        .iter()
        .filter_map(|s| {
            let now = state.unit(s.id)?;
            (now.location != s.location).then_some((s.id, s.location, now.location))
        })
        .collect()
}

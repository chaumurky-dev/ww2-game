use super::*;
use crate::action::UndoScope;
use crate::query::{moved_this_phase, undoable};

/// A movement-phase state; call `snap` once the pieces are placed.
fn phase_state(p: Power, phase: Phase) -> GameState {
    blank(p, phase)
}

/// Take the phase-start snapshot, as entering the phase would.
fn snap(st: &mut GameState) {
    crate::undo::phase_entered(st);
}

fn undo(st: &mut GameState, p: Power, scope: UndoScope) -> Result<Vec<Event>> {
    act(st, p, Action::UndoMoves { scope })
}

fn all() -> UndoScope {
    UndoScope::All
}

#[test]
fn undo_all_restores_units_control_and_treasury() {
    let mut st = phase_state(Power::Germany, Phase::CombatMove);
    st.treasury[Power::Germany] = 10;
    let armor = put(&mut st, UnitType::Armor, Power::Germany, "eastern_europe");
    set_control(&mut st, "ukraine_s_s_r", Some(Power::Ussr));
    put(&mut st, UnitType::Infantry, Power::Ussr, "karelia_s_s_r");
    let inf = put(
        &mut st,
        UnitType::Infantry,
        Power::Germany,
        "finland_norway",
    );
    let walker = put(&mut st, UnitType::Infantry, Power::Germany, "germany");
    snap(&mut st);
    let before = st.clone();
    // blitz an empty enemy territory, violate a neutral, and an ordinary step
    act(
        &mut st,
        Power::Germany,
        mv(&[armor], &["ukraine_s_s_r", "karelia_s_s_r"]),
    )
    .unwrap();
    act(&mut st, Power::Germany, mv(&[inf], &["sweden"])).unwrap();
    act(&mut st, Power::Germany, mv(&[walker], &["western_europe"])).unwrap();
    assert_eq!(st.control[loc("ukraine_s_s_r")], Some(Power::Germany));
    assert_eq!(st.treasury[Power::Germany], 7);
    assert_eq!(moved_this_phase(&st).len(), 3);
    assert!(undoable(&st, &all()));
    let ev = undo(&mut st, Power::Germany, all()).unwrap();
    assert_eq!(ev.len(), 1);
    match &ev[0] {
        Event::MovesUndone {
            power,
            units,
            cascaded,
        } => {
            assert_eq!(*power, Power::Germany);
            assert_eq!(units.len(), 3);
            assert!(cascaded.is_empty());
        }
        e => panic!("unexpected {e:?}"),
    }
    assert_eq!(
        st, before,
        "state, including moves_used, is back to the start"
    );
    assert_eq!(st.control[loc("ukraine_s_s_r")], Some(Power::Ussr));
    assert_eq!(st.unit(armor).unwrap().moves_used, 0);
    assert!(st.is_untouched_neutral(loc("sweden")));
    assert!(moved_this_phase(&st).is_empty());
    // units can move again
    act(&mut st, Power::Germany, mv(&[walker], &["western_europe"])).unwrap();
}

#[test]
fn undo_one_unit_of_a_group_keeps_the_others_moved() {
    let mut st = phase_state(Power::Germany, Phase::CombatMove);
    let ids = putn(&mut st, UnitType::Infantry, Power::Germany, "germany", 3);
    snap(&mut st);
    act(&mut st, Power::Germany, mv(&ids, &["western_europe"])).unwrap();
    undo(
        &mut st,
        Power::Germany,
        UndoScope::Units {
            units: vec![ids[1]],
        },
    )
    .unwrap();
    assert_eq!(st.unit(ids[0]).unwrap().location, loc("western_europe"));
    assert_eq!(st.unit(ids[1]).unwrap().location, loc("germany"));
    assert_eq!(st.unit(ids[1]).unwrap().moves_used, 0);
    assert_eq!(st.unit(ids[2]).unwrap().location, loc("western_europe"));
    assert_eq!(
        moved_this_phase(&st),
        vec![
            (ids[0], loc("germany"), loc("western_europe")),
            (ids[2], loc("germany"), loc("western_europe"))
        ]
    );
    // nothing left to take back for a unit that did not move
    assert!(undo(
        &mut st,
        Power::Germany,
        UndoScope::Units {
            units: vec![ids[1]]
        }
    )
    .is_err());
}

#[test]
fn undo_into_a_territory_only_removes_moves_ending_there() {
    let mut st = phase_state(Power::Germany, Phase::CombatMove);
    let a = put(&mut st, UnitType::Infantry, Power::Germany, "germany");
    let b = put(&mut st, UnitType::Infantry, Power::Germany, "germany");
    let c = put(&mut st, UnitType::Infantry, Power::Germany, "germany");
    snap(&mut st);
    act(&mut st, Power::Germany, mv(&[a], &["western_europe"])).unwrap();
    act(&mut st, Power::Germany, mv(&[b], &["eastern_europe"])).unwrap();
    act(&mut st, Power::Germany, mv(&[c], &["western_europe"])).unwrap();
    undo(
        &mut st,
        Power::Germany,
        UndoScope::Into {
            location: loc("western_europe"),
        },
    )
    .unwrap();
    assert_eq!(st.unit(a).unwrap().location, loc("germany"));
    assert_eq!(st.unit(c).unwrap().location, loc("germany"));
    assert_eq!(st.unit(b).unwrap().location, loc("eastern_europe"));
    assert_eq!(st.turn.move_journal.len(), 1);
}

#[test]
fn undoing_a_transport_move_cascades_to_the_load_that_needed_it() {
    let mut st = phase_state(Power::Uk, Phase::NonCombatMove);
    let t = put(&mut st, UnitType::Transport, Power::Uk, "sz_east_canada");
    let ids = putn(&mut st, UnitType::Infantry, Power::Uk, "united_kingdom", 2);
    snap(&mut st);
    act(&mut st, Power::Uk, mv(&[t], &["sz_north"])).unwrap();
    act(
        &mut st,
        Power::Uk,
        Action::Load {
            units: ids.clone(),
            transport: t,
        },
    )
    .unwrap();
    let ev = undo(&mut st, Power::Uk, UndoScope::Units { units: vec![t] }).unwrap();
    match &ev[0] {
        Event::MovesUndone {
            units, cascaded, ..
        } => {
            assert_eq!(units.iter().map(|u| u.id).collect::<Vec<_>>(), vec![t]);
            let mut c: Vec<_> = cascaded.iter().map(|u| u.id).collect();
            c.sort_unstable();
            assert_eq!(c, ids);
        }
        e => panic!("unexpected {e:?}"),
    }
    assert_eq!(st.unit(t).unwrap().location, loc("sz_east_canada"));
    assert_eq!(st.unit(ids[0]).unwrap().loaded_on, None);
    assert!(st.turn.move_journal.is_empty());
}

#[test]
fn undoing_a_load_cascades_to_its_unload() {
    let mut st = phase_state(Power::Uk, Phase::NonCombatMove);
    let t = put(&mut st, UnitType::Transport, Power::Uk, "sz_north");
    let i = put(&mut st, UnitType::Infantry, Power::Uk, "united_kingdom");
    snap(&mut st);
    act(
        &mut st,
        Power::Uk,
        Action::Load {
            units: vec![i],
            transport: t,
        },
    )
    .unwrap();
    act(
        &mut st,
        Power::Uk,
        Action::Unload {
            transport: t,
            units: vec![i],
            to: loc("united_kingdom"),
        },
    )
    .unwrap();
    // Taking back the unload alone leaves the unit aboard.
    undo(
        &mut st,
        Power::Uk,
        UndoScope::Into {
            location: loc("united_kingdom"),
        },
    )
    .unwrap();
    assert_eq!(st.unit(i).unwrap().loaded_on, Some(t));
    assert!(!st.unit(t).unwrap().frozen);
    assert_eq!(st.turn.move_journal.len(), 1);
}

#[test]
fn undo_is_illegal_out_of_phase_for_the_wrong_power_and_with_nothing_to_undo() {
    let mut st = phase_state(Power::Germany, Phase::CombatMove);
    let i = put(&mut st, UnitType::Infantry, Power::Germany, "germany");
    snap(&mut st);
    assert!(matches!(
        undo(&mut st, Power::Germany, all()),
        Err(RuleError::Illegal(_))
    ));
    act(&mut st, Power::Germany, mv(&[i], &["western_europe"])).unwrap();
    assert!(matches!(
        undo(&mut st, Power::Uk, all()),
        Err(RuleError::NotYourTurn { .. })
    ));
    assert_eq!(st.unit(i).unwrap().location, loc("western_europe"));
    for ph in [Phase::DevelopWeapons, Phase::Purchase, Phase::Mobilize] {
        let mut o = phase_state(Power::Germany, ph);
        assert!(matches!(
            undo(&mut o, Power::Germany, all()),
            Err(RuleError::WrongPhase { .. })
        ));
        assert!(!undoable(&o, &all()));
    }
    // A battle in progress blocks it too (Combat phase).
    let mut o = phase_state(Power::Germany, Phase::Combat);
    assert!(undo(&mut o, Power::Germany, all()).is_err());
    // No snapshot (a state saved before this feature): nothing to undo.
    let mut old = phase_state(Power::Germany, Phase::CombatMove);
    let j = put(&mut old, UnitType::Infantry, Power::Germany, "germany");
    act(&mut old, Power::Germany, mv(&[j], &["western_europe"])).unwrap();
    assert!(old.turn.move_journal.is_empty());
    assert!(undo(&mut old, Power::Germany, all()).is_err());
}

#[test]
fn a_move_that_drew_aa_fire_cannot_be_taken_back() {
    let mut st = phase_state(Power::Germany, Phase::CombatMove);
    let f = put(&mut st, UnitType::Fighter, Power::Germany, "germany");
    let w = put(&mut st, UnitType::Infantry, Power::Germany, "germany");
    set_control(&mut st, "western_europe", Some(Power::Uk));
    put(&mut st, UnitType::AaGun, Power::Uk, "western_europe");
    snap(&mut st);
    act(&mut st, Power::Germany, mv(&[w], &["eastern_europe"])).unwrap();
    // The fighter survives (no 1 rolled) but dice were rolled all the same.
    let ev = act_seed(
        &mut st,
        Power::Germany,
        mv(&[f], &["western_europe", "germany"]),
        seed_for(&[4]),
    )
    .unwrap();
    assert!(ev.iter().any(|e| matches!(e, Event::DiceRolled { .. })));
    assert!(st.unit(f).is_some());
    assert!(st.turn.move_journal.last().unwrap().aa_fired);
    let before = st.clone();
    for scope in [
        all(),
        UndoScope::Units { units: vec![w] },
        UndoScope::Units { units: vec![f] },
        UndoScope::Into {
            location: loc("eastern_europe"),
        },
    ] {
        assert!(!undoable(&st, &scope));
        match undo(&mut st, Power::Germany, scope) {
            Err(RuleError::Illegal(m)) => assert!(m.contains("anti-aircraft"), "{m}"),
            r => panic!("expected Illegal, got {r:?}"),
        }
    }
    assert_eq!(st, before);
}

#[test]
fn journal_clears_when_the_phase_ends_and_json_carries_no_snapshot_in_combat() {
    let mut st = phase_state(Power::Germany, Phase::Purchase);
    st.units.clear();
    let a = put(&mut st, UnitType::Infantry, Power::Germany, "germany");
    let b = put(
        &mut st,
        UnitType::Infantry,
        Power::Germany,
        "western_europe",
    );
    set_control(&mut st, "eastern_europe", Some(Power::Ussr));
    put(&mut st, UnitType::Infantry, Power::Ussr, "eastern_europe");
    assert!(st.turn.phase_start.is_none());
    act(&mut st, Power::Germany, Action::EndPhase).unwrap();
    assert_eq!(st.phase, Phase::CombatMove);
    assert!(st.turn.phase_start.is_some());
    assert!(st.turn.move_journal.is_empty());
    act(&mut st, Power::Germany, mv(&[a], &["eastern_europe"])).unwrap();
    act(&mut st, Power::Germany, mv(&[b], &["germany"])).unwrap();
    assert_eq!(st.turn.move_journal.len(), 2);
    let json = serde_json::to_string(&st).unwrap();
    assert!(json.contains("phase_start") && json.contains("move_journal"));
    act(&mut st, Power::Germany, Action::EndPhase).unwrap();
    assert_eq!(st.phase, Phase::Combat);
    assert!(st.turn.phase_start.is_none());
    assert!(st.turn.move_journal.is_empty());
    let json = serde_json::to_string(&st).unwrap();
    assert!(!json.contains("phase_start") && !json.contains("move_journal"));
    assert!(undo(&mut st, Power::Germany, all()).is_err());
    // After combat the non-combat phase starts with a fresh snapshot and journal.
    act(
        &mut st,
        Power::Germany,
        Action::BeginBattle {
            location: loc("eastern_europe"),
        },
    )
    .unwrap();
    for i in 0..200 {
        if st.phase != Phase::Combat {
            break;
        }
        let (who, a) = match pending_decision(&st) {
            Decision::SelectCasualties {
                power,
                eligible,
                hits,
                ..
            } => (
                power,
                Action::SelectCasualties {
                    units: eligible.into_iter().take(hits as usize).collect(),
                },
            ),
            Decision::RetreatOrPress { power, .. } => (power, Action::PressAttack),
            Decision::SubWithdrawal { power, .. } => (
                power,
                Action::WithdrawSubs {
                    subs: vec![],
                    to: None,
                },
            ),
            _ => (Power::Germany, Action::EndPhase),
        };
        act_seed(&mut st, who, a, i).unwrap();
    }
    assert_eq!(st.phase, Phase::NonCombatMove);
    assert!(st.turn.phase_start.is_some() && st.turn.move_journal.is_empty());
}

#[test]
fn a_two_step_move_by_the_same_unit_undoes_to_the_original_location() {
    let mut st = phase_state(Power::Germany, Phase::CombatMove);
    let t = put(&mut st, UnitType::Transport, Power::Germany, "sz_baltic");
    snap(&mut st);
    act(&mut st, Power::Germany, mv(&[t], &["sz_north"])).unwrap();
    act(&mut st, Power::Germany, mv(&[t], &["sz_east_canada"])).unwrap();
    assert_eq!(st.turn.move_journal.len(), 2);
    assert_eq!(
        moved_this_phase(&st),
        vec![(t, loc("sz_baltic"), loc("sz_east_canada"))]
    );
    undo(&mut st, Power::Germany, UndoScope::Units { units: vec![t] }).unwrap();
    assert_eq!(st.unit(t).unwrap().location, loc("sz_baltic"));
    assert_eq!(st.unit(t).unwrap().moves_used, 0);
    assert!(st.turn.move_journal.is_empty());
}

#[test]
fn state_with_a_journal_round_trips_through_json() {
    let mut st = phase_state(Power::Germany, Phase::CombatMove);
    let i = put(&mut st, UnitType::Infantry, Power::Germany, "germany");
    snap(&mut st);
    act(&mut st, Power::Germany, mv(&[i], &["western_europe"])).unwrap();
    let json = serde_json::to_string(&st).unwrap();
    let back: GameState = serde_json::from_str(&json).unwrap();
    assert_eq!(back, st);
    // and the restored state can still undo
    let mut back = back;
    undo(&mut back, Power::Germany, all()).unwrap();
    assert_eq!(back.unit(i).unwrap().location, loc("germany"));
    // an old stored state without the new fields still loads
    let mut v: serde_json::Value =
        serde_json::to_value(GameState::new(GameSettings::default())).unwrap();
    let turn = v["turn"].as_object_mut().unwrap();
    turn.remove("phase_start");
    turn.remove("move_journal");
    let old: GameState = serde_json::from_value(v).unwrap();
    assert!(old.turn.phase_start.is_none() && old.turn.move_journal.is_empty());
}

#[test]
fn action_and_event_json_shapes() {
    let a = Action::UndoMoves {
        scope: UndoScope::All,
    };
    assert_eq!(
        serde_json::to_string(&a).unwrap(),
        r#"{"type":"undo_moves","scope":{"type":"all"}}"#
    );
    let a: Action =
        serde_json::from_str(r#"{"type":"undo_moves","scope":{"type":"units","units":[3,4]}}"#)
            .unwrap();
    assert_eq!(
        a,
        Action::UndoMoves {
            scope: UndoScope::Units { units: vec![3, 4] }
        }
    );
    let a: Action = serde_json::from_str(
        r#"{"type":"undo_moves","scope":{"type":"into","location":"germany"}}"#,
    )
    .unwrap();
    assert_eq!(
        a,
        Action::UndoMoves {
            scope: UndoScope::Into {
                location: loc("germany")
            }
        }
    );
}

#[test]
fn replay_with_an_undo_in_the_log_reproduces_the_state() {
    let settings = GameSettings::default();
    let mut st = GameState::new(settings.clone());
    let mut log: Vec<(Power, Action, u64)> = vec![];
    let mut step = |st: &mut GameState, a: Action| {
        let seed = 100 + log.len() as u64;
        let p = st.current_power;
        apply(st, p, &a, seed).unwrap();
        log.push((p, a, seed));
    };
    step(&mut st, Action::EndPhase); // research -> purchase
    step(&mut st, Action::EndPhase); // purchase -> combat move
    let p = st.current_power;
    let movers: Vec<(UnitId, LocationId)> = st
        .units
        .iter()
        .filter(|u| u.owner == p && u.kind == UnitType::Infantry)
        .filter_map(|u| {
            crate::query::reachable(&st, u.id)
                .into_iter()
                .find(|r| !r.battle)
                .map(|r| (u.id, r.to))
        })
        .take(2)
        .collect();
    assert_eq!(movers.len(), 2);
    let m = |(id, to): (UnitId, LocationId)| Action::Move {
        units: vec![id],
        path: vec![to],
    };
    step(&mut st, m(movers[0]));
    step(&mut st, m(movers[1]));
    step(
        &mut st,
        Action::UndoMoves {
            scope: UndoScope::Units {
                units: vec![movers[0].0],
            },
        },
    );
    step(&mut st, Action::EndPhase);
    let again = replay(settings, &log).unwrap();
    assert_eq!(again, st);
    assert_eq!(st.unit(movers[0].0).unwrap().moves_used, 0);
    assert_eq!(st.unit(movers[1].0).unwrap().location, movers[1].1);
}

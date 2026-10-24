use super::*;
use crate::decision::Decision;
use crate::event::{BattleOutcome, DiceContext};

/// Germany attacks Ukraine from Eastern Europe.
fn ukraine_setup() -> GameState {
    let mut st = blank(Power::Germany, Phase::CombatMove);
    set_control(&mut st, "ukraine_s_s_r", Some(Power::Ussr));
    st
}

fn end_cm(st: &mut GameState, p: Power) {
    act(st, p, Action::EndPhase).unwrap();
}

fn outcome(ev: &[Event]) -> Option<BattleOutcome> {
    ev.iter().rev().find_map(|e| match e {
        Event::BattleEnded { outcome, .. } => Some(*outcome),
        _ => None,
    })
}

#[test]
fn simple_land_battle_attacker_wins_and_captures() {
    let mut st = ukraine_setup();
    let a = put(
        &mut st,
        UnitType::Infantry,
        Power::Germany,
        "eastern_europe",
    );
    put(&mut st, UnitType::Infantry, Power::Ussr, "ukraine_s_s_r");
    act(&mut st, Power::Germany, mv(&[a], &["ukraine_s_s_r"])).unwrap();
    end_cm(&mut st, Power::Germany);
    assert_eq!(st.phase, Phase::Combat);
    assert_eq!(
        crate::battle::pending_battles(&st),
        vec![loc("ukraine_s_s_r")]
    );
    // attacker rolls 1 (hit on 1), defender rolls 6 (miss)
    let ev = act_seed(
        &mut st,
        Power::Germany,
        Action::BeginBattle {
            location: loc("ukraine_s_s_r"),
        },
        seed_for(&[1, 6]),
    )
    .unwrap();
    assert_eq!(outcome(&ev), Some(BattleOutcome::AttackerWins));
    assert_eq!(st.control[loc("ukraine_s_s_r")], Some(Power::Germany));
    assert!(st.battle.is_none());
    assert_eq!(st.phase, Phase::NonCombatMove, "combat ends automatically");
    assert!(ev
        .iter()
        .any(|e| matches!(e, Event::TerritoryCaptured { .. })));
    let rolls: Vec<_> = ev
        .iter()
        .filter(|e| matches!(e, Event::DiceRolled { .. }))
        .collect();
    assert_eq!(rolls.len(), 2);
}

#[test]
fn mutual_destruction_defender_holds() {
    let mut st = ukraine_setup();
    let a = put(
        &mut st,
        UnitType::Infantry,
        Power::Germany,
        "eastern_europe",
    );
    let d = put(&mut st, UnitType::Infantry, Power::Ussr, "ukraine_s_s_r");
    act(&mut st, Power::Germany, mv(&[a], &["ukraine_s_s_r"])).unwrap();
    end_cm(&mut st, Power::Germany);
    let ev = act_seed(
        &mut st,
        Power::Germany,
        Action::BeginBattle {
            location: loc("ukraine_s_s_r"),
        },
        seed_for(&[1, 2]),
    )
    .unwrap();
    assert_eq!(outcome(&ev), Some(BattleOutcome::DefenderWins));
    assert!(st.unit(a).is_none() && st.unit(d).is_none());
    assert_eq!(st.control[loc("ukraine_s_s_r")], Some(Power::Ussr));
}

#[test]
fn casualties_are_simultaneous_and_defender_hit_units_still_fire() {
    let mut st = ukraine_setup();
    let a = put(
        &mut st,
        UnitType::Infantry,
        Power::Germany,
        "eastern_europe",
    );
    put(&mut st, UnitType::Infantry, Power::Ussr, "ukraine_s_s_r");
    act(&mut st, Power::Germany, mv(&[a], &["ukraine_s_s_r"])).unwrap();
    end_cm(&mut st, Power::Germany);
    let ev = act_seed(
        &mut st,
        Power::Germany,
        Action::BeginBattle {
            location: loc("ukraine_s_s_r"),
        },
        seed_for(&[1, 2]),
    )
    .unwrap();
    // both rolled, attacker infantry died although defender infantry was hit too
    assert!(ev.iter().any(|e| matches!(
        e,
        Event::DiceRolled {
            context: DiceContext::Defense { .. },
            ..
        }
    )));
}

#[test]
fn defender_chooses_casualties_when_it_matters() {
    let mut st = ukraine_setup();
    let atk = putn(
        &mut st,
        UnitType::Infantry,
        Power::Germany,
        "eastern_europe",
        2,
    );
    let inf = put(&mut st, UnitType::Infantry, Power::Ussr, "ukraine_s_s_r");
    let arm = put(&mut st, UnitType::Armor, Power::Ussr, "ukraine_s_s_r");
    act(&mut st, Power::Germany, mv(&atk, &["ukraine_s_s_r"])).unwrap();
    end_cm(&mut st, Power::Germany);
    // attackers roll [1,6] -> 1 hit; defenders roll [6,6]
    act_seed(
        &mut st,
        Power::Germany,
        Action::BeginBattle {
            location: loc("ukraine_s_s_r"),
        },
        seed_for(&[1, 6, 6, 6]),
    )
    .unwrap();
    match crate::pending_decision(&st) {
        Decision::SelectCasualties {
            power,
            hits,
            eligible,
            ..
        } => {
            assert_eq!(power, Power::Ussr);
            assert_eq!(hits, 1);
            assert_eq!(eligible.len(), 2);
        }
        d => panic!("unexpected {d:?}"),
    }
    // wrong actor / wrong count rejected
    assert!(act(
        &mut st,
        Power::Germany,
        Action::SelectCasualties { units: vec![inf] }
    )
    .is_err());
    assert!(act(
        &mut st,
        Power::Ussr,
        Action::SelectCasualties {
            units: vec![inf, arm]
        }
    )
    .is_err());
    assert!(act(
        &mut st,
        Power::Ussr,
        Action::SelectCasualties { units: vec![9999] }
    )
    .is_err());
    act(
        &mut st,
        Power::Ussr,
        Action::SelectCasualties { units: vec![arm] },
    )
    .unwrap();
    assert!(st.unit(arm).is_none() && st.unit(inf).is_some());
    // now attacker must choose retreat or press
    assert!(matches!(
        crate::pending_decision(&st),
        Decision::RetreatOrPress {
            power: Power::Germany,
            ..
        }
    ));
}

#[test]
fn retreat_moves_attackers_back_and_ends_battle() {
    let mut st = ukraine_setup();
    let atk = putn(
        &mut st,
        UnitType::Infantry,
        Power::Germany,
        "eastern_europe",
        2,
    );
    putn(&mut st, UnitType::Infantry, Power::Ussr, "ukraine_s_s_r", 2);
    act(&mut st, Power::Germany, mv(&atk, &["ukraine_s_s_r"])).unwrap();
    end_cm(&mut st, Power::Germany);
    // nobody hits
    act_seed(
        &mut st,
        Power::Germany,
        Action::BeginBattle {
            location: loc("ukraine_s_s_r"),
        },
        seed_for(&[6, 6, 6, 6]),
    )
    .unwrap();
    assert!(matches!(
        crate::pending_decision(&st),
        Decision::RetreatOrPress { .. }
    ));
    assert!(act(
        &mut st,
        Power::Germany,
        Action::Retreat { to: loc("germany") }
    )
    .is_err());
    let ev = act(
        &mut st,
        Power::Germany,
        Action::Retreat {
            to: loc("eastern_europe"),
        },
    )
    .unwrap();
    assert_eq!(outcome(&ev), Some(BattleOutcome::Retreated));
    assert!(atk
        .iter()
        .all(|&a| st.unit(a).unwrap().location == loc("eastern_europe")));
    assert_eq!(st.control[loc("ukraine_s_s_r")], Some(Power::Ussr));
}

#[test]
fn press_attack_fights_another_round() {
    let mut st = ukraine_setup();
    let atk = putn(
        &mut st,
        UnitType::Infantry,
        Power::Germany,
        "eastern_europe",
        2,
    );
    putn(&mut st, UnitType::Infantry, Power::Ussr, "ukraine_s_s_r", 2);
    act(&mut st, Power::Germany, mv(&atk, &["ukraine_s_s_r"])).unwrap();
    end_cm(&mut st, Power::Germany);
    act_seed(
        &mut st,
        Power::Germany,
        Action::BeginBattle {
            location: loc("ukraine_s_s_r"),
        },
        seed_for(&[6, 6, 6, 6]),
    )
    .unwrap();
    let ev = act(&mut st, Power::Germany, Action::PressAttack).unwrap();
    assert!(ev
        .iter()
        .any(|e| matches!(e, Event::BattleRound { round: 2, .. })));
}

#[test]
fn cannot_end_combat_with_pending_battle_or_battle_in_progress() {
    let mut st = ukraine_setup();
    let a = put(
        &mut st,
        UnitType::Infantry,
        Power::Germany,
        "eastern_europe",
    );
    putn(&mut st, UnitType::Infantry, Power::Ussr, "ukraine_s_s_r", 2);
    act(&mut st, Power::Germany, mv(&[a], &["ukraine_s_s_r"])).unwrap();
    end_cm(&mut st, Power::Germany);
    assert!(act(&mut st, Power::Germany, Action::EndPhase).is_err());
    act_seed(
        &mut st,
        Power::Germany,
        Action::BeginBattle {
            location: loc("ukraine_s_s_r"),
        },
        seed_for(&[6, 6, 6]),
    )
    .unwrap();
    assert!(act(&mut st, Power::Germany, Action::EndPhase).is_err());
    assert!(act(
        &mut st,
        Power::Germany,
        Action::BeginBattle {
            location: loc("ukraine_s_s_r")
        }
    )
    .is_err());
}

#[test]
fn air_only_victory_does_not_capture() {
    let mut st = ukraine_setup();
    let f = put(&mut st, UnitType::Fighter, Power::Germany, "eastern_europe");
    put(&mut st, UnitType::Infantry, Power::Ussr, "ukraine_s_s_r");
    act(&mut st, Power::Germany, mv(&[f], &["ukraine_s_s_r"])).unwrap();
    end_cm(&mut st, Power::Germany);
    // fighter hits on 3; defender infantry misses
    act_seed(
        &mut st,
        Power::Germany,
        Action::BeginBattle {
            location: loc("ukraine_s_s_r"),
        },
        seed_for(&[3, 6]),
    )
    .unwrap();
    assert_eq!(st.control[loc("ukraine_s_s_r")], Some(Power::Ussr));
    assert!(st.unit(f).is_some());
    assert_eq!(st.phase, Phase::NonCombatMove);
}

#[test]
fn defending_fighter_and_bomber_defense_numbers() {
    let mut st = ukraine_setup();
    let atk = putn(
        &mut st,
        UnitType::Infantry,
        Power::Germany,
        "eastern_europe",
        3,
    );
    let f = put(&mut st, UnitType::Fighter, Power::Ussr, "ukraine_s_s_r");
    act(&mut st, Power::Germany, mv(&atk, &["ukraine_s_s_r"])).unwrap();
    end_cm(&mut st, Power::Germany);
    // attackers miss (6,6,6); fighter rolls 4 -> hit
    let ev = act_seed(
        &mut st,
        Power::Germany,
        Action::BeginBattle {
            location: loc("ukraine_s_s_r"),
        },
        seed_for(&[6, 6, 6, 4]),
    )
    .unwrap();
    assert!(ev.iter().any(|e| matches!(
        e,
        Event::HitsScored {
            hits: 1,
            by: Power::Ussr,
            ..
        }
    )));
    assert!(st.unit(f).is_some());
}

#[test]
fn jet_power_defends_at_five() {
    let mut st = ukraine_setup();
    st.techs[Power::Ussr].push(Tech::JetPower);
    let atk = putn(
        &mut st,
        UnitType::Infantry,
        Power::Germany,
        "eastern_europe",
        3,
    );
    put(&mut st, UnitType::Fighter, Power::Ussr, "ukraine_s_s_r");
    act(&mut st, Power::Germany, mv(&atk, &["ukraine_s_s_r"])).unwrap();
    end_cm(&mut st, Power::Germany);
    let ev = act_seed(
        &mut st,
        Power::Germany,
        Action::BeginBattle {
            location: loc("ukraine_s_s_r"),
        },
        seed_for(&[6, 6, 6, 5]),
    )
    .unwrap();
    assert!(ev.iter().any(|e| matches!(
        e,
        Event::HitsScored {
            hits: 1,
            by: Power::Ussr,
            ..
        }
    )));
}

#[test]
fn heavy_bombers_roll_three_dice() {
    let mut st = ukraine_setup();
    st.techs[Power::Germany].push(Tech::HeavyBombers);
    let b = put(&mut st, UnitType::Bomber, Power::Germany, "eastern_europe");
    putn(&mut st, UnitType::Infantry, Power::Ussr, "ukraine_s_s_r", 3);
    act(&mut st, Power::Germany, mv(&[b], &["ukraine_s_s_r"])).unwrap();
    end_cm(&mut st, Power::Germany);
    let ev = act(
        &mut st,
        Power::Germany,
        Action::BeginBattle {
            location: loc("ukraine_s_s_r"),
        },
    )
    .unwrap();
    let n = ev.iter().find_map(|e| match e {
        Event::DiceRolled {
            context: DiceContext::Attack { .. },
            rolls,
            ..
        } => Some(rolls.len()),
        _ => None,
    });
    assert_eq!(n, Some(3));
}

#[test]
fn only_aa_and_complex_defenders_means_no_battle() {
    let mut st = ukraine_setup();
    let i = put(
        &mut st,
        UnitType::Infantry,
        Power::Germany,
        "eastern_europe",
    );
    put(&mut st, UnitType::AaGun, Power::Ussr, "ukraine_s_s_r");
    act(&mut st, Power::Germany, mv(&[i], &["ukraine_s_s_r"])).unwrap();
    assert_eq!(st.control[loc("ukraine_s_s_r")], Some(Power::Germany));
    end_cm(&mut st, Power::Germany);
    assert_eq!(st.phase, Phase::NonCombatMove);
}

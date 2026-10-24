use super::*;
use crate::state::VictoryReason;

// ---------- turn flow and income ----------

#[test]
fn full_turn_flow_collects_income_and_passes_turn() {
    let mut st = GameState::new(GameSettings::default());
    let mut phases = vec![st.phase];
    for _ in 0..8 {
        if st.current_power != Power::Ussr {
            break;
        }
        act(&mut st, Power::Ussr, Action::EndPhase).unwrap();
        phases.push(st.phase);
    }
    assert_eq!(
        &phases[..6],
        &[
            Phase::DevelopWeapons,
            Phase::Purchase,
            Phase::CombatMove,
            Phase::NonCombatMove,
            Phase::Mobilize,
            Phase::DevelopWeapons
        ]
    );
    assert_eq!(st.current_power, Power::Germany);
    assert_eq!(st.treasury[Power::Ussr], 48, "24 + 24 income");
}

#[test]
fn round_advances_after_usa_and_economic_victory_is_checked_then() {
    let mut st = GameState::new(GameSettings {
        weapons_development: false,
        ..Default::default()
    });
    assert_eq!(st.phase, Phase::Purchase);
    for _ in 0..5 {
        let p = st.current_power;
        for _ in 0..4 {
            act(&mut st, p, Action::EndPhase).unwrap();
        }
    }
    assert_eq!(st.round, 2);
    assert_eq!(st.current_power, Power::Ussr);
}

#[test]
fn eliminated_power_turn_is_run_trivially() {
    let mut st = blank(Power::Germany, Phase::Mobilize);
    put(&mut st, UnitType::Infantry, Power::Germany, "germany");
    put(&mut st, UnitType::Infantry, Power::Uk, "united_kingdom");
    for t in crate::MapData::get().territories() {
        if st.control[t] == Some(Power::Japan) {
            st.control[t] = None;
        }
    }
    // Germany ends -> UK; then UK ends -> Japan is skipped -> USA
    act(&mut st, Power::Germany, Action::EndPhase).unwrap();
    assert_eq!(st.current_power, Power::Uk);
    st.phase = Phase::Mobilize;
    let ev = act(&mut st, Power::Uk, Action::EndPhase).unwrap();
    assert_eq!(st.current_power, Power::Usa);
    assert!(ev.iter().any(|e| matches!(
        e,
        Event::TurnStarted {
            power: Power::Japan,
            ..
        }
    )));
    assert!(ev.iter().any(|e| matches!(
        e,
        Event::IncomeCollected {
            power: Power::Japan,
            amount: 0,
            ..
        }
    )));
}

#[test]
fn production_level_and_income_follow_control() {
    let mut st = GameState::new(GameSettings::default());
    assert_eq!(production_level(&st, Power::Usa), 36);
    st.control[loc("brazil")] = Some(Power::Germany);
    assert_eq!(production_level(&st, Power::Usa), 33);
    assert_eq!(production_level(&st, Power::Germany), 35);
    st.control[loc("eastern_usa")] = Some(Power::Japan);
    assert_eq!(income(&st, Power::Usa), 0);
    assert_eq!(production_level(&st, Power::Usa), 21);
}

// ---------- capitals / liberation ----------

#[test]
fn capital_capture_seizes_treasury_and_zeroes_income() {
    let mut st = blank(Power::Germany, Phase::CombatMove);
    st.treasury[Power::Ussr] = 40;
    st.treasury[Power::Germany] = 5;
    let a = put(&mut st, UnitType::Armor, Power::Germany, "karelia_s_s_r");
    st.control[loc("karelia_s_s_r")] = Some(Power::Germany);
    st.turn.start_control = st.control.clone();
    put(&mut st, UnitType::Infantry, Power::Ussr, "russia");
    act(&mut st, Power::Germany, mv(&[a], &["russia"])).unwrap();
    act(&mut st, Power::Germany, Action::EndPhase).unwrap();
    let ev = act_seed(
        &mut st,
        Power::Germany,
        Action::BeginBattle {
            location: loc("russia"),
        },
        seed_for(&[3, 6]),
    )
    .unwrap();
    assert_eq!(st.control[loc("russia")], Some(Power::Germany));
    assert_eq!(st.treasury[Power::Ussr], 0);
    assert_eq!(st.treasury[Power::Germany], 45);
    assert!(ev.iter().any(|e| matches!(
        e,
        Event::CapitalCaptured {
            capital_of: Power::Ussr,
            ipc_seized: 40,
            ..
        }
    )));
    assert_eq!(income(&st, Power::Ussr), 0);
}

#[test]
fn liberating_capital_returns_territory_but_not_ipcs() {
    let mut st = blank(Power::Uk, Phase::CombatMove);
    st.control[loc("russia")] = Some(Power::Germany);
    st.turn.start_control = st.control.clone();
    st.treasury[Power::Germany] = 77;
    let a = put(&mut st, UnitType::Armor, Power::Uk, "caucasus");
    put(&mut st, UnitType::Infantry, Power::Germany, "russia");
    st.control[loc("caucasus")] = Some(Power::Uk);
    st.turn.start_control = st.control.clone();
    act(&mut st, Power::Uk, mv(&[a], &["russia"])).unwrap();
    act(&mut st, Power::Uk, Action::EndPhase).unwrap();
    act_seed(
        &mut st,
        Power::Uk,
        Action::BeginBattle {
            location: loc("russia"),
        },
        seed_for(&[3, 6]),
    )
    .unwrap();
    assert_eq!(
        st.control[loc("russia")],
        Some(Power::Ussr),
        "returns to USSR"
    );
    assert_eq!(
        st.treasury[Power::Germany],
        77,
        "no seizure when liberating"
    );
    assert_eq!(st.treasury[Power::Ussr], 24, "IPCs are not returned");
}

#[test]
fn liberator_keeps_territory_while_owners_capital_is_held_then_it_reverts() {
    let mut st = blank(Power::Uk, Phase::CombatMove);
    st.control[loc("russia")] = Some(Power::Germany);
    st.control[loc("karelia_s_s_r")] = Some(Power::Germany);
    let a = put(&mut st, UnitType::Infantry, Power::Uk, "caucasus");
    st.control[loc("caucasus")] = Some(Power::Uk);
    put(&mut st, UnitType::Infantry, Power::Germany, "karelia_s_s_r");
    put(
        &mut st,
        UnitType::IndustrialComplex,
        Power::Germany,
        "karelia_s_s_r",
    );
    st.turn.start_control = st.control.clone();
    act(&mut st, Power::Uk, mv(&[a], &["karelia_s_s_r"])).unwrap();
    act(&mut st, Power::Uk, Action::EndPhase).unwrap();
    act_seed(
        &mut st,
        Power::Uk,
        Action::BeginBattle {
            location: loc("karelia_s_s_r"),
        },
        seed_for(&[1, 6]),
    )
    .unwrap();
    assert_eq!(
        st.control[loc("karelia_s_s_r")],
        Some(Power::Uk),
        "USSR capital is held: UK keeps it"
    );
    // now UK/US later free Russia: simulate directly
    let mut ev = vec![];
    crate::economy::capture_territory(&mut st, Power::Usa, loc("russia"), &mut ev);
    assert_eq!(st.control[loc("russia")], Some(Power::Ussr));
    assert_eq!(
        st.control[loc("karelia_s_s_r")],
        Some(Power::Ussr),
        "reverts when capital freed"
    );
    assert!(ev.iter().any(|e| matches!(
        e,
        Event::ControlRestored {
            power: Power::Ussr,
            ..
        }
    )));
    assert!(st
        .units_at(loc("karelia_s_s_r"))
        .filter(|u| u.kind == UnitType::IndustrialComplex)
        .all(|u| u.owner == Power::Ussr));
}

#[test]
fn ally_cannot_capture_ally_territory_and_neutral_never_liberated() {
    let mut st = blank(Power::Usa, Phase::CombatMove);
    let i = put(&mut st, UnitType::Infantry, Power::Usa, "eastern_usa");
    // Western USA is American; Eastern Canada is British-held: friendly, move allowed (not a capture)
    act(&mut st, Power::Usa, mv(&[i], &["eastern_canada"])).unwrap();
    assert_eq!(st.control[loc("eastern_canada")], Some(Power::Uk));
    let mut ev = vec![];
    st.control[loc("sweden")] = Some(Power::Germany);
    crate::economy::capture_territory(&mut st, Power::Uk, loc("sweden"), &mut ev);
    assert_eq!(
        st.control[loc("sweden")],
        Some(Power::Uk),
        "no original owner: plain capture"
    );
}

#[test]
fn aa_gun_captured_works_for_capturer_but_not_this_turn() {
    let mut st = blank(Power::Germany, Phase::CombatMove);
    let a = put(
        &mut st,
        UnitType::Infantry,
        Power::Germany,
        "eastern_europe",
    );
    set_control(&mut st, "ukraine_s_s_r", Some(Power::Ussr));
    let g = put(&mut st, UnitType::AaGun, Power::Ussr, "ukraine_s_s_r");
    act(&mut st, Power::Germany, mv(&[a], &["ukraine_s_s_r"])).unwrap();
    assert_eq!(st.unit(g).unwrap().owner, Power::Germany);
    st.phase = Phase::NonCombatMove;
    assert!(
        act(&mut st, Power::Germany, mv(&[g], &["eastern_europe"])).is_err(),
        "cannot move on capture turn"
    );
}

// ---------- victory ----------

fn finish_turn(st: &mut GameState, p: Power) -> Vec<Event> {
    st.current_power = p;
    st.phase = Phase::Mobilize;
    act(st, p, Action::EndPhase).unwrap()
}

#[test]
fn allied_military_victory() {
    let mut st = GameState::new(GameSettings::default());
    st.control[loc("germany")] = Some(Power::Uk);
    let ev = finish_turn(&mut st, Power::Uk);
    assert!(st.winner.is_none(), "Japan still holds its capital");
    assert!(!ev.iter().any(|e| matches!(e, Event::GameOver { .. })));
    st.control[loc("japan")] = Some(Power::Usa);
    let ev = finish_turn(&mut st, Power::Usa);
    assert_eq!(st.winner.map(|w| w.team), Some(Team::Allies));
    assert!(ev.iter().any(|e| matches!(
        e,
        Event::GameOver {
            winner: Team::Allies,
            reason: VictoryReason::AxisCapitalsHeld
        }
    )));
    assert!(matches!(
        act(&mut st, Power::Ussr, Action::EndPhase),
        Err(RuleError::GameOver)
    ));
    assert!(matches!(
        crate::pending_decision(&st),
        Decision::GameOver {
            winner: Some(Team::Allies)
        }
    ));
}

use crate::decision::Decision;

#[test]
fn axis_military_victory_needs_two_of_three_capitals() {
    let mut st = GameState::new(GameSettings::default());
    st.control[loc("russia")] = Some(Power::Germany);
    finish_turn(&mut st, Power::Germany);
    assert!(st.winner.is_none());
    st.control[loc("united_kingdom")] = Some(Power::Germany);
    finish_turn(&mut st, Power::Germany);
    assert_eq!(st.winner.map(|w| w.team), Some(Team::Axis));
}

#[test]
fn economic_victory_only_at_end_of_round() {
    let mut st = GameState::new(GameSettings {
        axis_economic_victory: Some(57),
        ..Default::default()
    });
    finish_turn(&mut st, Power::Japan);
    assert!(st.winner.is_none(), "not the end of the round");
    let ev = finish_turn(&mut st, Power::Usa);
    assert_eq!(st.winner.map(|w| w.team), Some(Team::Axis));
    assert!(ev.iter().any(|e| matches!(
        e,
        Event::GameOver {
            reason: VictoryReason::Economic,
            ..
        }
    )));
}

#[test]
fn default_economic_threshold_is_84_and_allied_optional() {
    let mut st = GameState::new(GameSettings::default());
    finish_turn(&mut st, Power::Usa);
    assert!(st.winner.is_none());
    let mut st = GameState::new(GameSettings {
        allied_economic_victory: Some(91),
        axis_economic_victory: None,
        ..Default::default()
    });
    finish_turn(&mut st, Power::Usa);
    assert!(
        st.winner.is_none(),
        "allies have only 24+30+36 = 90 combined"
    );
}

#[test]
fn allied_economic_victory_when_enabled() {
    let mut st = GameState::new(GameSettings {
        allied_economic_victory: Some(90),
        ..Default::default()
    });
    finish_turn(&mut st, Power::Usa);
    assert_eq!(st.winner.map(|w| w.team), Some(Team::Allies));
}

#[test]
fn total_victory_requires_own_capitals() {
    let mut st = GameState::new(GameSettings {
        total_victory: true,
        ..Default::default()
    });
    st.control[loc("germany")] = Some(Power::Uk);
    st.control[loc("japan")] = Some(Power::Usa);
    st.control[loc("russia")] = Some(Power::Germany);
    finish_turn(&mut st, Power::Usa);
    assert!(st.winner.is_none(), "Russia is lost: no total victory");
    st.control[loc("russia")] = Some(Power::Ussr);
    finish_turn(&mut st, Power::Usa);
    assert_eq!(st.winner.map(|w| w.team), Some(Team::Allies));
}

#[test]
fn concession_ends_game_when_whole_team_concedes() {
    let mut st = GameState::new(GameSettings::default());
    act(&mut st, Power::Japan, Action::Concede).unwrap();
    assert!(st.winner.is_none());
    let ev = act(&mut st, Power::Germany, Action::Concede).unwrap();
    assert_eq!(st.winner.map(|w| w.team), Some(Team::Allies));
    assert!(ev.iter().any(|e| matches!(
        e,
        Event::GameOver {
            reason: VictoryReason::Concession,
            ..
        }
    )));
}

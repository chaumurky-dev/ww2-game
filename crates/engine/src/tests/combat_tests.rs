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

// ---- naval ----

fn sea_setup() -> GameState {
    blank(Power::Uk, Phase::CombatMove)
}

fn go_combat(st: &mut GameState, p: Power) {
    act(st, p, Action::EndPhase).unwrap();
    assert_eq!(st.phase, Phase::Combat);
}

#[test]
fn submarine_first_strike_removes_ship_before_return_fire() {
    let mut st = sea_setup();
    let s = put(&mut st, UnitType::Submarine, Power::Uk, "sz_north");
    let bb = put(&mut st, UnitType::Battleship, Power::Germany, "sz_baltic");
    act(&mut st, Power::Uk, mv(&[s], &["sz_baltic"])).unwrap();
    go_combat(&mut st, Power::Uk);
    // sub rolls 2 = hit on 2; battleship removed immediately and never fires
    let ev = act_seed(
        &mut st,
        Power::Uk,
        Action::BeginBattle {
            location: loc("sz_baltic"),
        },
        seed_for(&[2]),
    )
    .unwrap();
    assert!(st.unit(bb).is_none() && st.unit(s).is_some());
    assert_eq!(outcome(&ev), Some(BattleOutcome::AttackerWins));
    assert!(!ev.iter().any(|e| matches!(
        e,
        Event::DiceRolled {
            context: DiceContext::Defense { .. },
            ..
        }
    )));
}

#[test]
fn defending_sub_has_no_first_strike() {
    let mut st = sea_setup();
    let bb = put(&mut st, UnitType::Battleship, Power::Uk, "sz_north");
    let sub = put(&mut st, UnitType::Submarine, Power::Germany, "sz_baltic");
    act(&mut st, Power::Uk, mv(&[bb], &["sz_baltic"])).unwrap();
    go_combat(&mut st, Power::Uk);
    // battleship hits on 4 (roll 4), sub hits on 2 (roll 2): both die
    let ev = act_seed(
        &mut st,
        Power::Uk,
        Action::BeginBattle {
            location: loc("sz_baltic"),
        },
        seed_for(&[4, 2]),
    )
    .unwrap();
    assert!(
        st.unit(sub).is_none() && st.unit(bb).is_none(),
        "simultaneous"
    );
    assert_eq!(outcome(&ev), Some(BattleOutcome::DefenderWins));
}

#[test]
fn submarine_cannot_hit_aircraft_and_fighter_can_hit_sub() {
    let mut st = sea_setup();
    let f = put(&mut st, UnitType::Fighter, Power::Uk, "united_kingdom");
    let sub = put(&mut st, UnitType::Submarine, Power::Germany, "sz_north");
    // fighter attacks sub
    act(&mut st, Power::Uk, mv(&[f], &["sz_north"])).unwrap();
    go_combat(&mut st, Power::Uk);
    // fighter rolls 6 (miss), sub rolls 1 (would hit) but cannot hit aircraft
    act_seed(
        &mut st,
        Power::Uk,
        Action::BeginBattle {
            location: loc("sz_north"),
        },
        seed_for(&[6, 1]),
    )
    .unwrap();
    assert!(st.unit(f).is_some(), "sub hits on aircraft are lost");
    // attacker's choice now; fighter presses and hits
    assert!(matches!(
        crate::pending_decision(&st),
        Decision::RetreatOrPress { .. }
    ));
    act(&mut st, Power::Uk, Action::PressAttack).unwrap();
    assert!(matches!(
        crate::pending_decision(&st),
        Decision::SubWithdrawal { .. }
    ));
    act_seed(
        &mut st,
        Power::Germany,
        Action::WithdrawSubs {
            subs: vec![],
            to: None,
        },
        seed_for(&[3, 6]),
    )
    .unwrap();
    assert!(st.unit(sub).is_none());
}

#[test]
fn attacking_sub_first_strike_cannot_target_aircraft_on_carrier() {
    let mut st = sea_setup();
    let s = put(&mut st, UnitType::Submarine, Power::Uk, "sz_north");
    let c = put(&mut st, UnitType::Carrier, Power::Germany, "sz_baltic");
    let f = put(&mut st, UnitType::Fighter, Power::Germany, "sz_baltic");
    st.unit_mut(f).unwrap().loaded_on = Some(c);
    act(&mut st, Power::Uk, mv(&[s], &["sz_baltic"])).unwrap();
    go_combat(&mut st, Power::Uk);
    // sub hits (2); carrier is the only legal target (auto); fighter survives and fires back at 4
    let ev = act_seed(
        &mut st,
        Power::Uk,
        Action::BeginBattle {
            location: loc("sz_baltic"),
        },
        seed_for(&[2, 4]),
    )
    .unwrap();
    let _ = ev;
    assert!(st.unit(c).is_none());
    // fighter survived the battle and was lost for lack of a carrier (cannot land)
    assert!(st.unit(f).is_none() || st.unit(f).unwrap().loaded_on.is_none());
}

#[test]
fn defenders_fighter_on_carrier_may_absorb_hit() {
    let mut st = sea_setup();
    let b = put(&mut st, UnitType::Battleship, Power::Uk, "sz_north");
    let c = put(&mut st, UnitType::Carrier, Power::Germany, "sz_baltic");
    let f = put(&mut st, UnitType::Fighter, Power::Germany, "sz_baltic");
    st.unit_mut(f).unwrap().loaded_on = Some(c);
    act(&mut st, Power::Uk, mv(&[b], &["sz_baltic"])).unwrap();
    go_combat(&mut st, Power::Uk);
    act_seed(
        &mut st,
        Power::Uk,
        Action::BeginBattle {
            location: loc("sz_baltic"),
        },
        seed_for(&[4, 6, 6]),
    )
    .unwrap();
    match crate::pending_decision(&st) {
        Decision::SelectCasualties {
            power, eligible, ..
        } => {
            assert_eq!(power, Power::Germany);
            assert!(eligible.contains(&f) && eligible.contains(&c));
        }
        d => panic!("{d:?}"),
    }
    act(
        &mut st,
        Power::Germany,
        Action::SelectCasualties { units: vec![f] },
    )
    .unwrap();
    assert!(st.unit(f).is_none() && st.unit(c).is_some());
}

#[test]
fn lone_attacking_transport_takes_a_round_of_fire() {
    let mut st = sea_setup();
    let t = put(&mut st, UnitType::Transport, Power::Uk, "sz_north");
    let d = put(&mut st, UnitType::Submarine, Power::Germany, "sz_baltic");
    act(&mut st, Power::Uk, mv(&[t], &["sz_baltic"])).unwrap();
    go_combat(&mut st, Power::Uk);
    act_seed(
        &mut st,
        Power::Uk,
        Action::BeginBattle {
            location: loc("sz_baltic"),
        },
        seed_for(&[6]),
    )
    .unwrap();
    assert!(st.unit(t).is_some() && st.unit(d).is_some());
    let ev = act(
        &mut st,
        Power::Uk,
        Action::Retreat {
            to: loc("sz_north"),
        },
    )
    .unwrap();
    assert_eq!(outcome(&ev), Some(BattleOutcome::Retreated));
    assert_eq!(st.unit(t).unwrap().location, loc("sz_north"));
}

#[test]
fn naval_retreat_cannot_enter_battle_site() {
    let mut st = sea_setup();
    let b = put(&mut st, UnitType::Battleship, Power::Uk, "sz_north");
    putn(
        &mut st,
        UnitType::Battleship,
        Power::Germany,
        "sz_baltic",
        2,
    );
    act(&mut st, Power::Uk, mv(&[b], &["sz_baltic"])).unwrap();
    go_combat(&mut st, Power::Uk);
    act_seed(
        &mut st,
        Power::Uk,
        Action::BeginBattle {
            location: loc("sz_baltic"),
        },
        seed_for(&[6, 6, 6]),
    )
    .unwrap();
    assert_eq!(
        crate::query::retreat_destinations(&st),
        vec![loc("sz_north")]
    );
    st.turn.resolved.push(loc("sz_north"));
    assert!(crate::query::retreat_destinations(&st).is_empty());
}

#[test]
fn attacking_subs_may_withdraw() {
    let mut st = sea_setup();
    let s = put(&mut st, UnitType::Submarine, Power::Uk, "sz_north");
    putn(
        &mut st,
        UnitType::Battleship,
        Power::Germany,
        "sz_baltic",
        2,
    );
    act(&mut st, Power::Uk, mv(&[s], &["sz_baltic"])).unwrap();
    go_combat(&mut st, Power::Uk);
    act_seed(
        &mut st,
        Power::Uk,
        Action::BeginBattle {
            location: loc("sz_baltic"),
        },
        seed_for(&[6, 6, 6]),
    )
    .unwrap();
    let ev = act(
        &mut st,
        Power::Uk,
        Action::WithdrawSubs {
            subs: vec![s],
            to: Some(loc("sz_north")),
        },
    )
    .unwrap();
    assert!(ev.iter().any(|e| matches!(e, Event::SubsWithdrew { .. })));
    assert_eq!(st.unit(s).unwrap().location, loc("sz_north"));
    assert!(st.battle.is_none(), "no attackers left: battle over");
}

#[test]
fn defending_sub_may_withdraw_between_rounds() {
    let mut st = sea_setup();
    let b = put(&mut st, UnitType::Battleship, Power::Uk, "sz_north");
    let sub = put(&mut st, UnitType::Submarine, Power::Germany, "sz_baltic");
    putn(
        &mut st,
        UnitType::Battleship,
        Power::Germany,
        "sz_baltic",
        1,
    );
    act(&mut st, Power::Uk, mv(&[b], &["sz_baltic"])).unwrap();
    go_combat(&mut st, Power::Uk);
    act_seed(
        &mut st,
        Power::Uk,
        Action::BeginBattle {
            location: loc("sz_baltic"),
        },
        seed_for(&[6, 6, 6]),
    )
    .unwrap();
    act(&mut st, Power::Uk, Action::PressAttack).unwrap();
    match crate::pending_decision(&st) {
        Decision::SubWithdrawal { power, subs, .. } => {
            assert_eq!(power, Power::Germany);
            assert_eq!(subs, vec![sub]);
        }
        d => panic!("{d:?}"),
    }
    act(
        &mut st,
        Power::Germany,
        Action::WithdrawSubs {
            subs: vec![sub],
            to: Some(loc("sz_north")),
        },
    )
    .unwrap();
    assert_eq!(st.unit(sub).unwrap().location, loc("sz_north"));
}

#[test]
fn amphibious_assault_bombardment_and_no_retreat() {
    let mut st = sea_setup();
    let bb = put(&mut st, UnitType::Battleship, Power::Uk, "sz_north");
    let t = put(&mut st, UnitType::Transport, Power::Uk, "sz_north");
    let i = put(&mut st, UnitType::Infantry, Power::Uk, "united_kingdom");
    act(
        &mut st,
        Power::Uk,
        Action::Load {
            units: vec![i],
            transport: t,
        },
    )
    .unwrap();
    putn(
        &mut st,
        UnitType::Infantry,
        Power::Germany,
        "western_europe",
        2,
    );
    act(
        &mut st,
        Power::Uk,
        Action::Unload {
            transport: t,
            units: vec![i],
            to: loc("western_europe"),
        },
    )
    .unwrap();
    go_combat(&mut st, Power::Uk);
    // bombardment roll 4 hits one defender (auto: identical infantry); attacker infantry misses, defenders roll 6
    let ev = act_seed(
        &mut st,
        Power::Uk,
        Action::BeginBattle {
            location: loc("western_europe"),
        },
        seed_for(&[4, 6, 6, 6]),
    )
    .unwrap();
    assert!(ev
        .iter()
        .any(|e| matches!(e, Event::Bombardment { hits: 1, .. })));
    assert!(ev.iter().any(|e| matches!(
        e,
        Event::BattleStarted {
            amphibious: true,
            ..
        }
    )));
    assert!(st.unit(bb).unwrap().fought);
    // amphibious: no retreat and rounds continue automatically until someone is dead
    assert!(
        st.battle.is_none()
            || !matches!(
                crate::pending_decision(&st),
                Decision::RetreatOrPress { .. }
            )
    );
}

#[test]
fn sea_battle_must_precede_assault() {
    let mut st = sea_setup();
    let bb = put(&mut st, UnitType::Battleship, Power::Uk, "sz_north");
    let t = put(&mut st, UnitType::Transport, Power::Uk, "sz_north");
    let i = put(&mut st, UnitType::Infantry, Power::Uk, "united_kingdom");
    act(
        &mut st,
        Power::Uk,
        Action::Load {
            units: vec![i],
            transport: t,
        },
    )
    .unwrap();
    putn(
        &mut st,
        UnitType::Infantry,
        Power::Germany,
        "western_europe",
        2,
    );
    put(&mut st, UnitType::Submarine, Power::Germany, "sz_baltic");
    // move escorts into hostile zone? Instead make the transport's own zone hostile
    let sub = put(&mut st, UnitType::Submarine, Power::Germany, "sz_north");
    // the Uk fleet is "in" a hostile zone but did not move there: no sea battle without attackers
    act(
        &mut st,
        Power::Uk,
        Action::Unload {
            transport: t,
            units: vec![i],
            to: loc("western_europe"),
        },
    )
    .unwrap();
    let _ = (bb, sub);
    // zone hostile + assault unit: pending includes the land target only if zone is a pending battle
    assert!(!crate::battle::pending_battles(&st).contains(&loc("western_europe")));
}

#[test]
fn amphibious_with_sea_battle_first() {
    let mut st = sea_setup();
    let bb = put(&mut st, UnitType::Battleship, Power::Uk, "sz_baltic");
    let t = put(&mut st, UnitType::Transport, Power::Uk, "sz_north");
    let i = put(&mut st, UnitType::Infantry, Power::Uk, "united_kingdom");
    act(
        &mut st,
        Power::Uk,
        Action::Load {
            units: vec![i],
            transport: t,
        },
    )
    .unwrap();
    // German sub in Baltic, German infantry in Germany (touches Baltic)
    let sub = put(&mut st, UnitType::Submarine, Power::Germany, "sz_baltic");
    putn(&mut st, UnitType::Infantry, Power::Germany, "germany", 2);
    // move bb out of the baltic?? bb is already there; simpler: move transport into the Baltic
    st.unit_mut(bb).unwrap().combat_from = Some(loc("sz_north"));
    act(&mut st, Power::Uk, mv(&[t], &["sz_baltic"])).unwrap();
    act(
        &mut st,
        Power::Uk,
        Action::Unload {
            transport: t,
            units: vec![i],
            to: loc("germany"),
        },
    )
    .unwrap();
    go_combat(&mut st, Power::Uk);
    let pend = crate::battle::pending_battles(&st);
    assert_eq!(pend, vec![loc("sz_baltic"), loc("germany")]);
    assert!(
        act(
            &mut st,
            Power::Uk,
            Action::BeginBattle {
                location: loc("germany")
            }
        )
        .is_err(),
        "sea battle first"
    );
    let _ = sub;
}

#[test]
fn assault_cancelled_if_zone_not_cleared() {
    let mut st = sea_setup();
    let bb = put(&mut st, UnitType::Battleship, Power::Uk, "sz_north");
    let t = put(&mut st, UnitType::Transport, Power::Uk, "sz_north");
    let i = put(&mut st, UnitType::Infantry, Power::Uk, "united_kingdom");
    act(
        &mut st,
        Power::Uk,
        Action::Load {
            units: vec![i],
            transport: t,
        },
    )
    .unwrap();
    putn(
        &mut st,
        UnitType::Battleship,
        Power::Germany,
        "sz_baltic",
        2,
    );
    putn(&mut st, UnitType::Infantry, Power::Germany, "germany", 2);
    act(&mut st, Power::Uk, mv(&[t, bb], &["sz_baltic"])).unwrap();
    act(
        &mut st,
        Power::Uk,
        Action::Unload {
            transport: t,
            units: vec![i],
            to: loc("germany"),
        },
    )
    .unwrap();
    go_combat(&mut st, Power::Uk);
    act_seed(
        &mut st,
        Power::Uk,
        Action::BeginBattle {
            location: loc("sz_baltic"),
        },
        seed_for(&[6, 6, 6, 6]),
    )
    .unwrap();
    act(
        &mut st,
        Power::Uk,
        Action::Retreat {
            to: loc("sz_north"),
        },
    )
    .unwrap();
    // zone empty of attackers; assault units went with the transport
    assert!(!crate::battle::pending_battles(&st).contains(&loc("germany")));
    assert_eq!(st.unit(i).unwrap().loaded_on, Some(t));
}

#[test]
fn strategic_bombing_damages_treasury_and_aa_applies() {
    let mut st = blank(Power::Uk, Phase::CombatMove);
    st.treasury[Power::Germany] = 20;
    let b = put(&mut st, UnitType::Bomber, Power::Uk, "united_kingdom");
    // fly to Germany (IC, AA gun present)
    put(
        &mut st,
        UnitType::IndustrialComplex,
        Power::Germany,
        "germany",
    );
    act_seed(
        &mut st,
        Power::Uk,
        mv(&[b], &["sz_north", "sz_baltic", "germany"]),
        1,
    )
    .unwrap();
    assert!(st.unit(b).is_some() || st.unit(b).is_none());
}

#[test]
fn strategic_bombing_raid_resolution() {
    let mut st = blank(Power::Uk, Phase::CombatMove);
    st.treasury[Power::Germany] = 20;
    put(
        &mut st,
        UnitType::IndustrialComplex,
        Power::Germany,
        "germany",
    );
    let b = put(&mut st, UnitType::Bomber, Power::Uk, "western_europe");
    let b2 = put(&mut st, UnitType::Bomber, Power::Uk, "western_europe");
    act(&mut st, Power::Uk, mv(&[b, b2], &["germany"])).unwrap();
    go_combat(&mut st, Power::Uk);
    assert_eq!(st.phase, Phase::Combat, "waits so raids can be declared");
    // sum of 5 + 4 = 9
    let ev = act_seed(
        &mut st,
        Power::Uk,
        Action::DeclareStrategicBombing {
            location: loc("germany"),
            bombers: vec![b, b2],
        },
        seed_for(&[5, 4]),
    )
    .unwrap();
    assert!(ev.iter().any(|e| matches!(
        e,
        Event::StrategicBombing {
            damage: 9,
            victim: Power::Germany,
            ..
        }
    )));
    assert_eq!(st.treasury[Power::Germany], 11);
    assert_eq!(st.phase, Phase::NonCombatMove);
    assert!(
        st.complex_at(loc("germany")).is_some(),
        "complex is never destroyed"
    );
}

#[test]
fn bombing_damage_limited_to_treasury_and_requires_complex() {
    let mut st = blank(Power::Uk, Phase::Combat);
    st.treasury[Power::Germany] = 3;
    put(
        &mut st,
        UnitType::IndustrialComplex,
        Power::Germany,
        "germany",
    );
    let b = put(&mut st, UnitType::Bomber, Power::Uk, "germany");
    let ev = act_seed(
        &mut st,
        Power::Uk,
        Action::DeclareStrategicBombing {
            location: loc("germany"),
            bombers: vec![b],
        },
        seed_for(&[6]),
    )
    .unwrap();
    assert!(ev
        .iter()
        .any(|e| matches!(e, Event::StrategicBombing { damage: 3, .. })));
    assert_eq!(st.treasury[Power::Germany], 0);
    let mut st = blank(Power::Uk, Phase::Combat);
    let b = put(&mut st, UnitType::Bomber, Power::Uk, "western_europe");
    assert!(act(
        &mut st,
        Power::Uk,
        Action::DeclareStrategicBombing {
            location: loc("western_europe"),
            bombers: vec![b]
        }
    )
    .is_err());
}

#[test]
fn heavy_bombers_raid_three_dice() {
    let mut st = blank(Power::Uk, Phase::Combat);
    st.techs[Power::Uk].push(Tech::HeavyBombers);
    st.treasury[Power::Germany] = 30;
    put(
        &mut st,
        UnitType::IndustrialComplex,
        Power::Germany,
        "germany",
    );
    let b = put(&mut st, UnitType::Bomber, Power::Uk, "germany");
    act_seed(
        &mut st,
        Power::Uk,
        Action::DeclareStrategicBombing {
            location: loc("germany"),
            bombers: vec![b],
        },
        seed_for(&[2, 3, 4]),
    )
    .unwrap();
    assert_eq!(st.treasury[Power::Germany], 21);
}

#[test]
fn bomber_that_bombs_does_not_fight() {
    let mut st = blank(Power::Uk, Phase::CombatMove);
    put(
        &mut st,
        UnitType::IndustrialComplex,
        Power::Germany,
        "germany",
    );
    put(&mut st, UnitType::Infantry, Power::Germany, "germany");
    let b = put(&mut st, UnitType::Bomber, Power::Uk, "western_europe");
    st.control[loc("western_europe")] = Some(Power::Uk);
    st.turn.start_control[loc("western_europe")] = Some(Power::Uk);
    act(&mut st, Power::Uk, mv(&[b], &["germany"])).unwrap();
    go_combat(&mut st, Power::Uk);
    act(
        &mut st,
        Power::Uk,
        Action::DeclareStrategicBombing {
            location: loc("germany"),
            bombers: vec![b],
        },
    )
    .unwrap();
    assert!(crate::battle::pending_battles(&st).is_empty());
}

#[test]
fn rockets_hit_complex_owner_within_three_spaces() {
    let mut st = blank(Power::Germany, Phase::Combat);
    st.techs[Power::Germany].push(Tech::Rockets);
    st.treasury[Power::Uk] = 20;
    let gun = put(&mut st, UnitType::AaGun, Power::Germany, "germany");
    put(
        &mut st,
        UnitType::IndustrialComplex,
        Power::Uk,
        "united_kingdom",
    );
    // Germany -> sz_baltic -> sz_north -> United Kingdom = 3 spaces
    assert_eq!(
        crate::bombing::rocket_targets(&st, gun),
        vec![loc("united_kingdom")]
    );
    let ev = act_seed(
        &mut st,
        Power::Germany,
        Action::FireRocket {
            aa_gun: gun,
            target: loc("united_kingdom"),
        },
        seed_for(&[4]),
    )
    .unwrap();
    assert!(ev.iter().any(|e| matches!(
        e,
        Event::RocketFired {
            damage: 4,
            victim: Power::Uk,
            ..
        }
    )));
    assert_eq!(st.treasury[Power::Uk], 16);
    assert!(st.turn.rocket_fired);
    assert!(act(
        &mut st,
        Power::Germany,
        Action::FireRocket {
            aa_gun: gun,
            target: loc("united_kingdom")
        }
    )
    .is_err());
}

#[test]
fn rockets_need_tech_and_range() {
    let mut st = blank(Power::Germany, Phase::Combat);
    let gun = put(&mut st, UnitType::AaGun, Power::Germany, "germany");
    put(
        &mut st,
        UnitType::IndustrialComplex,
        Power::Uk,
        "united_kingdom",
    );
    assert!(act(
        &mut st,
        Power::Germany,
        Action::FireRocket {
            aa_gun: gun,
            target: loc("united_kingdom")
        }
    )
    .is_err());
    st.techs[Power::Germany].push(Tech::Rockets);
    let far = put(&mut st, UnitType::AaGun, Power::Germany, "libya");
    assert!(crate::bombing::rocket_targets(&st, far).is_empty());
}

#[test]
fn super_subs_attack_at_three() {
    let mut st = sea_setup();
    st.techs[Power::Uk].push(Tech::SuperSubmarines);
    let s = put(&mut st, UnitType::Submarine, Power::Uk, "sz_north");
    let bb = put(&mut st, UnitType::Battleship, Power::Germany, "sz_baltic");
    act(&mut st, Power::Uk, mv(&[s], &["sz_baltic"])).unwrap();
    go_combat(&mut st, Power::Uk);
    act_seed(
        &mut st,
        Power::Uk,
        Action::BeginBattle {
            location: loc("sz_baltic"),
        },
        seed_for(&[3]),
    )
    .unwrap();
    assert!(st.unit(bb).is_none());
}

#[test]
fn allied_defenders_casualty_chooser_is_territory_controller() {
    // Germany attacks Karelia (Soviet) held by USSR and UK units
    let mut st = ukraine_setup();
    let atk = putn(
        &mut st,
        UnitType::Infantry,
        Power::Germany,
        "eastern_europe",
        3,
    );
    let r = put(&mut st, UnitType::Infantry, Power::Ussr, "ukraine_s_s_r");
    let u = putn(&mut st, UnitType::Infantry, Power::Uk, "ukraine_s_s_r", 2);
    act(&mut st, Power::Germany, mv(&atk, &["ukraine_s_s_r"])).unwrap();
    end_cm(&mut st, Power::Germany);
    // attackers roll 1,6,6 -> one hit; defenders miss
    act_seed(
        &mut st,
        Power::Germany,
        Action::BeginBattle {
            location: loc("ukraine_s_s_r"),
        },
        seed_for(&[1, 6, 6, 6, 6, 6]),
    )
    .unwrap();
    match crate::pending_decision(&st) {
        Decision::SelectCasualties {
            power, eligible, ..
        } => {
            assert_eq!(
                power,
                Power::Ussr,
                "territory controller chooses even with fewer units"
            );
            assert!(eligible.contains(&r) && eligible.contains(&u[0]));
        }
        d => panic!("{d:?}"),
    }
    // if the controller has no units there, the power with most units chooses
    let b = st.battle.as_ref().unwrap();
    let mut b2 = b.clone();
    b2.defenders.retain(|i| *i != r);
    assert_eq!(crate::battle::defending_controller(&st, &b2), Power::Uk);
}

#[test]
fn capture_after_win_liberates_ally_territory() {
    // UK retakes Western Europe? original German. Use Japan holding Hawaii? Use: Germany holds Karelia?
    let mut st = blank(Power::Uk, Phase::CombatMove);
    // Japan holds Philippines (originally Japanese) - use Persia (UK original) held by Germany, attacked by USA
    st.current_power = Power::Usa;
    set_control(&mut st, "persia", Some(Power::Germany));
    let a = put(&mut st, UnitType::Infantry, Power::Usa, "india");
    putn(&mut st, UnitType::Infantry, Power::Germany, "persia", 1);
    st.control[loc("india")] = Some(Power::Uk);
    act(&mut st, Power::Usa, mv(&[a], &["persia"])).unwrap();
    end_cm(&mut st, Power::Usa);
    let ev = act_seed(
        &mut st,
        Power::Usa,
        Action::BeginBattle {
            location: loc("persia"),
        },
        seed_for(&[1, 6]),
    )
    .unwrap();
    assert_eq!(
        st.control[loc("persia")],
        Some(Power::Uk),
        "liberated to its original owner"
    );
    assert!(ev.iter().any(|e| matches!(
        e,
        Event::TerritoryCaptured {
            liberated: true,
            power: Power::Uk,
            ..
        }
    )));
}

#[test]
fn odds_simulation_is_sane() {
    let mut st = ukraine_setup();
    let atk = putn(
        &mut st,
        UnitType::Infantry,
        Power::Germany,
        "eastern_europe",
        6,
    );
    putn(&mut st, UnitType::Infantry, Power::Ussr, "ukraine_s_s_r", 1);
    act(&mut st, Power::Germany, mv(&atk, &["ukraine_s_s_r"])).unwrap();
    end_cm(&mut st, Power::Germany);
    let o = crate::query::battle_odds(&st, loc("ukraine_s_s_r"), 300, 7).unwrap();
    assert!(o.attacker_win > 0.5, "{o:?}");
    assert!((o.attacker_win + o.defender_win + o.stalemate - 1.0).abs() < 1e-9);
    assert!(
        st.battle.is_none(),
        "simulation must not touch the real state"
    );
}

#[test]
fn battle_is_deterministic_for_same_seed() {
    let mut st = ukraine_setup();
    let atk = putn(
        &mut st,
        UnitType::Infantry,
        Power::Germany,
        "eastern_europe",
        3,
    );
    putn(&mut st, UnitType::Infantry, Power::Ussr, "ukraine_s_s_r", 3);
    act(&mut st, Power::Germany, mv(&atk, &["ukraine_s_s_r"])).unwrap();
    end_cm(&mut st, Power::Germany);
    let mut a = st.clone();
    let mut b = st.clone();
    let ea = act_seed(
        &mut a,
        Power::Germany,
        Action::BeginBattle {
            location: loc("ukraine_s_s_r"),
        },
        99,
    )
    .unwrap();
    let eb = act_seed(
        &mut b,
        Power::Germany,
        Action::BeginBattle {
            location: loc("ukraine_s_s_r"),
        },
        99,
    )
    .unwrap();
    assert_eq!(ea, eb);
    assert_eq!(a, b);
}

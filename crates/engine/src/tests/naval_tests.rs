use super::*;
use crate::decision::Decision;
use crate::event::{BattleOutcome, DiceContext};

fn outcome(ev: &[Event]) -> Option<BattleOutcome> {
    ev.iter().rev().find_map(|e| match e {
        Event::BattleEnded { outcome, .. } => Some(*outcome),
        _ => None,
    })
}

fn end_cm(st: &mut GameState, p: Power) {
    act(st, p, Action::EndPhase).unwrap();
}

fn ukraine_setup() -> GameState {
    let mut st = blank(Power::Germany, Phase::CombatMove);
    set_control(&mut st, "ukraine_s_s_r", Some(Power::Ussr));
    st
}

// ---- naval ----

fn sea_setup() -> GameState {
    blank(Power::Uk, Phase::CombatMove)
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

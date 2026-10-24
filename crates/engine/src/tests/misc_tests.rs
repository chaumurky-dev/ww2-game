use super::*;
use crate::dice::{rng_from_seed, RngCore};
use crate::map::MapData;
use crate::random_policy::RandomPolicy;
use crate::state::PurchaseItem;

fn rand_loc(r: &mut crate::dice::Rng) -> LocationId {
    LocationId((r.next_u32() % MapData::get().len() as u32) as u16)
}

fn rand_unit(st: &GameState, r: &mut crate::dice::Rng) -> UnitId {
    if st.units.is_empty() || divisible(u64::from(r.next_u32()), 10) {
        return r.next_u32() % 5000;
    }
    st.units[(r.next_u32() as usize) % st.units.len()].id
}

fn garbage_action(st: &GameState, r: &mut crate::dice::Rng) -> Action {
    let n = 1 + (r.next_u32() % 3) as usize;
    let units: Vec<UnitId> = (0..n).map(|_| rand_unit(st, r)).collect();
    match r.next_u32() % 16 {
        0 => Action::BuyResearch {
            dice: r.next_u32() % 5,
        },
        1 => Action::Purchase {
            items: vec![PurchaseItem {
                unit: UnitType::ALL[(r.next_u32() % 10) as usize],
                count: r.next_u32() % 6,
            }],
        },
        2 | 3 => {
            let steps = 1 + (r.next_u32() % 3) as usize;
            let mut path = vec![];
            // mostly adjacent steps from the first unit to give legal-looking input
            let mut cur = st
                .unit(units[0])
                .map(|u| u.location)
                .unwrap_or_else(|| rand_loc(r));
            for _ in 0..steps {
                let nb = MapData::get().neighbours(cur);
                cur = if divisible(u64::from(r.next_u32()), 8) {
                    rand_loc(r)
                } else {
                    nb[(r.next_u32() as usize) % nb.len()]
                };
                path.push(cur);
            }
            Action::Move { units, path }
        }
        4 => Action::Load {
            units,
            transport: rand_unit(st, r),
        },
        5 => Action::Unload {
            transport: rand_unit(st, r),
            units,
            to: rand_loc(r),
        },
        6 => Action::EndPhase,
        7 => Action::BeginBattle {
            location: rand_loc(r),
        },
        8 => Action::DeclareStrategicBombing {
            location: rand_loc(r),
            bombers: units,
        },
        9 => Action::SelectCasualties { units },
        10 => Action::Retreat { to: rand_loc(r) },
        11 => Action::PressAttack,
        12 => Action::WithdrawSubs {
            subs: units,
            to: if divisible(u64::from(r.next_u32()), 2) {
                Some(rand_loc(r))
            } else {
                None
            },
        },
        13 => Action::FireRocket {
            aa_gun: rand_unit(st, r),
            target: rand_loc(r),
        },
        14 => Action::Place {
            unit: UnitType::ALL[(r.next_u32() % 10) as usize],
            location: rand_loc(r),
            count: r.next_u32() % 4,
            from: None,
        },
        _ => Action::Move {
            units: vec![],
            path: vec![],
        },
    }
}

#[test]
fn garbage_actions_never_panic_and_never_mutate_on_error() {
    let mut r = rng_from_seed(2024);
    let mut accepted = 0;
    for game in 0..4u64 {
        let mut st = GameState::new(GameSettings {
            seed: game,
            ..Default::default()
        });
        let mut pol = RandomPolicy::new(game + 100);
        for step in 0..1200u64 {
            // interleave legal progress with garbage
            if step % 3 != 0 {
                if let Some((p, a)) = pol.next_action(&st) {
                    apply(&mut st, p, &a, step).expect("policy action legal");
                }
            }
            for _ in 0..5 {
                let a = garbage_action(&st, &mut r);
                let actor = Power::from_index((r.next_u32() % 5) as usize);
                let actor = if !divisible(u64::from(r.next_u32()), 3) {
                    st.current_power
                } else {
                    actor
                };
                let before = st.clone();
                match apply(&mut st, actor, &a, step) {
                    Ok(_) => accepted += 1,
                    Err(_) => assert_eq!(st, before, "state changed by rejected action {a:?}"),
                }
                super::fuzz_tests::check_invariants(&st);
            }
        }
    }
    assert!(
        accepted > 50,
        "garbage should sometimes be legal: {accepted}"
    );
}

#[test]
fn same_state_action_seed_gives_identical_results() {
    let mut st = GameState::new(GameSettings::default());
    let mut pol = RandomPolicy::new(3);
    for i in 0..300 {
        let (p, a) = pol.next_action(&st).unwrap();
        let mut s1 = st.clone();
        let mut s2 = st.clone();
        let e1 = apply(&mut s1, p, &a, i).unwrap();
        let e2 = apply(&mut s2, p, &a, i).unwrap();
        assert_eq!(e1, e2);
        assert_eq!(s1, s2);
        st = s1;
    }
}

#[test]
fn replay_reproduces_game() {
    let settings = GameSettings::default();
    let mut st = GameState::new(settings.clone());
    let mut pol = RandomPolicy::new(8);
    let mut log = vec![];
    for i in 0..500u64 {
        let (p, a) = pol.next_action(&st).unwrap();
        apply(&mut st, p, &a, i * 31).unwrap();
        log.push((p, a, i * 31));
    }
    assert_eq!(replay(settings, &log).unwrap(), st);
}

#[test]
fn replay_rejects_illegal_log() {
    let log = vec![(Power::Germany, Action::EndPhase, 1)];
    assert!(replay(GameSettings::default(), &log).is_err());
}

#[test]
fn state_json_shape_is_stable() {
    let st = GameState::new(GameSettings::default());
    let v: serde_json::Value = serde_json::to_value(&st).unwrap();
    assert_eq!(v["current_power"], "ussr");
    assert_eq!(v["phase"], "develop_weapons");
    assert_eq!(v["treasury"]["germany"], 32);
    assert_eq!(v["control"]["germany"], "germany");
    assert_eq!(v["settings"]["axis_economic_victory"], 84);
    assert!(v["units"][0]["location"].is_string());
    assert!(v["units"][0]["kind"].is_string());
    let a = serde_json::to_value(Action::Move {
        units: vec![1],
        path: vec![loc("russia")],
    })
    .unwrap();
    assert_eq!(a["type"], "move");
    assert_eq!(a["path"][0], "russia");
    let e = serde_json::to_value(Event::PhaseChanged {
        power: Power::Uk,
        phase: Phase::Combat,
    })
    .unwrap();
    assert_eq!(e["type"], "phase_changed");
    let d = serde_json::to_value(crate::pending_decision(&st)).unwrap();
    assert_eq!(d["type"], "phase");
}

#[test]
fn settings_deserialize_with_defaults() {
    let s: GameSettings = serde_json::from_str("{}").unwrap();
    assert_eq!(s, GameSettings::default());
    let s: GameSettings =
        serde_json::from_str(r#"{"weapons_development":false,"seed":5}"#).unwrap();
    assert!(!s.weapons_development && s.seed == 5 && s.axis_economic_victory == Some(84));
}

#[test]
fn unknown_location_in_json_is_rejected() {
    assert!(serde_json::from_str::<Action>(r#"{"type":"retreat","to":"atlantis"}"#).is_err());
}

#[test]
fn game_over_state_rejects_everything() {
    let mut st = GameState::new(GameSettings::default());
    act(&mut st, Power::Japan, Action::Concede).unwrap();
    act(&mut st, Power::Germany, Action::Concede).unwrap();
    assert!(st.is_over());
    assert!(matches!(
        act(&mut st, Power::Ussr, Action::EndPhase),
        Err(RuleError::GameOver)
    ));
    assert!(matches!(
        act(&mut st, Power::Ussr, Action::Concede),
        Err(RuleError::GameOver)
    ));
}

#[test]
fn wrong_phase_actions_are_rejected_with_phase_error() {
    let mut st = GameState::new(GameSettings::default());
    let e = act(&mut st, Power::Ussr, Action::Purchase { items: vec![] }).unwrap_err();
    assert!(matches!(
        e,
        RuleError::WrongPhase {
            phase: Phase::DevelopWeapons,
            ..
        }
    ));
    let e = act(&mut st, Power::Ussr, Action::PressAttack).unwrap_err();
    assert!(matches!(
        e,
        RuleError::WrongPhase { .. } | RuleError::Illegal(_)
    ));
}

#[test]
fn pending_decision_reports_phase_owner() {
    let st = GameState::new(GameSettings::default());
    assert_eq!(
        crate::pending_decision(&st),
        crate::Decision::Phase {
            power: Power::Ussr,
            phase: Phase::DevelopWeapons
        }
    );
}

#[test]
fn map_lookups() {
    let m = MapData::get();
    let russia = m.id_of("russia").unwrap();
    assert_eq!(russia.id(), "russia");
    assert_eq!(m.loc(russia).name, "Russia");
    assert!(m.neighbours(russia).contains(&m.id_of("caucasus").unwrap()));
    assert_eq!(
        m.original_owner(m.id_of("brazil").unwrap()),
        Some(Power::Usa)
    );
    assert_eq!(m.capital(Power::Japan), m.id_of("japan").unwrap());
    assert!(!m.canals().is_empty());
    assert_eq!(m.starting_ipcs(Power::Usa), 36);
}

#[test]
fn every_unit_type_stats_present() {
    for k in UnitType::ALL {
        let s = k.stats();
        assert!(s.cost > 0);
    }
    assert_eq!(UnitType::Battleship.stats().cost, 24);
    assert_eq!(UnitType::IndustrialComplex.stats().movement, 0);
}

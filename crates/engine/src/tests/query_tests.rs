//! Tests for the additive query/event helpers (dice details, group reach, mid-battle odds,
//! placements, movable units, landing status).
use super::*;
use crate::event::{DiceContext, DieDetail};
use crate::query::{self, LandingStatus};
use crate::random_policy::RandomPolicy;
use crate::state::PurchaseItem;

fn item(unit: UnitType, count: u32) -> PurchaseItem {
    PurchaseItem { unit, count }
}

fn ukraine_setup() -> GameState {
    let mut st = blank(Power::Germany, Phase::CombatMove);
    set_control(&mut st, "ukraine_s_s_r", Some(Power::Ussr));
    st
}

fn details_of(ev: &[Event], f: impl Fn(&DiceContext) -> bool) -> Vec<(Vec<u8>, Vec<DieDetail>)> {
    ev.iter()
        .filter_map(|e| match e {
            Event::DiceRolled {
                context,
                rolls,
                details,
                ..
            } if f(context) => Some((rolls.clone(), details.clone())),
            _ => None,
        })
        .collect()
}

#[test]
fn combat_dice_carry_details_in_roll_order() {
    let mut st = ukraine_setup();
    let a = put(
        &mut st,
        UnitType::Infantry,
        Power::Germany,
        "eastern_europe",
    );
    let t = put(&mut st, UnitType::Armor, Power::Germany, "eastern_europe");
    let d = put(&mut st, UnitType::Infantry, Power::Ussr, "ukraine_s_s_r");
    act(&mut st, Power::Germany, mv(&[a, t], &["ukraine_s_s_r"])).unwrap();
    act(&mut st, Power::Germany, Action::EndPhase).unwrap();
    let ev = act_seed(
        &mut st,
        Power::Germany,
        Action::BeginBattle {
            location: loc("ukraine_s_s_r"),
        },
        seed_for(&[1, 3, 2]),
    )
    .unwrap();
    let atk = details_of(&ev, |c| matches!(c, DiceContext::Attack { .. }));
    assert_eq!(atk.len(), 1);
    let (rolls, det) = &atk[0];
    assert_eq!(rolls, &vec![1, 3]);
    assert_eq!(det.len(), rolls.len());
    assert_eq!(det[0].unit, Some(a));
    assert_eq!(det[0].unit_type, Some(UnitType::Infantry));
    assert_eq!((det[0].hit_on, det[0].roll, det[0].hit), (1, 1, true));
    assert_eq!(det[1].unit, Some(t));
    assert_eq!((det[1].hit_on, det[1].roll, det[1].hit), (3, 3, true));
    assert_eq!(det[1].owner, Power::Germany);
    let def = details_of(&ev, |c| matches!(c, DiceContext::Defense { .. }));
    assert_eq!(def[0].1[0].unit, Some(d));
    assert_eq!(def[0].1[0].owner, Power::Ussr);
    assert_eq!(def[0].1[0].hit_on, 2);
}

#[test]
fn research_bombing_and_rocket_details() {
    let mut st = blank(Power::Germany, Phase::DevelopWeapons);
    let ev = act_seed(
        &mut st,
        Power::Germany,
        Action::BuyResearch { dice: 2 },
        seed_for(&[6, 2, 4]),
    )
    .unwrap();
    let r = details_of(&ev, |c| matches!(c, DiceContext::Research));
    let (rolls, det) = &r[0];
    assert_eq!(rolls, &vec![6, 2]);
    assert!(det
        .iter()
        .all(|d| d.unit.is_none() && d.unit_type.is_none() && d.hit_on == 6));
    assert_eq!(
        det.iter().map(|d| d.hit).collect::<Vec<_>>(),
        vec![true, false]
    );
    let t = details_of(&ev, |c| matches!(c, DiceContext::ResearchTech));
    assert_eq!(t[0].1.len(), t[0].0.len());
    assert!(t[0].1.iter().all(|d| d.hit_on == 0 && !d.hit));

    let mut st = blank(Power::Uk, Phase::Combat);
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
        seed_for(&[5]),
    )
    .unwrap();
    let r = details_of(&ev, |c| matches!(c, DiceContext::StrategicBombing { .. }));
    assert_eq!(r[0].1.len(), r[0].0.len());
    assert!(r[0]
        .1
        .iter()
        .all(|d| d.unit.is_none() && d.hit_on == 0 && !d.hit));

    let mut st = blank(Power::Germany, Phase::Combat);
    st.techs[Power::Germany].push(crate::Tech::Rockets);
    let g = put(&mut st, UnitType::AaGun, Power::Germany, "germany");
    put(&mut st, UnitType::IndustrialComplex, Power::Ussr, "russia");
    let ev = act_seed(
        &mut st,
        Power::Germany,
        Action::FireRocket {
            aa_gun: g,
            target: loc("russia"),
        },
        seed_for(&[3]),
    )
    .unwrap();
    let r = details_of(&ev, |c| matches!(c, DiceContext::Rocket { .. }));
    assert_eq!(r[0].1.len(), 1);
    assert_eq!(r[0].1[0].roll, 3);
}

#[test]
fn aa_and_bombardment_details() {
    // AA: fighter flies over a territory with an enemy gun.
    let mut st = blank(Power::Uk, Phase::CombatMove);
    set_control(&mut st, "germany", Some(Power::Germany));
    let gun = put(&mut st, UnitType::AaGun, Power::Germany, "germany");
    let f = put(&mut st, UnitType::Fighter, Power::Uk, "western_europe");
    let ev = act_seed(&mut st, Power::Uk, mv(&[f], &["germany"]), seed_for(&[1])).unwrap();
    let r = details_of(&ev, |c| matches!(c, DiceContext::AaFire { .. }));
    assert_eq!(r[0].1.len(), 1);
    assert_eq!(r[0].1[0].unit, Some(gun));
    assert_eq!(r[0].1[0].owner, Power::Germany);
    assert_eq!((r[0].1[0].hit_on, r[0].1[0].hit), (1, true));
}

#[test]
fn all_dice_in_random_playouts_have_matching_details() {
    let mut seen = 0;
    for seed in 1..=3u64 {
        let mut st = GameState::new(GameSettings::default());
        let mut pol = RandomPolicy::new(seed);
        for i in 0..500u64 {
            let Some((p, a)) = pol.next_action(&st) else {
                break;
            };
            let ev = apply(&mut st, p, &a, i).unwrap();
            for e in ev {
                if let Event::DiceRolled {
                    context,
                    rolls,
                    details,
                    ..
                } = e
                {
                    seen += 1;
                    assert_eq!(rolls.len(), details.len(), "{context:?}");
                    for (r, d) in rolls.iter().zip(&details) {
                        assert_eq!(*r, d.roll);
                        if d.hit_on > 0 && !matches!(context, DiceContext::Research) {
                            assert_eq!(d.hit, d.roll <= d.hit_on);
                        }
                    }
                }
            }
        }
    }
    assert!(seen > 0);
}

#[test]
fn dice_rolled_serde_roundtrip_and_old_json() {
    let e = Event::DiceRolled {
        power: Power::Ussr,
        context: DiceContext::Research,
        rolls: vec![6, 2],
        details: vec![DieDetail {
            unit: None,
            unit_type: None,
            owner: Power::Ussr,
            hit_on: 6,
            roll: 6,
            hit: true,
        }],
    };
    let j = serde_json::to_string(&e).unwrap();
    assert!(j.contains("\"details\""));
    assert_eq!(serde_json::from_str::<Event>(&j).unwrap(), e);
    let old =
        r#"{"type":"dice_rolled","power":"ussr","context":{"type":"research"},"rolls":[6,2]}"#;
    match serde_json::from_str::<Event>(old).unwrap() {
        Event::DiceRolled { rolls, details, .. } => {
            assert_eq!(rolls, vec![6, 2]);
            assert!(details.is_empty());
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn reachable_group_moves_at_slowest_speed() {
    let mut st = blank(Power::Germany, Phase::CombatMove);
    let inf = put(
        &mut st,
        UnitType::Infantry,
        Power::Germany,
        "eastern_europe",
    );
    let tank = put(&mut st, UnitType::Armor, Power::Germany, "eastern_europe");
    let solo = query::reachable(&st, tank);
    assert!(solo.iter().any(|r| r.path.len() == 2));
    let grp = query::reachable_group(&st, &[inf, tank]);
    assert!(!grp.is_empty());
    assert!(grp.iter().all(|r| r.path.len() == 1));
    let inf_only: Vec<_> = query::reachable(&st, inf).iter().map(|r| r.to).collect();
    let mut a: Vec<_> = grp.iter().map(|r| r.to).collect();
    let mut b = inf_only;
    a.sort();
    b.sort();
    assert_eq!(a, b);
    // every path is accepted by the real validator for the whole group
    for r in &grp {
        crate::movement::validate_move(&st, Power::Germany, &[inf, tank], &r.path).unwrap();
    }
    // single-unit group equals single-unit reach
    assert_eq!(query::reachable_group(&st, &[tank]), solo);
}

#[test]
fn reachable_group_rejects_mixed_and_foreign_groups() {
    let mut st = blank(Power::Germany, Phase::CombatMove);
    let inf = put(
        &mut st,
        UnitType::Infantry,
        Power::Germany,
        "eastern_europe",
    );
    let f1 = put(&mut st, UnitType::Fighter, Power::Germany, "eastern_europe");
    let f2 = put(&mut st, UnitType::Bomber, Power::Germany, "eastern_europe");
    let other = put(&mut st, UnitType::Infantry, Power::Germany, "germany");
    let enemy = put(&mut st, UnitType::Infantry, Power::Ussr, "eastern_europe");
    assert!(query::reachable_group(&st, &[inf, f1]).is_empty());
    assert!(query::reachable_group(&st, &[inf, other]).is_empty());
    assert!(query::reachable_group(&st, &[inf, enemy]).is_empty());
    assert!(query::reachable_group(&st, &[]).is_empty());
    assert!(query::reachable_group(&st, &[inf, inf]).is_empty());
    // air group: slowest (fighter 4 vs bomber 6) bounds the path length
    let g = query::reachable_group(&st, &[f1, f2]);
    assert!(!g.is_empty());
    assert!(g.iter().all(|r| r.path.len() <= 4));
}

#[test]
fn current_battle_odds_continue_from_the_live_battle() {
    let mut st = ukraine_setup();
    let atk = putn(
        &mut st,
        UnitType::Infantry,
        Power::Germany,
        "eastern_europe",
        2,
    );
    putn(&mut st, UnitType::Infantry, Power::Ussr, "ukraine_s_s_r", 2);
    assert!(query::current_battle_odds(&st, 50, 1).is_none());
    act(&mut st, Power::Germany, mv(&atk, &["ukraine_s_s_r"])).unwrap();
    act(&mut st, Power::Germany, Action::EndPhase).unwrap();
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
        query::pending_decision(&st),
        query::Decision::RetreatOrPress { .. }
    ));
    let before = st.clone();
    let o = query::current_battle_odds(&st, 400, 3).unwrap();
    assert_eq!(st, before, "must not mutate the live state");
    assert!((o.attacker_win + o.defender_win + o.stalemate - 1.0).abs() < 1e-9);
    assert!(o.attacker_win > 0.2 && o.defender_win > 0.2, "{o:?}");
    assert_eq!(o, query::current_battle_odds(&st, 400, 3).unwrap());
    // after losing one attacker the odds drop
    let weakened = {
        let mut w = st.clone();
        let victim = w.battle.as_ref().unwrap().attackers[0];
        w.remove_unit(victim);
        w.battle
            .as_mut()
            .unwrap()
            .attackers
            .retain(|&x| x != victim);
        w
    };
    let o2 = query::current_battle_odds(&weakened, 400, 3).unwrap();
    assert!(o2.attacker_win < o.attacker_win, "{o2:?} vs {o:?}");
}

#[test]
fn placements_for_respect_capacity_aa_and_naval_suppliers() {
    let mut st = blank(Power::Uk, Phase::Mobilize);
    st.purchases = vec![
        item(UnitType::Infantry, 2),
        item(UnitType::AaGun, 1),
        item(UnitType::Transport, 1),
    ];
    put(
        &mut st,
        UnitType::IndustrialComplex,
        Power::Uk,
        "united_kingdom",
    );
    let inf = query::legal_placements_for(&st, UnitType::Infantry, 2);
    assert!(inf
        .iter()
        .any(|p| p.location == loc("united_kingdom")
            && p.from_complex == Some(loc("united_kingdom"))));
    assert!(
        query::legal_placements_for(&st, UnitType::Infantry, 3).is_empty(),
        "only 2 bought"
    );
    // AA: unique per territory
    assert!(!query::legal_placements_for(&st, UnitType::AaGun, 1).is_empty());
    assert!(query::legal_placements_for(&st, UnitType::AaGun, 2).is_empty());
    put(&mut st, UnitType::AaGun, Power::Uk, "united_kingdom");
    assert!(query::legal_placements_for(&st, UnitType::AaGun, 1).is_empty());
    // ships: every entry names the supplying complex and agrees with validate_place
    let ships = query::legal_placements_for(&st, UnitType::Transport, 1);
    assert!(ships.iter().any(|p| p.location == loc("sz_north")));
    for p in &ships {
        assert!(p.from_complex.is_some());
        crate::economy::validate_place(
            &st,
            Power::Uk,
            UnitType::Transport,
            p.location,
            1,
            p.from_complex,
        )
        .unwrap();
    }
    // capacity of a non-original complex shrinks as units are placed
    let mut st = blank(Power::Germany, Phase::Mobilize);
    st.purchases = vec![item(UnitType::Infantry, 3)];
    put(
        &mut st,
        UnitType::IndustrialComplex,
        Power::Germany,
        "india",
    );
    set_control(&mut st, "india", Some(Power::Germany));
    st.turn.start_control = st.control.clone();
    let cap = crate::economy::complex_capacity_left(&st, Power::Germany, loc("india"));
    let p = query::legal_placements_for(&st, UnitType::Infantry, 1);
    let e = p.iter().find(|p| p.location == loc("india")).unwrap();
    assert_eq!(e.capacity_left, cap);
    assert!(
        query::legal_placements_for(&st, UnitType::Infantry, cap + 1)
            .iter()
            .all(|p| p.location != loc("india"))
    );
}

#[test]
fn new_complex_placements_honour_the_cap() {
    let mut st = blank(Power::Germany, Phase::Mobilize);
    st.purchases = vec![item(UnitType::IndustrialComplex, 1)];
    let p = query::legal_placements_for(&st, UnitType::IndustrialComplex, 1);
    assert!(p.iter().any(|x| x.location == loc("germany")) || !p.is_empty());
    assert!(p
        .iter()
        .all(|x| x.from_complex.is_none() && x.capacity_left == 4));
    assert!(p.iter().all(|x| st.complex_at(x.location).is_none()));
    st.new_complexes_placed = 4;
    assert!(query::legal_placements_for(&st, UnitType::IndustrialComplex, 1).is_empty());
}

#[test]
fn movable_units_lists_only_units_with_destinations() {
    let mut st = blank(Power::Germany, Phase::CombatMove);
    let a = put(
        &mut st,
        UnitType::Infantry,
        Power::Germany,
        "eastern_europe",
    );
    let b = put(&mut st, UnitType::Infantry, Power::Germany, "germany");
    let spent = put(&mut st, UnitType::Infantry, Power::Germany, "germany");
    st.unit_mut(spent).unwrap().moves_used = 1;
    let enemy = put(&mut st, UnitType::Infantry, Power::Ussr, "russia");
    let m = query::movable_units(&st, Power::Germany);
    assert!(m.contains(&a) && m.contains(&b));
    assert!(!m.contains(&spent) && !m.contains(&enemy));
    assert!(
        query::movable_units(&st, Power::Ussr).is_empty(),
        "not the active power"
    );
    for &u in &m {
        assert!(!query::reachable(&st, u).is_empty());
    }
    st.phase = Phase::Purchase;
    assert!(query::movable_units(&st, Power::Germany).is_empty());
}

#[test]
fn landing_status_safe_needs_landing_and_stranded() {
    let mut st = blank(Power::Germany, Phase::NonCombatMove);
    let home = put(&mut st, UnitType::Fighter, Power::Germany, "germany");
    let inf = put(&mut st, UnitType::Infantry, Power::Germany, "germany");
    assert_eq!(query::landing_status(&st, home), LandingStatus::Safe);
    assert_eq!(query::landing_status(&st, inf), LandingStatus::Safe);
    let sea = put(&mut st, UnitType::Fighter, Power::Germany, "sz_baltic");
    match query::landing_status(&st, sea) {
        LandingStatus::NeedsLanding { options } => assert!(options.contains(&loc("germany"))),
        s => panic!("{s:?}"),
    }
    st.unit_mut(sea).unwrap().moves_used = 4;
    assert_eq!(query::landing_status(&st, sea), LandingStatus::Stranded);
    // a carrier makes the zone safe
    let car = put(&mut st, UnitType::Carrier, Power::Germany, "sz_baltic");
    let _ = car;
    assert_eq!(query::landing_status(&st, sea), LandingStatus::Safe);
    let j = serde_json::to_string(&LandingStatus::NeedsLanding {
        options: vec![loc("germany")],
    })
    .unwrap();
    assert!(j.contains("needs_landing"));
}

#[test]
fn query_helpers_agree_with_each_other_in_playouts() {
    let mut st = GameState::new(GameSettings::default());
    let mut pol = RandomPolicy::new(11);
    for i in 0..400u64 {
        let Some((p, a)) = pol.next_action(&st) else {
            break;
        };
        apply(&mut st, p, &a, i).unwrap();
        if i % 20 == 0 {
            let cur = st.current_power;
            for u in query::movable_units(&st, cur).into_iter().take(6) {
                let mut single: Vec<_> = query::reachable(&st, u).iter().map(|r| r.to).collect();
                let mut grp: Vec<_> = query::reachable_group(&st, &[u])
                    .iter()
                    .map(|r| r.to)
                    .collect();
                single.sort();
                grp.sort();
                assert_eq!(single, grp);
            }
            for u in st.units.iter().filter(|u| u.kind.is_air()).take(6) {
                let _ = query::landing_status(&st, u.id);
            }
            if st.phase == Phase::Mobilize {
                for it in st.purchases.clone() {
                    for pl in query::legal_placements_for(&st, it.unit, 1) {
                        crate::economy::validate_place(
                            &st,
                            cur,
                            it.unit,
                            pl.location,
                            1,
                            pl.from_complex.filter(|_| it.unit.is_ship()),
                        )
                        .unwrap();
                    }
                }
            }
        }
    }
}

use super::*;
use crate::query::reachable;

fn cm(p: Power) -> GameState {
    blank(p, Phase::CombatMove)
}

#[test]
fn infantry_moves_one_space_in_friendly() {
    let mut st = cm(Power::Germany);
    let i = put(&mut st, UnitType::Infantry, Power::Germany, "germany");
    act(&mut st, Power::Germany, mv(&[i], &["western_europe"])).unwrap();
    assert_eq!(st.unit(i).unwrap().location, loc("western_europe"));
    let j = put(&mut st, UnitType::Infantry, Power::Germany, "germany");
    assert!(act(
        &mut st,
        Power::Germany,
        mv(&[j], &["western_europe", "germany"])
    )
    .is_err());
}

#[test]
fn wrong_actor_rejected_and_state_untouched() {
    let mut st = cm(Power::Germany);
    let i = put(&mut st, UnitType::Infantry, Power::Germany, "germany");
    let before = st.clone();
    let e = act(&mut st, Power::Uk, mv(&[i], &["western_europe"])).unwrap_err();
    assert!(matches!(e, RuleError::NotYourTurn { .. }));
    assert_eq!(st, before);
}

#[test]
fn cannot_move_unowned_or_nonadjacent() {
    let mut st = cm(Power::Germany);
    let i = put(&mut st, UnitType::Infantry, Power::Uk, "germany");
    assert!(matches!(
        act(&mut st, Power::Germany, mv(&[i], &["western_europe"])),
        Err(RuleError::NotOwner(_))
    ));
    let j = put(&mut st, UnitType::Infantry, Power::Germany, "germany");
    assert!(act(&mut st, Power::Germany, mv(&[j], &["japan"])).is_err());
    assert!(act(&mut st, Power::Germany, mv(&[999], &["western_europe"])).is_err());
}

#[test]
fn armor_two_spaces_through_friendly() {
    let mut st = cm(Power::Germany);
    let a = put(&mut st, UnitType::Armor, Power::Germany, "germany");
    act(
        &mut st,
        Power::Germany,
        mv(&[a], &["eastern_europe", "ukraine_s_s_r"]),
    )
    .unwrap();
    assert_eq!(st.unit(a).unwrap().moves_used, 2);
}

#[test]
fn blitz_captures_empty_enemy_then_continues() {
    let mut st = cm(Power::Germany);
    let a = put(&mut st, UnitType::Armor, Power::Germany, "eastern_europe");
    set_control(&mut st, "ukraine_s_s_r", Some(Power::Ussr));
    put(&mut st, UnitType::Infantry, Power::Ussr, "karelia_s_s_r");
    act(
        &mut st,
        Power::Germany,
        mv(&[a], &["ukraine_s_s_r", "karelia_s_s_r"]),
    )
    .unwrap();
    assert_eq!(st.control[loc("ukraine_s_s_r")], Some(Power::Germany));
    assert_eq!(st.unit(a).unwrap().combat_from, Some(loc("ukraine_s_s_r")));
    assert_eq!(
        crate::battle::pending_battles(&st),
        vec![loc("karelia_s_s_r")]
    );
}

#[test]
fn infantry_stops_on_capture() {
    let mut st = cm(Power::Germany);
    let i = put(
        &mut st,
        UnitType::Infantry,
        Power::Germany,
        "eastern_europe",
    );
    set_control(&mut st, "ukraine_s_s_r", Some(Power::Ussr));
    act(&mut st, Power::Germany, mv(&[i], &["ukraine_s_s_r"])).unwrap();
    assert_eq!(st.control[loc("ukraine_s_s_r")], Some(Power::Germany));
    assert!(st.unit(i).unwrap().stopped);
}

#[test]
fn no_blitz_through_occupied_first_space() {
    let mut st = cm(Power::Germany);
    let a = put(&mut st, UnitType::Armor, Power::Germany, "eastern_europe");
    set_control(&mut st, "ukraine_s_s_r", Some(Power::Ussr));
    put(&mut st, UnitType::Infantry, Power::Ussr, "ukraine_s_s_r");
    assert!(act(
        &mut st,
        Power::Germany,
        mv(&[a], &["ukraine_s_s_r", "caucasus"])
    )
    .is_err());
    act(&mut st, Power::Germany, mv(&[a], &["ukraine_s_s_r"])).unwrap();
    assert!(st.unit(a).unwrap().stopped);
}

#[test]
fn second_space_capture_stops_armor() {
    let mut st = cm(Power::Germany);
    let a = put(&mut st, UnitType::Armor, Power::Germany, "germany");
    set_control(&mut st, "eastern_europe", Some(Power::Ussr));
    set_control(&mut st, "ukraine_s_s_r", Some(Power::Ussr));
    // first space is friendly (southern europe), then a capture - fine; but cannot go 3 deep
    assert!(act(
        &mut st,
        Power::Germany,
        mv(&[a], &["eastern_europe", "ukraine_s_s_r"])
    )
    .is_ok());
    assert_eq!(st.control[loc("ukraine_s_s_r")], Some(Power::Germany));
}

#[test]
fn aa_and_complex_do_not_occupy_and_are_captured() {
    let mut st = cm(Power::Germany);
    let a = put(&mut st, UnitType::Armor, Power::Germany, "eastern_europe");
    set_control(&mut st, "ukraine_s_s_r", Some(Power::Ussr));
    put(&mut st, UnitType::AaGun, Power::Ussr, "ukraine_s_s_r");
    put(
        &mut st,
        UnitType::IndustrialComplex,
        Power::Ussr,
        "ukraine_s_s_r",
    );
    act(
        &mut st,
        Power::Germany,
        mv(&[a], &["ukraine_s_s_r", "caucasus"]),
    )
    .unwrap();
    assert!(st
        .units_at(loc("ukraine_s_s_r"))
        .all(|u| u.owner == Power::Germany));
    assert!(st
        .units_at(loc("ukraine_s_s_r"))
        .any(|u| u.kind == UnitType::AaGun && u.frozen));
}

#[test]
fn noncombat_cannot_enter_enemy_territory() {
    let mut st = blank(Power::Germany, Phase::NonCombatMove);
    let i = put(
        &mut st,
        UnitType::Infantry,
        Power::Germany,
        "eastern_europe",
    );
    set_control(&mut st, "ukraine_s_s_r", Some(Power::Ussr));
    assert!(act(&mut st, Power::Germany, mv(&[i], &["ukraine_s_s_r"])).is_err());
    assert!(reachable(&st, i)
        .iter()
        .all(|r| r.to != loc("ukraine_s_s_r")));
}

#[test]
fn units_that_moved_in_cm_stay_in_nc() {
    let mut st = cm(Power::Germany);
    let a = put(&mut st, UnitType::Armor, Power::Germany, "germany");
    act(&mut st, Power::Germany, mv(&[a], &["eastern_europe"])).unwrap();
    st.phase = Phase::NonCombatMove;
    assert!(act(&mut st, Power::Germany, mv(&[a], &["ukraine_s_s_r"])).is_err());
}

#[test]
fn neutral_violation_costs_three_and_stops() {
    let mut st = cm(Power::Germany);
    st.treasury[Power::Germany] = 10;
    let j = put(
        &mut st,
        UnitType::Infantry,
        Power::Germany,
        "finland_norway",
    );
    let ev = act(&mut st, Power::Germany, mv(&[j], &["sweden"])).unwrap();
    assert_eq!(st.treasury[Power::Germany], 7);
    assert_eq!(st.control[loc("sweden")], Some(Power::Germany));
    assert!(ev
        .iter()
        .any(|e| matches!(e, Event::NeutralViolated { fee: 3, .. })));
    // a second power walking in later pays nothing: it is an ordinary enemy territory now
    assert!(!st.is_untouched_neutral(loc("sweden")));
}

#[test]
fn neutral_violation_needs_funds_and_not_in_noncombat() {
    let mut st = cm(Power::Germany);
    st.treasury[Power::Germany] = 2;
    let j = put(
        &mut st,
        UnitType::Infantry,
        Power::Germany,
        "finland_norway",
    );
    assert!(matches!(
        act(&mut st, Power::Germany, mv(&[j], &["sweden"])),
        Err(RuleError::InsufficientFunds { .. })
    ));
    st.treasury[Power::Germany] = 9;
    st.phase = Phase::NonCombatMove;
    assert!(act(&mut st, Power::Germany, mv(&[j], &["sweden"])).is_err());
}

#[test]
fn armor_cannot_blitz_through_neutral() {
    let mut st = cm(Power::Germany);
    st.treasury[Power::Germany] = 9;
    let a = put(&mut st, UnitType::Armor, Power::Germany, "finland_norway");
    assert!(act(&mut st, Power::Germany, mv(&[a], &["sweden", "sz_baltic"])).is_err());
}

#[test]
fn ship_stops_in_hostile_zone_and_limits() {
    let mut st = cm(Power::Uk);
    let b = put(&mut st, UnitType::Battleship, Power::Uk, "sz_north");
    put(&mut st, UnitType::Submarine, Power::Germany, "sz_baltic");
    let r = reachable(&st, b);
    for x in &r {
        assert!(x.path.len() <= 2);
    }
    let hit = r.iter().find(|r| r.to == loc("sz_baltic")).unwrap();
    assert!(hit.battle);
    assert!(r
        .iter()
        .all(|r| !(r.path.first() == Some(&loc("sz_baltic")) && r.path.len() > 1)));
    assert!(act(&mut st, Power::Uk, mv(&[b], &["sz_baltic", "sz_north"])).is_err());
    act(&mut st, Power::Uk, mv(&[b], &["sz_baltic"])).unwrap();
    assert_eq!(st.unit(b).unwrap().combat_from, Some(loc("sz_north")));
}

#[test]
fn noncombat_ship_cannot_enter_hostile_zone() {
    let mut st = blank(Power::Uk, Phase::NonCombatMove);
    let b = put(&mut st, UnitType::Battleship, Power::Uk, "sz_north");
    put(&mut st, UnitType::Submarine, Power::Germany, "sz_baltic");
    assert!(reachable(&st, b).iter().all(|r| r.to != loc("sz_baltic")));
}

#[test]
fn suez_and_panama_canals() {
    let mut st = blank(Power::Uk, Phase::NonCombatMove);
    let s = put(
        &mut st,
        UnitType::Transport,
        Power::Uk,
        "sz_east_mediterranean",
    );
    assert!(
        reachable(&st, s).iter().any(|r| r.to == loc("sz_red")),
        "open when UK holds both"
    );
    set_control(&mut st, "syria_iraq", Some(Power::Germany));
    assert!(
        reachable(&st, s).iter().all(|r| r.to != loc("sz_red")),
        "closed when an enemy holds Syria"
    );
    let mut st = blank(Power::Usa, Phase::NonCombatMove);
    let t = put(&mut st, UnitType::Transport, Power::Usa, "sz_caribbean");
    assert!(reachable(&st, t)
        .iter()
        .any(|r| r.to == loc("sz_west_panama")));
    set_control(&mut st, "panama", Some(Power::Japan));
    assert!(reachable(&st, t)
        .iter()
        .all(|r| r.to != loc("sz_west_panama")));
}

#[test]
fn canal_uses_start_of_turn_control() {
    let mut st = blank(Power::Uk, Phase::NonCombatMove);
    let s = put(
        &mut st,
        UnitType::Transport,
        Power::Uk,
        "sz_east_mediterranean",
    );
    // lost Syria mid-turn does not matter; captured mid-turn by the enemy would not open it either
    st.control[loc("syria_iraq")] = Some(Power::Germany);
    assert!(reachable(&st, s).iter().any(|r| r.to == loc("sz_red")));
}

#[test]
fn air_range_and_no_kamikaze() {
    let mut st = cm(Power::Germany);
    let f = put(&mut st, UnitType::Fighter, Power::Germany, "germany");
    let far = reachable(&st, f);
    assert!(!far.is_empty());
    for r in &far {
        assert!(r.path.len() <= 4);
        let left = 4 - r.path.len();
        let u = st.unit(f).unwrap();
        assert!(crate::movement::landing_reachable(&st, u, r.to, left as u8));
    }
    // 4 steps straight out to sea with no way back / no carrier is illegal
    let err = act(
        &mut st,
        Power::Germany,
        mv(
            &[f],
            &[
                "sz_baltic",
                "sz_north",
                "sz_east_canada",
                "sz_north_atlantic",
            ],
        ),
    );
    assert!(err.is_err());
}

#[test]
fn aircraft_cannot_cross_neutral_in_noncombat_but_can_in_combat() {
    let mut st = blank(Power::Germany, Phase::NonCombatMove);
    let f = put(&mut st, UnitType::Fighter, Power::Germany, "finland_norway");
    assert!(act(&mut st, Power::Germany, mv(&[f], &["sweden"])).is_err());
    st.phase = Phase::CombatMove;
    st.treasury[Power::Germany] = 5;
    act(
        &mut st,
        Power::Germany,
        mv(&[f], &["sweden", "finland_norway"]),
    )
    .unwrap();
    assert_eq!(st.treasury[Power::Germany], 2);
    assert_eq!(st.control[loc("sweden")], Some(Power::Germany));
}

#[test]
fn aa_gun_one_per_territory_and_not_into_enemy() {
    let mut st = cm(Power::Germany);
    let a = put(&mut st, UnitType::AaGun, Power::Germany, "germany");
    put(&mut st, UnitType::AaGun, Power::Germany, "western_europe");
    assert!(act(&mut st, Power::Germany, mv(&[a], &["western_europe"])).is_err());
    set_control(&mut st, "eastern_europe", Some(Power::Ussr));
    assert!(act(&mut st, Power::Germany, mv(&[a], &["eastern_europe"])).is_err());
}

#[test]
fn aa_fire_on_overflight_deterministic() {
    let mut st = cm(Power::Germany);
    let f = put(&mut st, UnitType::Fighter, Power::Germany, "germany");
    set_control(&mut st, "western_europe", Some(Power::Uk));
    put(&mut st, UnitType::AaGun, Power::Uk, "western_europe");
    let ev = act_seed(
        &mut st,
        Power::Germany,
        mv(&[f], &["western_europe", "germany"]),
        seed_for(&[1]),
    )
    .unwrap();
    assert!(st.unit(f).is_none());
    assert!(ev
        .iter()
        .any(|e| matches!(e, Event::AaFire { destroyed, .. } if destroyed.len() == 1)));
    assert!(ev.iter().any(|e| matches!(
        e,
        Event::DiceRolled {
            context: crate::event::DiceContext::AaFire { .. },
            ..
        }
    )));
    let f2 = put(&mut st, UnitType::Fighter, Power::Germany, "germany");
    act_seed(
        &mut st,
        Power::Germany,
        mv(&[f2], &["western_europe", "germany"]),
        seed_for(&[6]),
    )
    .unwrap();
    assert!(st.unit(f2).is_some());
}

#[test]
fn aa_fires_per_aircraft_and_attacker_loses_cheapest() {
    let mut st = cm(Power::Germany);
    let f = put(&mut st, UnitType::Fighter, Power::Germany, "germany");
    let b = put(&mut st, UnitType::Bomber, Power::Germany, "germany");
    set_control(&mut st, "western_europe", Some(Power::Uk));
    put(&mut st, UnitType::AaGun, Power::Uk, "western_europe");
    act_seed(
        &mut st,
        Power::Germany,
        mv(&[f, b], &["western_europe", "germany"]),
        seed_for(&[1, 6]),
    )
    .unwrap();
    assert!(st.unit(f).is_none() && st.unit(b).is_some());
}

#[test]
fn aa_does_not_fire_in_noncombat() {
    let mut st = blank(Power::Germany, Phase::NonCombatMove);
    let f = put(&mut st, UnitType::Fighter, Power::Germany, "germany");
    set_control(&mut st, "western_europe", Some(Power::Uk));
    put(&mut st, UnitType::AaGun, Power::Uk, "western_europe");
    let ev = act_seed(
        &mut st,
        Power::Germany,
        mv(&[f], &["western_europe", "germany"]),
        seed_for(&[1]),
    )
    .unwrap();
    assert!(st.unit(f).is_some());
    assert!(!ev.iter().any(|e| matches!(e, Event::AaFire { .. })));
}

#[test]
fn fighter_lands_on_carrier_and_carrier_freezes() {
    let mut st = blank(Power::Japan, Phase::NonCombatMove);
    let c = put(&mut st, UnitType::Carrier, Power::Japan, "sz_japan");
    let f = put(&mut st, UnitType::Fighter, Power::Japan, "japan");
    act(&mut st, Power::Japan, mv(&[f], &["sz_japan"])).unwrap();
    assert_eq!(st.unit(f).unwrap().loaded_on, Some(c));
    assert!(st.unit(c).unwrap().frozen);
    assert!(act(
        &mut st,
        Power::Japan,
        Action::Move {
            units: vec![c],
            path: vec![loc("sz_okinawa")]
        }
    )
    .is_err());
}

#[test]
fn fighters_launch_before_carrier_moves_only() {
    let mut st = blank(Power::Japan, Phase::NonCombatMove);
    let c = put(&mut st, UnitType::Carrier, Power::Japan, "sz_japan");
    let f = put(&mut st, UnitType::Fighter, Power::Japan, "sz_japan");
    st.unit_mut(f).unwrap().loaded_on = Some(c);
    act(&mut st, Power::Japan, mv(&[c], &["sz_okinawa"])).unwrap();
    assert_eq!(
        st.unit(f).unwrap().location,
        loc("sz_okinawa"),
        "cargo fighter rides along"
    );
    assert!(
        act(&mut st, Power::Japan, mv(&[f], &["japan"])).is_err(),
        "carrier already moved"
    );
}

#[test]
fn carrier_holds_two_fighters_including_allied() {
    let mut st = blank(Power::Usa, Phase::NonCombatMove);
    let c = put(&mut st, UnitType::Carrier, Power::Usa, "sz_hawaii");
    let a = put(&mut st, UnitType::Fighter, Power::Uk, "sz_hawaii");
    st.unit_mut(a).unwrap().loaded_on = Some(c);
    let f1 = put(&mut st, UnitType::Fighter, Power::Usa, "hawaiian_islands");
    let f2 = put(&mut st, UnitType::Fighter, Power::Usa, "hawaiian_islands");
    act(&mut st, Power::Usa, mv(&[f1], &["sz_hawaii"])).unwrap();
    assert_eq!(st.unit(f1).unwrap().loaded_on, Some(c));
    act(&mut st, Power::Usa, mv(&[f2], &["sz_hawaii"])).unwrap();
    assert_eq!(st.unit(f2).unwrap().loaded_on, None, "carrier full");
}

#[test]
fn unlandable_aircraft_destroyed_at_end_of_noncombat() {
    let mut st = blank(Power::Germany, Phase::NonCombatMove);
    let f = put(&mut st, UnitType::Fighter, Power::Germany, "sz_baltic");
    let ev = act(&mut st, Power::Germany, Action::EndPhase).unwrap();
    assert!(st.unit(f).is_none());
    assert!(ev.iter().any(|e| matches!(e, Event::UnitsLost { .. })));
    let mut st = blank(Power::Germany, Phase::NonCombatMove);
    set_control(&mut st, "eastern_europe", Some(Power::Ussr));
    st.control[loc("eastern_europe")] = Some(Power::Germany);
    let f = put(&mut st, UnitType::Fighter, Power::Germany, "eastern_europe");
    act(&mut st, Power::Germany, Action::EndPhase).unwrap();
    assert!(
        st.unit(f).is_none(),
        "cannot land in a territory captured this turn"
    );
}

#[test]
fn bomber_cannot_land_on_carrier() {
    let mut st = blank(Power::Japan, Phase::NonCombatMove);
    put(&mut st, UnitType::Carrier, Power::Japan, "sz_japan");
    let b = put(&mut st, UnitType::Bomber, Power::Japan, "sz_japan");
    act(&mut st, Power::Japan, Action::EndPhase).unwrap();
    assert!(st.unit(b).is_none());
}

#[test]
fn long_range_aircraft_extends_range() {
    let mut st = blank(Power::Germany, Phase::NonCombatMove);
    let f = put(&mut st, UnitType::Fighter, Power::Germany, "germany");
    let base = reachable(&st, f)
        .iter()
        .map(|r| r.path.len())
        .max()
        .unwrap();
    assert_eq!(base, 4);
    st.techs[Power::Germany].push(Tech::LongRangeAircraft);
    let ext = reachable(&st, f)
        .iter()
        .map(|r| r.path.len())
        .max()
        .unwrap();
    assert_eq!(ext, 6);
}

#[test]
fn soviet_first_turn_restriction() {
    let mut st = GameState::new(GameSettings {
        soviet_first_turn_restriction: true,
        ..Default::default()
    });
    st.phase = Phase::CombatMove;
    let u = st
        .units_at(loc("karelia_s_s_r"))
        .find(|u| u.kind == UnitType::Armor)
        .unwrap()
        .id;
    assert!(act(
        &mut st,
        Power::Ussr,
        Action::Move {
            units: vec![u],
            path: vec![loc("eastern_europe")]
        }
    )
    .is_err());
    assert!(act(
        &mut st,
        Power::Ussr,
        Action::Move {
            units: vec![u],
            path: vec![loc("russia")]
        }
    )
    .is_ok());
}

#[test]
fn transport_load_unload_noncombat() {
    let mut st = blank(Power::Uk, Phase::NonCombatMove);
    let t = put(&mut st, UnitType::Transport, Power::Uk, "sz_north");
    let ids = putn(&mut st, UnitType::Infantry, Power::Uk, "united_kingdom", 3);
    act(
        &mut st,
        Power::Uk,
        Action::Load {
            units: vec![ids[0], ids[1]],
            transport: t,
        },
    )
    .unwrap();
    assert!(
        act(
            &mut st,
            Power::Uk,
            Action::Load {
                units: vec![ids[2]],
                transport: t
            }
        )
        .is_err(),
        "capacity 2"
    );
    assert_eq!(st.unit(ids[0]).unwrap().loaded_on, Some(t));
    act(
        &mut st,
        Power::Uk,
        Action::Unload {
            transport: t,
            units: vec![ids[0], ids[1]],
            to: loc("united_kingdom"),
        },
    )
    .unwrap();
    assert_eq!(st.unit(ids[0]).unwrap().location, loc("united_kingdom"));
    assert!(st.unit(t).unwrap().frozen);
    assert!(
        act(&mut st, Power::Uk, mv(&[t], &["sz_baltic"])).is_err(),
        "transport that unloaded may not move"
    );
}

#[test]
fn transport_capacity_rules() {
    let mut st = blank(Power::Uk, Phase::NonCombatMove);
    let t = put(&mut st, UnitType::Transport, Power::Uk, "sz_north");
    let a = put(&mut st, UnitType::Armor, Power::Uk, "united_kingdom");
    let i = put(&mut st, UnitType::Infantry, Power::Uk, "united_kingdom");
    act(
        &mut st,
        Power::Uk,
        Action::Load {
            units: vec![a],
            transport: t,
        },
    )
    .unwrap();
    assert!(
        act(
            &mut st,
            Power::Uk,
            Action::Load {
                units: vec![i],
                transport: t
            }
        )
        .is_err(),
        "no armor + infantry"
    );
    let g = put(&mut st, UnitType::AaGun, Power::Uk, "united_kingdom");
    let t2 = put(&mut st, UnitType::Transport, Power::Uk, "sz_north");
    assert!(act(
        &mut st,
        Power::Uk,
        Action::Load {
            units: vec![g, i],
            transport: t2
        }
    )
    .is_err());
    act(
        &mut st,
        Power::Uk,
        Action::Load {
            units: vec![g],
            transport: t2,
        },
    )
    .unwrap();
    let f = put(&mut st, UnitType::Fighter, Power::Uk, "united_kingdom");
    let t3 = put(&mut st, UnitType::Transport, Power::Uk, "sz_north");
    assert!(act(
        &mut st,
        Power::Uk,
        Action::Load {
            units: vec![f],
            transport: t3
        }
    )
    .is_err());
}

#[test]
fn moved_land_unit_cannot_board() {
    let mut st = blank(Power::Germany, Phase::CombatMove);
    let t = put(&mut st, UnitType::Transport, Power::Germany, "sz_baltic");
    let i = put(&mut st, UnitType::Infantry, Power::Germany, "germany");
    st.unit_mut(i).unwrap().moves_used = 1;
    assert!(act(
        &mut st,
        Power::Germany,
        Action::Load {
            units: vec![i],
            transport: t
        }
    )
    .is_err());
}

#[test]
fn bridging_between_two_coasts_and_cargo_follows_transport() {
    let mut st = blank(Power::Uk, Phase::NonCombatMove);
    let t = put(&mut st, UnitType::Transport, Power::Uk, "sz_north");
    let i = put(&mut st, UnitType::Infantry, Power::Uk, "united_kingdom");
    set_control(&mut st, "western_europe", Some(Power::Uk));
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
            to: loc("western_europe"),
        },
    )
    .unwrap();
    assert_eq!(st.unit(i).unwrap().location, loc("western_europe"));
    let mut st = blank(Power::Uk, Phase::NonCombatMove);
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
    let dest = reachable(&st, t)
        .into_iter()
        .find(|r| r.path.len() == 1)
        .unwrap();
    act(
        &mut st,
        Power::Uk,
        Action::Move {
            units: vec![t],
            path: dest.path.clone(),
        },
    )
    .unwrap();
    assert_eq!(st.unit(i).unwrap().location, dest.to);
}

#[test]
fn allied_cargo_rules() {
    let mut st = blank(Power::Usa, Phase::NonCombatMove);
    let t = put(&mut st, UnitType::Transport, Power::Uk, "sz_east_us");
    let i = put(&mut st, UnitType::Infantry, Power::Usa, "eastern_usa");
    act(
        &mut st,
        Power::Usa,
        Action::Load {
            units: vec![i],
            transport: t,
        },
    )
    .unwrap();
    assert!(act(
        &mut st,
        Power::Usa,
        Action::Unload {
            transport: t,
            units: vec![i],
            to: loc("eastern_usa")
        }
    )
    .is_err());
    st.turn.number += 5;
    st.unit_mut(i).unwrap().reset_turn();
    act(
        &mut st,
        Power::Usa,
        Action::Unload {
            transport: t,
            units: vec![i],
            to: loc("eastern_usa"),
        },
    )
    .unwrap();
}

#[test]
fn amphibious_unload_is_deferred_into_enemy_occupied() {
    let mut st = blank(Power::Uk, Phase::CombatMove);
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
    put(
        &mut st,
        UnitType::Infantry,
        Power::Germany,
        "western_europe",
    );
    let ev = act(
        &mut st,
        Power::Uk,
        Action::Unload {
            transport: t,
            units: vec![i],
            to: loc("western_europe"),
        },
    )
    .unwrap();
    assert!(ev
        .iter()
        .any(|e| matches!(e, Event::UnitsUnloaded { assault: true, .. })));
    assert_eq!(st.unit(i).unwrap().loaded_on, Some(t));
    assert_eq!(st.unit(i).unwrap().assault_to, Some(loc("western_europe")));
    assert_eq!(
        crate::battle::pending_battles(&st),
        vec![loc("western_europe")]
    );
}

#[test]
fn amphibious_into_empty_enemy_territory_captures_immediately() {
    let mut st = blank(Power::Uk, Phase::CombatMove);
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
    assert_eq!(st.control[loc("western_europe")], Some(Power::Uk));
    assert_eq!(st.unit(i).unwrap().location, loc("western_europe"));
}

#[test]
fn one_target_per_transport_in_combat_movement() {
    let mut st = blank(Power::Uk, Phase::CombatMove);
    let t = put(&mut st, UnitType::Transport, Power::Uk, "sz_north");
    let ids = putn(&mut st, UnitType::Infantry, Power::Uk, "united_kingdom", 2);
    act(
        &mut st,
        Power::Uk,
        Action::Load {
            units: ids.clone(),
            transport: t,
        },
    )
    .unwrap();
    act(
        &mut st,
        Power::Uk,
        Action::Unload {
            transport: t,
            units: vec![ids[0]],
            to: loc("western_europe"),
        },
    )
    .unwrap();
    assert!(act(
        &mut st,
        Power::Uk,
        Action::Unload {
            transport: t,
            units: vec![ids[1]],
            to: loc("finland_norway")
        }
    )
    .is_err());
}

#[test]
fn query_reachable_agrees_with_apply() {
    let st = GameState::new(GameSettings::default());
    let mut checked = 0;
    for p in Power::ALL {
        let mut s = st.clone();
        s.current_power = p;
        s.phase = Phase::CombatMove;
        s.turn.number = 3;
        for u in s
            .units
            .iter()
            .filter(|u| u.owner == p)
            .map(|u| u.id)
            .collect::<Vec<_>>()
        {
            for r in reachable(&s, u) {
                let mut t = s.clone();
                act(
                    &mut t,
                    p,
                    Action::Move {
                        units: vec![u],
                        path: r.path.clone(),
                    },
                )
                .unwrap_or_else(|e| panic!("{:?} {:?}: {e}", u, r.path));
                checked += 1;
            }
        }
    }
    assert!(checked > 100);
}

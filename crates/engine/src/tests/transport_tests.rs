use super::*;
use crate::query::reachable;

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

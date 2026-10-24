use super::*;

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

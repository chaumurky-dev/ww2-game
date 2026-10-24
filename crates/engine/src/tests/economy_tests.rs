use super::*;
use crate::state::PurchaseItem;

fn item(unit: UnitType, count: u32) -> PurchaseItem {
    PurchaseItem { unit, count }
}

fn buy(st: &mut GameState, p: Power, items: &[PurchaseItem]) -> Result<Vec<Event>> {
    act(
        st,
        p,
        Action::Purchase {
            items: items.to_vec(),
        },
    )
}

fn place(st: &mut GameState, p: Power, kind: UnitType, at: &str, n: u32) -> Result<Vec<Event>> {
    act(
        st,
        p,
        Action::Place {
            unit: kind,
            location: loc(at),
            count: n,
            from: None,
        },
    )
}

// ---------- technology ----------

#[test]
fn research_pays_five_per_die_and_gains_tech_on_six() {
    let mut st = blank(Power::Germany, Phase::DevelopWeapons);
    st.treasury[Power::Germany] = 20;
    // dice 6,3 then tech die 4 = long range aircraft
    let ev = act_seed(
        &mut st,
        Power::Germany,
        Action::BuyResearch { dice: 2 },
        seed_for(&[6, 3, 4]),
    )
    .unwrap();
    assert_eq!(st.treasury[Power::Germany], 10);
    assert!(st.has_tech(Power::Germany, Tech::LongRangeAircraft));
    assert!(ev.iter().any(|e| matches!(
        e,
        Event::TechGained {
            tech: Tech::LongRangeAircraft,
            ..
        }
    )));
    assert!(ev.iter().any(|e| matches!(
        e,
        Event::ResearchBought {
            dice: 2,
            cost: 10,
            ..
        }
    )));
}

#[test]
fn research_failure_gives_nothing() {
    let mut st = blank(Power::Germany, Phase::DevelopWeapons);
    st.treasury[Power::Germany] = 20;
    act_seed(
        &mut st,
        Power::Germany,
        Action::BuyResearch { dice: 2 },
        seed_for(&[5, 3]),
    )
    .unwrap();
    assert!(st.techs[Power::Germany].is_empty());
    assert_eq!(st.treasury[Power::Germany], 10);
}

#[test]
fn research_rerolls_owned_tech() {
    let mut st = blank(Power::Germany, Phase::DevelopWeapons);
    st.treasury[Power::Germany] = 20;
    st.techs[Power::Germany].push(Tech::JetPower);
    act_seed(
        &mut st,
        Power::Germany,
        Action::BuyResearch { dice: 1 },
        seed_for(&[6, 1, 5]),
    )
    .unwrap();
    assert!(st.has_tech(Power::Germany, Tech::IndustrialTechnology));
    assert_eq!(st.techs[Power::Germany].len(), 2);
}

#[test]
fn each_tech_maps_to_die_roll() {
    let order = [
        Tech::JetPower,
        Tech::Rockets,
        Tech::SuperSubmarines,
        Tech::LongRangeAircraft,
        Tech::IndustrialTechnology,
        Tech::HeavyBombers,
    ];
    for (i, t) in order.iter().enumerate() {
        let mut st = blank(Power::Uk, Phase::DevelopWeapons);
        st.treasury[Power::Uk] = 5;
        act_seed(
            &mut st,
            Power::Uk,
            Action::BuyResearch { dice: 1 },
            seed_for(&[6, (i + 1) as u8]),
        )
        .unwrap();
        assert_eq!(st.techs[Power::Uk], vec![*t]);
    }
}

#[test]
fn research_limits() {
    let mut st = blank(Power::Germany, Phase::DevelopWeapons);
    st.treasury[Power::Germany] = 9;
    assert!(matches!(
        act(&mut st, Power::Germany, Action::BuyResearch { dice: 2 }),
        Err(RuleError::InsufficientFunds { .. })
    ));
    act(&mut st, Power::Germany, Action::BuyResearch { dice: 1 }).unwrap();
    assert!(
        act(&mut st, Power::Germany, Action::BuyResearch { dice: 1 }).is_err(),
        "once per turn"
    );
    let mut st = blank(Power::Germany, Phase::Purchase);
    assert!(act(&mut st, Power::Germany, Action::BuyResearch { dice: 1 }).is_err());
}

#[test]
fn industrial_technology_discounts_purchases() {
    let mut st = blank(Power::Germany, Phase::Purchase);
    st.treasury[Power::Germany] = 10;
    st.techs[Power::Germany].push(Tech::IndustrialTechnology);
    buy(&mut st, Power::Germany, &[item(UnitType::Armor, 2)]).unwrap();
    assert_eq!(st.treasury[Power::Germany], 2);
    assert_eq!(
        crate::tech::unit_cost(&st, Power::Germany, UnitType::Battleship),
        23
    );
}

// ---------- purchase ----------

#[test]
fn purchase_replaces_previous_list_with_refund() {
    let mut st = blank(Power::Uk, Phase::Purchase);
    st.treasury[Power::Uk] = 30;
    buy(&mut st, Power::Uk, &[item(UnitType::Bomber, 1)]).unwrap();
    assert_eq!(st.treasury[Power::Uk], 15);
    buy(&mut st, Power::Uk, &[item(UnitType::Infantry, 10)]).unwrap();
    assert_eq!(st.treasury[Power::Uk], 0);
    assert_eq!(st.purchases, vec![item(UnitType::Infantry, 10)]);
    assert!(matches!(
        buy(&mut st, Power::Uk, &[item(UnitType::Infantry, 11)]),
        Err(RuleError::InsufficientFunds { .. })
    ));
    assert_eq!(
        st.purchases,
        vec![item(UnitType::Infantry, 10)],
        "failed purchase leaves state"
    );
}

#[test]
fn purchase_options_report_affordability() {
    let mut st = blank(Power::Uk, Phase::Purchase);
    st.treasury[Power::Uk] = 17;
    let opts = crate::query::purchase_options(&st);
    let inf = opts.iter().find(|o| o.unit == UnitType::Infantry).unwrap();
    assert_eq!((inf.cost, inf.max_more), (3, 5));
    let ic = opts
        .iter()
        .find(|o| o.unit == UnitType::IndustrialComplex)
        .unwrap();
    assert_eq!(ic.max_more, 1);
}

#[test]
fn four_new_complex_cap_and_option_off() {
    let mut st = blank(Power::Usa, Phase::Purchase);
    st.treasury[Power::Usa] = 100;
    st.new_complexes_placed = 3;
    assert!(buy(&mut st, Power::Usa, &[item(UnitType::IndustrialComplex, 2)]).is_err());
    buy(&mut st, Power::Usa, &[item(UnitType::IndustrialComplex, 1)]).unwrap();
    st.settings.original_complexes_only = true;
    assert!(buy(&mut st, Power::Usa, &[item(UnitType::IndustrialComplex, 1)]).is_err());
}

#[test]
fn purchase_only_in_purchase_phase() {
    let mut st = blank(Power::Uk, Phase::CombatMove);
    assert!(buy(&mut st, Power::Uk, &[item(UnitType::Infantry, 1)]).is_err());
}

// ---------- placement ----------

fn mobilize(p: Power, items: &[PurchaseItem]) -> GameState {
    let mut st = blank(p, Phase::Mobilize);
    st.purchases = items.to_vec();
    st
}

#[test]
fn original_complex_has_no_limit() {
    let mut st = mobilize(Power::Uk, &[item(UnitType::Infantry, 12)]);
    put(
        &mut st,
        UnitType::IndustrialComplex,
        Power::Uk,
        "united_kingdom",
    );
    place(&mut st, Power::Uk, UnitType::Infantry, "united_kingdom", 12).unwrap();
    assert_eq!(count(&st, "united_kingdom", UnitType::Infantry), 12);
    assert!(st.purchases.is_empty());
}

#[test]
fn captured_complex_limited_to_territory_value() {
    // Germany holds Karelia (value 3) since the start of its turn
    let mut st = mobilize(Power::Germany, &[item(UnitType::Infantry, 5)]);
    set_control(&mut st, "karelia_s_s_r", Some(Power::Germany));
    put(
        &mut st,
        UnitType::IndustrialComplex,
        Power::Germany,
        "karelia_s_s_r",
    );
    assert!(place(
        &mut st,
        Power::Germany,
        UnitType::Infantry,
        "karelia_s_s_r",
        4
    )
    .is_err());
    place(
        &mut st,
        Power::Germany,
        UnitType::Infantry,
        "karelia_s_s_r",
        2,
    )
    .unwrap();
    assert!(place(
        &mut st,
        Power::Germany,
        UnitType::Infantry,
        "karelia_s_s_r",
        2
    )
    .is_err());
    place(
        &mut st,
        Power::Germany,
        UnitType::Infantry,
        "karelia_s_s_r",
        1,
    )
    .unwrap();
}

#[test]
fn liberated_original_complex_is_unlimited_again() {
    let mut st = mobilize(Power::Ussr, &[item(UnitType::Infantry, 6)]);
    put(
        &mut st,
        UnitType::IndustrialComplex,
        Power::Ussr,
        "karelia_s_s_r",
    );
    place(&mut st, Power::Ussr, UnitType::Infantry, "karelia_s_s_r", 6).unwrap();
}

#[test]
fn complex_captured_this_turn_cannot_be_used() {
    let mut st = mobilize(Power::Germany, &[item(UnitType::Infantry, 1)]);
    st.control[loc("karelia_s_s_r")] = Some(Power::Germany); // start control is USSR
    put(
        &mut st,
        UnitType::IndustrialComplex,
        Power::Germany,
        "karelia_s_s_r",
    );
    assert!(place(
        &mut st,
        Power::Germany,
        UnitType::Infantry,
        "karelia_s_s_r",
        1
    )
    .is_err());
}

#[test]
fn allied_complex_cannot_be_used() {
    let mut st = mobilize(Power::Usa, &[item(UnitType::Infantry, 1)]);
    set_control(&mut st, "united_kingdom", Some(Power::Uk));
    put(
        &mut st,
        UnitType::IndustrialComplex,
        Power::Uk,
        "united_kingdom",
    );
    assert!(place(&mut st, Power::Usa, UnitType::Infantry, "united_kingdom", 1).is_err());
}

#[test]
fn zero_value_territory_complex_supports_one_unit() {
    let mut st = mobilize(
        Power::Uk,
        &[
            item(UnitType::Infantry, 2),
            item(UnitType::IndustrialComplex, 1),
        ],
    );
    // Gibraltar (value 0, UK original): place a new complex there
    place(
        &mut st,
        Power::Uk,
        UnitType::IndustrialComplex,
        "gibraltar",
        1,
    )
    .unwrap();
    assert_eq!(st.new_complexes_placed, 1);
    // newly placed complex cannot produce this turn
    assert!(place(&mut st, Power::Uk, UnitType::Infantry, "gibraltar", 1).is_err());
    // next turn it can: 1 unit only (value 0)
    st.turn.number += 1;
    st.turn.start_control = st.control.clone();
    place(&mut st, Power::Uk, UnitType::Infantry, "gibraltar", 1).unwrap();
    assert!(place(&mut st, Power::Uk, UnitType::Infantry, "gibraltar", 1).is_err());
}

#[test]
fn global_new_complex_limit_enforced_on_placement() {
    let mut st = mobilize(Power::Uk, &[item(UnitType::IndustrialComplex, 1)]);
    st.new_complexes_placed = 4;
    assert!(place(&mut st, Power::Uk, UnitType::IndustrialComplex, "india", 1).is_err());
    st.new_complexes_placed = 3;
    place(&mut st, Power::Uk, UnitType::IndustrialComplex, "india", 1).unwrap();
    assert_eq!(st.new_complexes_placed, 4);
    assert!(st.complex_at(loc("india")).is_some());
}

#[test]
fn complex_needs_start_of_turn_control_and_free_slot() {
    let mut st = mobilize(Power::Uk, &[item(UnitType::IndustrialComplex, 2)]);
    st.control[loc("syria_iraq")] = Some(Power::Uk);
    st.turn.start_control[loc("syria_iraq")] = Some(Power::Germany);
    assert!(place(
        &mut st,
        Power::Uk,
        UnitType::IndustrialComplex,
        "syria_iraq",
        1
    )
    .is_err());
    assert!(place(
        &mut st,
        Power::Uk,
        UnitType::IndustrialComplex,
        "sz_north",
        1
    )
    .is_err());
    place(&mut st, Power::Uk, UnitType::IndustrialComplex, "persia", 1).unwrap();
    assert!(place(&mut st, Power::Uk, UnitType::IndustrialComplex, "persia", 1).is_err());
}

#[test]
fn ships_go_to_adjacent_zone_free_of_enemies() {
    let mut st = mobilize(
        Power::Uk,
        &[item(UnitType::Transport, 2), item(UnitType::Battleship, 1)],
    );
    put(
        &mut st,
        UnitType::IndustrialComplex,
        Power::Uk,
        "united_kingdom",
    );
    place(&mut st, Power::Uk, UnitType::Transport, "sz_north", 1).unwrap();
    assert!(
        place(&mut st, Power::Uk, UnitType::Transport, "sz_baltic", 1).is_err(),
        "not adjacent to a complex"
    );
    put(&mut st, UnitType::Submarine, Power::Germany, "sz_north");
    assert!(
        place(&mut st, Power::Uk, UnitType::Battleship, "sz_north", 1).is_err(),
        "enemy in zone"
    );
    assert!(place(
        &mut st,
        Power::Uk,
        UnitType::Battleship,
        "united_kingdom",
        1
    )
    .is_err());
    assert!(crate::query::legal_placements(&st, UnitType::Battleship).is_empty());
}

#[test]
fn aircraft_land_only_and_aa_unique() {
    let mut st = mobilize(
        Power::Germany,
        &[item(UnitType::Fighter, 1), item(UnitType::AaGun, 2)],
    );
    put(
        &mut st,
        UnitType::IndustrialComplex,
        Power::Germany,
        "germany",
    );
    assert!(place(&mut st, Power::Germany, UnitType::Fighter, "sz_baltic", 1).is_err());
    place(&mut st, Power::Germany, UnitType::Fighter, "germany", 1).unwrap();
    place(&mut st, Power::Germany, UnitType::AaGun, "germany", 1).unwrap();
    assert!(place(&mut st, Power::Germany, UnitType::AaGun, "germany", 1).is_err());
}

#[test]
fn cannot_place_what_was_not_bought() {
    let mut st = mobilize(Power::Germany, &[item(UnitType::Infantry, 1)]);
    put(
        &mut st,
        UnitType::IndustrialComplex,
        Power::Germany,
        "germany",
    );
    assert!(place(&mut st, Power::Germany, UnitType::Infantry, "germany", 2).is_err());
    assert!(place(&mut st, Power::Germany, UnitType::Armor, "germany", 1).is_err());
    assert!(place(&mut st, Power::Germany, UnitType::Infantry, "germany", 0).is_err());
}

#[test]
fn unplaced_units_are_lost_at_end_of_mobilize() {
    let mut st = mobilize(Power::Germany, &[item(UnitType::Infantry, 3)]);
    put(
        &mut st,
        UnitType::IndustrialComplex,
        Power::Germany,
        "germany",
    );
    let ev = act(&mut st, Power::Germany, Action::EndPhase).unwrap();
    assert!(ev
        .iter()
        .any(|e| matches!(e, Event::UnitsLost { reason, .. } if reason.contains("placed"))));
    assert!(st.purchases.is_empty());
}

#[test]
fn shared_ship_allowance_between_adjacent_complexes() {
    // A zone adjacent to a limited complex: allowance is consumed by ships too.
    let mut st = mobilize(Power::Germany, &[item(UnitType::Transport, 3)]);
    set_control(&mut st, "karelia_s_s_r", Some(Power::Germany));
    put(
        &mut st,
        UnitType::IndustrialComplex,
        Power::Germany,
        "karelia_s_s_r",
    );
    place(
        &mut st,
        Power::Germany,
        UnitType::Transport,
        "sz_karelia",
        3,
    )
    .unwrap();
    assert!(st.purchases.is_empty());
}

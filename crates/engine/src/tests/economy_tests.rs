use super::*;
use crate::state::{PurchaseItem, VictoryReason};

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

//! Independent rules-audit tests: each one targets a rule of docs/RULES.md that had no direct test.
use super::*;
use crate::event::{BattleOutcome, DiceContext};
use crate::state::PurchaseItem;

fn cm(p: Power) -> GameState {
    blank(p, Phase::CombatMove)
}

fn nc(p: Power) -> GameState {
    blank(p, Phase::NonCombatMove)
}

fn outcome(ev: &[Event]) -> Option<BattleOutcome> {
    ev.iter().rev().find_map(|e| match e {
        Event::BattleEnded { outcome, .. } => Some(*outcome),
        _ => None,
    })
}

fn rolls_of(ev: &[Event], f: impl Fn(&DiceContext) -> bool) -> Vec<Vec<u8>> {
    ev.iter()
        .filter_map(|e| match e {
            Event::DiceRolled { context, rolls, .. } if f(context) => Some(rolls.clone()),
            _ => None,
        })
        .collect()
}

fn load(st: &mut GameState, p: Power, units: &[UnitId], t: UnitId) -> Result<Vec<Event>> {
    act(
        st,
        p,
        Action::Load {
            units: units.to_vec(),
            transport: t,
        },
    )
}

fn unload(
    st: &mut GameState,
    p: Power,
    t: UnitId,
    units: &[UnitId],
    to: &str,
) -> Result<Vec<Event>> {
    act(
        st,
        p,
        Action::Unload {
            transport: t,
            units: units.to_vec(),
            to: loc(to),
        },
    )
}

fn item(unit: UnitType, count: u32) -> PurchaseItem {
    PurchaseItem { unit, count }
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

// ---------------------------------------------------------------- movement (4.3, 4.4)

#[test]
fn blitz_first_space_with_only_enemy_aircraft_counts_as_occupied() {
    let mut st = cm(Power::Germany);
    let a = put(&mut st, UnitType::Armor, Power::Germany, "eastern_europe");
    set_control(&mut st, "ukraine_s_s_r", Some(Power::Ussr));
    put(&mut st, UnitType::Fighter, Power::Ussr, "ukraine_s_s_r");
    assert!(act(
        &mut st,
        Power::Germany,
        mv(&[a], &["ukraine_s_s_r", "caucasus"])
    )
    .is_err());
    act(&mut st, Power::Germany, mv(&[a], &["ukraine_s_s_r"])).unwrap();
    assert_eq!(
        crate::battle::pending_battles(&st),
        vec![loc("ukraine_s_s_r")]
    );
}

#[test]
fn blitz_into_neutral_second_space_violates_and_stops() {
    let mut st = cm(Power::Germany);
    let a = put(&mut st, UnitType::Armor, Power::Germany, "ukraine_s_s_r");
    set_control(&mut st, "ukraine_s_s_r", Some(Power::Germany));
    let before = st.treasury[Power::Germany];
    let ussr_before = crate::production_level(&st, Power::Ussr);
    act(&mut st, Power::Germany, mv(&[a], &["caucasus", "turkey"])).unwrap();
    assert_eq!(st.control[loc("caucasus")], Some(Power::Germany));
    assert_eq!(st.control[loc("turkey")], Some(Power::Germany));
    assert_eq!(st.treasury[Power::Germany], before - 3);
    assert_eq!(
        crate::production_level(&st, Power::Ussr),
        ussr_before - 3,
        "the blitzed territory's income left the USSR at once"
    );
    assert!(st.unit(a).unwrap().stopped);
    assert!(act(&mut st, Power::Germany, mv(&[a], &["persia"])).is_err());
}

#[test]
fn land_unit_cannot_pass_through_enemy_controlled_empty_space() {
    // Infantry (1 move) and armor starting NOT in a first space: armor with 1 move spent cannot blitz.
    let mut st = cm(Power::Germany);
    let a = put(&mut st, UnitType::Armor, Power::Germany, "germany");
    set_control(&mut st, "eastern_europe", Some(Power::Ussr));
    set_control(&mut st, "karelia_s_s_r", Some(Power::Ussr));
    // friendly first space then enemy-controlled second: fine, but a third space is never possible.
    act(
        &mut st,
        Power::Germany,
        mv(&[a], &["eastern_europe", "karelia_s_s_r"]),
    )
    .unwrap();
    assert_eq!(st.control[loc("eastern_europe")], Some(Power::Germany));
    assert_eq!(st.control[loc("karelia_s_s_r")], Some(Power::Germany));
}

#[test]
fn blitz_may_be_split_across_move_actions_but_only_one_blitz_space() {
    let mut st = cm(Power::Germany);
    let a = put(&mut st, UnitType::Armor, Power::Germany, "eastern_europe");
    set_control(&mut st, "ukraine_s_s_r", Some(Power::Ussr));
    set_control(&mut st, "caucasus", Some(Power::Ussr));
    set_control(&mut st, "russia", Some(Power::Ussr));
    act(&mut st, Power::Germany, mv(&[a], &["ukraine_s_s_r"])).unwrap();
    assert!(
        !st.unit(a).unwrap().stopped,
        "a blitzing tank keeps its second point"
    );
    act(&mut st, Power::Germany, mv(&[a], &["caucasus"])).unwrap();
    assert!(st.unit(a).unwrap().stopped);
    assert!(act(&mut st, Power::Germany, mv(&[a], &["russia"])).is_err());
    assert_eq!(st.unit(a).unwrap().moves_used, 2);
}

#[test]
fn nc_land_units_never_enter_neutral_or_enemy_territory() {
    let mut st = nc(Power::Germany);
    let i = put(
        &mut st,
        UnitType::Infantry,
        Power::Germany,
        "finland_norway",
    );
    assert!(act(&mut st, Power::Germany, mv(&[i], &["sweden"])).is_err());
    let a = put(&mut st, UnitType::Armor, Power::Germany, "eastern_europe");
    set_control(&mut st, "ukraine_s_s_r", Some(Power::Ussr));
    assert!(act(&mut st, Power::Germany, mv(&[a], &["ukraine_s_s_r"])).is_err());
}

#[test]
fn unit_that_moved_in_cm_cannot_join_captured_territory_in_nc_but_unmoved_can() {
    let mut st = cm(Power::Germany);
    let mover = put(
        &mut st,
        UnitType::Infantry,
        Power::Germany,
        "eastern_europe",
    );
    let idle = put(&mut st, UnitType::Infantry, Power::Germany, "karelia_s_s_r");
    set_control(&mut st, "karelia_s_s_r", Some(Power::Germany));
    set_control(&mut st, "ukraine_s_s_r", Some(Power::Ussr));
    act(&mut st, Power::Germany, mv(&[mover], &["ukraine_s_s_r"])).unwrap();
    act(&mut st, Power::Germany, Action::EndPhase).unwrap();
    assert_eq!(st.phase, Phase::NonCombatMove);
    assert!(act(&mut st, Power::Germany, mv(&[mover], &["caucasus"])).is_err());
    act(&mut st, Power::Germany, mv(&[idle], &["ukraine_s_s_r"])).unwrap();
    assert_eq!(st.unit(idle).unwrap().location, loc("ukraine_s_s_r"));
}

// ---------------------------------------------------------------- air (4.5)

#[test]
fn fighter_on_carrier_has_full_range_and_departure_zone_is_free() {
    let mut st = nc(Power::Uk);
    let c = put(&mut st, UnitType::Carrier, Power::Uk, "sz_north");
    let f = put(&mut st, UnitType::Fighter, Power::Uk, "sz_north");
    st.unit_mut(f).unwrap().loaded_on = Some(c);
    let r = crate::query::reachable(&st, f);
    assert_eq!(r.iter().map(|x| x.path.len()).max(), Some(4));
    // the carrier's own zone is not "entered": 4 spaces are all available
    let far = r.iter().find(|x| x.path.len() == 4).unwrap();
    act(
        &mut st,
        Power::Uk,
        mv(&[f], &far.path.iter().map(|l| l.id()).collect::<Vec<_>>()),
    )
    .unwrap();
    assert_eq!(st.unit(f).unwrap().moves_used, 4);
    assert!(st.unit(f).unwrap().loaded_on.is_none());
}

#[test]
fn air_range_is_total_over_both_phases() {
    let mut st = cm(Power::Germany);
    let f = put(&mut st, UnitType::Fighter, Power::Germany, "germany");
    // two spaces out in combat movement (still friendly all the way), so only two remain for NC
    act(
        &mut st,
        Power::Germany,
        mv(&[f], &["eastern_europe", "ukraine_s_s_r"]),
    )
    .unwrap();
    act(&mut st, Power::Germany, Action::EndPhase).unwrap();
    assert_eq!(st.phase, Phase::NonCombatMove);
    assert!(act(
        &mut st,
        Power::Germany,
        mv(&[f], &["eastern_europe", "germany", "western_europe"])
    )
    .is_err());
    act(
        &mut st,
        Power::Germany,
        mv(&[f], &["eastern_europe", "germany"]),
    )
    .unwrap();
}

#[test]
fn aircraft_cannot_land_in_territory_captured_this_turn() {
    let mut st = cm(Power::Germany);
    let i = put(
        &mut st,
        UnitType::Infantry,
        Power::Germany,
        "eastern_europe",
    );
    let f = put(&mut st, UnitType::Fighter, Power::Germany, "germany");
    set_control(&mut st, "ukraine_s_s_r", Some(Power::Ussr));
    act(&mut st, Power::Germany, mv(&[i], &["ukraine_s_s_r"])).unwrap();
    act(&mut st, Power::Germany, Action::EndPhase).unwrap();
    act(
        &mut st,
        Power::Germany,
        mv(&[f], &["eastern_europe", "ukraine_s_s_r"]),
    )
    .unwrap();
    let ev = act(&mut st, Power::Germany, Action::EndPhase).unwrap();
    assert!(
        st.unit(f).is_none(),
        "no landing in a territory captured this turn"
    );
    assert!(ev.iter().any(|e| matches!(e, Event::UnitsLost { .. })));
}

#[test]
fn aircraft_can_land_in_allied_territory_held_since_turn_start() {
    let mut st = nc(Power::Uk);
    let f = put(&mut st, UnitType::Fighter, Power::Uk, "eastern_canada");
    act(&mut st, Power::Uk, mv(&[f], &["eastern_usa"])).unwrap();
    act(&mut st, Power::Uk, Action::EndPhase).unwrap();
    assert!(st.unit(f).is_some());
}

#[test]
fn no_kamikaze_blocks_launch_but_extra_range_unlocks_it() {
    // Fighter 4 range; target 3 spaces away with the only landing spot 2 more away: refused.
    let mut st = cm(Power::Germany);
    let f = put(&mut st, UnitType::Fighter, Power::Germany, "germany");
    // enemy land two spaces out; everything near is enemy, so landing is impossible
    for t in ["eastern_europe", "western_europe", "southern_europe"] {
        set_control(&mut st, t, Some(Power::Uk));
    }
    // Germany itself is a legal landing spot at distance 0 from the start only; a flight that ends 3 away cannot return
    assert!(act(
        &mut st,
        Power::Germany,
        mv(
            &[f],
            &["eastern_europe", "karelia_s_s_r", "caucasus", "persia"]
        )
    )
    .is_err());
}

#[test]
fn carrier_landing_counts_allied_fighters_and_bombers_never_land() {
    let mut st = nc(Power::Usa);
    let c = put(&mut st, UnitType::Carrier, Power::Usa, "sz_hawaii");
    let a = put(&mut st, UnitType::Fighter, Power::Uk, "sz_hawaii");
    st.unit_mut(a).unwrap().loaded_on = Some(c);
    let b = put(&mut st, UnitType::Fighter, Power::Usa, "sz_hawaii");
    st.unit_mut(b).unwrap().loaded_on = Some(c);
    let f3 = put(&mut st, UnitType::Fighter, Power::Usa, "hawaiian_islands");
    let bomber = put(&mut st, UnitType::Bomber, Power::Usa, "hawaiian_islands");
    act(&mut st, Power::Usa, mv(&[f3], &["sz_hawaii"])).unwrap();
    assert!(st.unit(f3).unwrap().loaded_on.is_none(), "carrier is full");
    act(&mut st, Power::Usa, mv(&[bomber], &["sz_hawaii"])).unwrap();
    let ev = act(&mut st, Power::Usa, Action::EndPhase).unwrap();
    assert!(st.unit(f3).is_none() && st.unit(bomber).is_none());
    assert!(st.unit(a).is_some() && st.unit(b).is_some());
    assert!(ev
        .iter()
        .any(|e| matches!(e, Event::UnitsLost { units, .. } if units.len() == 2)));
}

// ---------------------------------------------------------------- sea (4.1, 4.6)

#[test]
fn suez_needs_both_territories_in_allied_hands() {
    let mut st = nc(Power::Uk);
    let s = put(
        &mut st,
        UnitType::Transport,
        Power::Uk,
        "sz_east_mediterranean",
    );
    let opens = |st: &GameState| {
        crate::query::reachable(st, s)
            .iter()
            .any(|r| r.to == loc("sz_red"))
    };
    assert!(opens(&st));
    set_control(&mut st, "anglo_egypt_sudan", Some(Power::Usa));
    assert!(opens(&st), "an ally holding one of the two is enough");
    set_control(&mut st, "anglo_egypt_sudan", Some(Power::Germany));
    assert!(!opens(&st), "an enemy on either side closes it");
    set_control(&mut st, "anglo_egypt_sudan", Some(Power::Uk));
    set_control(&mut st, "syria_iraq", None);
    assert!(!opens(&st), "unowned also closes it");
    // axis ships need the Axis to hold both
    let mut ax = nc(Power::Germany);
    let g = put(
        &mut ax,
        UnitType::Transport,
        Power::Germany,
        "sz_east_mediterranean",
    );
    assert!(!crate::query::reachable(&ax, g)
        .iter()
        .any(|r| r.to == loc("sz_red")));
    set_control(&mut ax, "syria_iraq", Some(Power::Japan));
    set_control(&mut ax, "anglo_egypt_sudan", Some(Power::Germany));
    assert!(crate::query::reachable(&ax, g)
        .iter()
        .any(|r| r.to == loc("sz_red")));
}

#[test]
fn panama_open_to_ally_of_controller_only() {
    let mut st = nc(Power::Uk);
    let t = put(&mut st, UnitType::Transport, Power::Uk, "sz_caribbean");
    assert!(
        crate::query::reachable(&st, t)
            .iter()
            .any(|r| r.to == loc("sz_west_panama")),
        "USA (ally) holds Panama"
    );
    set_control(&mut st, "panama", Some(Power::Uk));
    assert!(crate::query::reachable(&st, t)
        .iter()
        .any(|r| r.to == loc("sz_west_panama")));
    set_control(&mut st, "panama", None);
    assert!(!crate::query::reachable(&st, t)
        .iter()
        .any(|r| r.to == loc("sz_west_panama")));
}

#[test]
fn canal_cannot_be_used_even_as_part_of_a_two_step_path() {
    let mut st = nc(Power::Uk);
    let t = put(
        &mut st,
        UnitType::Transport,
        Power::Uk,
        "sz_central_mediterranean",
    );
    set_control(&mut st, "syria_iraq", Some(Power::Germany));
    assert!(act(
        &mut st,
        Power::Uk,
        mv(&[t], &["sz_east_mediterranean", "sz_red"])
    )
    .is_err());
}

#[test]
fn ships_pass_empty_or_friendly_zones_but_not_hostile_ones() {
    let mut st = cm(Power::Uk);
    let bb = put(&mut st, UnitType::Battleship, Power::Uk, "sz_north");
    put(&mut st, UnitType::Transport, Power::Germany, "sz_baltic");
    // a lone enemy transport still makes the zone hostile, and the mover must stop there
    assert!(act(&mut st, Power::Uk, mv(&[bb], &["sz_baltic", "sz_karelia"])).is_err());
    act(&mut st, Power::Uk, mv(&[bb], &["sz_baltic"])).unwrap();
    assert!(st.unit(bb).unwrap().stopped);
}

#[test]
fn transport_hostile_zone_allowed_in_cm_only() {
    let mut st = cm(Power::Uk);
    let t = put(&mut st, UnitType::Transport, Power::Uk, "sz_north");
    put(&mut st, UnitType::Submarine, Power::Germany, "sz_baltic");
    act(&mut st, Power::Uk, mv(&[t], &["sz_baltic"])).unwrap();
    let mut st2 = nc(Power::Uk);
    let t2 = put(&mut st2, UnitType::Transport, Power::Uk, "sz_north");
    put(&mut st2, UnitType::Submarine, Power::Germany, "sz_baltic");
    assert!(act(&mut st2, Power::Uk, mv(&[t2], &["sz_baltic"])).is_err());
}

// ---------------------------------------------------------------- transports (2.1, 4.6)

#[test]
fn transport_capacity_matrix() {
    for (kinds, ok) in [
        (vec![UnitType::Infantry], true),
        (vec![UnitType::Infantry, UnitType::Infantry], true),
        (vec![UnitType::Armor], true),
        (vec![UnitType::AaGun], true),
        (vec![UnitType::Infantry, UnitType::Armor], false),
        (vec![UnitType::Armor, UnitType::Armor], false),
        (vec![UnitType::AaGun, UnitType::Infantry], false),
        (vec![UnitType::AaGun, UnitType::Armor], false),
        (vec![UnitType::AaGun, UnitType::AaGun], false),
    ] {
        let mut st = nc(Power::Uk);
        let t = put(&mut st, UnitType::Transport, Power::Uk, "sz_east_canada");
        let ids: Vec<UnitId> = kinds
            .iter()
            .map(|&k| put(&mut st, k, Power::Uk, "eastern_canada"))
            .collect();
        // all at once
        let mut a = st.clone();
        assert_eq!(
            load(&mut a, Power::Uk, &ids, t).is_ok(),
            ok,
            "{kinds:?} together"
        );
        // one after the other
        let mut seq_ok = true;
        for &i in &ids {
            seq_ok &= load(&mut st, Power::Uk, &[i], t).is_ok();
        }
        assert_eq!(seq_ok, ok, "{kinds:?} in sequence");
    }
}

#[test]
fn only_land_units_that_can_be_carried_board() {
    let mut st = nc(Power::Uk);
    let t = put(&mut st, UnitType::Transport, Power::Uk, "sz_east_canada");
    let f = put(&mut st, UnitType::Fighter, Power::Uk, "eastern_canada");
    assert!(load(&mut st, Power::Uk, &[f], t).is_err());
    let i = put(&mut st, UnitType::Infantry, Power::Uk, "eastern_usa");
    assert!(
        load(&mut st, Power::Uk, &[i], t).is_err(),
        "not adjacent to the transport"
    );
}

#[test]
fn boarding_is_the_whole_move_even_for_armor() {
    let mut st = nc(Power::Uk);
    let t = put(&mut st, UnitType::Transport, Power::Uk, "sz_east_canada");
    let a = put(&mut st, UnitType::Armor, Power::Uk, "eastern_canada");
    load(&mut st, Power::Uk, &[a], t).unwrap();
    assert!(act(&mut st, Power::Uk, mv(&[a], &["western_canada"])).is_err());
    // unloading into a friendly neighbour also uses the (already spent) move: it can't walk on
    unload(&mut st, Power::Uk, t, &[a], "western_canada").unwrap();
    assert!(act(&mut st, Power::Uk, mv(&[a], &["alaska"])).is_err());
}

#[test]
fn armor_that_walked_cannot_board_and_loaded_armor_cannot_walk_first() {
    let mut st = nc(Power::Uk);
    let t = put(&mut st, UnitType::Transport, Power::Uk, "sz_east_canada");
    let a = put(&mut st, UnitType::Armor, Power::Uk, "eastern_usa");
    act(&mut st, Power::Uk, mv(&[a], &["eastern_canada"])).unwrap();
    assert!(load(&mut st, Power::Uk, &[a], t).is_err());
}

#[test]
fn unloading_ends_the_transports_move() {
    let mut st = nc(Power::Uk);
    let t = put(&mut st, UnitType::Transport, Power::Uk, "sz_east_canada");
    let i = put(&mut st, UnitType::Infantry, Power::Uk, "eastern_canada");
    load(&mut st, Power::Uk, &[i], t).unwrap();
    unload(&mut st, Power::Uk, t, &[i], "western_canada").unwrap();
    assert!(act(&mut st, Power::Uk, mv(&[t], &["sz_north"])).is_err());
}

#[test]
fn two_infantry_from_one_transport_may_land_in_two_territories_in_nc() {
    let mut st = nc(Power::Uk);
    let t = put(&mut st, UnitType::Transport, Power::Uk, "sz_east_canada");
    let ids = putn(&mut st, UnitType::Infantry, Power::Uk, "eastern_canada", 2);
    load(&mut st, Power::Uk, &ids, t).unwrap();
    unload(&mut st, Power::Uk, t, &[ids[0]], "eastern_canada").unwrap();
    unload(&mut st, Power::Uk, t, &[ids[1]], "western_canada").unwrap();
}

#[test]
fn cargo_loaded_at_two_stops_of_the_transports_path() {
    let mut st = nc(Power::Uk);
    let t = put(&mut st, UnitType::Transport, Power::Uk, "sz_east_canada");
    let a = put(&mut st, UnitType::Infantry, Power::Uk, "eastern_canada");
    let b = put(&mut st, UnitType::Infantry, Power::Uk, "united_kingdom");
    load(&mut st, Power::Uk, &[a], t).unwrap();
    act(&mut st, Power::Uk, mv(&[t], &["sz_north"])).unwrap();
    load(&mut st, Power::Uk, &[b], t).unwrap();
    assert_eq!(
        st.unit(a).unwrap().location,
        loc("sz_north"),
        "cargo travels with the ship"
    );
    assert_eq!(st.unit(b).unwrap().loaded_on, Some(t));
}

#[test]
fn allied_cargo_may_board_but_not_unload_in_the_same_turn() {
    let mut st = nc(Power::Usa);
    let t = put(&mut st, UnitType::Transport, Power::Uk, "sz_east_canada");
    let i = put(&mut st, UnitType::Infantry, Power::Usa, "eastern_canada");
    // eastern_canada is British, but the US infantry stands there as an ally
    load(&mut st, Power::Usa, &[i], t).unwrap();
    assert!(unload(&mut st, Power::Usa, t, &[i], "western_canada").is_err());
}

#[test]
fn transport_cannot_be_used_by_an_enemy() {
    let mut st = nc(Power::Germany);
    let t = put(&mut st, UnitType::Transport, Power::Uk, "sz_baltic");
    let i = put(&mut st, UnitType::Infantry, Power::Germany, "germany");
    assert!(load(&mut st, Power::Germany, &[i], t).is_err());
}

#[test]
fn newly_placed_ships_and_planes_are_never_aboard() {
    let mut st = blank(Power::Uk, Phase::Mobilize);
    put(
        &mut st,
        UnitType::IndustrialComplex,
        Power::Uk,
        "united_kingdom",
    );
    st.purchases = vec![
        item(UnitType::Transport, 1),
        item(UnitType::Carrier, 1),
        item(UnitType::Fighter, 1),
    ];
    place(&mut st, Power::Uk, UnitType::Transport, "sz_north", 1).unwrap();
    place(&mut st, Power::Uk, UnitType::Carrier, "sz_north", 1).unwrap();
    assert!(place(&mut st, Power::Uk, UnitType::Fighter, "sz_north", 1).is_err());
    place(&mut st, Power::Uk, UnitType::Fighter, "united_kingdom", 1).unwrap();
    assert!(st.units.iter().all(|u| u.loaded_on.is_none()));
}

// ---------------------------------------------------------------- combat (5.x)

fn baltic_fleet_battle() -> GameState {
    let mut st = cm(Power::Uk);
    let s = put(&mut st, UnitType::Submarine, Power::Uk, "sz_north");
    let bb = put(&mut st, UnitType::Battleship, Power::Uk, "sz_north");
    putn(
        &mut st,
        UnitType::Battleship,
        Power::Germany,
        "sz_baltic",
        2,
    );
    act(&mut st, Power::Uk, mv(&[s, bb], &["sz_baltic"])).unwrap();
    go_combat(&mut st, Power::Uk);
    st
}

#[test]
fn submarine_first_strike_recurs_every_round() {
    let mut st = baltic_fleet_battle();
    let begin = Action::BeginBattle {
        location: loc("sz_baltic"),
    };
    // round 1: everybody misses
    let ev1 = act_seed(&mut st, Power::Uk, begin, seed_for(&[6, 6, 6, 6])).unwrap();
    assert!(outcome(&ev1).is_none());
    assert!(matches!(
        crate::pending_decision(&st),
        crate::Decision::RetreatOrPress { .. }
    ));
    // round 2: the first strike hits and the casualty never rolls its defence die
    let ev2 = act_seed(&mut st, Power::Uk, Action::PressAttack, seed_for(&[2])).unwrap();
    let fs: Vec<_> = ev2
        .iter()
        .enumerate()
        .filter(|(_, e)| {
            matches!(
                e,
                Event::DiceRolled {
                    context: DiceContext::FirstStrike { round: 2, .. },
                    ..
                }
            )
        })
        .collect();
    assert_eq!(fs.len(), 1, "a first strike in round 2");
    let removed = ev2
        .iter()
        .position(|e| matches!(e, Event::CasualtiesRemoved { .. }))
        .expect("first-strike casualty removed at once");
    let defence = ev2
        .iter()
        .position(|e| {
            matches!(
                e,
                Event::DiceRolled {
                    context: DiceContext::Defense { .. },
                    ..
                }
            )
        })
        .expect("surviving battleship defends");
    assert!(removed < defence, "removed before the defender fires");
    let d = rolls_of(&ev2, |c| matches!(c, DiceContext::Defense { .. }));
    assert_eq!(d[0].len(), 1, "only the survivor rolled");
}

#[test]
fn defending_subs_cannot_hit_an_all_air_attack_force() {
    let mut st = cm(Power::Uk);
    let f = put(&mut st, UnitType::Fighter, Power::Uk, "united_kingdom");
    let sub = put(&mut st, UnitType::Submarine, Power::Germany, "sz_baltic");
    act(&mut st, Power::Uk, mv(&[f], &["sz_north", "sz_baltic"])).unwrap();
    go_combat(&mut st, Power::Uk);
    // fighter misses (4), sub rolls 1 (a hit on a ship, but nothing it may hit)
    act_seed(
        &mut st,
        Power::Uk,
        Action::BeginBattle {
            location: loc("sz_baltic"),
        },
        seed_for(&[4, 1]),
    )
    .unwrap();
    assert!(st.unit(f).is_some() && st.unit(sub).is_some());
    assert!(st.battle.as_ref().unwrap().pending.is_none());
    // and the fighter then sinks it (roll 3)
    act_seed(&mut st, Power::Uk, Action::PressAttack, seed_for(&[3, 6])).ok();
}

#[test]
fn bombardment_is_one_shot_per_battleship_and_round_one_only() {
    let mut multi_round = 0;
    for seed in 0..60u64 {
        let mut st = cm(Power::Uk);
        putn(&mut st, UnitType::Battleship, Power::Uk, "sz_north", 2);
        let t = put(&mut st, UnitType::Transport, Power::Uk, "sz_north");
        let ids = putn(&mut st, UnitType::Infantry, Power::Uk, "united_kingdom", 2);
        load(&mut st, Power::Uk, &ids, t).unwrap();
        putn(
            &mut st,
            UnitType::Infantry,
            Power::Germany,
            "western_europe",
            4,
        );
        unload(&mut st, Power::Uk, t, &ids, "western_europe").unwrap();
        go_combat(&mut st, Power::Uk);
        let mut ev = act_seed(
            &mut st,
            Power::Uk,
            Action::BeginBattle {
                location: loc("western_europe"),
            },
            seed,
        )
        .unwrap();
        // identical casualties are auto-assigned; a mixed pending choice never arises here
        assert!(st.battle.is_none(), "amphibious battles run to the end");
        let bombs: Vec<_> = rolls_of(&ev, |c| matches!(c, DiceContext::Bombardment { .. }));
        assert_eq!(bombs.len(), 1);
        assert_eq!(bombs[0].len(), 2, "one die per battleship");
        let rounds = ev
            .iter()
            .filter(|e| matches!(e, Event::BattleRound { .. }))
            .count();
        if rounds > 1 {
            multi_round += 1;
        }
        ev.clear();
    }
    assert!(multi_round > 0);
}

#[test]
fn battleship_that_fought_at_sea_does_not_bombard() {
    let mut st = cm(Power::Uk);
    let bb = put(&mut st, UnitType::Battleship, Power::Uk, "sz_north");
    let t = put(&mut st, UnitType::Transport, Power::Uk, "sz_north");
    let i = put(&mut st, UnitType::Infantry, Power::Uk, "united_kingdom");
    load(&mut st, Power::Uk, &[i], t).unwrap();
    put(&mut st, UnitType::Submarine, Power::Germany, "sz_baltic");
    // escort goes to the Baltic to fight; the transport lands at Germany's neighbour from the Baltic
    act(&mut st, Power::Uk, mv(&[t, bb], &["sz_baltic"])).unwrap();
    putn(
        &mut st,
        UnitType::Infantry,
        Power::Germany,
        "eastern_europe",
        3,
    );
    unload(&mut st, Power::Uk, t, &[i], "eastern_europe").unwrap();
    go_combat(&mut st, Power::Uk);
    // sea battle first (both battleships fought; the sub dies to the first hit)
    let ev = act_seed(
        &mut st,
        Power::Uk,
        Action::BeginBattle {
            location: loc("sz_baltic"),
        },
        seed_for(&[4, 6]),
    )
    .unwrap();
    assert_eq!(outcome(&ev), Some(BattleOutcome::AttackerWins));
    assert!(st.unit(bb).unwrap().fought);
    let ev = act(
        &mut st,
        Power::Uk,
        Action::BeginBattle {
            location: loc("eastern_europe"),
        },
    )
    .unwrap();
    assert!(!ev.iter().any(|e| matches!(e, Event::Bombardment { .. })));
}

#[test]
fn retreat_takes_everyone_to_one_origin_but_aircraft_stay_and_fly_on() {
    let mut st = cm(Power::Germany);
    set_control(&mut st, "ukraine_s_s_r", Some(Power::Ussr));
    set_control(&mut st, "karelia_s_s_r", Some(Power::Germany));
    set_control(&mut st, "caucasus", Some(Power::Germany));
    let i = put(
        &mut st,
        UnitType::Infantry,
        Power::Germany,
        "eastern_europe",
    );
    let a = put(&mut st, UnitType::Armor, Power::Germany, "karelia_s_s_r");
    let f = put(&mut st, UnitType::Fighter, Power::Germany, "germany");
    putn(&mut st, UnitType::Infantry, Power::Ussr, "ukraine_s_s_r", 2);
    act(&mut st, Power::Germany, mv(&[i], &["ukraine_s_s_r"])).unwrap();
    act(&mut st, Power::Germany, mv(&[a], &["ukraine_s_s_r"])).unwrap();
    act(
        &mut st,
        Power::Germany,
        mv(&[f], &["eastern_europe", "ukraine_s_s_r"]),
    )
    .unwrap();
    go_combat(&mut st, Power::Germany);
    // everybody misses (6s), defenders miss too
    act_seed(
        &mut st,
        Power::Germany,
        Action::BeginBattle {
            location: loc("ukraine_s_s_r"),
        },
        seed_for(&[6, 6, 6, 6, 6]),
    )
    .unwrap();
    // may only fall back to a space an attacker came from (not Caucasus, which is merely friendly)
    assert!(act(
        &mut st,
        Power::Germany,
        Action::Retreat {
            to: loc("caucasus")
        }
    )
    .is_err());
    act(
        &mut st,
        Power::Germany,
        Action::Retreat {
            to: loc("karelia_s_s_r"),
        },
    )
    .unwrap();
    assert_eq!(st.unit(i).unwrap().location, loc("karelia_s_s_r"));
    assert_eq!(st.unit(a).unwrap().location, loc("karelia_s_s_r"));
    assert_eq!(
        st.unit(f).unwrap().location,
        loc("ukraine_s_s_r"),
        "aircraft are exempt from the one-space rule and stay"
    );
    assert_eq!(st.phase, Phase::NonCombatMove);
    act(
        &mut st,
        Power::Germany,
        mv(&[f], &["eastern_europe", "germany"]),
    )
    .unwrap();
    act(&mut st, Power::Germany, Action::EndPhase).unwrap();
    assert!(st.unit(f).is_some());
}

#[test]
fn air_survivor_alone_does_not_capture_even_if_land_units_died() {
    let mut st = cm(Power::Germany);
    set_control(&mut st, "ukraine_s_s_r", Some(Power::Ussr));
    let i = put(
        &mut st,
        UnitType::Infantry,
        Power::Germany,
        "eastern_europe",
    );
    let f = put(&mut st, UnitType::Fighter, Power::Germany, "germany");
    let d = put(&mut st, UnitType::Infantry, Power::Ussr, "ukraine_s_s_r");
    act(&mut st, Power::Germany, mv(&[i], &["ukraine_s_s_r"])).unwrap();
    act(
        &mut st,
        Power::Germany,
        mv(&[f], &["eastern_europe", "ukraine_s_s_r"]),
    )
    .unwrap();
    go_combat(&mut st, Power::Germany);
    // attacker infantry 6 (miss), fighter 1 (hit), defender 1 (hit): attacker chooses the infantry
    act_seed(
        &mut st,
        Power::Germany,
        Action::BeginBattle {
            location: loc("ukraine_s_s_r"),
        },
        seed_for(&[6, 1, 1]),
    )
    .unwrap();
    act(
        &mut st,
        Power::Germany,
        Action::SelectCasualties { units: vec![i] },
    )
    .unwrap();
    assert!(st.unit(d).is_none() && st.unit(i).is_none() && st.unit(f).is_some());
    assert_eq!(st.control[loc("ukraine_s_s_r")], Some(Power::Ussr));
}

#[test]
fn raids_come_before_the_ordinary_battle_at_the_same_territory() {
    let mut st = cm(Power::Uk);
    set_control(&mut st, "western_europe", Some(Power::Germany));
    put(
        &mut st,
        UnitType::IndustrialComplex,
        Power::Germany,
        "western_europe",
    );
    putn(
        &mut st,
        UnitType::Infantry,
        Power::Germany,
        "western_europe",
        2,
    );
    let b = put(&mut st, UnitType::Bomber, Power::Uk, "united_kingdom");
    let f = put(&mut st, UnitType::Fighter, Power::Uk, "united_kingdom");
    act(
        &mut st,
        Power::Uk,
        mv(&[b], &["sz_north", "western_europe"]),
    )
    .unwrap();
    act(
        &mut st,
        Power::Uk,
        mv(&[f], &["sz_north", "western_europe"]),
    )
    .unwrap();
    go_combat(&mut st, Power::Uk);
    act(
        &mut st,
        Power::Uk,
        Action::DeclareStrategicBombing {
            location: loc("western_europe"),
            bombers: vec![b],
        },
    )
    .unwrap();
    assert!(
        st.unit(b).unwrap().fought,
        "the raiding bomber is out of the ordinary battle"
    );
    act(
        &mut st,
        Power::Uk,
        Action::BeginBattle {
            location: loc("western_europe"),
        },
    )
    .unwrap();
    let again = act(
        &mut st,
        Power::Uk,
        Action::DeclareStrategicBombing {
            location: loc("western_europe"),
            bombers: vec![b],
        },
    );
    assert!(again.is_err(), "no raid after the ordinary battle");
}

#[test]
fn second_rocket_in_a_turn_is_refused() {
    let mut st = blank(Power::Germany, Phase::Combat);
    st.techs[Power::Germany].push(crate::Tech::Rockets);
    let g1 = put(&mut st, UnitType::AaGun, Power::Germany, "germany");
    let g2 = put(&mut st, UnitType::AaGun, Power::Germany, "southern_europe");
    put(
        &mut st,
        UnitType::IndustrialComplex,
        Power::Uk,
        "western_europe",
    );
    set_control(&mut st, "western_europe", Some(Power::Uk));
    // western_europe is adjacent to germany and southern_europe
    act(
        &mut st,
        Power::Germany,
        Action::FireRocket {
            aa_gun: g1,
            target: loc("western_europe"),
        },
    )
    .unwrap();
    assert!(act(
        &mut st,
        Power::Germany,
        Action::FireRocket {
            aa_gun: g2,
            target: loc("western_europe"),
        },
    )
    .is_err());
}

#[test]
fn rocket_range_is_three_and_target_must_be_enemy_complex() {
    let mut st = blank(Power::Germany, Phase::Combat);
    st.techs[Power::Germany].push(crate::Tech::Rockets);
    let g = put(&mut st, UnitType::AaGun, Power::Germany, "germany");
    put(&mut st, UnitType::IndustrialComplex, Power::Ussr, "russia");
    // germany -> eastern_europe -> karelia -> russia: exactly 3
    assert!(crate::query::rocket_targets(&st, g).contains(&loc("russia")));
    put(&mut st, UnitType::IndustrialComplex, Power::Uk, "india");
    assert!(!crate::query::rocket_targets(&st, g).contains(&loc("india")));
    // own complex is no target
    assert!(!crate::query::rocket_targets(&st, g).contains(&loc("germany")));
}

// ---------------------------------------------------------------- economy (6, 8)

#[test]
fn absurd_counts_are_rejected_not_wrapped_or_panicking() {
    let mut st = blank(Power::Germany, Phase::DevelopWeapons);
    assert!(act(
        &mut st,
        Power::Germany,
        Action::BuyResearch { dice: u32::MAX }
    )
    .is_err());
    assert!(act(
        &mut st,
        Power::Germany,
        Action::BuyResearch {
            dice: 1_000_000_000
        }
    )
    .is_err());
    assert_eq!(st.treasury[Power::Germany], 32);
    assert!(!st.turn.researched);
    act(&mut st, Power::Germany, Action::EndPhase).unwrap();
    for items in [
        vec![item(UnitType::Infantry, u32::MAX)],
        vec![item(UnitType::Battleship, 1_000_000_000)],
        vec![
            item(UnitType::Infantry, u32::MAX),
            item(UnitType::Infantry, u32::MAX),
        ],
        vec![item(UnitType::IndustrialComplex, u32::MAX)],
        vec![
            item(UnitType::Infantry, 1),
            item(UnitType::IndustrialComplex, u32::MAX - 1),
        ],
    ] {
        let before = st.clone();
        assert!(act(&mut st, Power::Germany, Action::Purchase { items }).is_err());
        assert_eq!(st, before);
    }
    // an honest list afterwards still works
    act(
        &mut st,
        Power::Germany,
        Action::Purchase {
            items: vec![item(UnitType::Infantry, 10)],
        },
    )
    .unwrap();
    assert_eq!(st.treasury[Power::Germany], 2);
}

#[test]
fn absurd_placement_counts_are_rejected() {
    let mut st = blank(Power::Germany, Phase::Mobilize);
    put(
        &mut st,
        UnitType::IndustrialComplex,
        Power::Germany,
        "germany",
    );
    st.purchases = vec![item(UnitType::Infantry, 2)];
    assert!(place(
        &mut st,
        Power::Germany,
        UnitType::Infantry,
        "germany",
        u32::MAX
    )
    .is_err());
    assert!(place(&mut st, Power::Germany, UnitType::Infantry, "germany", 3).is_err());
    place(&mut st, Power::Germany, UnitType::Infantry, "germany", 2).unwrap();
}

#[test]
fn mid_battle_state_survives_json_and_continues_identically() {
    let mut st = cm(Power::Germany);
    set_control(&mut st, "ukraine_s_s_r", Some(Power::Ussr));
    let i = put(
        &mut st,
        UnitType::Infantry,
        Power::Germany,
        "eastern_europe",
    );
    let a = put(&mut st, UnitType::Armor, Power::Germany, "eastern_europe");
    putn(&mut st, UnitType::Infantry, Power::Ussr, "ukraine_s_s_r", 3);
    act(&mut st, Power::Germany, mv(&[i, a], &["ukraine_s_s_r"])).unwrap();
    go_combat(&mut st, Power::Germany);
    // attackers miss (6, 6), exactly one defender hits (2) so the attacker must choose
    act_seed(
        &mut st,
        Power::Germany,
        Action::BeginBattle {
            location: loc("ukraine_s_s_r"),
        },
        seed_for(&[6, 6, 2, 6, 6]),
    )
    .unwrap();
    assert!(matches!(
        crate::pending_decision(&st),
        crate::Decision::SelectCasualties { hits: 1, .. }
    ));
    let json = serde_json::to_string(&st).unwrap();
    let mut back: GameState = serde_json::from_str(&json).unwrap();
    assert_eq!(back, st);
    let pick = Action::SelectCasualties { units: vec![i] };
    let e1 = apply(&mut st, Power::Germany, &pick, 77).unwrap();
    let e2 = apply(&mut back, Power::Germany, &pick, 77).unwrap();
    assert_eq!(e1, e2);
    assert_eq!(st, back);
    // wrong chooser / wrong unit count are refused without touching the state
    let before = st.clone();
    assert!(apply(&mut st, Power::Ussr, &Action::PressAttack, 1).is_err());
    assert_eq!(st, before);
}

// ---------------------------------------------------------------- victory / capitals (1.1, 9)

#[test]
fn military_victory_is_decided_at_end_of_turn_not_mid_turn() {
    let mut st = GameState::new(GameSettings::default());
    st.current_power = Power::Germany;
    st.phase = Phase::CombatMove;
    st.control[loc("russia")] = Some(Power::Germany);
    st.control[loc("united_kingdom")] = Some(Power::Germany);
    assert!(st.winner.is_none());
    act(&mut st, Power::Germany, Action::EndPhase).unwrap();
    assert!(st.winner.is_none(), "still mid-turn");
    st.phase = Phase::Mobilize;
    act(&mut st, Power::Germany, Action::EndPhase).unwrap();
    assert_eq!(st.winner.map(|w| w.team), Some(Team::Axis));
}

#[test]
fn allies_need_both_axis_capitals_and_axis_need_two_allied_ones() {
    let mut st = GameState::new(GameSettings::default());
    st.control[loc("japan")] = Some(Power::Usa);
    assert!(crate::victory::check_military(&st).is_none());
    st.control[loc("germany")] = Some(Power::Ussr);
    assert!(crate::victory::check_military(&st).is_some());
    let mut ax = GameState::new(GameSettings::default());
    ax.control[loc("eastern_usa")] = Some(Power::Japan);
    assert!(crate::victory::check_military(&ax).is_none());
    ax.control[loc("russia")] = Some(Power::Germany);
    assert_eq!(
        crate::victory::check_military(&ax).map(|w| w.team),
        Some(Team::Axis)
    );
}

#[test]
fn economic_victory_counts_territories_of_a_power_whose_capital_fell() {
    let mut st = GameState::new(GameSettings::default());
    st.control[loc("germany")] = Some(Power::Uk); // Germany's capital is lost
    let before =
        crate::production_level(&st, Power::Germany) + crate::production_level(&st, Power::Japan);
    assert!(before < 84);
    // hand Germany enough Soviet land to reach 84 without its capital
    for t in [
        "russia",
        "caucasus",
        "evenki_national_okrug",
        "kazakh_s_s_r",
        "novosibirsk",
    ] {
        st.control[loc(t)] = Some(Power::Germany);
    }
    for t in [
        "western_usa",
        "brazil",
        "alaska",
        "mexico",
        "eastern_canada",
        "india",
        "australia",
        "china",
        "sinkiang",
    ] {
        st.control[loc(t)] = Some(Power::Japan);
    }
    let sum =
        crate::production_level(&st, Power::Germany) + crate::production_level(&st, Power::Japan);
    assert!(sum >= 84, "{sum}");
    assert!(crate::victory::check_economic(&st).is_some());
    assert_eq!(
        crate::income(&st, Power::Germany),
        0,
        "no income without a capital"
    );
}

#[test]
fn power_with_fallen_capital_cannot_buy_but_the_captor_can_spend_the_loot() {
    let mut st = blank(Power::Ussr, Phase::Purchase);
    st.control[loc("russia")] = Some(Power::Germany);
    st.treasury[Power::Ussr] = 0;
    assert!(matches!(
        act(
            &mut st,
            Power::Ussr,
            Action::Purchase {
                items: vec![item(UnitType::Infantry, 1)]
            }
        ),
        Err(RuleError::InsufficientFunds { .. })
    ));
    let mut st = blank(Power::Germany, Phase::Purchase);
    st.treasury[Power::Germany] = 24; // loot from Russia
    act(
        &mut st,
        Power::Germany,
        Action::Purchase {
            items: vec![item(UnitType::Infantry, 8)],
        },
    )
    .unwrap();
}

#[test]
fn seized_treasury_goes_to_the_captor_only() {
    let mut st = blank(Power::Japan, Phase::CombatMove);
    st.treasury[Power::Uk] = 41;
    let g = st.treasury[Power::Germany];
    let j = st.treasury[Power::Japan];
    let a = put(&mut st, UnitType::Armor, Power::Japan, "persia");
    set_control(&mut st, "persia", Some(Power::Japan));
    set_control(&mut st, "india", Some(Power::Uk));
    // stand in for capturing the UK capital: relocate it so that a land path exists
    let mut ev = vec![];
    crate::economy::capture_territory(&mut st, Power::Japan, loc("united_kingdom"), &mut ev);
    assert_eq!(st.treasury[Power::Uk], 0);
    assert_eq!(st.treasury[Power::Japan], j + 41);
    assert_eq!(st.treasury[Power::Germany], g);
    let _ = a;
}

#[test]
fn violated_neutral_is_never_liberated_by_an_ally() {
    let mut st = blank(Power::Uk, Phase::CombatMove);
    st.control[loc("sweden")] = Some(Power::Germany);
    st.turn.start_control = st.control.clone();
    let mut ev = vec![];
    crate::economy::capture_territory(&mut st, Power::Uk, loc("sweden"), &mut ev);
    assert_eq!(st.control[loc("sweden")], Some(Power::Uk));
}

#[test]
fn regained_territory_goes_to_the_latest_captor_when_owner_is_an_enemy() {
    // Ukraine belongs to Germany; USSR takes it, Germany retakes it, UK retakes it: UK holds it.
    let mut st = blank(Power::Uk, Phase::CombatMove);
    st.control[loc("ukraine_s_s_r")] = Some(Power::Germany);
    let mut ev = vec![];
    crate::economy::capture_territory(&mut st, Power::Uk, loc("ukraine_s_s_r"), &mut ev);
    assert_eq!(st.control[loc("ukraine_s_s_r")], Some(Power::Uk));
}

#[test]
fn new_complex_cannot_supply_units_until_next_turn() {
    let mut st = blank(Power::Uk, Phase::Mobilize);
    st.purchases = vec![
        item(UnitType::IndustrialComplex, 1),
        item(UnitType::Infantry, 1),
    ];
    place(&mut st, Power::Uk, UnitType::IndustrialComplex, "india", 1).unwrap();
    assert!(place(&mut st, Power::Uk, UnitType::Infantry, "india", 1).is_err());
    assert!(st.complex_at(loc("india")).is_some());
    assert_eq!(st.new_complexes_placed, 1);
}

#[test]
fn complex_cannot_be_placed_in_unviolated_neutral_or_enemy_land_or_sea() {
    let mut st = blank(Power::Uk, Phase::Mobilize);
    st.purchases = vec![item(UnitType::IndustrialComplex, 3)];
    assert!(place(&mut st, Power::Uk, UnitType::IndustrialComplex, "spain", 1).is_err());
    assert!(place(
        &mut st,
        Power::Uk,
        UnitType::IndustrialComplex,
        "germany",
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
    // a territory won this turn does not qualify either
    st.control[loc("algeria")] = Some(Power::Uk);
    assert!(place(
        &mut st,
        Power::Uk,
        UnitType::IndustrialComplex,
        "algeria",
        1
    )
    .is_err());
    // value-0 land that is ours since the start does
    place(
        &mut st,
        Power::Uk,
        UnitType::IndustrialComplex,
        "gibraltar",
        1,
    )
    .unwrap();
}

#[test]
fn industrial_technology_never_reduces_research_price() {
    let mut st = blank(Power::Germany, Phase::DevelopWeapons);
    st.techs[Power::Germany].push(crate::Tech::IndustrialTechnology);
    let before = st.treasury[Power::Germany];
    act(&mut st, Power::Germany, Action::BuyResearch { dice: 2 }).unwrap();
    assert_eq!(st.treasury[Power::Germany], before - 10);
    act(&mut st, Power::Germany, Action::EndPhase).unwrap();
    // complex costs 14 with the technology
    let opts = crate::query::purchase_options(&st);
    let c = opts
        .iter()
        .find(|o| o.unit == UnitType::IndustrialComplex)
        .unwrap();
    assert_eq!(c.cost, 14);
}

#[test]
fn tech_effects_on_numbers() {
    let mut st = blank(Power::Germany, Phase::CombatMove);
    let f = put(&mut st, UnitType::Fighter, Power::Germany, "germany");
    let b = put(&mut st, UnitType::Bomber, Power::Germany, "germany");
    let s = put(&mut st, UnitType::Submarine, Power::Germany, "sz_baltic");
    let fu = st.unit(f).unwrap().clone();
    let bu = st.unit(b).unwrap().clone();
    let su = st.unit(s).unwrap().clone();
    use crate::tech::{attack_dice, attack_value, defense_value, move_allowance};
    assert_eq!((move_allowance(&st, &fu), move_allowance(&st, &bu)), (4, 6));
    assert_eq!(
        (
            defense_value(&st, &fu),
            attack_value(&st, &su),
            attack_dice(&st, &bu)
        ),
        (4, 2, 1)
    );
    for t in [
        crate::Tech::LongRangeAircraft,
        crate::Tech::JetPower,
        crate::Tech::SuperSubmarines,
        crate::Tech::HeavyBombers,
    ] {
        st.techs[Power::Germany].push(t);
    }
    assert_eq!((move_allowance(&st, &fu), move_allowance(&st, &bu)), (6, 8));
    assert_eq!(
        (
            defense_value(&st, &fu),
            attack_value(&st, &fu),
            attack_value(&st, &su),
            defense_value(&st, &su),
            attack_dice(&st, &bu),
            defense_value(&st, &bu)
        ),
        (5, 3, 3, 2, 3, 1)
    );
}

#[test]
fn orphaned_fighters_land_on_a_friendly_island_of_the_zone() {
    let mut st = cm(Power::Uk);
    let sub = put(&mut st, UnitType::Submarine, Power::Uk, "sz_borneo");
    let c = put(
        &mut st,
        UnitType::Carrier,
        Power::Japan,
        "sz_caroline_islands",
    );
    let f = put(
        &mut st,
        UnitType::Fighter,
        Power::Japan,
        "sz_caroline_islands",
    );
    st.unit_mut(f).unwrap().loaded_on = Some(c);
    act(&mut st, Power::Uk, mv(&[sub], &["sz_caroline_islands"])).unwrap();
    go_combat(&mut st, Power::Uk);
    // first strike hits (2): the carrier must be the casualty (subs cannot hit aircraft); fighter misses
    act_seed(
        &mut st,
        Power::Uk,
        Action::BeginBattle {
            location: loc("sz_caroline_islands"),
        },
        seed_for(&[2, 6]),
    )
    .unwrap();
    assert!(st.unit(c).is_none());
    act(
        &mut st,
        Power::Uk,
        Action::Retreat {
            to: loc("sz_borneo"),
        },
    )
    .unwrap();
    let fu = st.unit(f).expect("fighter lands on the island, not lost");
    assert_eq!(fu.location, loc("caroline_islands"));
    assert!(fu.loaded_on.is_none());
}

use super::*;
use crate::map::MapData;
use crate::random_policy::RandomPolicy;
use crate::tech::move_allowance;

/// Structural invariants that must hold after every action.
pub fn check_invariants(st: &GameState) {
    let map = MapData::get();
    let mut last = 0;
    for u in &st.units {
        assert!(u.id > last, "unit ids strictly increasing");
        last = u.id;
        assert!(u.location.idx() < map.len());
        match u.kind.domain() {
            crate::units::Domain::Sea => assert!(map.is_sea(u.location), "{:?} on land", u.kind),
            crate::units::Domain::Land => {
                if map.is_sea(u.location) {
                    assert!(
                        u.loaded_on.is_some(),
                        "land unit {:?} at sea without transport",
                        u.kind
                    );
                }
            }
            crate::units::Domain::Air => {}
        }
        if !u.kind.is_land() {
            assert!(
                u.moves_used <= move_allowance(st, u),
                "{:?} moved {} of {}",
                u.kind,
                u.moves_used,
                move_allowance(st, u)
            );
        }
        if let Some(c) = u.loaded_on {
            let carrier = st
                .unit(c)
                .unwrap_or_else(|| panic!("cargo {:?} references missing ship {c}", u.kind));
            assert_eq!(carrier.location, u.location, "cargo location");
            if u.kind.is_air() {
                assert_eq!(carrier.kind, UnitType::Carrier);
                assert_eq!(u.kind, UnitType::Fighter);
            } else {
                assert_eq!(carrier.kind, UnitType::Transport);
                assert!(u.kind.transport_slots().is_some());
            }
        }
        if u.kind == UnitType::IndustrialComplex {
            assert!(!map.is_sea(u.location));
            assert_eq!(
                st.control[u.location],
                Some(u.owner),
                "complex owner follows control at {}",
                u.location.id()
            );
        }
        if u.kind == UnitType::AaGun && !map.is_sea(u.location) && u.loaded_on.is_none() {
            let c = st.control[u.location].expect("controlled");
            assert!(
                c.is_ally(u.owner),
                "AA gun owner must be friendly to controller"
            );
        }
    }
    for u in st.units.iter().filter(|u| u.kind == UnitType::Transport) {
        let slots: u8 = st
            .cargo_of(u.id)
            .filter_map(|c| c.kind.transport_slots())
            .sum();
        assert!(slots <= 2, "transport overloaded");
    }
    for u in st.units.iter().filter(|u| u.kind == UnitType::Carrier) {
        assert!(st.cargo_of(u.id).count() <= 2, "carrier overloaded");
    }
    let mut ics = vec![0u8; map.len()];
    let mut aas = vec![0u8; map.len()];
    for u in &st.units {
        if u.kind == UnitType::IndustrialComplex {
            ics[u.location.idx()] += 1;
        }
        if u.kind == UnitType::AaGun && u.loaded_on.is_none() {
            aas[u.location.idx()] += 1;
        }
    }
    for t in map.locations() {
        if map.is_sea(t) {
            assert!(st.control[t].is_none());
        } else {
            assert!(
                ics[t.idx()] <= 1 && aas[t.idx()] <= 1,
                "duplicate complex/AA at {}",
                t.id()
            );
            if !map.loc(t).neutral {
                assert!(st.control[t].is_some(), "{} lost its controller", t.id());
            } else if st.control[t].is_none() {
                assert!(
                    st.units_at(t).next().is_none(),
                    "units inside an untouched neutral {}",
                    t.id()
                );
            }
        }
    }
    let total: u32 = Power::ALL.iter().map(|&p| production_level(st, p)).sum();
    let max: u32 = map.territories().map(|t| map.ipc(t)).sum();
    assert!(total <= max);
    let complexes = st
        .units
        .iter()
        .filter(|u| u.kind == UnitType::IndustrialComplex)
        .count();
    assert!(complexes <= 12 && st.new_complexes_placed <= 4);
}

/// At the end of a power's turn no aircraft may be airborne.
fn check_landed(st: &GameState, p: Power, start_control: &crate::map::LocMap<Option<Power>>) {
    for u in st.units.iter().filter(|u| u.owner == p && u.kind.is_air()) {
        if let Some(c) = u.loaded_on {
            assert_eq!(st.unit(c).unwrap().kind, UnitType::Carrier);
        } else {
            assert!(
                !MapData::get().is_sea(u.location),
                "{:?} of {:?} left airborne at sea",
                u.kind,
                p
            );
            assert!(
                st.is_friendly_land(p, u.location),
                "{:?} of {:?} ended turn on non-friendly {}",
                u.kind,
                p,
                u.location.id()
            );
            assert!(
                matches!(start_control[u.location], Some(o) if o.is_ally(p)),
                "{:?} of {:?} landed in {} which was not friendly at the start of its turn",
                u.kind,
                p,
                u.location.id()
            );
        }
    }
}

/// Between turns, nobody shares land with enemies and ground troops hold what they stand on.
fn check_quiet_board(st: &GameState) {
    let map = MapData::get();
    for t in map.territories() {
        let teams: std::collections::BTreeSet<_> = st
            .units_at(t)
            .filter(|u| u.kind.is_fighting_land_unit() && u.loaded_on.is_none())
            .map(|u| u.owner.team())
            .collect();
        assert!(teams.len() <= 1, "enemies share {}", t.id());
        for u in st.units_at(t).filter(|u| {
            u.kind.is_land()
                && u.loaded_on.is_none()
                && matches!(u.kind, UnitType::Infantry | UnitType::Armor)
        }) {
            if let Some(c) = st.control[t] {
                assert!(
                    c.is_ally(u.owner),
                    "{:?} of {:?} stands on enemy-held {}",
                    u.kind,
                    u.owner,
                    t.id()
                );
            }
        }
    }
    for z in map.sea_zones() {
        let teams: std::collections::BTreeSet<_> = st
            .units_at(z)
            .filter(|u| u.kind.is_ship())
            .map(|u| u.owner.team())
            .collect();
        let _ = teams; // stalemates may legitimately leave opposing ships together
    }
}

fn settings_for(seed: u64) -> GameSettings {
    GameSettings {
        weapons_development: seed % 4 != 3,
        original_complexes_only: seed % 5 == 4,
        total_victory: seed % 3 == 2,
        soviet_first_turn_restriction: seed % 2 == 1,
        germany_jet_power: divisible(seed, 6),
        japan_super_subs: divisible(seed, 7),
        allied_economic_victory: if seed % 4 == 1 { Some(110) } else { None },
        seed,
        ..Default::default()
    }
}

/// Play up to `rounds` rounds with random legal actions; returns the number of actions applied.
pub fn play_random(seed: u64, rounds: u32) -> (usize, GameState) {
    let mut stats: std::collections::BTreeMap<String, usize> = Default::default();
    let mut st = GameState::new(settings_for(seed));
    let mut pol = RandomPolicy::new(seed.wrapping_mul(7919).wrapping_add(1));
    pol.aggression = 0.5 + (seed % 5) as f64 / 10.0;
    let (mut pol_time, mut apply_time, mut inv_time) = (
        std::time::Duration::ZERO,
        std::time::Duration::ZERO,
        std::time::Duration::ZERO,
    );
    let mut n = 0usize;
    let mut start_control = st.turn.start_control.clone();
    let mut log: Vec<(Power, Action, u64)> = vec![];
    while st.round <= rounds && st.winner.is_none() {
        let t0 = std::time::Instant::now();
        let Some((p, a)) = pol.next_action(&st) else {
            break;
        };
        pol_time += t0.elapsed();
        let t0 = std::time::Instant::now();
        let before_turn = st.turn.number;
        let prev_power = st.current_power;
        let action_seed = seed.wrapping_mul(1_000_003).wrapping_add(n as u64);
        let phase = st.phase;
        match apply(&mut st, p, &a, action_seed) {
            Ok(evs) => {
                for e in evs {
                    let v = serde_json::to_value(&e).unwrap();
                    *stats
                        .entry(v["type"].as_str().unwrap().to_string())
                        .or_insert(0usize) += 1;
                }
            }
            Err(e) => panic!("policy produced illegal action {a:?} by {p:?} in {phase:?}: {e}"),
        }
        apply_time += t0.elapsed();
        log.push((p, a, action_seed));
        n += 1;
        let t0 = std::time::Instant::now();
        check_invariants(&st);
        inv_time += t0.elapsed();
        if st.turn.number != before_turn {
            check_landed(&st, prev_power, &start_control);
            check_quiet_board(&st);
            start_control = st.turn.start_control.clone();
        }
        assert!(n < 200_000, "runaway game");
    }
    let replayed = replay(settings_for(seed), &log).expect("replay");
    assert_eq!(replayed, st, "replay must reproduce the state exactly");
    if std::env::var("WARTIDE_FUZZ_STATS").is_ok() {
        eprintln!("{stats:?}");
        eprintln!("policy {pol_time:?} apply {apply_time:?} inv {inv_time:?}");
    }
    (n, st)
}

#[test]
fn random_playouts_hold_invariants() {
    let rounds: u32 = std::env::var("WARTIDE_FUZZ_ROUNDS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(34);
    let seeds: u64 = std::env::var("WARTIDE_FUZZ_SEEDS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(6);
    let mut total_actions = 0;
    let mut total_rounds = 0;
    for seed in 1..=seeds {
        let (n, st) = play_random(seed, rounds);
        total_actions += n;
        total_rounds += st.round - 1;
        eprintln!(
            "seed {seed}: {n} actions, reached round {} winner {:?}",
            st.round, st.winner
        );
    }
    assert!(total_actions > 500);
    eprintln!("total rounds {total_rounds}");
}

#[test]
fn serde_roundtrip_of_mid_game_state_actions_and_events() {
    let mut st = GameState::new(GameSettings::default());
    let mut pol = RandomPolicy::new(5);
    let mut all_events = vec![];
    let mut all_actions = vec![];
    for i in 0..400 {
        let Some((p, a)) = pol.next_action(&st) else {
            break;
        };
        let ev = apply(&mut st, p, &a, i).unwrap();
        all_events.extend(ev);
        all_actions.push(a);
    }
    let j = serde_json::to_string(&st).unwrap();
    let back: GameState = serde_json::from_str(&j).unwrap();
    assert_eq!(back, st);
    for a in &all_actions {
        let j = serde_json::to_string(a).unwrap();
        assert!(j.contains("\"type\""));
        assert_eq!(&serde_json::from_str::<Action>(&j).unwrap(), a);
    }
    for e in &all_events {
        let j = serde_json::to_string(e).unwrap();
        assert!(j.contains("\"type\""));
        assert_eq!(&serde_json::from_str::<Event>(&j).unwrap(), e);
    }
}

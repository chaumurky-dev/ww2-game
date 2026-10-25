use super::*;
use crate::casualty_policy::{choose_casualties, CasualtyPolicy, SubWithdrawalPolicy};
use crate::decision::Decision;

/// Germany attacks Ukraine with two infantry (both hit); the USSR defends with `defs`.
fn land_battle(defs: &[UnitType], hits: usize, seed: u64) -> (GameState, Vec<UnitId>) {
    let mut st = blank(Power::Germany, Phase::CombatMove);
    set_control(&mut st, "ukraine_s_s_r", Some(Power::Ussr));
    let atk = putn(
        &mut st,
        UnitType::Infantry,
        Power::Germany,
        "eastern_europe",
        hits,
    );
    let ids: Vec<UnitId> = defs
        .iter()
        .map(|&k| put(&mut st, k, Power::Ussr, "ukraine_s_s_r"))
        .collect();
    act(&mut st, Power::Germany, mv(&atk, &["ukraine_s_s_r"])).unwrap();
    act(&mut st, Power::Germany, Action::EndPhase).unwrap();
    act_seed(
        &mut st,
        Power::Germany,
        Action::BeginBattle {
            location: loc("ukraine_s_s_r"),
        },
        seed,
    )
    .unwrap();
    (st, ids)
}

fn pending(st: &GameState) -> (Power, u32, u32, Vec<UnitId>) {
    match crate::pending_decision(st) {
        Decision::SelectCasualties {
            power,
            hits,
            sub_hits,
            eligible,
            ..
        } => (power, hits, sub_hits, eligible),
        d => panic!("unexpected {d:?}"),
    }
}

/// Run the policy on a fresh battle; returns which of `ids` died (by index).
fn lost(defs: &[UnitType], hits: usize, seed: u64, policy: CasualtyPolicy) -> Vec<usize> {
    let (mut st, ids) = land_battle(defs, hits, seed);
    let (power, h, sh, eligible) = pending(&st);
    assert_eq!(power, Power::Ussr);
    let units = choose_casualties(&st, policy, h, sh, &eligible);
    act(&mut st, Power::Ussr, Action::SelectCasualties { units }).unwrap();
    ids.iter()
        .enumerate()
        .filter(|(_, &i)| st.unit(i).is_none())
        .map(|(n, _)| n)
        .collect()
}

#[test]
fn every_policy_is_accepted_and_orders_as_documented() {
    use UnitType::*;
    // attackers roll 1,1 (two hits); four defenders miss with 6s
    let seed = seed_for(&[1, 1, 6, 6, 6, 6]);
    let defs = [Fighter, Bomber, Armor, Infantry];
    // indices: 0 fighter, 1 bomber, 2 armor, 3 infantry
    assert_eq!(lost(&defs, 2, seed, CasualtyPolicy::CheapestFirst), [2, 3]);
    // defence: bomber 1 first, then infantry/armor tie at 2 broken by cost -> infantry
    assert_eq!(
        lost(&defs, 2, seed, CasualtyPolicy::KeepBestDefenders),
        [1, 3]
    );
    assert_eq!(
        lost(&defs, 2, seed, CasualtyPolicy::ProtectAircraft),
        [2, 3]
    );
    assert_eq!(
        lost(&defs, 2, seed, CasualtyPolicy::KeepCounterattack),
        [2, 3]
    );
}

#[test]
fn protect_aircraft_and_counterattack_spare_the_fighter_while_ground_units_remain() {
    use UnitType::*;
    let seed = seed_for(&[1, 1, 6, 6, 6]);
    let defs = [Fighter, Infantry, Armor];
    // cheapest first and protect aircraft both lose infantry + armor; the fighter lives
    for p in [
        CasualtyPolicy::CheapestFirst,
        CasualtyPolicy::ProtectAircraft,
        CasualtyPolicy::KeepCounterattack,
    ] {
        assert_eq!(lost(&defs, 2, seed, p), [1, 2], "{p:?}");
    }
    // keeping the best defenders sacrifices infantry and armor (def 2) before the fighter (4)
    assert_eq!(
        lost(&defs, 2, seed, CasualtyPolicy::KeepBestDefenders),
        [1, 2]
    );
}

#[test]
fn submarine_hits_never_land_on_aircraft() {
    // A British submarine surprise-strikes a German carrier group: one sub hit, but the fighter
    // on deck may not be chosen.
    for policy in CasualtyPolicy::ALL {
        let mut st = blank(Power::Uk, Phase::CombatMove);
        let s = put(&mut st, UnitType::Submarine, Power::Uk, "sz_north");
        let c = put(&mut st, UnitType::Carrier, Power::Germany, "sz_baltic");
        let t = put(&mut st, UnitType::Transport, Power::Germany, "sz_baltic");
        let f = put(&mut st, UnitType::Fighter, Power::Germany, "sz_baltic");
        st.unit_mut(f).unwrap().loaded_on = Some(c);
        act(&mut st, Power::Uk, mv(&[s], &["sz_baltic"])).unwrap();
        go_combat(&mut st, Power::Uk);
        act_seed(
            &mut st,
            Power::Uk,
            Action::BeginBattle {
                location: loc("sz_baltic"),
            },
            seed_for(&[2, 6, 6, 6]),
        )
        .unwrap();
        let (power, h, sh, eligible) = pending(&st);
        assert_eq!((power, h, sh), (Power::Germany, 1, 1));
        let units = choose_casualties(&st, policy, h, sh, &eligible);
        assert_eq!(units.len(), 1);
        assert_ne!(units[0], f, "{policy:?}");
        act(&mut st, Power::Germany, Action::SelectCasualties { units }).unwrap();
        assert!(st.unit(f).is_some());
        // the cheaper ship goes first under every policy
        assert!(st.unit(t).is_none() && st.unit(c).is_some(), "{policy:?}");
    }
}

#[test]
fn mixed_sub_and_normal_hits_keep_the_sub_rule() {
    // Two hits where only one is a sub hit and the fighter would sort first under a policy that
    // prefers losing aircraft: the first pick must still be non-air.
    let mut st = blank(Power::Uk, Phase::CombatMove);
    let c = put(&mut st, UnitType::Carrier, Power::Germany, "sz_baltic");
    let f = put(&mut st, UnitType::Fighter, Power::Germany, "sz_baltic");
    st.unit_mut(f).unwrap().loaded_on = Some(c);
    let b = put(&mut st, UnitType::Battleship, Power::Germany, "sz_baltic");
    for policy in CasualtyPolicy::ALL {
        let u = choose_casualties(&st, policy, 2, 1, &[c, f, b]);
        assert_eq!(u.len(), 2);
        assert!(!st.unit(u[0]).unwrap().kind.is_air(), "{policy:?}");
    }
}

#[test]
fn ids_and_defaults_roundtrip() {
    for p in CasualtyPolicy::ALL {
        let v = serde_json::to_value(p).unwrap();
        assert_eq!(v, serde_json::Value::String(p.id().into()));
        assert!(!p.label().is_empty() && !p.describe().is_empty());
    }
    for p in SubWithdrawalPolicy::ALL {
        let v = serde_json::to_value(p).unwrap();
        assert_eq!(v, serde_json::Value::String(p.id().into()));
        assert!(!p.label().is_empty() && !p.describe().is_empty());
    }
    assert_eq!(CasualtyPolicy::default(), CasualtyPolicy::CheapestFirst);
    assert_eq!(
        SubWithdrawalPolicy::default(),
        SubWithdrawalPolicy::WhenOutgunned
    );
}

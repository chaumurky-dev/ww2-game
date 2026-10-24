//! Mobilize phase: put purchased units where they are needed.
use crate::eval::Intel;
use wartide_engine::economy::complex_capacity_left;
use wartide_engine::query::legal_placements;
use wartide_engine::{Action, GameState, LocationId, MapData, Power, UnitType};

fn grad(intel: &Intel, l: LocationId) -> f64 {
    intel.grad[l.idx()].min(60.0)
}

fn placed_here(state: &GameState, l: LocationId) -> f64 {
    state
        .turn
        .placed
        .iter()
        .find(|x| x.0 == l)
        .map_or(0.0, |x| f64::from(x.1))
}

/// Next placement action (or `EndPhase` when nothing is left / placeable).
pub fn next_place(state: &GameState, me: Power, intel: &Intel, rejected: &[Action]) -> Action {
    let map = MapData::get();
    // Order: complexes, AA guns, land/air, then ships.
    let mut items: Vec<(UnitType, u32)> =
        state.purchases.iter().map(|i| (i.unit, i.count)).collect();
    items.sort_by_key(|(k, _)| match k {
        UnitType::IndustrialComplex => 0,
        UnitType::AaGun => 1,
        k if k.is_ship() => 3,
        _ => 2,
    });
    for (kind, count) in items {
        let spots = legal_placements(state, kind);
        if spots.is_empty() {
            continue;
        }
        let mut scored: Vec<(f64, LocationId)> = spots
            .iter()
            .map(|&l| {
                let i = l.idx();
                let s = match kind {
                    UnitType::IndustrialComplex => {
                        f64::from(map.ipc(l)) * 2.0 - grad(intel, l) - intel.threat[i] * 0.3
                    }
                    UnitType::AaGun => intel.threat[i] + f64::from(map.ipc(l)),
                    k if k.is_ship() => {
                        let near = map
                            .neighbours(l)
                            .iter()
                            .map(|&n| grad(intel, n))
                            .fold(60.0, f64::min);
                        -2.0 * intel.threat[i] - 0.1 * near
                    }
                    _ => {
                        let need = intel.garrison_need(state, crate::Style::Balanced, l);
                        let def = intel.defense[i] + 2.0 * placed_here(state, l);
                        2.0 * (need - def).clamp(0.0, 6.0) - 0.5 * grad(intel, l)
                    }
                };
                (s, l)
            })
            .collect();
        scored.sort_by(|a, b| b.0.total_cmp(&a.0).then(a.1.cmp(&b.1)));
        for (s, l) in scored {
            let mut n = 1;
            if !kind.is_ship() && kind != UnitType::AaGun && kind != UnitType::IndustrialComplex {
                let need = intel.garrison_need(state, crate::Style::Balanced, l);
                let def = intel.defense[l.idx()] + 2.0 * placed_here(state, l);
                n = if need > def {
                    (((need - def) / 2.0).ceil() as u32).max(1)
                } else {
                    count
                };
                n = n.min(count).min(complex_capacity_left(state, me, l).max(1));
            }
            let _ = s;
            for k in [n, 1] {
                let a = Action::Place {
                    unit: kind,
                    location: l,
                    count: k,
                    from: None,
                };
                if !rejected.contains(&a) {
                    return a;
                }
            }
        }
    }
    Action::EndPhase
}

//! Non-combat movement: land aircraft, reinforce threatened territories, advance towards the
//! front and shuttle transports.
use crate::eval::{mix, Intel};
use crate::garrison::spare_set;
use crate::Style;
use std::collections::HashSet;
use wartide_engine::movement::is_landing_spot;
use wartide_engine::query::{reachable, unload_options};
use wartide_engine::tech::defense_value;
use wartide_engine::{
    apply, Action, GameState, LocationId, MapData, Power, Unit, UnitId, UnitType,
};

/// Result of non-combat planning.
#[derive(Default)]
pub struct NcPlan {
    /// Actions in order (without the final `EndPhase`).
    pub actions: Vec<Action>,
    /// Most threatened territory we hold, if any.
    pub hold: Option<LocationId>,
    /// Units moved towards the front.
    pub advanced: u32,
    /// Transports repositioned or loaded.
    pub ferry: u32,
}

fn grad(intel: &Intel, l: LocationId) -> f64 {
    intel.grad[l.idx()].min(60.0)
}

fn push(sc: &mut GameState, me: Power, a: Action, seed: u64, out: &mut Vec<Action>) -> bool {
    if apply(sc, me, &a, seed).is_ok() {
        out.push(a);
        true
    } else {
        false
    }
}

fn land_aircraft(sc: &mut GameState, me: Power, intel: &Intel, seed: u64, out: &mut NcPlan) {
    let air: Vec<UnitId> = sc
        .units
        .iter()
        .filter(|u| u.owner == me && u.kind.is_air() && u.loaded_on.is_none())
        .map(|u| u.id)
        .collect();
    let map = MapData::get();
    let mut groups: Vec<(LocationId, Vec<LocationId>, Vec<UnitId>)> = vec![];
    for id in air {
        let u = sc.unit(id).unwrap().clone();
        if is_landing_spot(sc, &u, u.location) && !sc.enemy_occupied_land(me, u.location) {
            continue;
        }
        let mut best: Option<(f64, Vec<LocationId>)> = None;
        for r in reachable(sc, id).into_iter().filter(|r| r.landing) {
            let mut s = 0.0;
            if map.is_sea(r.to) {
                s += 3.0;
            } else {
                if sc.complex_at(r.to).is_some_and(|c| c.owner.is_ally(me)) {
                    s += 3.0;
                }
                if map.capital_of(r.to) == Some(me) {
                    s += 2.0;
                }
            }
            let deficit = (intel.threat[r.to.idx()] - intel.defense[r.to.idx()]).max(0.0);
            s -= deficit * 0.4;
            s -= r.path.len() as f64 * 0.2;
            s -= grad(intel, r.to) * 0.02;
            if best.as_ref().is_none_or(|b| s > b.0) {
                best = Some((s, r.path));
            }
        }
        if let Some((_, path)) = best {
            match groups.iter_mut().find(|g| g.0 == u.location && g.1 == path) {
                Some(g) => g.2.push(id),
                None => groups.push((u.location, path, vec![id])),
            }
        }
    }
    for (i, (_, path, units)) in groups.into_iter().enumerate() {
        // Mixed groups may fail on range; fall back to singles.
        let a = Action::Move {
            units: units.clone(),
            path: path.clone(),
        };
        if !push(sc, me, a, mix(&[seed, 1, i as u64]), &mut out.actions) {
            for (j, &id) in units.iter().enumerate() {
                let a = Action::Move {
                    units: vec![id],
                    path: path.clone(),
                };
                push(sc, me, a, mix(&[seed, 2, j as u64]), &mut out.actions);
            }
        }
    }
}

/// Aircraft that are safely landed shift towards the front when a safe base is closer.
fn stage_air(sc: &mut GameState, me: Power, intel: &Intel, seed: u64, out: &mut NcPlan) {
    let air: Vec<UnitId> = sc
        .units
        .iter()
        .filter(|u| u.owner == me && u.kind.is_air() && u.loaded_on.is_none())
        .map(|u| u.id)
        .collect();
    let mut groups: Vec<(LocationId, Vec<LocationId>, Vec<UnitId>)> = vec![];
    for id in air {
        let u = sc.unit(id).unwrap().clone();
        let cur = intel.air_grad[u.location.idx()];
        let mut best: Option<(f64, Vec<LocationId>)> = None;
        for r in reachable(sc, id).into_iter().filter(|r| r.landing) {
            let d = r.to.idx();
            let safe = intel.threat[d] <= intel.defense[d] * 1.2 + 2.0;
            let gain = cur - intel.air_grad[d];
            if safe && gain >= 1.0 && best.as_ref().is_none_or(|b| gain > b.0) {
                best = Some((gain, r.path));
            }
        }
        if let Some((_, path)) = best {
            match groups.iter_mut().find(|g| g.0 == u.location && g.1 == path) {
                Some(g) => g.2.push(id),
                None => groups.push((u.location, path, vec![id])),
            }
        }
    }
    for (i, (_, path, units)) in groups.into_iter().enumerate() {
        for (j, &id) in units.iter().enumerate() {
            let a = Action::Move {
                units: vec![id],
                path: path.clone(),
            };
            push(
                sc,
                me,
                a,
                mix(&[seed, 20 + i as u64, j as u64]),
                &mut out.actions,
            );
        }
    }
}

fn deficit(need: f64, vdef: f64) -> f64 {
    (need - vdef).max(0.0)
}

fn land_moves(
    sc: &mut GameState,
    me: Power,
    style: Style,
    intel: &Intel,
    seed: u64,
    out: &mut NcPlan,
) {
    let map = MapData::get();
    let mut vdef = intel.defense.clone();
    let need: Vec<f64> = (0..map.len())
        .map(|i| intel.garrison_need(sc, style, LocationId(i as u16)))
        .collect();
    let mut units: Vec<(f64, UnitId)> = sc
        .units
        .iter()
        .filter(|u| {
            u.owner == me
                && matches!(u.kind, UnitType::Infantry | UnitType::Armor)
                && u.loaded_on.is_none()
        })
        .map(|u| (intel.threat[u.location.idx()], u.id))
        .collect();
    units.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
    let risk = match style {
        Style::Aggressive => 1.0,
        Style::Balanced => 4.0,
        Style::Cautious => 8.0,
    };
    let mut groups: Vec<(LocationId, Vec<LocationId>, Vec<UnitId>)> = vec![];
    for (_, id) in units {
        let u = sc.unit(id).unwrap();
        let cur = u.location;
        let dv = f64::from(defense_value(sc, u));
        let rs = reachable(sc, id);
        if rs.is_empty() {
            continue;
        }
        let leave_loss = deficit(need[cur.idx()], vdef[cur.idx()] - dv)
            - deficit(need[cur.idx()], vdef[cur.idx()]);
        let stay = -grad(intel, cur);
        let mut best = (stay, None::<Vec<LocationId>>, cur);
        for r in rs
            .iter()
            .filter(|r| !r.battle && !map.is_sea(r.to) && sc.is_friendly_land(me, r.to))
        {
            let d = r.to.idx();
            let gain = deficit(need[d], vdef[d]) - deficit(need[d], vdef[d] + dv);
            let mut s = 3.0 * (gain - leave_loss) - grad(intel, r.to);
            if intel.threat[d] > (vdef[d] + dv) * 1.5 + 1.0 && sc.complex_at(r.to).is_none() {
                s -= risk;
            }
            if s > best.0 + 0.4 {
                best = (s, Some(r.path.clone()), r.to);
            }
        }
        if let Some(path) = best.1 {
            vdef[cur.idx()] -= dv;
            vdef[best.2.idx()] += dv;
            match groups.iter_mut().find(|g| g.0 == cur && g.1 == path) {
                Some(g) => g.2.push(id),
                None => groups.push((cur, path, vec![id])),
            }
        }
    }
    for (i, (_, path, units)) in groups.into_iter().enumerate() {
        let a = Action::Move {
            units: units.clone(),
            path: path.clone(),
        };
        if push(sc, me, a, mix(&[seed, 3, i as u64]), &mut out.actions) {
            out.advanced += units.len() as u32;
            continue;
        }
        for (j, &id) in units.iter().enumerate() {
            let a = Action::Move {
                units: vec![id],
                path: path.clone(),
            };
            if push(sc, me, a, mix(&[seed, 4, j as u64]), &mut out.actions) {
                out.advanced += 1;
            }
        }
    }
}

fn supply(sc: &GameState, me: Power, intel: &Intel, spare: &HashSet<UnitId>, z: LocationId) -> f64 {
    let map = MapData::get();
    sc.units
        .iter()
        .filter(|u| {
            u.owner == me
                && matches!(u.kind, UnitType::Infantry | UnitType::Armor)
                && u.loaded_on.is_none()
                && u.moves_used == 0
                && !u.moved_cm
                && spare.contains(&u.id)
                && map.adjacent(u.location, z)
                && grad(intel, u.location) >= 8.0
        })
        .map(|u| if u.kind == UnitType::Armor { 2.0 } else { 1.0 })
        .sum()
}

fn transports(
    sc: &mut GameState,
    me: Power,
    intel: &Intel,
    spare: &HashSet<UnitId>,
    seed: u64,
    out: &mut NcPlan,
) {
    let map = MapData::get();
    let trs: Vec<UnitId> = sc
        .units
        .iter()
        .filter(|u| u.owner == me && u.kind == UnitType::Transport && !u.frozen)
        .map(|u| u.id)
        .collect();
    for (i, tid) in trs.into_iter().enumerate() {
        let seed = mix(&[seed, 10, i as u64]);
        let Some(t) = sc.unit(tid).cloned() else {
            continue;
        };
        let cargo: Vec<&Unit> = sc.cargo_of(tid).filter(|c| c.owner == me).collect();
        if !cargo.is_empty() {
            let ids: Vec<UnitId> = cargo.iter().map(|c| c.id).collect();
            let fresh = cargo
                .iter()
                .any(|c| c.loaded_turn.is_some_and(|n| n >= sc.turn.number));
            if fresh {
                continue;
            }
            // Land at a friendly coast that connects to the front by land...
            let best = unload_options(sc, tid)
                .into_iter()
                .filter(|&l| sc.is_friendly_land(me, l) && grad(intel, l) < 40.0)
                .min_by(|a, b| grad(intel, *a).total_cmp(&grad(intel, *b)));
            if let Some(to) = best {
                let a = Action::Unload {
                    transport: tid,
                    units: ids,
                    to,
                };
                if push(sc, me, a, seed, &mut out.actions) {
                    out.ferry += 1;
                }
                continue;
            }
            // ...otherwise sail towards the enemy coast, staying out of danger.
            let here = intel.sea_grad[t.location.idx()];
            let mut b = (here - 1.0, None::<Vec<LocationId>>);
            for r in reachable(sc, tid).into_iter().filter(|r| !r.battle) {
                let g = intel.sea_grad[r.to.idx()];
                if intel.threat[r.to.idx()] <= 2.0 && g < b.0 {
                    b = (g, Some(r.path));
                }
            }
            if let Some(path) = b.1 {
                let a = Action::Move {
                    units: vec![tid],
                    path,
                };
                if push(sc, me, a, seed, &mut out.actions) {
                    out.ferry += 1;
                }
            }
            continue;
        }
        // Empty transport: go where cargo waits.
        let here = supply(sc, me, intel, spare, t.location) - intel.threat[t.location.idx()] * 1.5;
        let mut best = (here, None::<Vec<LocationId>>);
        for r in reachable(sc, tid).into_iter().filter(|r| !r.battle) {
            let s = supply(sc, me, intel, spare, r.to)
                - intel.threat[r.to.idx()] * 1.5
                - r.path.len() as f64 * 0.2;
            if s > best.0 + 0.5 {
                best = (s, Some(r.path));
            }
        }
        if let Some(path) = best.1 {
            let a = Action::Move {
                units: vec![tid],
                path,
            };
            if !push(sc, me, a, seed, &mut out.actions) {
                continue;
            }
        }
        let at = sc.unit(tid).unwrap().location;
        let mut load: Vec<&Unit> = sc
            .units
            .iter()
            .filter(|u| {
                u.owner == me
                    && matches!(u.kind, UnitType::Infantry | UnitType::Armor)
                    && u.loaded_on.is_none()
                    && u.moves_used == 0
                    && !u.moved_cm
                    && !u.frozen
                    && spare.contains(&u.id)
                    && map.adjacent(u.location, at)
                    && grad(intel, u.location) >= 8.0
            })
            .collect();
        load.sort_by_key(|u| (u.kind != UnitType::Armor, u.id));
        let ids: Vec<UnitId> = match load.first() {
            Some(f) if f.kind == UnitType::Armor => vec![f.id],
            Some(_) => load.iter().take(2).map(|u| u.id).collect(),
            None => continue,
        };
        let a = Action::Load {
            units: ids,
            transport: tid,
        };
        if push(sc, me, a, seed ^ 1, &mut out.actions) {
            out.ferry += 1;
        }
    }
}

/// Plan the non-combat phase.
pub fn plan_noncombat(
    state: &GameState,
    me: Power,
    style: Style,
    intel: &Intel,
    seed: u64,
) -> NcPlan {
    let mut out = NcPlan::default();
    let mut sc = state.clone();
    let spare = spare_set(state, me, style, intel);
    land_aircraft(&mut sc, me, intel, seed, &mut out);
    stage_air(&mut sc, me, intel, seed, &mut out);
    land_moves(&mut sc, me, style, intel, seed, &mut out);
    transports(&mut sc, me, intel, &spare, seed, &mut out);
    let map = MapData::get();
    out.hold = map
        .territories()
        .filter(|&t| state.control[t] == Some(me) && intel.threat[t.idx()] > 0.0)
        .filter(|&t| state.units_at(t).any(|u| u.owner == me && u.kind.is_land()))
        .max_by(|a, b| {
            (intel.threat[a.idx()] + f64::from(map.ipc(*a)))
                .total_cmp(&(intel.threat[b.idx()] + f64::from(map.ipc(*b))))
        });
    out
}

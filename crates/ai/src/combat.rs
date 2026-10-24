//! Combat-movement planning: land attacks, free grabs, amphibious assaults, sea attacks and
//! strategic bombing. The plan is built by replaying every move on a scratch copy of the state so
//! each emitted action was legal when planned.
use crate::eval::{mix, piece, sim, threshold, value, Intel, Piece};
use crate::garrison::spare_set;
use crate::Style;
use std::collections::{BTreeMap, HashMap, HashSet};
use wartide_engine::query::{battle_odds, reachable, Reach};
use wartide_engine::{
    apply, Action, GameState, LocationId, MapData, Power, Unit, UnitId, UnitType,
};

/// Kind of operation, for commentary.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AttackKind {
    /// Overland assault.
    Land,
    /// Taking an undefended territory.
    Grab,
    /// Assault from transports.
    Amphibious,
    /// Naval battle.
    Sea,
    /// Strategic bombing raid.
    Bombing,
}

/// One planned operation.
#[derive(Clone, Debug)]
pub struct AttackNote {
    /// Where.
    pub target: LocationId,
    /// What.
    pub kind: AttackKind,
    /// Estimated win chance.
    pub win: f64,
    /// Infantry committed.
    pub inf: u32,
    /// Armor committed.
    pub arm: u32,
    /// Aircraft committed.
    pub air: u32,
    /// Ships committed.
    pub ships: u32,
}

/// Result of combat planning.
#[derive(Default)]
pub struct CombatPlan {
    /// Actions in order (without the final `EndPhase`).
    pub actions: Vec<Action>,
    /// Operations for the chat summary.
    pub notes: Vec<AttackNote>,
}

struct Pkg {
    tr: UnitId,
    loads: Vec<Action>,
    path: Vec<LocationId>,
    cargo: Vec<UnitId>,
}

struct Cand {
    units: Vec<UnitId>,
    from: LocationId,
    path: Vec<LocationId>,
    pieces: Vec<Piece>,
    pkg: Option<usize>,
}

impl Cand {
    fn rank(&self) -> u32 {
        self.pieces.iter().map(|p| p.cost).sum::<u32>() * 1000 + self.units[0]
    }
}

fn defenders(state: &GameState, me: Power, t: LocationId) -> (Vec<Piece>, bool, f64) {
    let sea = MapData::get().is_sea(t);
    let mut out = vec![];
    let mut ipc = 0.0;
    for u in state.units_at(t).filter(|u| !u.owner.is_ally(me)) {
        let fights = if sea {
            u.kind.is_ship() || (u.kind == UnitType::Fighter && u.loaded_on.is_some())
        } else {
            u.kind.is_fighting_land_unit()
        };
        if fights {
            out.push(piece(state, u));
            ipc += f64::from(u.kind.stats().cost);
        }
    }
    (out, !sea && state.enemy_aa_at(me, t).is_some(), ipc)
}

fn note_of(
    state: &GameState,
    kind: AttackKind,
    target: LocationId,
    win: f64,
    ids: &[UnitId],
) -> AttackNote {
    let mut n = AttackNote {
        target,
        kind,
        win,
        inf: 0,
        arm: 0,
        air: 0,
        ships: 0,
    };
    for k in ids.iter().filter_map(|&i| state.unit(i)).map(|u| u.kind) {
        match k {
            UnitType::Infantry => n.inf += 1,
            UnitType::Armor => n.arm += 1,
            k if k.is_air() => n.air += 1,
            k if k.is_ship() => n.ships += 1,
            _ => {}
        }
    }
    n
}

/// Replay a set of candidates onto the scratch state, recording actions.
fn execute(
    sc: &mut GameState,
    me: Power,
    chosen: &[&Cand],
    pkgs: &[Pkg],
    target: LocationId,
    seed: u64,
    acts: &mut Vec<Action>,
) -> bool {
    type Group = ((LocationId, Vec<LocationId>), Vec<UnitId>);
    let mut groups: Vec<Group> = vec![];
    for c in chosen.iter().filter(|c| c.pkg.is_none()) {
        let key = (c.from, c.path.clone());
        match groups.iter_mut().find(|g| g.0 == key) {
            Some(g) => g.1.extend(c.units.iter().copied()),
            None => groups.push((key, c.units.clone())),
        }
    }
    let mut list: Vec<Action> = groups
        .into_iter()
        .map(|((_, path), units)| Action::Move { units, path })
        .collect();
    for c in chosen {
        if let Some(i) = c.pkg {
            let p = &pkgs[i];
            list.extend(p.loads.iter().cloned());
            if !p.path.is_empty() {
                list.push(Action::Move {
                    units: vec![p.tr],
                    path: p.path.clone(),
                });
            }
            list.push(Action::Unload {
                transport: p.tr,
                units: p.cargo.clone(),
                to: target,
            });
        }
    }
    for (i, a) in list.into_iter().enumerate() {
        if apply(sc, me, &a, mix(&[seed, i as u64])).is_err() {
            return false;
        }
        acts.push(a);
    }
    true
}

fn build_pkgs(
    state: &GameState,
    me: Power,
    style: Style,
    intel: &Intel,
    spare: &HashSet<UnitId>,
) -> (Vec<Pkg>, BTreeMap<LocationId, Vec<Cand>>) {
    let map = MapData::get();
    let mut pkgs = vec![];
    let mut cands: BTreeMap<LocationId, Vec<Cand>> = BTreeMap::new();
    let limit = match style {
        Style::Aggressive => 5.0,
        Style::Balanced => 3.0,
        Style::Cautious => 1.5,
    };
    for t in state
        .units
        .iter()
        .filter(|u| u.owner == me && u.kind == UnitType::Transport && !u.frozen)
    {
        let cargo_now: Vec<&Unit> = state.cargo_of(t.id).filter(|c| c.owner == me).collect();
        let (cargo, loads): (Vec<UnitId>, Vec<Action>) = if cargo_now.is_empty() {
            let mut cands: Vec<&Unit> = state
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
                        && map.adjacent(u.location, t.location)
                })
                .collect();
            cands.sort_by_key(|u| (u.kind != UnitType::Armor, u.id));
            let ids: Vec<UnitId> = match cands.first() {
                Some(f) if f.kind == UnitType::Armor => vec![f.id],
                Some(_) => cands.iter().take(2).map(|u| u.id).collect(),
                None => continue,
            };
            let l = Action::Load {
                units: ids.clone(),
                transport: t.id,
            };
            (ids, vec![l])
        } else {
            (cargo_now.iter().map(|c| c.id).collect(), vec![])
        };
        let mut zones: Vec<(LocationId, Vec<LocationId>)> = vec![(t.location, vec![])];
        for r in reachable(state, t.id) {
            if !r.battle {
                zones.push((r.to, r.path));
            }
        }
        let mut best: BTreeMap<LocationId, (f64, LocationId, Vec<LocationId>)> = BTreeMap::new();
        for (z, path) in zones {
            let th = intel.threat[z.idx()];
            if th > limit {
                continue;
            }
            for &target in map.neighbours(z) {
                if map.is_sea(target)
                    || state.is_untouched_neutral(target)
                    || !(state.is_enemy_controlled(me, target)
                        || state.enemy_occupied_land(me, target))
                {
                    continue;
                }
                if best.get(&target).is_none_or(|b| th < b.0) {
                    best.insert(target, (th, z, path.clone()));
                }
            }
        }
        for (target, (_, _, path)) in best {
            let idx = pkgs.len();
            let pieces = cargo
                .iter()
                .filter_map(|&i| state.unit(i))
                .map(|u| piece(state, u))
                .collect();
            pkgs.push(Pkg {
                tr: t.id,
                loads: loads.clone(),
                path: path.clone(),
                cargo: cargo.clone(),
            });
            cands.entry(target).or_default().push(Cand {
                units: cargo.clone(),
                from: t.location,
                path,
                pieces,
                pkg: Some(idx),
            });
        }
    }
    (pkgs, cands)
}

/// Plan the whole combat-movement phase.
pub fn plan_combat_move(
    state: &GameState,
    me: Power,
    style: Style,
    intel: &Intel,
    seed: u64,
) -> CombatPlan {
    let mut out = CombatPlan::default();
    if state.soviet_restricted() {
        return out;
    }
    let map = MapData::get();
    let thr = threshold(style);
    let spare = spare_set(state, me, style, intel);
    let (pkgs, mut pools) = build_pkgs(state, me, style, intel, &spare);
    let mut reach: HashMap<UnitId, Vec<Reach>> = HashMap::new();
    for u in state.units.iter().filter(|u| u.owner == me) {
        if u.kind == UnitType::IndustrialComplex
            || (u.loaded_on.is_some() && u.kind != UnitType::Fighter)
        {
            continue;
        }
        let r = reachable(state, u.id);
        if !r.is_empty() {
            reach.insert(u.id, r);
        }
    }
    for u in state.units.iter().filter(|u| u.owner == me) {
        let Some(rs) = reach.get(&u.id) else { continue };
        for r in rs {
            let to = r.to;
            let ok = if map.is_sea(to) {
                r.battle
                    && ((u.kind.is_ship()
                        && u.kind != UnitType::Transport
                        && !(u.kind == UnitType::Carrier && state.cargo_of(u.id).next().is_some()))
                        || (u.kind.is_air() && u.loaded_on.is_none()))
            } else if state.is_untouched_neutral(to) {
                false
            } else if u.kind.is_air() {
                r.battle
            } else if matches!(u.kind, UnitType::Infantry | UnitType::Armor) {
                (r.battle || state.is_enemy_controlled(me, to)) && spare.contains(&u.id)
            } else {
                false
            };
            if ok {
                pools.entry(to).or_default().push(Cand {
                    units: vec![u.id],
                    from: u.location,
                    path: r.path.clone(),
                    pieces: vec![piece(state, u)],
                    pkg: None,
                });
            }
        }
    }

    let mut order: Vec<(f64, LocationId)> = pools
        .keys()
        .map(|&t| {
            let v = if map.is_sea(t) {
                -1.0
            } else {
                value(state, me, t)
            };
            (v, t)
        })
        .collect();
    order.sort_by(|a, b| b.0.total_cmp(&a.0).then(a.1.cmp(&b.1)));

    let mut sc = state.clone();
    let mut used: HashSet<UnitId> = HashSet::new();
    let mut used_tr: HashSet<UnitId> = HashSet::new();
    for (val, target) in order {
        let sea = map.is_sea(target);
        let (dp, aa, def_ipc) = defenders(state, me, target);
        let capital = map.capital_of(target).is_some_and(|c| !c.is_ally(me));
        let mut cs: Vec<&Cand> = pools[&target]
            .iter()
            .filter(|c| {
                c.units.iter().all(|u| !used.contains(u))
                    && c.pkg.is_none_or(|i| !used_tr.contains(&pkgs[i].tr))
            })
            .collect();
        if cs.is_empty() {
            continue;
        }
        cs.sort_by_key(|c| c.rank());
        if sea && (dp.is_empty() || style == Style::Cautious) {
            continue;
        }
        let seed_t = mix(&[seed, u64::from(target.0)]);
        // Undefended land: grab with the cheapest land unit.
        if !sea && dp.is_empty() {
            let Some(c) = cs.iter().find(|c| c.pieces.iter().any(|p| p.land)) else {
                continue;
            };
            if val < 1.0 && intel.threat[target.idx()] > 6.0 {
                continue;
            }
            let (backup, n) = (sc.clone(), out.actions.len());
            if execute(&mut sc, me, &[c], &pkgs, target, seed_t, &mut out.actions) {
                used.extend(c.units.iter().copied());
                if let Some(i) = c.pkg {
                    used_tr.insert(pkgs[i].tr);
                }
                out.notes
                    .push(note_of(state, AttackKind::Grab, target, 1.0, &c.units));
            } else {
                sc = backup;
                out.actions.truncate(n);
            }
            continue;
        }
        let need_land = !sea;
        let amph_bonus = 0.0;
        let thr_t = if capital { (thr - 0.2).max(0.45) } else { thr };
        let goal = (thr_t + 0.05).min(0.95);
        let mut chosen: Vec<usize> = vec![];
        let mut atk: Vec<Piece> = vec![];
        let mut odds = None;
        for (i, c) in cs.iter().enumerate() {
            chosen.push(i);
            atk.extend(c.pieces.iter().copied());
            if atk.iter().map(|p| f64::from(p.atk)).sum::<f64>() < 1.0 {
                continue;
            }
            let o = sim(&atk, &dp, aa, need_land, 80, seed_t);
            if o.win >= goal {
                odds = Some(o);
                break;
            }
        }
        if odds.is_none() {
            let o = sim(&atk, &dp, aa, need_land, 80, seed_t);
            if o.win >= thr_t {
                odds = Some(o);
            }
        }
        let Some(mut o) = odds else { continue };
        // Trim the dearest contributors while the odds stay above the goal.
        let mut by_cost = chosen.clone();
        by_cost.sort_by_key(|&i| std::cmp::Reverse(cs[i].rank()));
        for i in by_cost {
            if chosen.len() <= 1 {
                break;
            }
            let rest: Vec<usize> = chosen.iter().copied().filter(|&j| j != i).collect();
            let pieces: Vec<Piece> = rest
                .iter()
                .flat_map(|&j| cs[j].pieces.iter().copied())
                .collect();
            let t = sim(&pieces, &dp, aa, need_land, 80, seed_t);
            if t.win >= goal.max(o.win - 0.03) {
                chosen = rest;
                o = t;
            }
        }
        let pk_amph = chosen.iter().any(|&i| cs[i].pkg.is_some());
        let min_win = thr_t + if pk_amph { 0.05 } else { 0.0 } + amph_bonus;
        if o.win < min_win {
            continue;
        }
        let aggr = if style == Style::Aggressive { 1.4 } else { 1.0 };
        let objective = if sea { def_ipc * 0.5 } else { val * 2.5 * aggr };
        let ev = o.win * objective + o.kill * 0.8 - o.lose;
        if !capital && ev <= 0.0 {
            continue;
        }
        let picks: Vec<&Cand> = chosen.iter().map(|&i| cs[i]).collect();
        let (backup, n) = (sc.clone(), out.actions.len());
        let ok = execute(&mut sc, me, &picks, &pkgs, target, seed_t, &mut out.actions);
        let real = if ok {
            battle_odds(&sc, target, 60, seed_t)
        } else {
            None
        };
        if !ok
            || real
                .as_ref()
                .is_some_and(|r| r.attacker_win < min_win - 0.15)
        {
            sc = backup;
            out.actions.truncate(n);
            continue;
        }
        let win = real.map_or(o.win, |r| r.attacker_win);
        let ids: Vec<UnitId> = picks.iter().flat_map(|c| c.units.iter().copied()).collect();
        for c in &picks {
            used.extend(c.units.iter().copied());
            if let Some(i) = c.pkg {
                used_tr.insert(pkgs[i].tr);
            }
        }
        let kind = if sea {
            AttackKind::Sea
        } else if pk_amph {
            AttackKind::Amphibious
        } else {
            AttackKind::Land
        };
        out.notes.push(note_of(state, kind, target, win, &ids));
    }
    plan_bombing(state, me, style, &reach, &used, &mut sc, seed, &mut out);
    out
}

#[allow(clippy::too_many_arguments)]
fn plan_bombing(
    state: &GameState,
    me: Power,
    style: Style,
    reach: &HashMap<UnitId, Vec<Reach>>,
    used: &HashSet<UnitId>,
    sc: &mut GameState,
    seed: u64,
    out: &mut CombatPlan,
) {
    if style == Style::Cautious && state.round < 3 {
        return;
    }
    let map = MapData::get();
    let mut groups: Vec<(LocationId, Vec<LocationId>, Vec<UnitId>)> = vec![];
    for u in state
        .units
        .iter()
        .filter(|u| u.owner == me && u.kind == UnitType::Bomber && !used.contains(&u.id))
    {
        let Some(rs) = reach.get(&u.id) else { continue };
        let best = rs
            .iter()
            .filter(|r| {
                !map.is_sea(r.to)
                    && state.is_enemy_controlled(me, r.to)
                    && state.complex_at(r.to).is_some_and(|c| !c.owner.is_ally(me))
                    && !state.enemy_occupied_land(me, r.to)
                    && !state.is_untouched_neutral(r.to)
            })
            .max_by_key(|r| (map.ipc(r.to), std::cmp::Reverse(r.path.len())));
        if let Some(r) = best {
            match groups
                .iter_mut()
                .find(|g| g.0 == u.location && g.1 == r.path)
            {
                Some(g) => g.2.push(u.id),
                None => groups.push((u.location, r.path.clone(), vec![u.id])),
            }
        }
    }
    for (i, (_, path, units)) in groups.into_iter().enumerate() {
        let a = Action::Move {
            units: units.clone(),
            path: path.clone(),
        };
        if apply(sc, me, &a, mix(&[seed, 777, i as u64])).is_ok() {
            let target = *path.last().unwrap();
            out.notes
                .push(note_of(state, AttackKind::Bombing, target, 1.0, &units));
            out.actions.push(a);
        }
    }
}

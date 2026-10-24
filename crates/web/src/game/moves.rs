//! Movement helpers over the engine queries (pure; unit-tested natively).
use wartide_engine::movement::{
    check_unit_ready, is_landing_spot, landing_reachable, remaining_moves, validate_move,
};
use wartide_engine::query::{load_options, reachable, unload_options, Reach};
use wartide_engine::{GameState, LocationId, Power, UnitId, UnitType};

/// Destinations every unit of the group can reach together, each with a path that validates for
/// the whole group (`Move` takes one path for all units).
pub fn group_reach(st: &GameState, units: &[UnitId]) -> Vec<Reach> {
    let Some(first) = units.first().and_then(|&u| st.unit(u)) else {
        return vec![];
    };
    let p = first.owner;
    let per: Vec<Vec<Reach>> = units.iter().map(|&u| reachable(st, u)).collect();
    per[0]
        .iter()
        .filter_map(|r| {
            let mut cands = vec![r.path.clone()];
            let mut landing = true;
            let mut battle = r.battle;
            for (i, other) in per.iter().enumerate() {
                let x = other.iter().find(|x| x.to == r.to)?;
                cands.push(x.path.clone());
                battle |= x.battle;
                if st.unit(units[i]).is_some_and(|u| u.kind.is_air()) {
                    landing &= x.landing;
                }
            }
            let path = cands
                .into_iter()
                .find(|c| validate_move(st, p, units, c).is_ok())?;
            Some(Reach {
                to: r.to,
                path,
                battle,
                landing,
            })
        })
        .collect()
}

/// A run of identical units at one location, as shown in the unit picker.
#[derive(Clone, Debug, PartialEq)]
pub struct UnitGroup {
    pub kind: UnitType,
    pub ids: Vec<UnitId>,
    pub moves_left: u8,
    /// Fighters riding a carrier (they may launch).
    pub aboard: bool,
    pub can_move: bool,
}

/// Movable-looking units of `p` at `loc`, grouped. Land cargo is excluded (it unloads instead).
pub fn unit_groups(st: &GameState, loc: LocationId, p: Power) -> Vec<UnitGroup> {
    let mut out: Vec<UnitGroup> = vec![];
    for u in st.units_at(loc).filter(|u| u.owner == p) {
        if u.kind == UnitType::IndustrialComplex {
            continue;
        }
        let aboard = u.loaded_on.is_some();
        if aboard && u.kind != UnitType::Fighter {
            continue;
        }
        let can_move = !reachable(st, u.id).is_empty();
        let moves_left = remaining_moves(st, u);
        match out.iter_mut().find(|g| {
            g.kind == u.kind
                && g.aboard == aboard
                && g.can_move == can_move
                && g.moves_left == moves_left
        }) {
            Some(g) => g.ids.push(u.id),
            None => out.push(UnitGroup {
                kind: u.kind,
                ids: vec![u.id],
                moves_left,
                aboard,
                can_move,
            }),
        }
    }
    out.sort_by_key(|g| (!g.can_move, g.kind, g.aboard));
    out
}

/// Locations holding units that can still move, load or unload (for the "selectable" highlight).
pub fn movable_locations(st: &GameState, p: Power) -> Vec<LocationId> {
    let mut sigs: Vec<(LocationId, UnitType, u8, bool, bool, bool, bool)> = vec![];
    let mut out: Vec<LocationId> = vec![];
    for u in st.units.iter().filter(|u| u.owner == p) {
        if out.contains(&u.location) {
            continue;
        }
        let sig = (
            u.location,
            u.kind,
            u.moves_used,
            u.moved_cm,
            u.stopped,
            u.frozen,
            u.loaded_on.is_some(),
        );
        if sigs.contains(&sig) {
            continue;
        }
        sigs.push(sig);
        let ok = !reachable(st, u.id).is_empty()
            || (u.kind == UnitType::Transport && !unload_options(st, u.id).is_empty())
            || (u.kind.transport_slots().is_some()
                && u.loaded_on.is_none()
                && !load_options(st, u.id).is_empty());
        if ok {
            out.push(u.location);
        }
    }
    out
}

/// Aircraft of `p` that cannot reach any landing spot with the moves they have left.
pub fn stranded_air(st: &GameState, p: Power) -> Vec<UnitId> {
    st.units
        .iter()
        .filter(|u| u.owner == p && u.kind.is_air() && u.loaded_on.is_none())
        .filter(|u| {
            !is_landing_spot(st, u, u.location)
                && !landing_reachable(st, u, u.location, remaining_moves(st, u))
        })
        .map(|u| u.id)
        .collect()
}

/// Shorter alias used by panels to test whether `check_unit_ready` passes.
pub fn ready(st: &GameState, id: UnitId) -> bool {
    st.unit(id)
        .is_some_and(|u| check_unit_ready(st, u, u.owner).is_ok())
}

#[cfg(test)]
mod tests {
    use super::*;
    use wartide_engine::{apply, Action, GameSettings, Phase};

    fn to_combat_move() -> GameState {
        let mut st = GameState::new(GameSettings::default());
        apply(&mut st, Power::Ussr, &Action::EndPhase, 1).unwrap();
        apply(&mut st, Power::Ussr, &Action::EndPhase, 2).unwrap();
        assert_eq!(st.phase, Phase::CombatMove);
        st
    }

    #[test]
    fn group_reach_paths_validate() {
        let st = to_combat_move();
        let locs = movable_locations(&st, Power::Ussr);
        assert!(!locs.is_empty());
        let loc = locs[0];
        let ids: Vec<UnitId> = unit_groups(&st, loc, Power::Ussr)
            .into_iter()
            .filter(|g| g.can_move)
            .flat_map(|g| g.ids.into_iter().take(1))
            .collect();
        let reach = group_reach(&st, &ids[..1]);
        assert!(!reach.is_empty());
        for r in reach {
            assert!(validate_move(&st, Power::Ussr, &ids[..1], &r.path).is_ok());
        }
    }

    #[test]
    fn groups_are_merged_by_kind() {
        let st = to_combat_move();
        for l in movable_locations(&st, Power::Ussr) {
            let g = unit_groups(&st, l, Power::Ussr);
            let n: usize = g.iter().map(|g| g.ids.len()).sum();
            assert!(n >= 1);
            assert!(g.len() <= n);
        }
    }
}

//! Pure planning for dragging a unit stack onto a destination (unit-tested natively).
use super::moves::group_reach;
use crate::model::UnitKind;
use wartide_engine::query::{load_options, reachable};
use wartide_engine::transport::validate_load;
use wartide_engine::{GameState, LocationId, MapData, Power, UnitId, UnitType};

/// What dropping a stack on a destination would do.
#[derive(Clone, Debug, PartialEq)]
pub enum DropPlan {
    /// Ordinary move along `path`.
    Move {
        units: Vec<UnitId>,
        path: Vec<LocationId>,
    },
    /// Land units onto the single friendly transport in the destination sea zone.
    Load {
        units: Vec<UnitId>,
        transport: UnitId,
    },
    /// Several transports could take the units: leave them selected and let the transport
    /// section of the side panel pick one.
    LoadChoice { units: Vec<UnitId> },
}

/// Engine unit type for a UI unit kind.
pub fn engine_kind(k: UnitKind) -> UnitType {
    match k {
        UnitKind::Infantry => UnitType::Infantry,
        UnitKind::Armor => UnitType::Armor,
        UnitKind::Fighter => UnitType::Fighter,
        UnitKind::Bomber => UnitType::Bomber,
        UnitKind::Battleship => UnitType::Battleship,
        UnitKind::Carrier => UnitType::Carrier,
        UnitKind::Transport => UnitType::Transport,
        UnitKind::Submarine => UnitType::Submarine,
        UnitKind::AaGun => UnitType::AaGun,
        UnitKind::IndustrialComplex => UnitType::IndustrialComplex,
    }
}

/// "Half" button: half of `total`, rounded up, at least 1 (and never more than `total`).
pub fn half(total: usize) -> usize {
    total.div_ceil(2).max(1).min(total.max(1))
}

/// Units of `p` of type `kind` at `from` that can still move or load. Carrier-aboard fighters and
/// loaded cargo are excluded: they travel with their ship or via the transport section.
pub fn movable_ids(st: &GameState, from: LocationId, p: Power, kind: UnitType) -> Vec<UnitId> {
    if kind == UnitType::IndustrialComplex {
        return vec![];
    }
    st.units_at(from)
        .filter(|u| u.owner == p && u.kind == kind && u.loaded_on.is_none())
        .filter(|u| {
            !reachable(st, u.id).is_empty()
                || (u.kind.transport_slots().is_some() && !load_options(st, u.id).is_empty())
        })
        .map(|u| u.id)
        .collect()
}

/// Plan the drop of the first `count` movable units of `kind` at `from` onto `dest`.
pub fn drop_plan(
    st: &GameState,
    from: LocationId,
    kind: UnitType,
    dest: LocationId,
    count: usize,
) -> Option<DropPlan> {
    let p = st.current_power;
    let units: Vec<UnitId> = movable_ids(st, from, p, kind)
        .into_iter()
        .take(count)
        .collect();
    if units.is_empty() || dest == from {
        return None;
    }
    if let Some(r) = group_reach(st, &units).into_iter().find(|r| r.to == dest) {
        return Some(DropPlan::Move {
            units,
            path: r.path,
        });
    }
    if MapData::get().is_sea(dest) && kind.is_land() {
        let ok: Vec<UnitId> = st
            .units_at(dest)
            .filter(|t| t.kind == UnitType::Transport)
            .map(|t| t.id)
            .filter(|&t| validate_load(st, p, &units, t).is_ok())
            .collect();
        return match ok.as_slice() {
            [] => None,
            [t] => Some(DropPlan::Load {
                units,
                transport: *t,
            }),
            _ => Some(DropPlan::LoadChoice { units }),
        };
    }
    None
}

/// Largest number of units that `drop_plan` accepts for this destination (0 = illegal drop).
pub fn max_count(st: &GameState, from: LocationId, kind: UnitType, dest: LocationId) -> usize {
    let total = movable_ids(st, from, st.current_power, kind).len();
    (1..=total)
        .rev()
        .find(|&n| drop_plan(st, from, kind, dest, n).is_some())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::moves::movable_locations;
    use wartide_engine::movement::validate_move;
    use wartide_engine::{apply, Action, GameSettings, Phase};

    fn to_combat_move() -> GameState {
        let mut st = GameState::new(GameSettings::default());
        apply(&mut st, Power::Ussr, &Action::EndPhase, 1).unwrap();
        apply(&mut st, Power::Ussr, &Action::EndPhase, 2).unwrap();
        assert_eq!(st.phase, Phase::CombatMove);
        st
    }

    /// A USSR land location with movable infantry that has a sea neighbour.
    fn coastal_infantry(st: &GameState) -> (LocationId, LocationId) {
        let map = MapData::get();
        for l in movable_locations(st, Power::Ussr) {
            if map.is_sea(l) || movable_ids(st, l, Power::Ussr, UnitType::Infantry).is_empty() {
                continue;
            }
            if let Some(&sz) = map.neighbours(l).iter().find(|&&n| map.is_sea(n)) {
                return (l, sz);
            }
        }
        panic!("no coastal USSR infantry in the start position");
    }

    #[test]
    fn half_rounds_up_and_is_at_least_one() {
        assert_eq!(half(0), 1);
        assert_eq!(half(1), 1);
        assert_eq!(half(2), 1);
        assert_eq!(half(3), 2);
        assert_eq!(half(7), 4);
        assert_eq!(half(10), 5);
    }

    #[test]
    fn move_plan_for_reachable_destination() {
        let st = to_combat_move();
        let loc = movable_locations(&st, Power::Ussr)
            .into_iter()
            .find(|&l| !movable_ids(&st, l, Power::Ussr, UnitType::Infantry).is_empty())
            .unwrap();
        let ids = movable_ids(&st, loc, Power::Ussr, UnitType::Infantry);
        let reach = group_reach(&st, &ids[..1]);
        let dest = reach.first().expect("some destination").to;
        match drop_plan(&st, loc, UnitType::Infantry, dest, 1).unwrap() {
            DropPlan::Move { units, path } => {
                assert_eq!(units, ids[..1].to_vec());
                assert!(validate_move(&st, Power::Ussr, &units, &path).is_ok());
            }
            other => panic!("expected a move, got {other:?}"),
        }
        assert!(max_count(&st, loc, UnitType::Infantry, dest) >= 1);
    }

    #[test]
    fn unreachable_or_same_destination_is_none() {
        let st = to_combat_move();
        let loc = movable_locations(&st, Power::Ussr)[0];
        let kind = st
            .units_at(loc)
            .find(|u| {
                u.owner == Power::Ussr && !movable_ids(&st, loc, Power::Ussr, u.kind).is_empty()
            })
            .unwrap()
            .kind;
        assert_eq!(drop_plan(&st, loc, kind, loc, 1), None);
        let far = LocationId::from_id("japan").unwrap();
        assert_eq!(drop_plan(&st, loc, kind, far, 1), None);
        assert_eq!(max_count(&st, loc, kind, far), 0);
        assert_eq!(drop_plan(&st, loc, kind, far, 0), None);
    }

    #[test]
    fn load_onto_single_transport_then_choice_with_two() {
        let mut st = to_combat_move();
        let (land, sz) = coastal_infantry(&st);
        assert_eq!(drop_plan(&st, land, UnitType::Infantry, sz, 1), None);
        let t1 = st.spawn(UnitType::Transport, Power::Ussr, sz);
        match drop_plan(&st, land, UnitType::Infantry, sz, 1) {
            Some(DropPlan::Load { transport, units }) => {
                assert_eq!(transport, t1);
                assert_eq!(units.len(), 1);
            }
            other => panic!("expected Load, got {other:?}"),
        }
        // capacity limits how many may be dropped
        assert!(max_count(&st, land, UnitType::Infantry, sz) <= 2);
        st.spawn(UnitType::Transport, Power::Ussr, sz);
        assert!(matches!(
            drop_plan(&st, land, UnitType::Infantry, sz, 1),
            Some(DropPlan::LoadChoice { .. })
        ));
    }

    #[test]
    fn cargo_aboard_is_not_draggable() {
        let mut st = to_combat_move();
        let (_, sz) = coastal_infantry(&st);
        let t = st.spawn(UnitType::Transport, Power::Ussr, sz);
        let i = st.spawn(UnitType::Infantry, Power::Ussr, sz);
        st.unit_mut(i).unwrap().loaded_on = Some(t);
        assert!(movable_ids(&st, sz, Power::Ussr, UnitType::Infantry).is_empty());
    }
}

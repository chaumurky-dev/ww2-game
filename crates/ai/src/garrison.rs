//! Which land units can leave their territory without endangering it.
use crate::eval::Intel;
use crate::Style;
use std::collections::HashSet;
use wartide_engine::{GameState, MapData, Power, Unit, UnitId, UnitType};

/// Land units that may leave their territory without endangering it.
pub fn spare_set(state: &GameState, me: Power, style: Style, intel: &Intel) -> HashSet<UnitId> {
    let map = MapData::get();
    let mut spare = HashSet::new();
    for t in map.territories() {
        let mut mine: Vec<&Unit> = state
            .units_at(t)
            .filter(|u| u.owner == me && matches!(u.kind, UnitType::Infantry | UnitType::Armor))
            .collect();
        if mine.is_empty() {
            continue;
        }
        let need = intel.garrison_need(state, style, t);
        let mut acc: f64 = state
            .units_at(t)
            .filter(|u| u.owner != me && u.owner.is_ally(me) && u.kind.is_fighting_land_unit())
            .map(|u| f64::from(wartide_engine::tech::defense_value(state, u)))
            .sum();
        mine.sort_by_key(|u| (u.kind != UnitType::Infantry, u.id));
        let min_keep = if need > 0.0 && map.capital_of(t) == Some(me) {
            2
        } else {
            0
        };
        for (i, u) in mine.iter().enumerate() {
            if acc < need || i < min_keep {
                acc += f64::from(wartide_engine::tech::defense_value(state, u));
            } else {
                spare.insert(u.id);
            }
        }
    }
    for u in &state.units {
        if u.owner == me && (u.kind.is_air() || u.kind.is_ship()) {
            spare.insert(u.id);
        }
    }
    spare
}

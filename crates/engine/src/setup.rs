//! The printed starting position.
use crate::map::{LocMap, MapData};
use crate::power::{PerPower, Power};
use crate::state::{GameSettings, GameState, Phase, TurnState};
use crate::tech::Tech;
use crate::units::UnitType;

impl GameState {
    /// The printed 1942 setup: USSR to move, round 1.
    pub fn new(settings: GameSettings) -> GameState {
        let map = MapData::get();
        let mut treasury = PerPower::<u32>::default();
        for p in Power::ALL {
            treasury[p] = map.starting_ipcs(p);
        }
        let mut control = LocMap::<Option<Power>>::new();
        for t in map.territories() {
            control[t] = map.original_owner(t);
        }
        let mut techs = PerPower::<Vec<Tech>>::default();
        if settings.germany_jet_power {
            techs[Power::Germany].push(Tech::JetPower);
        }
        if settings.japan_super_subs {
            techs[Power::Japan].push(Tech::SuperSubmarines);
        }
        let phase = if settings.weapons_development {
            Phase::DevelopWeapons
        } else {
            Phase::Purchase
        };
        let mut st = GameState {
            settings,
            round: 1,
            current_power: Power::Ussr,
            phase,
            treasury,
            techs,
            control: control.clone(),
            units: vec![],
            next_unit_id: 1,
            purchases: vec![],
            new_complexes_placed: 0,
            turn: TurnState {
                number: 1,
                start_control: control,
                resolved: vec![],
                researched: false,
                rocket_fired: false,
                placed: vec![],
            },
            battle: None,
            conceded: vec![],
            winner: None,
        };
        for s in map.starting_units() {
            for _ in 0..s.count {
                let id = st.spawn(s.unit, s.owner, s.location);
                debug_assert!(id > 0);
            }
        }
        debug_assert!(st
            .units
            .iter()
            .any(|u| u.kind == UnitType::IndustrialComplex));
        st
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map::LocationId;

    fn count(st: &GameState, loc: &str, owner: Power, k: UnitType) -> usize {
        let l = LocationId::from_id(loc).unwrap();
        st.units_at(l)
            .filter(|u| u.owner == owner && u.kind == k)
            .count()
    }

    #[test]
    fn printed_stacks() {
        let st = GameState::new(GameSettings::default());
        assert_eq!(count(&st, "russia", Power::Ussr, UnitType::Infantry), 4);
        assert_eq!(count(&st, "russia", Power::Ussr, UnitType::Armor), 2);
        assert_eq!(count(&st, "germany", Power::Germany, UnitType::Bomber), 1);
        assert_eq!(count(&st, "sz_north", Power::Uk, UnitType::Battleship), 1);
        assert_eq!(
            count(&st, "united_kingdom", Power::Uk, UnitType::Fighter),
            2
        );
        assert_eq!(
            count(&st, "sz_caroline_islands", Power::Japan, UnitType::Carrier),
            1
        );
        assert_eq!(
            count(&st, "western_usa", Power::Usa, UnitType::IndustrialComplex),
            1
        );
        let complexes = st
            .units
            .iter()
            .filter(|u| u.kind == UnitType::IndustrialComplex)
            .count();
        assert_eq!(complexes, 8);
        let total: u32 = MapData::get()
            .starting_units()
            .iter()
            .map(|s| s.count)
            .sum();
        assert_eq!(st.units.len() as u32, total);
    }

    #[test]
    fn neutrals_unowned_and_no_purchase_phase_skip() {
        let st = GameState::new(GameSettings::default());
        let spain = LocationId::from_id("spain").unwrap();
        assert_eq!(st.control[spain], None);
        assert!(st.is_untouched_neutral(spain));
        let st2 = GameState::new(GameSettings {
            weapons_development: false,
            ..Default::default()
        });
        assert_eq!(st2.phase, Phase::Purchase);
    }

    #[test]
    fn serde_roundtrip() {
        let st = GameState::new(GameSettings::default());
        let j = serde_json::to_string(&st).unwrap();
        let back: GameState = serde_json::from_str(&j).unwrap();
        assert_eq!(st, back);
        assert!(j.contains("\"russia\":\"ussr\""));
    }
}

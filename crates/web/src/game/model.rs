//! Board view-model derived from the engine's `GameState` (pure; unit-tested natively).
use super::conv::*;
use crate::components::board::{BoardModel, Highlight, Stack};
use std::collections::BTreeMap;
use wartide_engine::{GameState, LocationId, MapData, UnitId, UnitType};

/// Interaction overlay: what the board should highlight on top of the plain state.
#[derive(Default, Clone, PartialEq, Debug)]
pub struct Overlay {
    pub selected: Option<LocationId>,
    pub picked: Vec<UnitId>,
    /// Reachable destinations of the picked units; `true` when ending there starts a battle.
    pub dests: Vec<(LocationId, bool)>,
    pub placements: Vec<LocationId>,
    pub battles: Vec<LocationId>,
    pub movable: Vec<LocationId>,
    pub flash: Vec<LocationId>,
}

/// Locations worth a brief highlight when `events` arrive (moves, battles, captures, placements).
pub fn event_locations(events: &[wartide_engine::Event]) -> Vec<LocationId> {
    use wartide_engine::Event as E;
    let mut v: Vec<LocationId> = vec![];
    for e in events {
        match e {
            E::UnitsMoved { from, path, .. } => v.push(*path.last().unwrap_or(from)),
            E::UnitsUnloaded { to, .. } => v.push(*to),
            E::TerritoryCaptured { location, .. } | E::BattleStarted { location, .. } => {
                v.push(*location)
            }
            E::Retreated { to, .. } => v.push(*to),
            E::UnitsPlaced { location, .. } => v.push(*location),
            _ => {}
        }
    }
    v.sort_by_key(|l| l.0);
    v.dedup();
    v
}

/// Where units taken back by `MovesUndone` stand again (the places to flash).
pub fn undo_locations(
    after: &wartide_engine::GameState,
    events: &[wartide_engine::Event],
) -> Vec<LocationId> {
    use wartide_engine::Event as E;
    events
        .iter()
        .filter_map(|e| match e {
            E::MovesUndone {
                units, cascaded, ..
            } => Some(units.iter().chain(cascaded)),
            _ => None,
        })
        .flatten()
        .filter_map(|r| after.unit(r.id).map(|u| u.location))
        .collect()
}

/// Unit stacks at a location grouped by (power, unit type); cargo is folded into its ship.
pub fn stacks_at(st: &GameState, loc: LocationId, picked: &[UnitId]) -> Vec<Stack> {
    let mut m: BTreeMap<(usize, UnitType), Stack> = BTreeMap::new();
    for u in st.units_at(loc).filter(|u| u.loaded_on.is_none()) {
        let e = m.entry((u.owner.index(), u.kind)).or_insert_with(|| Stack {
            power: u.owner,
            kind: ui_kind(u.kind),
            count: 0,
            cargo: vec![],
            picked: 0,
            draggable: false,
        });
        e.count += 1;
        if picked.contains(&u.id) {
            e.picked += 1;
        }
        for c in st.cargo_of(u.id) {
            let k = ui_kind(c.kind);
            match e.cargo.iter_mut().find(|x| x.0 == k) {
                Some(x) => x.1 += 1,
                None => e.cargo.push((k, 1)),
            }
        }
    }
    m.into_values().collect()
}

/// Multi-line hover text listing everything at a location.
pub fn tooltip(st: &GameState, loc: LocationId) -> String {
    let map = MapData::get();
    let l = map.loc(loc);
    let mut head = l.name.clone();
    if l.ipc > 0 {
        head.push_str(&format!(" ({} IPC)", l.ipc));
    }
    if let Some(c) = st.controller(loc) {
        head.push_str(&format!(" - {}", c.name()));
    }
    let mut lines = vec![head];
    for p in wartide_engine::Power::ALL {
        let mut v: Vec<(UnitType, u32)> = vec![];
        let mut cargo: Vec<(UnitType, u32)> = vec![];
        for u in st.units_at(loc).filter(|u| u.owner == p) {
            if u.loaded_on.is_some() {
                cargo.push((u.kind, 1));
            } else {
                v.push((u.kind, 1));
            }
        }
        if v.is_empty() && cargo.is_empty() {
            continue;
        }
        let mut line = format!("{}: {}", p.name(), describe_counts(v));
        if !cargo.is_empty() {
            line.push_str(&format!(" (aboard: {})", describe_counts(cargo)));
        }
        lines.push(line);
    }
    lines.join("\n")
}

/// Build the complete board model for a state plus the interaction overlay.
pub fn board_model(st: &GameState, ov: &Overlay) -> BoardModel {
    let map = MapData::get();
    let mut b = BoardModel::default();
    for l in map.locations() {
        let key = loc_key(l);
        if !map.is_sea(l) {
            if let Some(c) = st.controller(l) {
                b.owners.insert(key.clone(), c);
            }
        }
        let stacks = stacks_at(st, l, &ov.picked);
        if !stacks.is_empty() {
            b.stacks.insert(key.clone(), stacks);
        }
        b.tips.insert(key, tooltip(st, l));
    }
    for &l in &ov.movable {
        b.highlights.insert(loc_key(l), Highlight::Movable);
    }
    for &l in &ov.battles {
        b.highlights.insert(loc_key(l), Highlight::Battle);
    }
    for &l in &ov.placements {
        b.highlights.insert(loc_key(l), Highlight::Place);
    }
    for &(l, battle) in &ov.dests {
        let h = if battle {
            Highlight::Attack
        } else {
            Highlight::Move
        };
        b.highlights.insert(loc_key(l), h);
    }
    b.selected = ov.selected.map(loc_key);
    b.flash = ov.flash.iter().map(|&l| loc_key(l)).collect();
    b
}

#[cfg(test)]
mod tests {
    use super::*;
    use wartide_engine::{GameSettings, Power};

    fn start() -> GameState {
        GameState::new(GameSettings::default())
    }

    #[test]
    fn owners_and_stacks_from_state() {
        let st = start();
        let b = board_model(&st, &Overlay::default());
        let g = LocationId::from_id("germany").unwrap();
        assert_eq!(b.owners.get("germany"), Some(&Power::Germany));
        let stacks = b.stacks.get(&loc_key(g)).expect("units in germany");
        let total: u32 = stacks.iter().map(|s| s.count).sum();
        let real = st.units_at(g).filter(|u| u.loaded_on.is_none()).count() as u32;
        assert_eq!(total, real);
        // grouped: one stack per (power, kind)
        let mut kinds: Vec<_> = stacks.iter().map(|s| (s.power, s.kind)).collect();
        kinds.dedup();
        assert_eq!(kinds.len(), stacks.len());
    }

    #[test]
    fn cargo_is_folded_into_transport() {
        let mut st = start();
        let sz = MapData::get().sea_zones().next().unwrap();
        let t = st.spawn(UnitType::Transport, Power::Germany, sz);
        let i = st.spawn(UnitType::Infantry, Power::Germany, sz);
        st.unit_mut(i).unwrap().loaded_on = Some(t);
        let stacks = stacks_at(&st, sz, &[]);
        let tr = stacks
            .iter()
            .find(|s| s.kind == crate::model::UnitKind::Transport)
            .unwrap();
        assert_eq!(tr.cargo, vec![(crate::model::UnitKind::Infantry, 1)]);
        assert!(!stacks
            .iter()
            .any(|s| s.kind == crate::model::UnitKind::Infantry && s.power == Power::Germany));
        assert!(tooltip(&st, sz).contains("aboard: 1 infantry"));
    }

    #[test]
    fn highlights_priority() {
        let st = start();
        let a = LocationId::from_id("germany").unwrap();
        let ov = Overlay {
            battles: vec![a],
            dests: vec![(a, true)],
            selected: Some(a),
            ..Default::default()
        };
        let b = board_model(&st, &ov);
        assert_eq!(b.highlights.get("germany"), Some(&Highlight::Attack));
        assert_eq!(b.selected.as_deref(), Some("germany"));
    }

    #[test]
    fn event_locations_dedup_and_targets() {
        use wartide_engine::{Event, LocationId, Power as P};
        let a = LocationId::from_id("russia").unwrap();
        let b = LocationId::from_id("karelia_s_s_r").unwrap();
        let evs = vec![
            Event::UnitsMoved {
                power: P::Ussr,
                units: vec![],
                from: a,
                path: vec![b],
            },
            Event::TerritoryCaptured {
                power: P::Ussr,
                location: b,
                from: None,
                liberated: false,
            },
        ];
        assert_eq!(event_locations(&evs), vec![b]);
        assert!(event_locations(&[]).is_empty());
    }
}

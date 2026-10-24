//! Local stand-in for the in-game state, used while `api::USE_MOCK_GAME_STATE` is true
//! (`GameStateView` is an opaque JSON value in the protocol until the engine lands).
//! Owners and starting units come from the engine's real `map.json`.
use crate::components::board::{BoardModel, Stack};
use crate::map::MapInfo;
use crate::model::{LogEntry, Power, Treasury};

/// Board at the start of a game: original owners plus the engine's starting units.
pub fn initial_board(map: &MapInfo) -> BoardModel {
    let mut b = BoardModel::default();
    for (id, i) in &map.data.info {
        if let Some(p) = i.owner {
            b.owners.insert(id.clone(), p);
        }
    }
    for (loc, power, kind, count) in &map.data.starting_units {
        b.stacks.entry(loc.clone()).or_default().push(Stack {
            power: *power,
            kind: *kind,
            count: *count,
        });
    }
    b
}

pub fn treasury() -> Vec<Treasury> {
    Power::ALL
        .iter()
        .zip([24, 32, 30, 25, 40])
        .map(|(p, ipc)| Treasury { power: *p, ipc })
        .collect()
}

pub fn log(round: u32) -> Vec<LogEntry> {
    vec![LogEntry {
        round,
        text: "(mock) Game log will show engine events once the engine is wired in".into(),
    }]
}

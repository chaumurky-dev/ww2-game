//! Income, production, capital capture, liberation, purchasing and placement.
use crate::map::MapData;
use crate::power::Power;
use crate::state::GameState;

/// Summed IPC value of the territories `p` controls, regardless of capital status.
pub fn production_level(state: &GameState, p: Power) -> u32 {
    let map = MapData::get();
    map.territories()
        .filter(|&t| state.control[t] == Some(p))
        .map(|t| map.ipc(t))
        .sum()
}

/// IPC `p` would collect now: its production level, or 0 if its capital is not in its own hands.
pub fn income(state: &GameState, p: Power) -> u32 {
    if state.control[p.capital()] == Some(p) {
        production_level(state, p)
    } else {
        0
    }
}

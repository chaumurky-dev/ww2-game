//! Test helpers and rule-family tests.
#![allow(dead_code)]
use crate::dice::{d6, rng_from_seed};
use crate::*;

mod audit_tests;
mod bombing_tests;
mod combat_tests;
mod economy_tests;
mod flow_tests;
mod fuzz_tests;
mod misc_tests;
mod movement_tests;
mod naval_tests;
mod query_tests;
mod transport_tests;

pub fn loc(id: &str) -> LocationId {
    LocationId::from_id(id).unwrap_or_else(|| panic!("no location {id}"))
}

/// A state with no units, printed control, `power` to act in `phase`.
pub fn blank(power: Power, phase: Phase) -> GameState {
    let mut st = GameState::new(GameSettings::default());
    st.units.clear();
    st.current_power = power;
    st.phase = phase;
    st.turn.number = 5;
    st.turn.start_control = st.control.clone();
    st
}

pub fn put(st: &mut GameState, kind: UnitType, owner: Power, at: &str) -> UnitId {
    let l = loc(at);
    let id = st.spawn(kind, owner, l);
    st.unit_mut(id).unwrap().born_turn = 0;
    id
}

pub fn putn(st: &mut GameState, kind: UnitType, owner: Power, at: &str, n: usize) -> Vec<UnitId> {
    (0..n).map(|_| put(st, kind, owner, at)).collect()
}

/// Apply an action with a fixed seed.
pub fn act(st: &mut GameState, p: Power, a: Action) -> Result<Vec<Event>> {
    apply(st, p, &a, 42)
}

pub fn act_seed(st: &mut GameState, p: Power, a: Action, seed: u64) -> Result<Vec<Event>> {
    apply(st, p, &a, seed)
}

pub fn mv(units: &[UnitId], path: &[&str]) -> Action {
    Action::Move {
        units: units.to_vec(),
        path: path.iter().map(|s| loc(s)).collect(),
    }
}

pub fn set_control(st: &mut GameState, at: &str, p: Option<Power>) {
    let l = loc(at);
    st.control[l] = p;
    st.turn.start_control[l] = p;
}

/// Find a seed whose first dice equal `want`.
pub fn seed_for(want: &[u8]) -> u64 {
    for s in 0..5_000_000u64 {
        let mut r = rng_from_seed(s);
        if want.iter().all(|&w| d6(&mut r) == w) {
            return s;
        }
    }
    panic!("no seed for {want:?}");
}

pub fn count(st: &GameState, at: &str, kind: UnitType) -> usize {
    st.units_at(loc(at)).filter(|u| u.kind == kind).count()
}

/// `x` divisible by `n` (avoids version-dependent clippy lints on `%`).
pub fn divisible(x: u64, n: u64) -> bool {
    x.checked_rem(n) == Some(0)
}

/// End combat movement and check the Combat phase begins.
pub fn go_combat(st: &mut GameState, p: Power) {
    act(st, p, Action::EndPhase).unwrap();
    assert_eq!(st.phase, Phase::Combat);
}

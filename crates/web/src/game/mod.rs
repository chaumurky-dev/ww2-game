//! Interactive game screen driven by the real rules engine (`wartide_engine::GameState`).
pub mod battle;
pub mod battle_board;
pub mod conv;
pub mod ctx;
pub mod dice;
pub mod drag;
#[cfg(test)]
mod fuzz_tests;
pub mod live;
pub mod log;
#[cfg(test)]
mod log_tests;
mod logfmt_battle;
pub mod model;
pub mod moves;
pub mod panels;
pub mod replay;
pub mod sandbox;
pub mod screen;
pub mod topbar;

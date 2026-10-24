//! Wartide rules engine: a pure, deterministic implementation of the mechanics.
#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub mod action;
pub mod apply;
pub mod battle;
pub mod bombing;
pub mod casualties;
pub mod choices;
pub mod combat;
pub mod decision;
pub mod dice;
pub mod economy;
pub mod error;
pub mod event;
pub mod map;
pub mod movement;
pub mod odds;
pub mod power;
pub mod query;
pub mod random_policy;
pub mod setup;
pub mod state;
pub mod tech;
pub mod transport;
pub mod units;
pub mod victory;

pub use action::Action;
pub use apply::{apply, replay};
pub use decision::{pending_decision, Decision};
pub use economy::{income, production_level};
pub use error::{Result, RuleError};
pub use event::Event;
pub use map::{LocationId, MapData};
pub use power::{PerPower, Power, Team};
pub use state::{GameSettings, GameState, Phase};
pub use tech::Tech;
pub use units::{Unit, UnitId, UnitType};

#[cfg(test)]
mod tests;

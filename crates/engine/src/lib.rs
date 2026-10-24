//! Wartide rules engine: a pure, deterministic implementation of the mechanics.
#![forbid(unsafe_code)]

pub mod action;
pub mod economy;
pub mod error;
pub mod event;
pub mod map;
pub mod power;
pub mod setup;
pub mod state;
pub mod tech;
pub mod units;

pub use action::Action;
pub use economy::{income, production_level};
pub use error::{Result, RuleError};
pub use event::Event;
pub use map::{LocationId, MapData};
pub use power::{PerPower, Power, Team};
pub use state::{GameSettings, GameState, Phase};
pub use tech::Tech;
pub use units::{Unit, UnitId, UnitType};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn printed_setup_incomes() {
        let st = GameState::new(GameSettings::default());
        let inc: Vec<u32> = Power::ALL.iter().map(|&p| income(&st, p)).collect();
        assert_eq!(inc, vec![24, 32, 30, 25, 36]);
        let tr: Vec<u32> = Power::ALL.iter().map(|&p| st.treasury[p]).collect();
        assert_eq!(tr, vec![24, 32, 30, 25, 36]);
        assert_eq!(st.current_power, Power::Ussr);
        assert_eq!(st.phase, Phase::DevelopWeapons);
        assert_eq!(st.round, 1);
    }
}

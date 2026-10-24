//! Computer player for Wartide.
//!
//! The server drives a bot by calling [`Bot::next_action`] repeatedly while the bot's power is the
//! one that must act (see `wartide_engine::pending_decision`). Each call returns the next action and
//! zero or more chat lines the bot wants to say to its team.
//!
//! The public API below is a stable contract used by the server; the planner behind it is free to
//! change. This initial version wraps the engine's random policy; the real heuristic planner
//! replaces it in `planner.rs`.

use serde::{Deserialize, Serialize};
use wartide_engine::{Action, GameState, Power};

pub mod battle;
pub mod combat;
pub mod commentary;
pub mod economy;
pub mod eval;
pub mod garrison;
pub mod movement;
pub mod placement;
pub mod planner;
pub mod selfplay;

/// How a bot plays and talks. Persona ids match the server's persona catalog.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct Persona {
    pub id: String,
    pub name: String,
    pub style: Style,
    pub verbosity: Verbosity,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Style {
    Aggressive,
    Balanced,
    Cautious,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Verbosity {
    Chatty,
    Normal,
    Quiet,
}

impl Default for Persona {
    fn default() -> Self {
        Persona {
            id: "balanced".into(),
            name: "Staff Officer".into(),
            style: Style::Balanced,
            verbosity: Verbosity::Normal,
        }
    }
}

/// A line of team chat the bot wants to post, in order.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Commentary {
    pub text: String,
}

/// One computer player controlling one power.
pub struct Bot {
    persona: Persona,
    power: Power,
    seed: u64,
    fallback: wartide_engine::random_policy::RandomPolicy,
    mem: planner::Memory,
}

impl Bot {
    /// `seed` makes the bot deterministic for a given game (server passes `hash(game_seed, power)`).
    pub fn new(persona: Persona, power: Power, seed: u64) -> Self {
        Bot {
            persona,
            power,
            seed,
            fallback: wartide_engine::random_policy::RandomPolicy::new(seed),
            mem: planner::Memory::default(),
        }
    }

    pub fn persona(&self) -> &Persona {
        &self.persona
    }

    pub fn power(&self) -> Power {
        self.power
    }

    pub fn seed(&self) -> u64 {
        self.seed
    }

    /// Returns the next action for this bot's power, or `None` if it is not this power's decision.
    /// The server applies the action with `wartide_engine::apply` and posts the commentary to the
    /// bot's team chat.
    pub fn next_action(&mut self, state: &GameState) -> Option<(Action, Vec<Commentary>)> {
        self.decide(state)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wartide_engine::GameSettings;

    #[test]
    fn bot_only_acts_for_its_own_power() {
        let state = GameState::new(GameSettings::default());
        let mut germany = Bot::new(Persona::default(), Power::Germany, 1);
        assert!(germany.next_action(&state).is_none());
        let mut ussr = Bot::new(Persona::default(), Power::Ussr, 1);
        assert!(ussr.next_action(&state).is_some());
    }
}

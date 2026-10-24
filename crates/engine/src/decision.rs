//! Who must act next and what kind of input is needed.
use crate::combat::battle_prompt;
use crate::map::LocationId;
use crate::power::{Power, Team};
use crate::state::{GameState, Phase};
use crate::units::UnitId;
use serde::{Deserialize, Serialize};

/// The pending input of the game.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Decision {
    /// The power must act in this phase (any phase action or `EndPhase`).
    Phase {
        /// Acting power.
        power: Power,
        /// Phase.
        phase: Phase,
    },
    /// Hits must be assigned.
    SelectCasualties {
        /// Who chooses.
        power: Power,
        /// Battle site.
        location: LocationId,
        /// Hits to assign.
        hits: u32,
        /// Hits among them that submarines scored (cannot hit aircraft).
        sub_hits: u32,
        /// Units that may be chosen.
        eligible: Vec<UnitId>,
    },
    /// The attacker chooses to retreat, press on, or withdraw subs.
    RetreatOrPress {
        /// Attacking power.
        power: Power,
        /// Battle site.
        location: LocationId,
    },
    /// A defending power may withdraw submarines.
    SubWithdrawal {
        /// Defending power.
        power: Power,
        /// Battle site.
        location: LocationId,
        /// Submarines that may leave.
        subs: Vec<UnitId>,
    },
    /// The game is over.
    GameOver {
        /// Winning team.
        winner: Option<Team>,
    },
}

/// What the game is waiting for.
pub fn pending_decision(state: &GameState) -> Decision {
    if let Some(w) = state.winner {
        return Decision::GameOver {
            winner: Some(w.team),
        };
    }
    if let Some(d) = battle_prompt(state) {
        return d;
    }
    Decision::Phase {
        power: state.current_power,
        phase: state.phase,
    }
}

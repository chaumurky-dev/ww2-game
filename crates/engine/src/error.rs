//! Rule violations returned by [`crate::apply`].
use crate::map::LocationId;
use crate::power::Power;
use crate::state::Phase;
use serde::{Deserialize, Serialize};

/// Result alias used across the engine.
pub type Result<T> = std::result::Result<T, RuleError>;

/// Why an action was rejected. The state is never modified when this is returned.
#[derive(Debug, Clone, PartialEq, thiserror::Error, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RuleError {
    /// The game already has a winner.
    #[error("the game is over")]
    GameOver,
    /// Another power must act now.
    #[error("it is not {actor:?}'s decision; {expected:?} must act")]
    NotYourTurn {
        /// Who tried to act.
        actor: Power,
        /// Who must act.
        expected: Power,
    },
    /// The action is not valid in the current phase / battle step.
    #[error("action not allowed now (phase {phase:?}): {reason}")]
    WrongPhase {
        /// Current phase.
        phase: Phase,
        /// Short explanation.
        reason: String,
    },
    /// Unknown unit id.
    #[error("unknown unit {0}")]
    UnknownUnit(u32),
    /// A unit that does not belong to the actor.
    #[error("unit {0} is not yours")]
    NotOwner(u32),
    /// Not enough IPC.
    #[error("insufficient funds: need {need}, have {have}")]
    InsufficientFunds {
        /// Required IPC.
        need: u32,
        /// Available IPC.
        have: u32,
    },
    /// Location is not valid for the request.
    #[error("illegal location {0:?}")]
    BadLocation(LocationId),
    /// Any other illegal request.
    #[error("illegal: {0}")]
    Illegal(String),
}

impl RuleError {
    /// Convenience constructor for [`RuleError::Illegal`].
    pub fn illegal(msg: impl Into<String>) -> Self {
        RuleError::Illegal(msg.into())
    }
}

/// Shorthand: `Err(RuleError::illegal(..))`.
pub fn illegal<T>(msg: impl Into<String>) -> Result<T> {
    Err(RuleError::illegal(msg))
}

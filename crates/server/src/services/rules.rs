//! Engine integration seam.
//!
//! TODO(engine): replace [`PassthroughRules`] with an implementation backed by `wartide-engine`
//! (`engine::apply`, seeded RNG from `seed`). The server only treats state/action/events as
//! opaque JSON; everything else (turn order, win detection) is derived from `events`.
use anyhow::Result;
use serde_json::{json, Value};
use wartide_protocol::game::{GameMode, GameSettings};

pub trait Rules: Send + Sync + 'static {
    /// Initial state for a new game.
    fn initial_state(&self, mode: GameMode, settings: &GameSettings, seed: i64) -> Value;

    /// Apply `action` to `state`. `seed` is derived deterministically from the game seed and
    /// action seq so that replays are reproducible.
    fn apply(&self, state: &Value, action: &Value, seed: u64) -> Result<(Value, Value)>;
}

/// Accepts every action, records it, and changes nothing.
/// Convention understood by the server until the engine lands:
/// - `{"type":"end_turn"}` advances to the next power.
/// - an event `{"type":"game_over","winner":"axis"|"allies"}` finishes the game.
pub struct PassthroughRules;

impl Rules for PassthroughRules {
    fn initial_state(&self, mode: GameMode, settings: &GameSettings, seed: i64) -> Value {
        json!({ "placeholder": true, "mode": mode, "settings": settings, "seed": seed })
    }

    fn apply(&self, state: &Value, _action: &Value, _seed: u64) -> Result<(Value, Value)> {
        Ok((state.clone(), json!([])))
    }
}

/// Derive a per-action RNG seed.
pub fn action_seed(game_seed: i64, seq: i64) -> u64 {
    (game_seed as u64)
        .wrapping_mul(6364136223846793005)
        .wrapping_add(seq as u64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn passthrough_keeps_state() {
        let r = PassthroughRules;
        let st = r.initial_state(GameMode::TwoPlayer, &GameSettings::default(), 1);
        let (new, ev) = r.apply(&st, &json!({"type":"noop"}), 7).unwrap();
        assert_eq!(new, st);
        assert_eq!(ev, json!([]));
    }

    #[test]
    fn seeds_differ_per_seq() {
        assert_ne!(action_seed(5, 1), action_seed(5, 2));
        assert_eq!(action_seed(5, 1), action_seed(5, 1));
    }
}

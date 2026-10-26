//! Engine integration: settings mapping, seed derivation and applying actions. All game rules
//! live in `wartide-engine`; the server stores its `GameState` / `Action` / `Event` as JSON.
use engine::{Power, Team};
use serde_json::Value;
use wartide_engine as engine;
use wartide_protocol::game::GameSettings;

use crate::error::{AppError, AppResult};

/// The engine-backed rules implementation.
#[derive(Clone, Copy, Debug, Default)]
pub struct EngineRules;

/// What the engine is waiting for, in protocol terms.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pending {
    /// Power that must act (None once the game is over).
    pub power: Option<Power>,
    /// Winner once the game is over.
    pub game_over: Option<Option<Team>>,
    /// The decision is a casualty/withdrawal choice for a defender (not the power whose turn it is).
    pub defender: bool,
}

/// Map the stored/requested settings onto the engine's, with the per-game seed.
pub fn engine_settings(s: &GameSettings, seed: i64) -> engine::GameSettings {
    engine::GameSettings {
        weapons_development: s.weapons_development,
        axis_economic_victory: s.axis_economic_victory,
        allied_economic_victory: s.allied_economic_victory,
        soviet_first_turn_restriction: s.soviet_first_turn_restriction,
        original_complexes_only: s.original_complexes_only,
        total_victory: s.total_victory,
        germany_jet_power: s.germany_jet_power,
        japan_super_subs: s.japan_super_subs,
        seed: seed as u64,
    }
}

pub fn pending_of(state: &engine::GameState) -> Pending {
    use engine::Decision::*;
    let decision = engine::pending_decision(state);
    match decision {
        Phase { power, .. } | RetreatOrPress { power, .. } => Pending {
            power: Some(power),
            game_over: None,
            defender: false,
        },
        SelectCasualties { power, .. } | SubWithdrawal { power, .. } => Pending {
            power: Some(power),
            game_over: None,
            defender: power != state.current_power,
        },
        GameOver { winner } => Pending {
            power: None,
            game_over: Some(winner),
            defender: false,
        },
    }
}

/// Phase as the snake_case string the engine serialises.
pub fn phase_str(state: &engine::GameState) -> String {
    match serde_json::to_value(state.phase) {
        Ok(Value::String(s)) => s,
        _ => String::new(),
    }
}

impl EngineRules {
    /// Fresh game at the printed setup.
    pub fn initial_state(&self, settings: &GameSettings, seed: i64) -> engine::GameState {
        engine::GameState::new(engine_settings(settings, seed))
    }
}

/// Apply with the derived seed; the state is untouched on error.
pub fn apply_typed(
    gs: &mut engine::GameState,
    power: Power,
    action: &engine::Action,
    game_seed: i64,
    seq: i64,
) -> AppResult<Vec<engine::Event>> {
    engine::apply(
        gs,
        power,
        action,
        wartide_protocol::seed::action_seed(game_seed as u64, seq),
    )
    .map_err(|e| AppError::BadRequest("illegal_action", e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_map_to_engine() {
        let s = GameSettings {
            axis_economic_victory: None,
            allied_economic_victory: Some(100),
            total_victory: true,
            ..GameSettings::default()
        };
        let e = engine_settings(&s, 9);
        assert_eq!(e.axis_economic_victory, None);
        assert_eq!(e.allied_economic_victory, Some(100));
        assert!(e.total_victory && e.weapons_development);
        assert_eq!(e.seed, 9);
    }

    #[test]
    fn apply_typed_roundtrip_and_errors() {
        let r = EngineRules;
        let mut gs = r.initial_state(&GameSettings::default(), 3);
        let power = pending_of(&gs).power.unwrap();
        let ev = apply_typed(&mut gs, power, &engine::Action::EndPhase, 3, 1).unwrap();
        assert_eq!(phase_str(&gs), "purchase");
        assert!(!ev.is_empty());
        let before = gs.clone();
        let bad = engine::Action::Move {
            units: vec![99999],
            path: vec![engine::LocationId::from_id("germany").unwrap()],
        };
        let err = apply_typed(&mut gs, power, &bad, 3, 2).unwrap_err();
        assert_eq!(err.code(), "illegal_action");
        assert_eq!(gs, before);
    }
}

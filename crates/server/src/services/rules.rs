//! Engine integration: settings mapping, seed derivation, (de)serialisation of states and the
//! conversion between protocol and engine enums. All game rules live in `wartide-engine`; the
//! server only stores the engine's `GameState` / `Action` / `Event` as JSON.
use serde_json::Value;
use wartide_engine as engine;
use wartide_protocol::game::{GameSettings, Power, Team};

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

pub fn power_from_engine(p: engine::Power) -> Power {
    // Both enums share the same snake_case ids.
    Power::parse(p.id()).expect("power ids match")
}

pub fn power_to_engine(p: Power) -> engine::Power {
    engine::Power::from_id(p.as_str()).expect("power ids match")
}

pub fn team_from_engine(t: engine::Team) -> Team {
    match t {
        engine::Team::Axis => Team::Axis,
        engine::Team::Allies => Team::Allies,
    }
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

/// FNV-1a (64 bit) over `bytes`, finished with a splitmix64 avalanche so that nearby inputs
/// (consecutive seqs) give unrelated seeds. Stable forever: stored games replay from it.
fn stable_hash(domain: &[u8], parts: &[u64]) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    let mut feed = |bytes: &[u8]| {
        for b in bytes {
            h ^= u64::from(*b);
            h = h.wrapping_mul(0x100000001b3);
        }
    };
    feed(domain);
    for p in parts {
        feed(&p.to_le_bytes());
    }
    let mut z = h.wrapping_add(0x9e3779b97f4a7c15);
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d049bb133111eb);
    z ^ (z >> 31)
}

/// RNG seed for action number `seq` of a game: `FNV1a("wartide/action" ++ le(game_seed) ++ le(seq))`
/// run through a splitmix64 finaliser. Replays use the same function.
pub fn action_seed(game_seed: i64, seq: i64) -> u64 {
    stable_hash(b"wartide/action", &[game_seed as u64, seq as u64])
}

/// Seed of the computer player for `power` in a game.
pub fn bot_seed(game_seed: i64, power: Power) -> u64 {
    stable_hash(b"wartide/bot", &[game_seed as u64, power as u64])
}

/// Seed of the random-policy fallback for one decision (`seq` = last applied action) and attempt.
pub fn fallback_seed(game_seed: i64, seq: i64, attempt: u32) -> u64 {
    stable_hash(
        b"wartide/fallback",
        &[game_seed as u64, seq as u64, u64::from(attempt)],
    )
}

pub fn parse_state(v: &Value) -> AppResult<engine::GameState> {
    serde_json::from_value(v.clone())
        .map_err(|e| AppError::Internal(anyhow::anyhow!("stored game state is invalid: {e}")))
}

pub fn pending_of(state: &engine::GameState) -> Pending {
    use engine::Decision::*;
    let decision = engine::pending_decision(state);
    match decision {
        Phase { power, .. } | RetreatOrPress { power, .. } => Pending {
            power: Some(power_from_engine(power)),
            game_over: None,
            defender: false,
        },
        SelectCasualties { power, .. } | SubWithdrawal { power, .. } => Pending {
            power: Some(power_from_engine(power)),
            game_over: None,
            defender: power != state.current_power,
        },
        GameOver { winner } => Pending {
            power: None,
            game_over: Some(winner.map(team_from_engine)),
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

    /// Apply a JSON action to a JSON state on behalf of the power the engine is waiting for.
    /// Returns the new state and the events as JSON. Engine errors become `illegal_action`.
    pub fn apply_json(
        &self,
        state: &Value,
        action: &Value,
        game_seed: i64,
        seq: i64,
    ) -> AppResult<(Value, Value)> {
        let mut gs = parse_state(state)?;
        let action = parse_action(action)?;
        let power = pending_of(&gs)
            .power
            .ok_or_else(|| AppError::conflict("not_in_progress", "game is over"))?;
        let events = apply_typed(&mut gs, power, &action, game_seed, seq)?;
        Ok((to_json(&gs)?, to_json(&events)?))
    }
}

pub fn parse_action(v: &Value) -> AppResult<engine::Action> {
    serde_json::from_value(v.clone())
        .map_err(|e| AppError::BadRequest("illegal_action", format!("invalid action: {e}")))
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
        power_to_engine(power),
        action,
        action_seed(game_seed, seq),
    )
    .map_err(|e| AppError::BadRequest("illegal_action", e.to_string()))
}

pub fn to_json<T: serde::Serialize>(t: &T) -> AppResult<Value> {
    serde_json::to_value(t).map_err(|e| AppError::Internal(e.into()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn seeds_are_stable_and_distinct() {
        // pinned values: changing the hash would break replays of stored games
        assert_eq!(action_seed(5, 1), action_seed(5, 1));
        assert_ne!(action_seed(5, 1), action_seed(5, 2));
        assert_ne!(action_seed(5, 1), action_seed(6, 1));
        assert_ne!(action_seed(-1, 1), action_seed(1, 1));
        assert_ne!(bot_seed(5, Power::Ussr), bot_seed(5, Power::Germany));
        assert_ne!(action_seed(5, 1), bot_seed(5, Power::Germany));
        assert_ne!(fallback_seed(5, 1, 0), fallback_seed(5, 1, 1));
    }

    #[test]
    fn seed_value_is_pinned() {
        let v = action_seed(42, 7);
        assert_eq!(v, action_seed(42, 7));
        // computed with an independent implementation; guards against accidental changes
        assert_eq!(v, 4_232_985_104_476_163_571);
    }

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
    fn apply_json_roundtrip_and_errors() {
        let r = EngineRules;
        let gs = r.initial_state(&GameSettings::default(), 3);
        let st = to_json(&gs).unwrap();
        let (new, ev) = r
            .apply_json(&st, &json!({"type":"end_phase"}), 3, 1)
            .unwrap();
        assert_eq!(new["phase"], "purchase");
        assert!(ev.is_array());
        let err = r
            .apply_json(&st, &json!({"type":"nonsense"}), 3, 1)
            .unwrap_err();
        assert_eq!(err.code(), "illegal_action");
        let err = r
            .apply_json(
                &st,
                &json!({"type":"move","units":[99999],"path":["germany"]}),
                3,
                1,
            )
            .unwrap_err();
        assert_eq!(err.code(), "illegal_action");
    }

    #[test]
    fn power_conversions() {
        for p in Power::ALL {
            assert_eq!(power_from_engine(power_to_engine(p)), p);
        }
    }
}

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::game::GameSummary;

/// Defender preferences the server applies automatically (re-exported from the engine).
pub use wartide_engine::casualty_policy::{CasualtyPolicy, SubWithdrawalPolicy};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LadderStats {
    pub ladder: String,
    pub rating: f64,
    pub rating_deviation: f64,
    pub games_played: i32,
    pub wins: i32,
    pub losses: i32,
    pub rank_position: Option<i64>,
    /// Consecutive wins in this ladder, counting back from the most recent finished game.
    #[serde(default)]
    pub current_win_streak: i32,
    #[serde(default)]
    pub best_win_streak: i32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PublicProfile {
    pub id: Uuid,
    pub username: String,
    pub display_name: String,
    pub avatar: Option<String>,
    pub bio: Option<String>,
    pub country: Option<String>,
    pub timezone: Option<String>,
    pub created_at: DateTime<Utc>,
    pub stats: Vec<LadderStats>,
    /// Last 10 finished or active public/group games. Group games are redacted (no seats/host).
    #[serde(default)]
    pub recent_games: Vec<GameSummary>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UpdateProfileRequest {
    pub display_name: Option<String>,
    pub avatar: Option<String>,
    pub bio: Option<String>,
    pub country: Option<String>,
    pub timezone: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UserSettings {
    pub email_turn_notifications: bool,
    #[serde(default = "default_true")]
    pub email_match_found: bool,
    /// How the server picks your casualties when you defend.
    #[serde(default)]
    pub casualty_policy: CasualtyPolicy,
    /// Whether the server pulls your submarines out of a battle when you defend.
    #[serde(default)]
    pub sub_withdrawal: SubWithdrawalPolicy,
}

fn default_true() -> bool {
    true
}

impl Default for UserSettings {
    fn default() -> Self {
        Self {
            email_turn_notifications: true,
            email_match_found: true,
            casualty_policy: CasualtyPolicy::default(),
            sub_withdrawal: SubWithdrawalPolicy::default(),
        }
    }
}

/// Partial update of [`UserSettings`].
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UpdateSettingsRequest {
    pub email_turn_notifications: Option<bool>,
    #[serde(default)]
    pub email_match_found: Option<bool>,
    #[serde(default)]
    pub casualty_policy: Option<CasualtyPolicy>,
    #[serde(default)]
    pub sub_withdrawal: Option<SubWithdrawalPolicy>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_roundtrip_and_old_json_defaults() {
        let s = UserSettings {
            email_turn_notifications: false,
            email_match_found: true,
            casualty_policy: CasualtyPolicy::ProtectAircraft,
            sub_withdrawal: SubWithdrawalPolicy::Always,
        };
        let back: UserSettings = serde_json::from_str(&serde_json::to_string(&s).unwrap()).unwrap();
        assert_eq!(back, s);
        let old: UserSettings =
            serde_json::from_str(r#"{"email_turn_notifications":true}"#).unwrap();
        assert_eq!(old.casualty_policy, CasualtyPolicy::CheapestFirst);
        assert_eq!(old.sub_withdrawal, SubWithdrawalPolicy::WhenOutgunned);
        let req: UpdateSettingsRequest =
            serde_json::from_str(r#"{"casualty_policy":"keep_counterattack"}"#).unwrap();
        assert_eq!(req.casualty_policy, Some(CasualtyPolicy::KeepCounterattack));
        assert!(
            serde_json::from_str::<UpdateSettingsRequest>(r#"{"casualty_policy":"x"}"#).is_err()
        );
    }
}

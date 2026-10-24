use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LadderStats {
    pub ladder: String,
    pub rating: f64,
    pub rating_deviation: f64,
    pub games_played: i32,
    pub wins: i32,
    pub losses: i32,
    pub rank_position: Option<i64>,
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
}

impl Default for UserSettings {
    fn default() -> Self {
        Self {
            email_turn_notifications: true,
        }
    }
}

/// Partial update of [`UserSettings`].
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UpdateSettingsRequest {
    pub email_turn_notifications: Option<bool>,
}

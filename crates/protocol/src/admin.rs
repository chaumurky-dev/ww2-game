use serde::{Deserialize, Serialize};

/// `GET /api/admin/stats`
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdminStats {
    pub users: i64,
    pub verified_users: i64,
    pub games_lobby: i64,
    pub games_in_progress: i64,
    pub games_finished: i64,
    pub games_abandoned: i64,
    pub queue_two_player: i64,
    pub queue_five_player: i64,
}

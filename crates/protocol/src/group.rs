use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::game::UserRef;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Group {
    pub id: Uuid,
    pub name: String,
    pub slug: String,
    pub description: Option<String>,
    pub member_count: i64,
    /// Only present for owners/admins.
    pub invite_code: Option<String>,
    /// The caller's role, if a member.
    pub my_role: Option<GroupRole>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GroupRole {
    Owner,
    Admin,
    Member,
}

impl GroupRole {
    pub fn can_manage(self) -> bool {
        matches!(self, GroupRole::Owner | GroupRole::Admin)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GroupMember {
    pub user: UserRef,
    pub role: GroupRole,
    pub joined_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateGroupRequest {
    pub name: String,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JoinGroupRequest {
    pub invite_code: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetMemberRoleRequest {
    pub role: GroupRole,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LeaderboardEntry {
    pub rank: i64,
    pub user: UserRef,
    pub rating: f64,
    pub rating_deviation: f64,
    pub games_played: i32,
    pub wins: i32,
    pub losses: i32,
}

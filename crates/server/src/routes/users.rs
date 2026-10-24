use axum::extract::{Path, Query, State};
use axum::Json;
use serde::Deserialize;
use wartide_protocol::game::GameMode;
use wartide_protocol::group::LeaderboardEntry;
use wartide_protocol::user::*;

use crate::error::AppResult;
use crate::extract::AuthUser;
use crate::services::{groups, ratings, users};
use crate::state::AppState;

pub async fn profile(
    State(s): State<AppState>,
    Path(username): Path<String>,
) -> AppResult<Json<PublicProfile>> {
    Ok(Json(users::public_profile(&s, &username).await?))
}

pub async fn update_profile(
    State(s): State<AppState>,
    AuthUser(u): AuthUser,
    Json(req): Json<UpdateProfileRequest>,
) -> AppResult<Json<PublicProfile>> {
    users::update_profile(&s, u.id, req).await?;
    Ok(Json(users::public_profile(&s, &u.username).await?))
}

pub async fn get_settings(
    State(s): State<AppState>,
    AuthUser(u): AuthUser,
) -> AppResult<Json<UserSettings>> {
    Ok(Json(users::get_settings(&s, u.id).await?))
}

pub async fn update_settings(
    State(s): State<AppState>,
    AuthUser(u): AuthUser,
    Json(req): Json<UpdateSettingsRequest>,
) -> AppResult<Json<UserSettings>> {
    Ok(Json(users::update_settings(&s, u.id, req).await?))
}

#[derive(Deserialize)]
pub struct ModeQuery {
    pub mode: Option<GameMode>,
}

/// Public ladder (`mode` defaults to two_player).
pub async fn public_leaderboard(
    State(s): State<AppState>,
    Query(q): Query<ModeQuery>,
) -> AppResult<Json<Vec<LeaderboardEntry>>> {
    let two = q.mode.unwrap_or(GameMode::TwoPlayer) == GameMode::TwoPlayer;
    Ok(Json(
        groups::leaderboard_for_ladder(&s, &ratings::ladder_for(None, two), 100).await?,
    ))
}

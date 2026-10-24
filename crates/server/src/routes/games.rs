use std::time::Duration;

use axum::extract::{Path, Query, State};
use axum::Json;
use serde::Deserialize;
use uuid::Uuid;
use wartide_protocol::game::*;

use crate::error::AppResult;
use crate::extract::{AuthUser, VerifiedUser};
use crate::services::games as svc;
use crate::state::AppState;

pub async fn create(
    State(s): State<AppState>,
    VerifiedUser(u): VerifiedUser,
    Json(mut req): Json<CreatePrivateGameRequest>,
) -> AppResult<Json<GameSummary>> {
    if req.settings.turn_timeout_secs == 0 {
        req.settings.turn_timeout_secs = s.config.default_turn_timeout_secs;
    }
    s.limiter.check(
        &format!("game_create:{}", u.id),
        10,
        Duration::from_secs(3600),
    )?;
    Ok(Json(svc::create_private(&s, &u, req).await?))
}

pub async fn join(
    State(s): State<AppState>,
    VerifiedUser(u): VerifiedUser,
    Json(req): Json<JoinByInviteRequest>,
) -> AppResult<Json<GameSummary>> {
    Ok(Json(svc::join_by_invite(&s, &u, &req.code).await?))
}

#[derive(Deserialize)]
pub struct ListQuery {
    /// `active` (default) or `finished`
    pub status: Option<String>,
}

pub async fn list_mine(
    State(s): State<AppState>,
    AuthUser(u): AuthUser,
    Query(q): Query<ListQuery>,
) -> AppResult<Json<Vec<GameSummary>>> {
    let active = q.status.as_deref() != Some("finished");
    Ok(Json(svc::list_mine(&s, u.id, active).await?))
}

pub async fn get(
    State(s): State<AppState>,
    AuthUser(u): AuthUser,
    Path(id): Path<Uuid>,
) -> AppResult<Json<GameSummary>> {
    Ok(Json(svc::summary_for_viewer(&s, id, u.id).await?))
}

pub async fn state(
    State(s): State<AppState>,
    AuthUser(u): AuthUser,
    Path(id): Path<Uuid>,
) -> AppResult<Json<GameStateResponse>> {
    Ok(Json(svc::state_view(&s, u.id, id).await?))
}

#[derive(Deserialize)]
pub struct SinceQuery {
    #[serde(default)]
    pub since: i64,
}

pub async fn actions(
    State(s): State<AppState>,
    AuthUser(u): AuthUser,
    Path(id): Path<Uuid>,
    Query(q): Query<SinceQuery>,
) -> AppResult<Json<Vec<GameActionEnvelope>>> {
    Ok(Json(svc::actions_since(&s, u.id, id, q.since).await?))
}

pub async fn submit_action(
    State(s): State<AppState>,
    VerifiedUser(u): VerifiedUser,
    Path(id): Path<Uuid>,
    Json(req): Json<SubmitActionRequest>,
) -> AppResult<Json<GameActionEnvelope>> {
    Ok(Json(svc::submit_action(&s, &u, id, req.action).await?))
}

pub async fn start(
    State(s): State<AppState>,
    VerifiedUser(u): VerifiedUser,
    Path(id): Path<Uuid>,
) -> AppResult<Json<GameSummary>> {
    Ok(Json(svc::start(&s, &u, id).await?))
}

pub async fn resign(
    State(s): State<AppState>,
    VerifiedUser(u): VerifiedUser,
    Path(id): Path<Uuid>,
) -> AppResult<Json<GameSummary>> {
    Ok(Json(svc::resign(&s, &u, id).await?))
}

pub async fn abandon(
    State(s): State<AppState>,
    VerifiedUser(u): VerifiedUser,
    Path(id): Path<Uuid>,
) -> AppResult<Json<GameSummary>> {
    Ok(Json(svc::abandon(&s, &u, id).await?))
}

#[derive(Deserialize)]
pub struct PublicQuery {
    /// `active` (default) or `finished`
    pub status: Option<String>,
    pub page: Option<i64>,
}

const PUBLIC_PER_PAGE: i64 = 20;

/// Public (matchmade) games to watch, newest first.
pub async fn list_public(
    State(s): State<AppState>,
    AuthUser(u): AuthUser,
    Query(q): Query<PublicQuery>,
) -> AppResult<Json<wartide_protocol::Paginated<GameSummary>>> {
    let finished = q.status.as_deref() == Some("finished");
    Ok(Json(
        svc::list_public(&s, u.id, finished, q.page.unwrap_or(1), PUBLIC_PER_PAGE).await?,
    ))
}

use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use serde::Serialize;
use wartide_protocol::game::*;

use crate::error::AppResult;
use crate::extract::{AuthUser, VerifiedUser};
use crate::services::matchmaking as svc;
use crate::state::AppState;

pub async fn enqueue(
    State(s): State<AppState>,
    VerifiedUser(u): VerifiedUser,
    Json(req): Json<MatchmakingRequest>,
) -> AppResult<StatusCode> {
    svc::enqueue(&s, u.id, req.mode).await?;
    Ok(StatusCode::ACCEPTED)
}

pub async fn dequeue(State(s): State<AppState>, AuthUser(u): AuthUser) -> AppResult<StatusCode> {
    svc::dequeue(&s, u.id).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Serialize)]
pub struct QueueStatus {
    pub queued: bool,
    pub mode: Option<GameMode>,
}

pub async fn status(
    State(s): State<AppState>,
    AuthUser(u): AuthUser,
) -> AppResult<Json<QueueStatus>> {
    let mode = svc::status(&s, u.id).await?;
    Ok(Json(QueueStatus {
        queued: mode.is_some(),
        mode,
    }))
}

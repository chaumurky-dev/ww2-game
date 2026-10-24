use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::Json;
use serde::Deserialize;
use uuid::Uuid;
use wartide_protocol::chat::*;

use crate::error::AppResult;
use crate::extract::AuthUser;
use crate::services::chat as svc;
use crate::state::AppState;

#[derive(Deserialize)]
pub struct ChannelQuery {
    pub channel: Option<ChatChannel>,
}

pub async fn history(
    State(s): State<AppState>,
    AuthUser(u): AuthUser,
    Path(id): Path<Uuid>,
    Query(q): Query<ChannelQuery>,
) -> AppResult<Json<Vec<ChatMessage>>> {
    Ok(Json(svc::history(&s, u.id, id, q.channel).await?))
}

pub async fn send(
    State(s): State<AppState>,
    AuthUser(u): AuthUser,
    Path(id): Path<Uuid>,
    Json(req): Json<SendChatRequest>,
) -> AppResult<(StatusCode, Json<ChatMessage>)> {
    Ok((StatusCode::CREATED, Json(svc::send(&s, &u, id, req).await?)))
}

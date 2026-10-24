use axum::extract::{Path, State};
use axum::Json;
use uuid::Uuid;
use wartide_protocol::admin::AdminStats;
use wartide_protocol::game::GameSummary;

use crate::error::AppResult;
use crate::extract::AdminUser;
use crate::services::admin as svc;
use crate::state::AppState;

pub async fn stats(State(s): State<AppState>, _admin: AdminUser) -> AppResult<Json<AdminStats>> {
    Ok(Json(svc::stats(&s).await?))
}

pub async fn abandon_game(
    State(s): State<AppState>,
    AdminUser(a): AdminUser,
    Path(id): Path<Uuid>,
) -> AppResult<Json<GameSummary>> {
    Ok(Json(svc::abandon_game(&s, a.id, id).await?))
}

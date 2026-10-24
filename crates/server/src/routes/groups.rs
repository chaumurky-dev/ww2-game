use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::Json;
use uuid::Uuid;
use wartide_protocol::game::GameMode;
use wartide_protocol::group::*;

use crate::error::AppResult;
use crate::extract::AuthUser;
use crate::routes::users::ModeQuery;
use crate::services::groups as svc;
use crate::state::AppState;

pub async fn create(
    State(s): State<AppState>,
    AuthUser(u): AuthUser,
    Json(req): Json<CreateGroupRequest>,
) -> AppResult<(StatusCode, Json<Group>)> {
    Ok((StatusCode::CREATED, Json(svc::create(&s, &u, req).await?)))
}

pub async fn list_mine(
    State(s): State<AppState>,
    AuthUser(u): AuthUser,
) -> AppResult<Json<Vec<Group>>> {
    Ok(Json(svc::list_mine(&s, u.id).await?))
}

pub async fn get(
    State(s): State<AppState>,
    AuthUser(u): AuthUser,
    Path(id): Path<Uuid>,
) -> AppResult<Json<Group>> {
    Ok(Json(svc::get(&s, u.id, id).await?))
}

pub async fn by_slug(
    State(s): State<AppState>,
    AuthUser(u): AuthUser,
    Path(slug): Path<String>,
) -> AppResult<Json<Group>> {
    Ok(Json(svc::get_by_slug(&s, u.id, &slug).await?))
}

pub async fn join(
    State(s): State<AppState>,
    AuthUser(u): AuthUser,
    Json(req): Json<JoinGroupRequest>,
) -> AppResult<Json<Group>> {
    Ok(Json(svc::join(&s, u.id, &req.invite_code).await?))
}

pub async fn leave(
    State(s): State<AppState>,
    AuthUser(u): AuthUser,
    Path(id): Path<Uuid>,
) -> AppResult<StatusCode> {
    svc::leave(&s, u.id, id).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn rotate_invite(
    State(s): State<AppState>,
    AuthUser(u): AuthUser,
    Path(id): Path<Uuid>,
) -> AppResult<Json<Group>> {
    Ok(Json(svc::rotate_invite(&s, u.id, id).await?))
}

pub async fn members(
    State(s): State<AppState>,
    AuthUser(u): AuthUser,
    Path(id): Path<Uuid>,
) -> AppResult<Json<Vec<GroupMember>>> {
    Ok(Json(svc::members(&s, u.id, id).await?))
}

pub async fn set_role(
    State(s): State<AppState>,
    AuthUser(u): AuthUser,
    Path((id, user_id)): Path<(Uuid, Uuid)>,
    Json(req): Json<SetMemberRoleRequest>,
) -> AppResult<StatusCode> {
    svc::set_role(&s, u.id, id, user_id, req.role).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn remove_member(
    State(s): State<AppState>,
    AuthUser(u): AuthUser,
    Path((id, user_id)): Path<(Uuid, Uuid)>,
) -> AppResult<StatusCode> {
    svc::remove_member(&s, u.id, id, user_id).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn leaderboard(
    State(s): State<AppState>,
    AuthUser(u): AuthUser,
    Path(id): Path<Uuid>,
    Query(q): Query<ModeQuery>,
) -> AppResult<Json<Vec<LeaderboardEntry>>> {
    let five = q.mode == Some(GameMode::FivePlayer);
    Ok(Json(svc::leaderboard(&s, u.id, id, five).await?))
}

//! Request extractors: the authenticated user (from the `wt_session` cookie).
use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use chrono::{DateTime, Utc};
use sqlx::FromRow;
use uuid::Uuid;
use wartide_protocol::auth::SessionUser;
use wartide_protocol::game::UserRef;

use crate::error::AppError;
use crate::state::AppState;
use crate::util::hash_token;

pub const SESSION_COOKIE: &str = "wt_session";

#[derive(Debug, Clone, FromRow)]
pub struct CurrentUser {
    pub id: Uuid,
    pub email: String,
    pub username: String,
    pub display_name: String,
    pub avatar: Option<String>,
    pub email_verified: bool,
    pub is_admin: bool,
    pub timezone: Option<String>,
    pub created_at: DateTime<Utc>,
}

impl CurrentUser {
    pub fn to_session_user(&self) -> SessionUser {
        SessionUser {
            id: self.id,
            username: self.username.clone(),
            display_name: self.display_name.clone(),
            avatar: self.avatar.clone(),
            email_verified: self.email_verified,
            is_admin: self.is_admin,
            timezone: self.timezone.clone(),
            created_at: self.created_at,
        }
    }
    pub fn to_user_ref(&self) -> UserRef {
        UserRef {
            id: self.id,
            username: self.username.clone(),
            display_name: self.display_name.clone(),
            avatar: self.avatar.clone(),
        }
    }
}

/// Extract the raw cookie value for `name` from a Cookie header.
pub fn cookie_value(headers: &axum::http::HeaderMap, name: &str) -> Option<String> {
    for h in headers.get_all(axum::http::header::COOKIE) {
        let Ok(s) = h.to_str() else { continue };
        for part in s.split(';') {
            if let Some((k, v)) = part.trim().split_once('=') {
                if k == name {
                    return Some(v.to_string());
                }
            }
        }
    }
    None
}

pub async fn user_from_token(
    state: &AppState,
    token: &str,
) -> Result<Option<CurrentUser>, AppError> {
    let hash = hash_token(&state.config.session_secret, token);
    let u = sqlx::query_as::<_, CurrentUser>(
        "SELECT u.id, u.email, u.username, u.display_name, u.avatar, u.email_verified, u.is_admin, u.timezone, u.created_at
         FROM sessions s JOIN users u ON u.id = s.user_id
         WHERE s.token_hash = $1 AND s.expires_at > now() AND u.deleted_at IS NULL",
    )
    .bind(hash)
    .fetch_optional(&state.db)
    .await?;
    Ok(u)
}

pub struct AuthUser(pub CurrentUser);

impl FromRequestParts<AppState> for AuthUser {
    type Rejection = AppError;
    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, AppError> {
        let token = cookie_value(&parts.headers, SESSION_COOKIE).ok_or(AppError::Unauthorized)?;
        user_from_token(state, &token)
            .await?
            .map(AuthUser)
            .ok_or(AppError::Unauthorized)
    }
}

/// Authenticated and email-verified (required to create/join games).
pub struct VerifiedUser(pub CurrentUser);

impl FromRequestParts<AppState> for VerifiedUser {
    type Rejection = AppError;
    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, AppError> {
        let AuthUser(u) = AuthUser::from_request_parts(parts, state).await?;
        if !u.email_verified {
            return Err(AppError::EmailUnverified);
        }
        Ok(VerifiedUser(u))
    }
}

/// Authenticated administrator (`users.is_admin`, set via SQL only).
pub struct AdminUser(pub CurrentUser);

impl FromRequestParts<AppState> for AdminUser {
    type Rejection = AppError;
    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, AppError> {
        let AuthUser(u) = AuthUser::from_request_parts(parts, state).await?;
        if !u.is_admin {
            return Err(AppError::forbidden("administrator access required"));
        }
        Ok(AdminUser(u))
    }
}

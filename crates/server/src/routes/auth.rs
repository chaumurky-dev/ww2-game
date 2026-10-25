use axum::extract::State;
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use wartide_protocol::auth::*;

use std::time::Duration;

use crate::error::{AppError, AppResult};
use crate::extract::{cookie_value, AuthUser, SESSION_COOKIE};
use crate::services::auth as svc;
use crate::state::AppState;

fn session_cookie(state: &AppState, token: &str, max_age_secs: i64) -> HeaderValue {
    let secure = if state.config.cookie_secure {
        "; Secure"
    } else {
        ""
    };
    HeaderValue::from_str(&format!(
        "{SESSION_COOKIE}={token}; HttpOnly{secure}; SameSite=Lax; Path=/; Max-Age={max_age_secs}"
    ))
    .expect("valid cookie")
}

pub async fn register(
    State(s): State<AppState>,
    Json(req): Json<RegisterRequest>,
) -> AppResult<(StatusCode, Json<SessionUser>)> {
    Ok((StatusCode::CREATED, Json(svc::register(&s, req).await?)))
}

pub async fn login(
    State(s): State<AppState>,
    Json(req): Json<LoginRequest>,
) -> AppResult<Response> {
    let (token, user) = svc::login(&s, req).await?;
    let mut res = Json(user).into_response();
    res.headers_mut().insert(
        header::SET_COOKIE,
        session_cookie(&s, &token, svc::SESSION_DAYS * 86400),
    );
    Ok(res)
}

pub async fn logout(State(s): State<AppState>, headers: HeaderMap) -> AppResult<Response> {
    if let Some(t) = cookie_value(&headers, SESSION_COOKIE) {
        svc::logout(&s, &t).await?;
    }
    let mut res = StatusCode::NO_CONTENT.into_response();
    res.headers_mut()
        .insert(header::SET_COOKIE, session_cookie(&s, "", 0));
    Ok(res)
}

pub async fn me(AuthUser(u): AuthUser) -> Json<SessionUser> {
    Json(u.to_session_user())
}

pub async fn verify_email(
    State(s): State<AppState>,
    Json(req): Json<VerifyEmailRequest>,
) -> AppResult<StatusCode> {
    svc::verify_email(&s, &req.token).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn resend_verification(
    State(s): State<AppState>,
    AuthUser(u): AuthUser,
) -> AppResult<StatusCode> {
    svc::resend_verification(&s, &u).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn forgot_password(
    State(s): State<AppState>,
    Json(req): Json<ForgotPasswordRequest>,
) -> AppResult<StatusCode> {
    svc::forgot_password(&s, &req.email).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn reset_password(
    State(s): State<AppState>,
    Json(req): Json<ResetPasswordRequest>,
) -> AppResult<StatusCode> {
    svc::reset_password(&s, req).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn change_password(
    State(s): State<AppState>,
    AuthUser(u): AuthUser,
    headers: HeaderMap,
    Json(req): Json<ChangePasswordRequest>,
) -> AppResult<StatusCode> {
    s.limiter
        .check(&format!("pw:{}", u.id), 5, Duration::from_secs(15 * 60))?;
    let token = cookie_value(&headers, SESSION_COOKIE).ok_or(AppError::Unauthorized)?;
    svc::change_password(&s, &u, &token, req).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn delete_me(
    State(s): State<AppState>,
    AuthUser(u): AuthUser,
    Json(req): Json<DeleteAccountRequest>,
) -> AppResult<Response> {
    s.limiter
        .check(&format!("del:{}", u.id), 3, Duration::from_secs(3600))?;
    svc::delete_account(&s, &u, &req.password).await?;
    let mut res = StatusCode::NO_CONTENT.into_response();
    res.headers_mut()
        .insert(header::SET_COOKIE, session_cookie(&s, "", 0));
    Ok(res)
}

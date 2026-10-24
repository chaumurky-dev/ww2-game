use argon2::password_hash::{
    rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString,
};
use argon2::Argon2;
use chrono::{Duration, Utc};
use uuid::Uuid;
use wartide_protocol::auth::*;

use crate::error::{is_unique_violation, AppError, AppResult};
use crate::extract::CurrentUser;
use crate::state::AppState;
use crate::util::*;

pub const SESSION_DAYS: i64 = 30;
const MIN_PASSWORD: usize = 8;

pub fn hash_password(password: &str) -> AppResult<String> {
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map(|h| h.to_string())
        .map_err(|e| AppError::Internal(anyhow::anyhow!("hash error: {e}")))
}

pub fn verify_password(password: &str, hash: &str) -> bool {
    PasswordHash::new(hash)
        .map(|p| {
            Argon2::default()
                .verify_password(password.as_bytes(), &p)
                .is_ok()
        })
        .unwrap_or(false)
}

fn check_password(p: &str) -> AppResult<()> {
    if p.len() < MIN_PASSWORD {
        return Err(AppError::BadRequest(
            "weak_password",
            format!("password must be at least {MIN_PASSWORD} characters"),
        ));
    }
    if p.len() > 256 {
        return Err(AppError::bad("password too long"));
    }
    Ok(())
}

async fn issue_email_token(
    state: &AppState,
    user_id: Uuid,
    purpose: &str,
    ttl: Duration,
) -> AppResult<String> {
    let token = new_token();
    sqlx::query(
        "INSERT INTO email_tokens (token_hash, user_id, purpose, expires_at) VALUES ($1,$2,$3,$4)",
    )
    .bind(hash_token(&state.config.session_secret, &token))
    .bind(user_id)
    .bind(purpose)
    .bind(Utc::now() + ttl)
    .execute(&state.db)
    .await?;
    Ok(token)
}

pub async fn send_verification(
    state: &AppState,
    user_id: Uuid,
    email: &str,
    username: &str,
) -> AppResult<()> {
    // invalidate older outstanding verification tokens
    sqlx::query(
        "DELETE FROM email_tokens WHERE user_id=$1 AND purpose='verify' AND used_at IS NULL",
    )
    .bind(user_id)
    .execute(&state.db)
    .await?;
    let token = issue_email_token(state, user_id, "verify", Duration::hours(48)).await?;
    let link = format!(
        "{}/verify?token={}",
        state.config.base_url.trim_end_matches('/'),
        token
    );
    state.mailer.send(
        email,
        "Verify your Wartide email",
        &format!("Welcome to Wartide, {username}!\n\nConfirm your email address:\n{link}\n\nThe link expires in 48 hours."),
    );
    Ok(())
}

pub async fn register(state: &AppState, req: RegisterRequest) -> AppResult<SessionUser> {
    let email = req.email.trim().to_string();
    let username = req.username.trim().to_string();
    if !valid_email(&email) {
        return Err(AppError::BadRequest(
            "invalid_email",
            "invalid email address".into(),
        ));
    }
    if !valid_username(&username) {
        return Err(AppError::BadRequest(
            "invalid_username",
            "username must be 3-24 characters: letters, digits, _ or -".into(),
        ));
    }
    check_password(&req.password)?;
    let hash = tokio::task::spawn_blocking({
        let p = req.password.clone();
        move || hash_password(&p)
    })
    .await
    .map_err(|e| AppError::Internal(e.into()))??;
    let id = Uuid::new_v4();
    let res = sqlx::query_as::<_, CurrentUser>(
        "INSERT INTO users (id, email, username, display_name, password_hash) VALUES ($1,$2,$3,$3,$4)
         RETURNING id, email, username, display_name, avatar, email_verified, created_at",
    )
    .bind(id)
    .bind(&email)
    .bind(&username)
    .bind(hash)
    .fetch_one(&state.db)
    .await;
    let user = match res {
        Ok(u) => u,
        Err(e) if is_unique_violation(&e) => {
            return Err(AppError::conflict(
                "already_exists",
                "email or username already taken",
            ))
        }
        Err(e) => return Err(e.into()),
    };
    send_verification(state, user.id, &user.email, &user.username).await?;
    Ok(user.to_session_user())
}

pub async fn verify_email(state: &AppState, token: &str) -> AppResult<()> {
    let hash = hash_token(&state.config.session_secret, token);
    let row: Option<(Uuid,)> = sqlx::query_as(
        "UPDATE email_tokens SET used_at = now()
         WHERE token_hash=$1 AND purpose='verify' AND used_at IS NULL AND expires_at > now()
         RETURNING user_id",
    )
    .bind(hash)
    .fetch_optional(&state.db)
    .await?;
    let (uid,) = row
        .ok_or_else(|| AppError::BadRequest("invalid_token", "invalid or expired token".into()))?;
    sqlx::query("UPDATE users SET email_verified = TRUE WHERE id=$1")
        .bind(uid)
        .execute(&state.db)
        .await?;
    Ok(())
}

pub async fn resend_verification(state: &AppState, user: &CurrentUser) -> AppResult<()> {
    if user.email_verified {
        return Err(AppError::conflict(
            "already_verified",
            "email already verified",
        ));
    }
    send_verification(state, user.id, &user.email, &user.username).await
}

/// Returns the raw session token on success.
pub async fn login(state: &AppState, req: LoginRequest) -> AppResult<(String, SessionUser)> {
    let row: Option<(Uuid, String)> = sqlx::query_as(
        "SELECT id, password_hash FROM users WHERE lower(email)=lower($1) OR lower(username)=lower($1)",
    )
    .bind(req.login.trim())
    .fetch_optional(&state.db)
    .await?;
    let bad = || AppError::BadRequest("invalid_credentials", "invalid login or password".into());
    let (id, hash) = row.ok_or_else(bad)?;
    let pw = req.password.clone();
    let ok = tokio::task::spawn_blocking(move || verify_password(&pw, &hash))
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    if !ok {
        return Err(bad());
    }
    let token = new_token();
    sqlx::query("INSERT INTO sessions (token_hash, user_id, expires_at) VALUES ($1,$2,$3)")
        .bind(hash_token(&state.config.session_secret, &token))
        .bind(id)
        .bind(Utc::now() + Duration::days(SESSION_DAYS))
        .execute(&state.db)
        .await?;
    let user = sqlx::query_as::<_, CurrentUser>(
        "SELECT id, email, username, display_name, avatar, email_verified, created_at FROM users WHERE id=$1",
    )
    .bind(id)
    .fetch_one(&state.db)
    .await?;
    Ok((token, user.to_session_user()))
}

pub async fn logout(state: &AppState, token: &str) -> AppResult<()> {
    sqlx::query("DELETE FROM sessions WHERE token_hash=$1")
        .bind(hash_token(&state.config.session_secret, token))
        .execute(&state.db)
        .await?;
    Ok(())
}

/// Always succeeds from the caller's point of view, to avoid leaking which emails exist.
pub async fn forgot_password(state: &AppState, email: &str) -> AppResult<()> {
    let row: Option<(Uuid, String, String)> =
        sqlx::query_as("SELECT id, email, username FROM users WHERE lower(email)=lower($1)")
            .bind(email.trim())
            .fetch_optional(&state.db)
            .await?;
    if let Some((id, email, username)) = row {
        let token = issue_email_token(state, id, "reset", Duration::hours(2)).await?;
        let link = format!(
            "{}/reset-password?token={}",
            state.config.base_url.trim_end_matches('/'),
            token
        );
        state.mailer.send(
            &email,
            "Reset your Wartide password",
            &format!("Hi {username},\n\nReset your password here:\n{link}\n\nThe link expires in 2 hours. If you did not ask for this, ignore this email."),
        );
    }
    Ok(())
}

pub async fn reset_password(state: &AppState, req: ResetPasswordRequest) -> AppResult<()> {
    check_password(&req.new_password)?;
    let hash = tokio::task::spawn_blocking({
        let p = req.new_password.clone();
        move || hash_password(&p)
    })
    .await
    .map_err(|e| AppError::Internal(e.into()))??;
    let row: Option<(Uuid,)> = sqlx::query_as(
        "UPDATE email_tokens SET used_at = now()
         WHERE token_hash=$1 AND purpose='reset' AND used_at IS NULL AND expires_at > now()
         RETURNING user_id",
    )
    .bind(hash_token(&state.config.session_secret, &req.token))
    .fetch_optional(&state.db)
    .await?;
    let (uid,) = row
        .ok_or_else(|| AppError::BadRequest("invalid_token", "invalid or expired token".into()))?;
    sqlx::query("UPDATE users SET password_hash=$2 WHERE id=$1")
        .bind(uid)
        .bind(hash)
        .execute(&state.db)
        .await?;
    // log out everywhere
    sqlx::query("DELETE FROM sessions WHERE user_id=$1")
        .bind(uid)
        .execute(&state.db)
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn password_roundtrip() {
        let h = hash_password("correct horse").unwrap();
        assert!(h.starts_with("$argon2id$"));
        assert!(verify_password("correct horse", &h));
        assert!(!verify_password("wrong", &h));
        assert!(!verify_password("x", "garbage"));
    }

    #[test]
    fn weak_passwords_rejected() {
        assert!(check_password("short").is_err());
        assert!(check_password("longenough").is_ok());
    }
}

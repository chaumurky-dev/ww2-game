use argon2::password_hash::phc::PasswordHash;
use argon2::password_hash::{PasswordHasher, PasswordVerifier};
use argon2::Argon2;
use std::sync::{Arc, OnceLock};
use std::time::Duration as StdDuration;

use chrono::{Duration, Utc};
use tokio::sync::Semaphore;
use uuid::Uuid;
use wartide_protocol::auth::*;

use crate::email_templates;
use crate::error::{is_unique_violation, AppError, AppResult};
use crate::extract::CurrentUser;
use crate::state::AppState;
use crate::util::*;

pub const SESSION_DAYS: i64 = 30;
const MIN_PASSWORD: usize = 8;
const HOUR: StdDuration = StdDuration::from_secs(3600);
const LOGIN_WINDOW: StdDuration = StdDuration::from_secs(15 * 60);
const LOGIN_FAILURES: usize = 10;

/// Bounds concurrent Argon2 work so a flood of logins cannot exhaust memory.
fn hash_gate() -> &'static Arc<Semaphore> {
    static GATE: OnceLock<Arc<Semaphore>> = OnceLock::new();
    GATE.get_or_init(|| {
        let n = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(2)
            .max(2);
        Arc::new(Semaphore::new(n))
    })
}

async fn gated<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> AppResult<T> {
    let permit = hash_gate()
        .clone()
        .acquire_owned()
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    tokio::task::spawn_blocking(move || {
        let _permit = permit;
        f()
    })
    .await
    .map_err(|e| AppError::Internal(e.into()))
}

pub async fn hash_password_async(password: &str) -> AppResult<String> {
    let p = password.to_string();
    gated(move || hash_password(&p)).await?
}

pub async fn verify_password_async(password: &str, hash: &str) -> AppResult<bool> {
    let (p, h) = (password.to_string(), hash.to_string());
    gated(move || verify_password(&p, &h)).await
}

/// A valid Argon2 hash nobody knows the password for; verified against when no account matches so
/// login timing does not reveal which identifiers exist.
fn dummy_hash() -> &'static str {
    static DUMMY: OnceLock<String> = OnceLock::new();
    DUMMY.get_or_init(|| {
        hash_password("dummy-password-for-timing-equalisation").expect("dummy hash")
    })
}

pub fn hash_password(password: &str) -> AppResult<String> {
    // argon2 0.6 generates the salt itself (getrandom); output stays standard PHC `$argon2id$...`.
    Argon2::default()
        .hash_password(password.as_bytes())
        .map(|h| h.to_string())
        .map_err(|e| AppError::Internal(anyhow::anyhow!("hash error: {e}")))
}

pub fn verify_password(password: &str, hash: &str) -> bool {
    hash.parse::<PasswordHash>()
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
    state
        .mailer
        .send_rendered(email, email_templates::verification(username, &link));
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
    if crate::services::users::is_reserved_name(&username) {
        return Err(AppError::BadRequest(
            "invalid_username",
            "that username is reserved".into(),
        ));
    }
    check_password(&req.password)?;
    state
        .limiter
        .check(&format!("register:{}", email.to_lowercase()), 3, HOUR)?;
    let hash = hash_password_async(&req.password).await?;
    let id = Uuid::new_v4();
    let res = sqlx::query_as::<_, CurrentUser>(
        "INSERT INTO users (id, email, username, display_name, password_hash) VALUES ($1,$2,$3,$3,$4)
         RETURNING id, email, username, display_name, avatar, email_verified, is_admin, timezone, created_at",
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
    // Defeat account pre-hijacking: whoever registered with this address and holds a session
    // loses it once the real owner proves control of the mailbox.
    sqlx::query("DELETE FROM sessions WHERE user_id=$1")
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
    // Per-address limit; over the limit we silently do nothing (uniform response).
    if state
        .limiter
        .check(&format!("resend:{}", user.email.to_lowercase()), 3, HOUR)
        .is_err()
    {
        return Ok(());
    }
    send_verification(state, user.id, &user.email, &user.username).await
}

/// Returns the raw session token on success.
pub async fn login(state: &AppState, req: LoginRequest) -> AppResult<(String, SessionUser)> {
    let fail_key = format!("loginfail:{}", req.login.trim().to_lowercase());
    state
        .limiter
        .ensure_below(&fail_key, LOGIN_FAILURES, LOGIN_WINDOW)?;
    let row: Option<(Uuid, String)> = sqlx::query_as(
        "SELECT id, password_hash FROM users WHERE deleted_at IS NULL AND (lower(email)=lower($1) OR lower(username)=lower($1))",
    )
    .bind(req.login.trim())
    .fetch_optional(&state.db)
    .await?;
    let bad = || AppError::BadRequest("invalid_credentials", "invalid login or password".into());
    let (id, hash) = match row {
        Some(r) => r,
        None => {
            // burn the same Argon2 time as a real check
            let _ = verify_password_async(&req.password, dummy_hash()).await?;
            state.limiter.record(&fail_key);
            return Err(bad());
        }
    };
    let ok = verify_password_async(&req.password, &hash).await?;
    if !ok {
        state.limiter.record(&fail_key);
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
        "SELECT id, email, username, display_name, avatar, email_verified, is_admin, timezone, created_at FROM users WHERE id=$1",
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
    if state
        .limiter
        .check(&format!("forgot:{}", email.trim().to_lowercase()), 3, HOUR)
        .is_err()
    {
        return Ok(());
    }
    let row: Option<(Uuid, String, String)> = sqlx::query_as(
        "SELECT id, email, username FROM users WHERE deleted_at IS NULL AND lower(email)=lower($1)",
    )
    .bind(email.trim())
    .fetch_optional(&state.db)
    .await?;
    if let Some((id, email, username)) = row {
        sqlx::query(
            "DELETE FROM email_tokens WHERE user_id=$1 AND purpose='reset' AND used_at IS NULL",
        )
        .bind(id)
        .execute(&state.db)
        .await?;
        let token = issue_email_token(state, id, "reset", Duration::hours(2)).await?;
        let link = format!(
            "{}/reset-password?token={}",
            state.config.base_url.trim_end_matches('/'),
            token
        );
        state
            .mailer
            .send_rendered(&email, email_templates::password_reset(&username, &link));
    }
    Ok(())
}

/// Change the password of a logged-in user. All other sessions are revoked; `keep_token` (the
/// caller's current session token) survives.
pub async fn change_password(
    state: &AppState,
    user: &CurrentUser,
    keep_token: &str,
    req: ChangePasswordRequest,
) -> AppResult<()> {
    check_password(&req.new_password)?;
    let (hash,): (String,) = sqlx::query_as("SELECT password_hash FROM users WHERE id=$1")
        .bind(user.id)
        .fetch_one(&state.db)
        .await?;
    let ok = verify_password_async(&req.current_password, &hash).await?;
    if !ok {
        return Err(AppError::BadRequest(
            "invalid_credentials",
            "current password is incorrect".into(),
        ));
    }
    let new_hash = hash_password_async(&req.new_password).await?;
    sqlx::query("UPDATE users SET password_hash=$2 WHERE id=$1")
        .bind(user.id)
        .bind(new_hash)
        .execute(&state.db)
        .await?;
    sqlx::query("DELETE FROM email_tokens WHERE user_id=$1 AND purpose='reset'")
        .bind(user.id)
        .execute(&state.db)
        .await?;
    sqlx::query("DELETE FROM sessions WHERE user_id=$1 AND token_hash <> $2")
        .bind(user.id)
        .bind(hash_token(&state.config.session_secret, keep_token))
        .execute(&state.db)
        .await?;
    Ok(())
}

/// Soft-delete: scrub identifying data but keep the row so games, seats and chat stay intact.
pub async fn delete_account(state: &AppState, user: &CurrentUser, password: &str) -> AppResult<()> {
    let (hash,): (String,) = sqlx::query_as("SELECT password_hash FROM users WHERE id=$1")
        .bind(user.id)
        .fetch_one(&state.db)
        .await?;
    if !verify_password_async(password, &hash).await? {
        return Err(AppError::ForbiddenCode(
            "invalid_credentials",
            "password is incorrect".into(),
        ));
    }
    let mut tx = state.db.begin().await?;
    sqlx::query(
        "UPDATE users SET
            email = 'deleted-' || id::text || '@deleted.invalid',
            username = 'deleted-' || id::text,
            display_name = 'Deleted user',
            password_hash = '!',
            avatar = NULL, bio = NULL, country = NULL, timezone = NULL,
            email_verified = FALSE, email_turn_notifications = FALSE, email_match_found = FALSE,
            is_admin = FALSE, deleted_at = now()
         WHERE id=$1 AND deleted_at IS NULL",
    )
    .bind(user.id)
    .execute(&mut *tx)
    .await?;
    for q in [
        "DELETE FROM sessions WHERE user_id=$1",
        "DELETE FROM email_tokens WHERE user_id=$1",
        "DELETE FROM matchmaking_queue WHERE user_id=$1",
        "DELETE FROM group_members WHERE user_id=$1 AND role <> 'owner'",
    ] {
        sqlx::query(q).bind(user.id).execute(&mut *tx).await?;
    }
    tx.commit().await?;
    Ok(())
}

pub async fn reset_password(state: &AppState, req: ResetPasswordRequest) -> AppResult<()> {
    check_password(&req.new_password)?;
    let hash = hash_password_async(&req.new_password).await?;
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
    // log out everywhere, and burn any other outstanding reset links
    sqlx::query("DELETE FROM sessions WHERE user_id=$1")
        .bind(uid)
        .execute(&state.db)
        .await?;
    sqlx::query("DELETE FROM email_tokens WHERE user_id=$1 AND purpose='reset'")
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

    /// Hash produced by argon2 0.5.3 with the same parameters (m=19456,t=2,p=1): existing stored
    /// hashes must keep verifying after the 0.6 upgrade.
    #[test]
    fn hash_from_argon2_0_5_still_verifies() {
        let legacy = "$argon2id$v=19$m=19456,t=2,p=1$yKHPgcOGg5l0pfpv/rGd4w$nD70fsMt9LSzEAcOIQ95w7yyliVH5rNNNQMBOYy9/b0";
        assert!(verify_password("correct horse", legacy));
        assert!(!verify_password("wrong horse", legacy));
    }

    #[test]
    fn dummy_hash_is_a_valid_argon2_hash() {
        let h = dummy_hash();
        assert!(h.starts_with("$argon2id$"));
        assert!(!verify_password("anything", h));
    }

    #[test]
    fn weak_passwords_rejected() {
        assert!(check_password("short").is_err());
        assert!(check_password("longenough").is_ok());
    }
}

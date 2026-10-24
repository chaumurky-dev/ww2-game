use chrono::{DateTime, Utc};
use sqlx::FromRow;
use uuid::Uuid;
use wartide_protocol::user::*;

use crate::error::{AppError, AppResult};
use crate::state::AppState;

#[derive(FromRow)]
struct ProfileRow {
    id: Uuid,
    username: String,
    display_name: String,
    avatar: Option<String>,
    bio: Option<String>,
    country: Option<String>,
    timezone: Option<String>,
    created_at: DateTime<Utc>,
}

#[derive(FromRow)]
struct StatsRow {
    ladder: String,
    rating: f64,
    deviation: f64,
    games: i32,
    wins: i32,
    losses: i32,
    rank_position: i64,
}

pub async fn public_profile(state: &AppState, username: &str) -> AppResult<PublicProfile> {
    let p = sqlx::query_as::<_, ProfileRow>(
        "SELECT id, username, display_name, avatar, bio, country, timezone, created_at
         FROM users WHERE lower(username)=lower($1)",
    )
    .bind(username)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::not_found("user"))?;
    // Group ladders are only shown on the group page, not on the public profile.
    let stats = sqlx::query_as::<_, StatsRow>(
        "SELECT * FROM (
            SELECT ladder, rating, deviation, games, wins, losses,
                   RANK() OVER (PARTITION BY ladder ORDER BY rating DESC) AS rank_position, user_id
            FROM ratings WHERE ladder LIKE 'public:%'
         ) r WHERE user_id=$1 ORDER BY ladder",
    )
    .bind(p.id)
    .fetch_all(&state.db)
    .await?;
    Ok(PublicProfile {
        id: p.id,
        username: p.username,
        display_name: p.display_name,
        avatar: p.avatar,
        bio: p.bio,
        country: p.country,
        timezone: p.timezone,
        created_at: p.created_at,
        stats: stats
            .into_iter()
            .map(|s| LadderStats {
                ladder: s.ladder,
                rating: s.rating,
                rating_deviation: s.deviation,
                games_played: s.games,
                wins: s.wins,
                losses: s.losses,
                rank_position: Some(s.rank_position),
            })
            .collect(),
    })
}

pub async fn update_profile(
    state: &AppState,
    user_id: Uuid,
    req: UpdateProfileRequest,
) -> AppResult<()> {
    let clean = |s: Option<String>, max: usize| -> AppResult<Option<String>> {
        match s {
            None => Ok(None),
            Some(v) if v.chars().count() > max => {
                Err(AppError::bad(format!("value too long (max {max})")))
            }
            Some(v) => Ok(Some(v.trim().to_string())),
        }
    };
    let display_name = clean(req.display_name, 40)?;
    if matches!(&display_name, Some(d) if d.is_empty()) {
        return Err(AppError::bad("display name cannot be empty"));
    }
    // COALESCE: only provided fields change; an empty string clears optional fields.
    sqlx::query(
        "UPDATE users SET
            display_name = COALESCE($2, display_name),
            avatar = CASE WHEN $3::text IS NULL THEN avatar ELSE NULLIF($3,'') END,
            bio = CASE WHEN $4::text IS NULL THEN bio ELSE NULLIF($4,'') END,
            country = CASE WHEN $5::text IS NULL THEN country ELSE NULLIF($5,'') END,
            timezone = CASE WHEN $6::text IS NULL THEN timezone ELSE NULLIF($6,'') END
         WHERE id=$1",
    )
    .bind(user_id)
    .bind(display_name)
    .bind(clean(req.avatar, 512)?)
    .bind(clean(req.bio, 500)?)
    .bind(clean(req.country, 64)?)
    .bind(clean(req.timezone, 64)?)
    .execute(&state.db)
    .await?;
    Ok(())
}

pub async fn get_settings(state: &AppState, user_id: Uuid) -> AppResult<UserSettings> {
    let (n,): (bool,) = sqlx::query_as("SELECT email_turn_notifications FROM users WHERE id=$1")
        .bind(user_id)
        .fetch_one(&state.db)
        .await?;
    Ok(UserSettings {
        email_turn_notifications: n,
    })
}

pub async fn update_settings(
    state: &AppState,
    user_id: Uuid,
    req: UpdateSettingsRequest,
) -> AppResult<UserSettings> {
    if let Some(n) = req.email_turn_notifications {
        sqlx::query("UPDATE users SET email_turn_notifications=$2 WHERE id=$1")
            .bind(user_id)
            .bind(n)
            .execute(&state.db)
            .await?;
    }
    get_settings(state, user_id).await
}

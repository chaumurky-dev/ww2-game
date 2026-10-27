use std::collections::HashMap;

use chrono::{DateTime, Utc};
use sqlx::FromRow;
use uuid::Uuid;
use wartide_protocol::game::{GameKind, GameStatus, GameSummary};
use wartide_protocol::user::*;

use crate::error::{AppError, AppResult};
use crate::services::games;
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
         FROM users WHERE deleted_at IS NULL AND lower(username)=lower($1)",
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
    let streaks = win_streaks(state, p.id).await?;
    let recent_games = recent_games(state, p.id).await?;
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
            .map(|s| {
                let (cur, best) = streaks.get(&s.ladder).copied().unwrap_or((0, 0));
                LadderStats {
                    ladder: s.ladder,
                    rating: s.rating,
                    rating_deviation: s.deviation,
                    games_played: s.games,
                    wins: s.wins,
                    losses: s.losses,
                    rank_position: Some(s.rank_position),
                    current_win_streak: cur,
                    best_win_streak: best,
                }
            })
            .collect(),
        recent_games,
    })
}

/// Returns (current, best) consecutive-win counts per public ladder, from finished rated games.
/// Games without a winner (abandoned) are ignored.
pub fn streaks(results: &[bool]) -> (i32, i32) {
    let (mut cur, mut best) = (0, 0);
    for won in results {
        if *won {
            cur += 1;
            best = best.max(cur);
        } else {
            cur = 0;
        }
    }
    (cur, best)
}

async fn win_streaks(state: &AppState, user: Uuid) -> AppResult<HashMap<String, (i32, i32)>> {
    // oldest -> newest; the team is derived from the first power of the user's seat
    let rows: Vec<(String, bool)> = sqlx::query_as(
        "SELECT g.ladder,
                (g.winner_team = CASE WHEN s.powers[1] IN ('germany','japan') THEN 'axis' ELSE 'allies' END) AS won
         FROM games g JOIN game_seats s ON s.game_id = g.id AND s.user_id = $1 AND NOT s.is_bot
         WHERE g.status=$2 AND g.winner_team IS NOT NULL AND g.ladder LIKE 'public:%'
         ORDER BY g.updated_at ASC, g.id",
    )
    .bind(user)
    .bind(GameStatus::Finished)
    .fetch_all(&state.db)
    .await?;
    let mut by: HashMap<String, Vec<bool>> = HashMap::new();
    for (ladder, won) in rows {
        by.entry(ladder).or_default().push(won);
    }
    Ok(by.into_iter().map(|(l, r)| (l, streaks(&r))).collect())
}

/// Last 10 finished or active public games of a user. The profile is world-readable, so private
/// and group games (which not everyone may open) are never listed.
async fn recent_games(state: &AppState, user: Uuid) -> AppResult<Vec<GameSummary>> {
    let ids: Vec<(Uuid,)> = sqlx::query_as(
        "SELECT g.id FROM games g
         WHERE g.status IN ($2,$3)
           AND g.kind=$4
           AND EXISTS (SELECT 1 FROM game_seats s WHERE s.game_id=g.id AND s.user_id=$1)
         ORDER BY g.updated_at DESC LIMIT 10",
    )
    .bind(user)
    .bind(GameStatus::InProgress)
    .bind(GameStatus::Finished)
    .bind(GameKind::Public)
    .fetch_all(&state.db)
    .await?;
    let mut out = Vec::new();
    for (id,) in ids {
        let mut g = games::summary(state, id, None).await?;
        if g.kind != GameKind::Public {
            games::redact_for_profile(&mut g);
        }
        out.push(g);
    }
    Ok(out)
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
    if let Some(d) = &display_name {
        check_display_name(d)?;
        if is_reserved_name(d) {
            let (is_admin,): (bool,) = sqlx::query_as("SELECT is_admin FROM users WHERE id=$1")
                .bind(user_id)
                .fetch_one(&state.db)
                .await?;
            if !is_admin {
                return Err(AppError::bad("that display name is reserved"));
            }
        }
    }
    let timezone = clean(req.timezone, 64)?;
    if let Some(tz) = timezone.as_deref().filter(|t| !t.is_empty()) {
        if !valid_timezone(tz) {
            return Err(AppError::bad(
                "invalid timezone (expected an IANA name like Europe/Berlin)",
            ));
        }
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
    .bind(timezone)
    .execute(&state.db)
    .await?;
    Ok(())
}

const RESERVED_NAMES: [&str; 6] = [
    "admin",
    "administrator",
    "moderator",
    "support",
    "wartide",
    "system",
];

/// Names that could be mistaken for staff (compared case-insensitively).
pub fn is_reserved_name(name: &str) -> bool {
    let n = name.trim().to_lowercase();
    RESERVED_NAMES.contains(&n.as_str())
}

/// Reject control characters, bidi overrides/isolates and zero-width characters, which allow
/// spoofed or invisible names.
pub fn check_display_name(name: &str) -> AppResult<()> {
    let bad = |c: char| {
        c.is_control()
            || matches!(c, '\u{200B}'..='\u{200F}' | '\u{202A}'..='\u{202E}' | '\u{2060}'
                | '\u{2066}'..='\u{2069}' | '\u{FEFF}')
    };
    if name.chars().any(bad) {
        return Err(AppError::bad("display name contains invalid characters"));
    }
    Ok(())
}

/// IANA-style zone name: `^[A-Za-z_]+(/[A-Za-z0-9_+-]+)*$`.
pub fn valid_timezone(tz: &str) -> bool {
    let mut parts = tz.split('/');
    let first = parts.next().unwrap_or("");
    !first.is_empty()
        && first.chars().all(|c| c.is_ascii_alphabetic() || c == '_')
        && parts.all(|p| {
            !p.is_empty()
                && p.chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '+' | '-'))
        })
}

pub async fn get_settings(state: &AppState, user_id: Uuid) -> AppResult<UserSettings> {
    let (n, m, cp, sw): (bool, bool, String, String) = sqlx::query_as(
        "SELECT email_turn_notifications, email_match_found, casualty_policy, sub_withdrawal FROM users WHERE id=$1",
    )
    .bind(user_id)
    .fetch_one(&state.db)
    .await?;
    Ok(UserSettings {
        email_turn_notifications: n,
        email_match_found: m,
        casualty_policy: parse_policy(&cp),
        sub_withdrawal: parse_policy(&sw),
    })
}

/// Stored policy id; an unrecognised value (never written by this code) falls back to the default.
fn parse_policy<T: serde::de::DeserializeOwned + Default>(s: &str) -> T {
    serde_json::from_value(serde_json::Value::String(s.to_string())).unwrap_or_default()
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
    if let Some(m) = req.email_match_found {
        sqlx::query("UPDATE users SET email_match_found=$2 WHERE id=$1")
            .bind(user_id)
            .bind(m)
            .execute(&state.db)
            .await?;
    }
    if let Some(p) = req.casualty_policy {
        sqlx::query("UPDATE users SET casualty_policy=$2 WHERE id=$1")
            .bind(user_id)
            .bind(p.id())
            .execute(&state.db)
            .await?;
    }
    if let Some(p) = req.sub_withdrawal {
        sqlx::query("UPDATE users SET sub_withdrawal=$2 WHERE id=$1")
            .bind(user_id)
            .bind(p.id())
            .execute(&state.db)
            .await?;
    }
    get_settings(state, user_id).await
}

#[cfg(test)]
mod tests {
    use super::streaks;

    #[test]
    fn win_streaks() {
        assert_eq!(streaks(&[]), (0, 0));
        assert_eq!(streaks(&[true, true, false, true]), (1, 2));
        assert_eq!(streaks(&[false, true, true, true]), (3, 3));
        assert_eq!(streaks(&[true, false]), (0, 1));
    }
}

#[cfg(test)]
mod tz_tests {
    use super::valid_timezone;

    #[test]
    fn timezone_pattern() {
        for ok in [
            "UTC",
            "Europe/Berlin",
            "America/Argentina/Buenos_Aires",
            "Etc/GMT+5",
            "America/Port-au-Prince",
        ] {
            assert!(valid_timezone(ok), "{ok}");
        }
        for bad in [
            "",
            "/UTC",
            "Europe/",
            "Europe//Berlin",
            "a b",
            "<script>",
            "Europe/Berlin;drop",
            "1Europe",
            "../etc",
        ] {
            assert!(!valid_timezone(bad), "{bad}");
        }
    }
}

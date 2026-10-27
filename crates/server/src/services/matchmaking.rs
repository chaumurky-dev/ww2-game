use std::time::Duration;

use chrono::{DateTime, Utc};
use rand::seq::SliceRandom;
use uuid::Uuid;
use wartide_protocol::game::*;
use wartide_protocol::ws::ServerMessage;

use crate::error::{AppError, AppResult};
use crate::services::{games, notify, ratings};
use crate::state::AppState;

pub const BASE_WINDOW: f64 = 100.0;
pub const WIDEN_PER_MINUTE: f64 = 50.0;

/// Acceptable rating difference after waiting `wait_secs`: +-100, plus 50 per full minute.
pub fn window(wait_secs: i64) -> f64 {
    BASE_WINDOW + WIDEN_PER_MINUTE * (wait_secs.max(0) / 60) as f64
}

#[derive(Debug, Clone)]
pub struct QueueEntry {
    pub user_id: Uuid,
    pub rating: f64,
    pub enqueued_at: DateTime<Utc>,
}

/// Find groups of `needed` players whose rating spread fits inside every member's window.
/// Returns user ids per match. Greedy over the rating-sorted queue; each player is used once.
pub fn find_matches(entries: &[QueueEntry], needed: usize, now: DateTime<Utc>) -> Vec<Vec<Uuid>> {
    let mut sorted: Vec<&QueueEntry> = entries.iter().collect();
    sorted.sort_by(|a, b| a.rating.total_cmp(&b.rating));
    let mut used = vec![false; sorted.len()];
    let mut out = Vec::new();
    if needed == 0 {
        return out;
    }
    let mut i = 0;
    while i + needed <= sorted.len() {
        // skip used
        let idxs: Vec<usize> = (i..sorted.len())
            .filter(|k| !used[*k])
            .take(needed)
            .collect();
        if idxs.len() < needed {
            break;
        }
        let group: Vec<&&QueueEntry> = idxs.iter().map(|k| &sorted[*k]).collect();
        let spread = group.last().unwrap().rating - group.first().unwrap().rating;
        let min_window = group
            .iter()
            .map(|e| window((now - e.enqueued_at).num_seconds()))
            .fold(f64::INFINITY, f64::min);
        if spread <= min_window {
            out.push(group.iter().map(|e| e.user_id).collect());
            for k in idxs {
                used[k] = true;
            }
        }
        i += 1;
        while i < sorted.len() && used[i] {
            i += 1;
        }
    }
    out
}

pub async fn enqueue(state: &AppState, user: Uuid, mode: GameMode) -> AppResult<()> {
    if mode == GameMode::Custom {
        return Err(AppError::bad(
            "matchmaking is only for 2-player and 5-player games",
        ));
    }
    // already in an active public game of this mode? allow; but not twice in queue
    let ladder = ratings::ladder_for(None, mode == GameMode::TwoPlayer);
    let rating = ratings::get_rating(&state.db, user, &ladder).await?.rating;
    let res = sqlx::query(
        "INSERT INTO matchmaking_queue (user_id, mode, rating) VALUES ($1,$2,$3) ON CONFLICT (user_id) DO NOTHING",
    )
    .bind(user)
    .bind(mode)
    .bind(rating)
    .execute(&state.db)
    .await?;
    if res.rows_affected() == 0 {
        return Err(AppError::conflict(
            "already_queued",
            "you are already in the matchmaking queue",
        ));
    }
    Ok(())
}

pub async fn dequeue(state: &AppState, user: Uuid) -> AppResult<()> {
    sqlx::query("DELETE FROM matchmaking_queue WHERE user_id=$1")
        .bind(user)
        .execute(&state.db)
        .await?;
    Ok(())
}

pub async fn status(state: &AppState, user: Uuid) -> AppResult<Option<GameMode>> {
    let r: Option<(GameMode,)> =
        sqlx::query_as("SELECT mode FROM matchmaking_queue WHERE user_id=$1")
            .bind(user)
            .fetch_optional(&state.db)
            .await?;
    Ok(r.map(|(m,)| m))
}

/// One matching pass for both modes. Returns the created game ids.
pub async fn run_once(state: &AppState) -> AppResult<Vec<Uuid>> {
    let mut created = Vec::new();
    for mode in [GameMode::TwoPlayer, GameMode::FivePlayer] {
        let rows: Vec<(Uuid, f64, DateTime<Utc>)> = sqlx::query_as(
            "SELECT q.user_id, q.rating, q.enqueued_at FROM matchmaking_queue q
             JOIN users u ON u.id=q.user_id WHERE q.mode=$1 AND u.email_verified",
        )
        .bind(mode)
        .fetch_all(&state.db)
        .await?;
        let entries: Vec<QueueEntry> = rows
            .into_iter()
            .map(|(user_id, rating, enqueued_at)| QueueEntry {
                user_id,
                rating,
                enqueued_at,
            })
            .collect();
        for mut group in find_matches(&entries, mode.seat_count(), Utc::now()) {
            // claim the players atomically; if someone left meanwhile, skip this group
            let claimed: Vec<(Uuid,)> = sqlx::query_as(
                "DELETE FROM matchmaking_queue WHERE user_id = ANY($1) RETURNING user_id",
            )
            .bind(&group)
            .fetch_all(&state.db)
            .await?;
            if claimed.len() != group.len() {
                for (u,) in claimed {
                    let _ = enqueue(state, u, mode).await;
                }
                continue;
            }
            group.shuffle(&mut rand::rng());
            let id = games::create_public(state, mode, &group).await?;
            let name = games::load_game(state, id).await?.name;
            for u in &group {
                state.hub.publish_user(
                    *u,
                    ServerMessage::MatchFound {
                        game_id: GameId(id),
                    },
                );
                if let Err(e) = notify::email_match_found(state, *u, &name, id, mode).await {
                    tracing::warn!(error = ?e, "match-found email failed");
                }
            }
            created.push(id);
        }
    }
    Ok(created)
}

pub fn spawn(state: AppState) {
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(Duration::from_secs(10));
        loop {
            tick.tick().await;
            if let Err(e) = run_once(&state).await {
                tracing::error!(error = ?e, "matchmaking pass failed");
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration as D;

    fn e(rating: f64, waited_secs: i64, now: DateTime<Utc>) -> QueueEntry {
        QueueEntry {
            user_id: Uuid::new_v4(),
            rating,
            enqueued_at: now - D::seconds(waited_secs),
        }
    }

    #[test]
    fn window_widens() {
        assert_eq!(window(0), 100.0);
        assert_eq!(window(59), 100.0);
        assert_eq!(window(60), 150.0);
        assert_eq!(window(300), 350.0);
        assert_eq!(window(-5), 100.0);
    }

    #[test]
    fn matches_close_pair_only() {
        let now = Utc::now();
        let q = vec![e(1500.0, 0, now), e(1580.0, 0, now), e(1900.0, 0, now)];
        let m = find_matches(&q, 2, now);
        assert_eq!(m.len(), 1);
        assert!(m[0].contains(&q[0].user_id) && m[0].contains(&q[1].user_id));
    }

    #[test]
    fn far_apart_wait_until_window_widens() {
        let now = Utc::now();
        let q = vec![e(1500.0, 0, now), e(1700.0, 0, now)];
        assert!(find_matches(&q, 2, now).is_empty());
        let later = vec![e(1500.0, 240, now), e(1700.0, 240, now)]; // window 300
        assert_eq!(find_matches(&later, 2, now).len(), 1);
        // the narrower window of the two governs
        let mixed = vec![e(1500.0, 0, now), e(1700.0, 600, now)];
        assert!(find_matches(&mixed, 2, now).is_empty());
    }

    #[test]
    fn five_player_needs_five_and_makes_multiple_groups() {
        let now = Utc::now();
        let four: Vec<_> = (0..4).map(|i| e(1500.0 + i as f64, 0, now)).collect();
        assert!(find_matches(&four, 5, now).is_empty());
        let ten: Vec<_> = (0..10).map(|i| e(1500.0 + i as f64, 0, now)).collect();
        let m = find_matches(&ten, 5, now);
        assert_eq!(m.len(), 2);
        let mut all: Vec<_> = m.concat();
        all.sort();
        all.dedup();
        assert_eq!(all.len(), 10);
    }
}

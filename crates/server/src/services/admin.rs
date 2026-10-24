//! Administrator-only operations (`users.is_admin`, set via SQL).
use uuid::Uuid;
use wartide_protocol::admin::AdminStats;
use wartide_protocol::game::{GameId, GameSummary};
use wartide_protocol::ws::ServerMessage;

use crate::error::{AppError, AppResult};
use crate::services::games;
use crate::state::AppState;

pub async fn stats(state: &AppState) -> AppResult<AdminStats> {
    let (users, verified_users): (i64, i64) = sqlx::query_as(
        "SELECT count(*), count(*) FILTER (WHERE email_verified) FROM users WHERE deleted_at IS NULL",
    )
    .fetch_one(&state.db)
    .await?;
    let by_status: Vec<(String, i64)> =
        sqlx::query_as("SELECT status, count(*) FROM games GROUP BY status")
            .fetch_all(&state.db)
            .await?;
    let by_queue: Vec<(String, i64)> =
        sqlx::query_as("SELECT mode, count(*) FROM matchmaking_queue GROUP BY mode")
            .fetch_all(&state.db)
            .await?;
    let get = |v: &[(String, i64)], k: &str| v.iter().find(|(s, _)| s == k).map_or(0, |(_, n)| *n);
    Ok(AdminStats {
        users,
        verified_users,
        games_lobby: get(&by_status, "lobby"),
        games_in_progress: get(&by_status, "in_progress"),
        games_finished: get(&by_status, "finished"),
        games_abandoned: get(&by_status, "abandoned"),
        queue_two_player: get(&by_queue, "two_player"),
        queue_five_player: get(&by_queue, "five_player"),
    })
}

/// Abandon any lobby / in-progress game (never rated).
pub async fn abandon_game(state: &AppState, admin: Uuid, id: Uuid) -> AppResult<GameSummary> {
    let n = sqlx::query(
        "UPDATE games SET status='abandoned', turn_deadline=NULL, updated_at=now()
         WHERE id=$1 AND status IN ('lobby','in_progress')",
    )
    .bind(id)
    .execute(&state.db)
    .await?
    .rows_affected();
    if n == 0 {
        // distinguish unknown game from already-over
        games::load_game(state, id).await?;
        return Err(AppError::conflict("already_over", "game is already over"));
    }
    tracing::info!(game = %id, admin = %admin, "game abandoned by admin");
    state.hub.publish_game(
        id,
        ServerMessage::GameUpdated {
            game_id: GameId(id),
            seq: -1,
        },
    );
    games::summary(state, id, Some(admin)).await
}

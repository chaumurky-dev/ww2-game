use std::time::Duration;

use async_trait::async_trait;
use uuid::Uuid;
use wartide_protocol::game::{GameSettings, GameStatus, Power};

use crate::error::AppResult;
use crate::services::{bots, chat, games};
use crate::state::AppState;

pub struct ExpiredTurn {
    pub game_id: Uuid,
    pub current_power: String,
    pub round: i32,
}

/// Called for every game whose `turn_deadline` has passed.
#[async_trait]
pub trait TurnTimeoutHandler: Send + Sync + 'static {
    async fn handle(&self, state: &AppState, expired: ExpiredTurn) -> AppResult<()>;
}

/// Consecutive missed turns after which a seat is handed to a computer player for good.
pub const MAX_CONSECUTIVE_TIMEOUTS: i32 = 3;

/// Default behaviour. Bot-controlled decisions are simply (re)started. For a human whose clock
/// ran out: post a system message, count the miss, restart their clock and let a computer player
/// play that power until the engine moves on to a different power (a temporary takeover). After
/// three consecutive misses the seat becomes bot-controlled permanently and the player counts as
/// resigned for ratings.
pub struct DefaultTimeoutHandler;

#[async_trait]
impl TurnTimeoutHandler for DefaultTimeoutHandler {
    async fn handle(&self, state: &AppState, e: ExpiredTurn) -> AppResult<()> {
        let id = e.game_id;
        let takeover = {
            let _guard = state.runtime.lock(id).await;
            let g = games::load_game(state, id).await?;
            // Re-check under the lock: the player may have acted in the meantime.
            if g.status != GameStatus::InProgress
                || g.turn_deadline.is_none_or(|d| d >= chrono::Utc::now())
            {
                return Ok(());
            }
            let Some(power) = g.pending_power.as_deref().and_then(Power::parse) else {
                return Ok(());
            };
            let seats = games::load_seats(state, id).await?;
            let Some(seat) = games::seat_for_power(&seats, power) else {
                return Ok(());
            };
            if seat.is_bot {
                None // bot turn that is simply due
            } else if g.pending_power != g.current_power {
                // A defender decision the driver resolves automatically (never asked of the human;
                // only reachable after a restart). No penalty: just restart the driver. The turn
                // owner's clock is left as it is, so their own timeout is judged on the next sweep.
                tracing::info!(game = %id, power = power.as_str(), "resuming automatic defender decision");
                None
            } else {
                tracing::info!(game = %id, power = power.as_str(), "turn timed out");
                let missed: i32 = sqlx::query_scalar(
                    "UPDATE game_seats SET timeouts=timeouts+1, missed_in_row=missed_in_row+1
                     WHERE game_id=$1 AND seat=$2 RETURNING missed_in_row",
                )
                .bind(id)
                .bind(seat.seat)
                .fetch_one(&state.db)
                .await?;
                let settings: GameSettings =
                    serde_json::from_value(g.settings.clone()).unwrap_or_default();
                // Restart the clock so the next sweep does not count this miss again.
                sqlx::query("UPDATE games SET turn_deadline = now() + make_interval(secs => $2), updated_at=now() WHERE id=$1")
                    .bind(id)
                    .bind(f64::from(settings.turn_timeout_secs))
                    .execute(&state.db)
                    .await?;
                let mut msg = format!(
                    "timeout: {} ran out of time in round {}; a computer player plays this turn.",
                    power.as_str(),
                    g.round
                );
                if missed >= MAX_CONSECUTIVE_TIMEOUTS {
                    sqlx::query("UPDATE game_seats SET is_bot=TRUE, resigned=TRUE, bot_persona=$3 WHERE game_id=$1 AND seat=$2")
                        .bind(id)
                        .bind(seat.seat)
                        .bind(bots::default_for_seat(seat.seat as usize))
                        .execute(&state.db)
                        .await?;
                    msg.push_str(" Three missed turns in a row: a computer player now takes over this seat for good.");
                }
                chat::post_system_message(state, id, &msg).await?;
                Some(power)
            }
        };
        bots::spawn_driver(state, id, takeover);
        Ok(())
    }
}

/// One sweep: handle every expired turn. Returns the number handled.
pub async fn sweep(state: &AppState) -> AppResult<usize> {
    let rows: Vec<(Uuid, Option<String>, i32)> = sqlx::query_as(
        "SELECT id, pending_power, round FROM games
         WHERE status=$1 AND turn_deadline IS NOT NULL AND turn_deadline < now() LIMIT 100",
    )
    .bind(GameStatus::InProgress)
    .fetch_all(&state.db)
    .await?;
    let mut n = 0;
    for (game_id, power, round) in rows {
        let expired = ExpiredTurn {
            game_id,
            current_power: power.unwrap_or_default(),
            round,
        };
        match state.timeout_handler.handle(state, expired).await {
            Ok(()) => n += 1,
            Err(e) => tracing::error!(error = ?e, game = %game_id, "timeout handler failed"),
        }
    }
    Ok(n)
}

/// Delete expired sessions and e-mail tokens. Returns the number of rows removed.
pub async fn purge_expired(state: &AppState) -> AppResult<u64> {
    let mut n = 0;
    for q in [
        "DELETE FROM sessions WHERE expires_at < now()",
        "DELETE FROM email_tokens WHERE expires_at < now()",
    ] {
        n += sqlx::query(q).execute(&state.db).await?.rows_affected();
    }
    Ok(n)
}

pub fn spawn(state: AppState) {
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(Duration::from_secs(60));
        loop {
            tick.tick().await;
            if let Err(e) = sweep(&state).await {
                tracing::error!(error = ?e, "timeout sweep failed");
            }
            if let Err(e) = purge_expired(&state).await {
                tracing::error!(error = ?e, "token purge failed");
            }
        }
    });
}

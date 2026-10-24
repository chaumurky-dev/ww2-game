use std::time::Duration;

use async_trait::async_trait;
use uuid::Uuid;

use crate::error::AppResult;
use crate::services::{chat, games};
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

/// Default behaviour: log, post a system message, skip the turn.
///
/// TODO(engine): instead of skipping, call `wartide-ai` to auto-play the turn for the absent
/// player (and for bot seats, play their turn here without any "timeout" message).
pub struct DefaultTimeoutHandler;

#[async_trait]
impl TurnTimeoutHandler for DefaultTimeoutHandler {
    async fn handle(&self, state: &AppState, e: ExpiredTurn) -> AppResult<()> {
        let seats = games::load_seats(state, e.game_id).await?;
        let power = wartide_protocol::game::Power::parse(&e.current_power);
        let seat = power.and_then(|p| seats.iter().find(|s| s.controls(p)));
        if seat.is_some_and(|s| s.is_bot) {
            // TODO(engine): bot turn -> ask the AI for actions and apply them via games::apply_action.
            tracing::info!(game = %e.game_id, power = %e.current_power, "bot turn due (AI not wired yet); ending turn");
            return games::advance_turn(state, e.game_id).await;
        }
        tracing::info!(game = %e.game_id, power = %e.current_power, "turn timed out");
        chat::post_system_message(
            state,
            e.game_id,
            &format!(
                "timeout: {} ran out of time in round {}; their turn was skipped.",
                e.current_power, e.round
            ),
        )
        .await?;
        games::advance_turn(state, e.game_id).await
    }
}

/// One sweep: handle every expired turn. Returns the number handled.
pub async fn sweep(state: &AppState) -> AppResult<usize> {
    let rows: Vec<(Uuid, Option<String>, i32)> = sqlx::query_as(
        "SELECT id, current_power, round FROM games
         WHERE status='in_progress' AND turn_deadline IS NOT NULL AND turn_deadline < now() LIMIT 100",
    )
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

pub fn spawn(state: AppState) {
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(Duration::from_secs(60));
        loop {
            tick.tick().await;
            if let Err(e) = sweep(&state).await {
                tracing::error!(error = ?e, "timeout sweep failed");
            }
        }
    });
}

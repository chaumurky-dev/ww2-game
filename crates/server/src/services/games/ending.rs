use uuid::Uuid;
use wartide_protocol::game::*;
use wartide_protocol::ws::ServerMessage;

use super::*;
use crate::error::{AppError, AppResult};
use crate::extract::CurrentUser;
use crate::services::{bots, chat, ratings};
use crate::state::AppState;

// ---------- ending ----------

/// Finish a game (idempotent). Applies ratings if the game is rated and a winner is known.
/// Resigned seats (including players replaced after repeated timeouts) always count as losses.
pub async fn finish_game(state: &AppState, id: Uuid, winner: Option<Team>) -> AppResult<()> {
    let row: Option<(Option<String>,)> = sqlx::query_as(
        "UPDATE games SET status='finished', winner_team=$2, turn_deadline=NULL, updated_at=now()
         WHERE id=$1 AND status='in_progress' RETURNING ladder",
    )
    .bind(id)
    .bind(winner.map(|t| enum_str(&t)))
    .fetch_optional(&state.db)
    .await?;
    let Some((ladder,)) = row else { return Ok(()) };
    state.runtime.forget(id);
    if let (Some(ladder), Some(winner)) = (ladder, winner) {
        let seats = load_seats(state, id).await?;
        let mut win = Vec::new();
        let mut lose = Vec::new();
        for s in &seats {
            let Some(u) = s.user_id else { continue };
            if s.resigned {
                lose.push(u)
            } else if !s.is_bot {
                if s.team() == winner {
                    win.push(u)
                } else {
                    lose.push(u)
                }
            }
        }
        ratings::apply_game_result(&state.db, &ladder, &[win, lose]).await?;
    }
    let line = match winner {
        Some(Team::Axis) => "Game over: the Axis win.",
        Some(Team::Allies) => "Game over: the Allies win.",
        None => "Game over: the game ended without a winner.",
    };
    if let Err(e) = chat::post_system_message(state, id, line).await {
        tracing::warn!(error = ?e, game = %id, "could not post game-over message");
    }
    state.hub.publish_game(
        id,
        ServerMessage::GameUpdated {
            game_id: GameId(id),
            seq: -1,
        },
    );
    Ok(())
}

/// A seated player resigns: a computer player ("brass") takes over all their powers and the game
/// continues. The seat is flagged `resigned`; at game end the player counts as a loss whatever the
/// outcome. If no active human is left the game ends at once and the other team wins.
pub async fn resign(state: &AppState, user: &CurrentUser, id: Uuid) -> AppResult<GameSummary> {
    {
        let _guard = state.runtime.lock(id).await;
        let (game, seats, mine) = require_participant(state, id, user.id).await?;
        if game.status != "in_progress" {
            return Err(AppError::conflict(
                "not_in_progress",
                "game is not in progress",
            ));
        }
        if mine.is_bot {
            return Err(AppError::conflict(
                "already_resigned",
                "you already left this game",
            ));
        }
        sqlx::query("UPDATE game_seats SET is_bot=TRUE, resigned=TRUE, bot_persona='brass' WHERE game_id=$1 AND seat=$2")
            .bind(id)
            .bind(mine.seat)
            .execute(&state.db)
            .await?;
        state
            .runtime
            .bots
            .lock()
            .unwrap()
            .retain(|(g, p), _| !(*g == id && mine.controls(*p)));
        let name = bots::persona_name("brass").unwrap_or("a computer player");
        let _ = chat::post_system_message(
            state,
            id,
            &format!(
                "{} resigned; {name} takes over.",
                user.display_name.as_str()
            ),
        )
        .await;
        let humans_left = seats
            .iter()
            .any(|s| s.seat != mine.seat && s.user_id.is_some() && !s.is_bot);
        if humans_left {
            // If it was their move the bot must act now rather than at the old deadline.
            sqlx::query(
                "UPDATE games SET turn_deadline=now(), updated_at=now() WHERE id=$1 AND pending_power = ANY($2)",
            )
            .bind(id)
            .bind(&mine.powers)
            .execute(&state.db)
            .await?;
        }
        state.hub.publish_game(
            id,
            ServerMessage::GameUpdated {
                game_id: GameId(id),
                seq: -1,
            },
        );
        if !humans_left {
            let winner = match mine.team() {
                Team::Axis => Team::Allies,
                Team::Allies => Team::Axis,
            };
            finish_game(state, id, Some(winner)).await?;
        }
    }
    bots::spawn_driver(state, id, None);
    summary(state, id, Some(user.id)).await
}

/// Host abandons a game (lobby or in progress). Never rated.
pub async fn abandon(state: &AppState, user: &CurrentUser, id: Uuid) -> AppResult<GameSummary> {
    let game = load_game(state, id).await?;
    if game.host_id != Some(user.id) {
        return Err(AppError::forbidden("only the host can abandon the game"));
    }
    if !matches!(game.status.as_str(), "lobby" | "in_progress") {
        return Err(AppError::conflict("already_over", "game is already over"));
    }
    sqlx::query(
        "UPDATE games SET status='abandoned', turn_deadline=NULL, updated_at=now() WHERE id=$1",
    )
    .bind(id)
    .execute(&state.db)
    .await?;
    state.runtime.forget(id);
    state.hub.publish_game(
        id,
        ServerMessage::GameUpdated {
            game_id: GameId(id),
            seq: -1,
        },
    );
    summary(state, id, Some(user.id)).await
}

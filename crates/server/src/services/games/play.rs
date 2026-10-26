use chrono::{DateTime, Duration, Utc};
use sqlx::types::Json;
use sqlx::PgConnection;
use uuid::Uuid;
use wartide_engine as engine;
use wartide_protocol::game::*;
use wartide_protocol::ws::ServerMessage;

use super::*;
use crate::error::{AppError, AppResult};
use crate::extract::CurrentUser;
use crate::services::transition::{transition, Notice, Transition};
use crate::services::{bots, notify, rules};
use crate::state::AppState;

/// Set `turn_deadline` for a freshly started turn. Bot turns are due immediately so the
/// background task picks them up.
pub async fn set_deadline(
    conn: &mut PgConnection,
    id: Uuid,
    timeout_secs: u32,
    bot: bool,
) -> AppResult<()> {
    let deadline = if bot {
        Utc::now()
    } else {
        Utc::now() + Duration::seconds(timeout_secs as i64)
    };
    sqlx::query("UPDATE games SET turn_deadline=$2 WHERE id=$1")
        .bind(id)
        .bind(deadline)
        .execute(&mut *conn)
        .await?;
    Ok(())
}

/// Tell the player whose turn it is (WS + optional email). Never fails the caller.
/// `email` is false when the same seat just moved on to another of its own powers: the player
/// still gets a `YourTurn` for the new power but no extra mail.
pub async fn notify_turn(state: &AppState, id: Uuid, email: bool) {
    let res: AppResult<()> = async {
        let g = load_game(state, id).await?;
        if g.status != "in_progress" {
            return Ok(());
        }
        let Some(power) = g.pending_power.as_deref().and_then(Power::parse) else {
            return Ok(());
        };
        let seats = load_seats(state, id).await?;
        let Some(seat) = seat_for_power(&seats, power) else {
            return Ok(());
        };
        let Some(uid) = seat.user_id.filter(|_| !seat.is_bot) else {
            return Ok(());
        };
        state.hub.publish_user(
            uid,
            ServerMessage::YourTurn {
                game_id: GameId(id),
                power: Some(power),
            },
        );
        if email {
            notify::email_your_turn(state, uid, &g.name, id, power, g.round).await?;
        }
        Ok(())
    }
    .await;
    if let Err(e) = res {
        tracing::warn!(error = ?e, game = %id, "failed to notify turn");
    }
}

/// Who is submitting an action.
#[derive(Debug, Clone, Copy)]
pub enum Actor {
    /// A human on this seat; the seat must control the pending power and not be bot-controlled.
    Human(i32),
    /// The server plays the pending power (computer player or timeout takeover).
    Bot,
    /// The server resolves a defender decision for a human seat from that player's battle
    /// preferences (no seat ownership check; never starts a clock or notifies).
    Auto,
}

/// Result of one applied action.
pub struct Applied {
    pub envelope: GameActionEnvelope,
    pub state: engine::GameState,
}

/// Submit an action as `user`: checks seat/turn ownership, applies it through the engine and
/// appends it to the log atomically, then lets the computer players continue.
pub async fn submit_action(
    state: &AppState,
    user: &CurrentUser,
    id: Uuid,
    action: engine::Action,
) -> AppResult<GameActionEnvelope> {
    let mine = require_seat(state, user, id).await?;
    let applied = {
        let _guard = state.runtime.lock(id).await;
        apply_locked(state, id, Actor::Human(mine.seat), action, None).await?
    };
    bots::spawn_driver(state, id, None);
    Ok(applied.envelope)
}

/// Core action pipeline shared by humans and computer players. The caller must hold the
/// per-game lock (`state.runtime.lock(id)`). `expect_seq` makes the call fail with a
/// `stale_state` conflict when the log moved on since the caller looked.
///
/// The acting power is always the one `pending_decision` names. Engine errors surface as 400
/// `illegal_action` carrying the engine's message.
pub async fn apply_locked(
    state: &AppState,
    id: Uuid,
    actor: Actor,
    action: engine::Action,
    expect_seq: Option<i64>,
) -> AppResult<Applied> {
    let mut tx = state.db.begin().await?;
    let game: GameRow = sqlx::query_as(&format!(
        "SELECT {GAME_COLS} FROM games WHERE id=$1 FOR UPDATE"
    ))
    .bind(id)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(|| AppError::not_found("game"))?;
    if game.status != "in_progress" {
        return Err(AppError::conflict(
            "not_in_progress",
            "game is not in progress",
        ));
    }
    let (seq, Json(mut gs)): (i64, Json<engine::GameState>) =
        sqlx::query_as("SELECT seq, state FROM game_states WHERE game_id=$1 FOR UPDATE")
            .bind(id)
            .fetch_one(&mut *tx)
            .await?;
    if expect_seq.is_some_and(|e| e != seq) {
        return Err(AppError::conflict(
            "stale_state",
            "the game moved on in the meantime",
        ));
    }
    let next_seq = seq + 1;
    let before = rules::pending_of(&gs);
    let power = before
        .power
        .ok_or_else(|| AppError::conflict("not_in_progress", "game is over"))?;
    let seats: Vec<SeatRow> = sqlx::query_as(SEAT_SELECT)
        .bind(id)
        .fetch_all(&mut *tx)
        .await?;
    let power_seat = seat_for_power(&seats, power)
        .ok_or_else(|| AppError::Internal(anyhow::anyhow!("no seat controls {power:?}")))?;
    if let Actor::Human(seat) = actor {
        if power_seat.seat != seat || power_seat.is_bot {
            return Err(AppError::conflict(
                "not_your_turn",
                format!("it is {}'s turn", power.as_str()),
            ));
        }
    }
    let events = rules::apply_typed(&mut gs, power, &action, game.seed, next_seq)?;
    let after = rules::pending_of(&gs);

    let created_at: DateTime<Utc> = sqlx::query_scalar(
        "INSERT INTO game_actions (game_id, seq, actor_seat, action, events) VALUES ($1,$2,$3,$4,$5) RETURNING created_at",
    )
    .bind(id)
    .bind(next_seq)
    .bind(power_seat.seat)
    .bind(Json(&action))
    .bind(Json(&events))
    .fetch_one(&mut *tx)
    .await?;
    sqlx::query("UPDATE game_states SET seq=$2, state=$3, updated_at=now() WHERE game_id=$1")
        .bind(id)
        .bind(next_seq)
        .bind(Json(&gs))
        .execute(&mut *tx)
        .await?;
    sqlx::query("UPDATE games SET round=$2, current_power=$3, phase=$4, pending_power=$5, updated_at=now() WHERE id=$1")
        .bind(id)
        .bind(gs.round as i32)
        .bind(gs.current_power.as_str())
        .bind(rules::phase_str(&gs))
        .bind(after.power.map(Power::as_str))
        .execute(&mut *tx)
        .await?;
    if let Actor::Human(seat) = actor {
        sqlx::query("UPDATE game_seats SET missed_in_row=0 WHERE game_id=$1 AND seat=$2 AND missed_in_row<>0")
            .bind(id)
            .bind(seat)
            .execute(&mut *tx)
            .await?;
    }

    // Control passes to another power: start its clock (bot turns are due immediately).
    let step = transition(&before, &after, power_seat, &seats);
    if let Transition::HandOff { bot, .. } = step {
        let settings: GameSettings =
            serde_json::from_value(game.settings.clone()).unwrap_or_default();
        set_deadline(&mut tx, id, settings.turn_timeout_secs, bot).await?;
    }
    tx.commit().await?;

    state.hub.publish_game(
        id,
        ServerMessage::GameUpdated {
            game_id: GameId(id),
            seq: next_seq,
        },
    );
    match step {
        Transition::GameOver(winner) => finish_game(state, id, winner).await?,
        Transition::HandOff {
            notice: Notice::InApp,
            ..
        } => notify_turn(state, id, false).await,
        Transition::HandOff {
            notice: Notice::Email,
            ..
        } => notify_turn(state, id, true).await,
        Transition::HandOff {
            notice: Notice::None,
            ..
        }
        | Transition::Same => {}
    }
    Ok(Applied {
        envelope: GameActionEnvelope {
            seq: next_seq,
            actor: Some(power_seat.seat as u8),
            action,
            events,
            created_at,
        },
        state: gs,
    })
}

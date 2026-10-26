use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::{FromRow, PgConnection};
use uuid::Uuid;
use wartide_protocol::game::*;

use crate::error::{AppError, AppResult};
use crate::extract::CurrentUser;
use crate::services::bots;
use crate::state::AppState;

// ---------- rows ----------

#[derive(FromRow, Clone, Debug)]
pub struct GameRow {
    pub id: Uuid,
    pub name: String,
    pub kind: String,
    pub mode: String,
    pub status: String,
    pub settings: Value,
    pub group_id: Option<Uuid>,
    pub host_id: Option<Uuid>,
    pub ladder: Option<String>,
    pub seed: i64,
    pub round: i32,
    pub current_power: Option<String>,
    pub phase: Option<String>,
    pub pending_power: Option<String>,
    pub turn_deadline: Option<DateTime<Utc>>,
    pub winner_team: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

pub(super) const GAME_COLS: &str = "id, name, kind, mode, status, settings, group_id, host_id, ladder, seed, round, current_power, phase, pending_power, turn_deadline, winner_team, created_at, updated_at";

#[derive(FromRow, Clone, Debug, Default)]
pub struct SeatRow {
    pub seat: i32,
    pub powers: Vec<String>,
    pub user_id: Option<Uuid>,
    pub is_bot: bool,
    pub bot_persona: Option<String>,
    pub resigned: bool,
    pub timeouts: i32,
    pub missed_in_row: i32,
    pub username: Option<String>,
    pub display_name: Option<String>,
    pub avatar: Option<String>,
}

impl SeatRow {
    pub fn powers(&self) -> Vec<Power> {
        self.powers.iter().filter_map(|p| Power::parse(p)).collect()
    }
    pub fn team(&self) -> Team {
        self.powers()
            .first()
            .map(|p| p.team())
            .unwrap_or(Team::Allies)
    }
    pub fn controls(&self, p: Power) -> bool {
        self.powers.iter().any(|x| x == p.as_str())
    }
    pub(super) fn to_assignment(&self) -> SeatAssignment {
        SeatAssignment {
            seat: self.seat as u8,
            powers: self.powers(),
            user: self.user_id.map(|id| UserRef {
                id,
                username: self.username.clone().unwrap_or_default(),
                display_name: self.display_name.clone().unwrap_or_default(),
                avatar: self.avatar.clone(),
            }),
            is_bot: self.is_bot,
            bot_persona: self.bot_persona.clone(),
            bot_name: self.bot_persona.as_deref().map(|p| {
                bots::persona_name(p)
                    .map(str::to_string)
                    .unwrap_or_else(|| p.to_string())
            }),
            resigned: self.resigned,
            timeouts: self.timeouts.max(0) as u32,
        }
    }
}

pub(super) const SEAT_SELECT: &str = "SELECT s.seat, s.powers, s.user_id, s.is_bot, s.bot_persona, s.resigned, s.timeouts, s.missed_in_row, u.username, u.display_name, u.avatar
    FROM game_seats s LEFT JOIN users u ON u.id = s.user_id WHERE s.game_id = $1 ORDER BY s.seat";

pub async fn load_game(state: &AppState, id: Uuid) -> AppResult<GameRow> {
    sqlx::query_as::<_, GameRow>(&format!("SELECT {GAME_COLS} FROM games WHERE id=$1"))
        .bind(id)
        .fetch_optional(&state.db)
        .await?
        .ok_or_else(|| AppError::not_found("game"))
}

pub async fn load_seats(state: &AppState, id: Uuid) -> AppResult<Vec<SeatRow>> {
    Ok(sqlx::query_as::<_, SeatRow>(SEAT_SELECT)
        .bind(id)
        .fetch_all(&state.db)
        .await?)
}

/// The seat that controls `power` (every power belongs to exactly one seat).
pub fn seat_for_power(seats: &[SeatRow], power: Power) -> Option<&SeatRow> {
    seats.iter().find(|s| s.controls(power))
}

/// The seat held by `user` in `game`, if any.
pub async fn seat_of(state: &AppState, game: Uuid, user: Uuid) -> AppResult<Option<SeatRow>> {
    Ok(load_seats(state, game)
        .await?
        .into_iter()
        .find(|s| s.user_id == Some(user)))
}

/// Load game + seats and require that `user` is seated (or, for public games, anyone may read).
pub async fn require_participant(
    state: &AppState,
    game: Uuid,
    user: Uuid,
) -> AppResult<(GameRow, Vec<SeatRow>, SeatRow)> {
    let g = load_game(state, game).await?;
    let seats = load_seats(state, game).await?;
    let mine = seats
        .iter()
        .find(|s| s.user_id == Some(user))
        .cloned()
        .ok_or_else(|| AppError::forbidden("you are not a player in this game"))?;
    Ok((g, seats, mine))
}

pub async fn seat_is_bot_for(conn: &mut PgConnection, game: Uuid, p: Power) -> AppResult<bool> {
    let r: Option<(bool,)> =
        sqlx::query_as("SELECT is_bot FROM game_seats WHERE game_id=$1 AND $2 = ANY(powers)")
            .bind(game)
            .bind(p.as_str())
            .fetch_optional(&mut *conn)
            .await?;
    Ok(r.map(|(b,)| b).unwrap_or(false))
}

/// The seat `user` occupies in game `id`, or 403.
pub async fn require_seat(state: &AppState, user: &CurrentUser, id: Uuid) -> AppResult<SeatRow> {
    load_seats(state, id)
        .await?
        .into_iter()
        .find(|s| s.user_id == Some(user.id))
        .ok_or_else(|| AppError::forbidden("you are not a player in this game"))
}

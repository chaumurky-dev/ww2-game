use chrono::{DateTime, Utc};
use sqlx::types::Json;
use sqlx::FromRow;
use uuid::Uuid;
use wartide_engine as engine;
use wartide_protocol::game::*;

use super::*;
use crate::error::{AppError, AppResult};
use crate::state::AppState;

pub async fn summary(state: &AppState, id: Uuid, viewer: Option<Uuid>) -> AppResult<GameSummary> {
    let g = load_game(state, id).await?;
    let seats = load_seats(state, id).await?;
    let my_seat = viewer.and_then(|v| {
        seats
            .iter()
            .find(|s| s.user_id == Some(v))
            .map(|s| s.seat as u8)
    });
    let is_host = viewer.is_some() && g.host_id == viewer;
    let invite_code = if is_host && g.status == GameStatus::Lobby {
        sqlx::query_as::<_, (String,)>("SELECT code FROM game_invites WHERE game_id=$1")
            .bind(id)
            .fetch_optional(&state.db)
            .await?
            .map(|(c,)| c)
    } else {
        None
    };
    Ok(GameSummary {
        id: GameId(g.id),
        name: g.name.clone(),
        kind: g.kind,
        mode: g.mode,
        status: g.status,
        settings: serde_json::from_value(g.settings.clone()).unwrap_or_default(),
        seats: seats.iter().map(SeatRow::to_assignment).collect(),
        round: g.round,
        current_power: g.current_power.as_deref().and_then(Power::parse),
        phase: g.phase.clone(),
        pending_power: g.pending_power.as_deref().and_then(Power::parse),
        turn_deadline: g.turn_deadline,
        group_id: g.group_id,
        host_id: g.host_id,
        invite_code,
        created_at: g.created_at,
        updated_at: g.updated_at,
        winner: g.winner_team.as_deref().and_then(Team::from_id),
        my_seat,
    })
}

/// Strip everything but name/mode/status/result from a summary shown on a public profile.
pub fn redact_for_profile(g: &mut GameSummary) {
    for seat in &mut g.seats {
        seat.user = None;
    }
    g.host_id = None;
    g.group_id = None;
    g.invite_code = None;
    g.my_seat = None;
}

/// Public games may be read by any logged-in user (spectating). Finished group games are also
/// readable by group members (see [`reader_access`]); finished private games only by their players.
pub fn is_spectatable(g: &GameRow) -> bool {
    g.kind == GameKind::Public
}

/// What a user may read of a game.
#[derive(Debug, Clone)]
pub enum Access {
    Player(SeatRow),
    Spectator,
}

async fn is_group_member(state: &AppState, group: Option<Uuid>, user: Uuid) -> AppResult<bool> {
    let Some(group) = group else {
        return Ok(false);
    };
    let row: Option<(i32,)> =
        sqlx::query_as("SELECT 1 FROM group_members WHERE group_id=$1 AND user_id=$2")
            .bind(group)
            .bind(user)
            .fetch_optional(&state.db)
            .await?;
    Ok(row.is_some())
}

/// Load the game and decide whether `user` may read it: seated players always, anyone else only
/// when the game is spectatable.
pub async fn reader_access(
    state: &AppState,
    game: Uuid,
    user: Uuid,
) -> AppResult<(GameRow, Access)> {
    let g = load_game(state, game).await?;
    let seats = load_seats(state, game).await?;
    match seats.into_iter().find(|s| s.user_id == Some(user)) {
        Some(mine) => Ok((g, Access::Player(mine))),
        None if is_spectatable(&g) => Ok((g, Access::Spectator)),
        None if g.status == GameStatus::Finished
            && is_group_member(state, g.group_id, user).await? =>
        {
            Ok((g, Access::Spectator))
        }
        None => Err(AppError::forbidden("you are not a player in this game")),
    }
}

/// Paginated list of public (matchmade) games for the "watch" page, newest first.
pub async fn list_public(
    state: &AppState,
    viewer: Uuid,
    finished: bool,
    page: i64,
    per_page: i64,
) -> AppResult<wartide_protocol::Paginated<GameSummary>> {
    let page = page.max(1);
    let status = if finished {
        GameStatus::Finished
    } else {
        GameStatus::InProgress
    };
    let (total,): (i64,) = sqlx::query_as("SELECT count(*) FROM games WHERE kind=$2 AND status=$1")
        .bind(status)
        .bind(GameKind::Public)
        .fetch_one(&state.db)
        .await?;
    let ids: Vec<(Uuid,)> = sqlx::query_as(
        "SELECT id FROM games WHERE kind=$4 AND status=$1
         ORDER BY created_at DESC, id LIMIT $2 OFFSET $3",
    )
    .bind(status)
    .bind(per_page)
    .bind((page - 1) * per_page)
    .bind(GameKind::Public)
    .fetch_all(&state.db)
    .await?;
    let mut items = Vec::with_capacity(ids.len());
    for (id,) in ids {
        items.push(summary(state, id, Some(viewer)).await?);
    }
    Ok(wartide_protocol::Paginated {
        items,
        total,
        page,
        per_page,
    })
}

/// Default title for a game that was created without one.
pub fn default_name(
    kind: GameKind,
    mode: GameMode,
    id: Uuid,
    public_number: Option<i64>,
) -> String {
    match kind {
        GameKind::Private => format!("Private game #{}", short_id(id)),
        GameKind::Public => {
            let m = match mode {
                GameMode::TwoPlayer => "2p",
                GameMode::FivePlayer => "5p",
                GameMode::Custom => "custom",
            };
            match public_number {
                Some(n) => format!("Ranked {m} #{n}"),
                None => format!("Ranked {m} #{}", short_id(id)),
            }
        }
    }
}

fn short_id(id: Uuid) -> String {
    id.simple().to_string()[..4].to_uppercase()
}

pub(super) fn clean_game_name(name: &str) -> AppResult<Option<String>> {
    let n = name.split_whitespace().collect::<Vec<_>>().join(" ");
    if n.is_empty() {
        return Ok(None);
    }
    if n.chars().count() > 60 {
        return Err(AppError::bad("game name too long (max 60 characters)"));
    }
    if n.chars().any(char::is_control) {
        return Err(AppError::bad("game name contains invalid characters"));
    }
    Ok(Some(n))
}

/// Summary for a viewer; private unfinished games are only visible to participants.
pub async fn summary_for_viewer(
    state: &AppState,
    id: Uuid,
    viewer: Uuid,
) -> AppResult<GameSummary> {
    reader_access(state, id, viewer).await?;
    summary(state, id, Some(viewer)).await
}

pub async fn list_mine(state: &AppState, user: Uuid, active: bool) -> AppResult<Vec<GameSummary>> {
    let statuses: &[GameStatus] = if active {
        &[GameStatus::Lobby, GameStatus::InProgress]
    } else {
        &[GameStatus::Finished, GameStatus::Abandoned]
    };
    let ids: Vec<(Uuid,)> = sqlx::query_as(
        "SELECT g.id FROM games g WHERE g.status = ANY($2)
         AND EXISTS (SELECT 1 FROM game_seats s WHERE s.game_id=g.id AND s.user_id=$1)
         ORDER BY g.updated_at DESC LIMIT 100",
    )
    .bind(user)
    .bind(statuses)
    .fetch_all(&state.db)
    .await?;
    let mut out = Vec::new();
    for (id,) in ids {
        out.push(summary(state, id, Some(user)).await?);
    }
    Ok(out)
}

// ---------- actions ----------

pub async fn actions_since(
    state: &AppState,
    user: Uuid,
    id: Uuid,
    since: i64,
) -> AppResult<Vec<GameActionEnvelope>> {
    reader_access(state, id, user).await?;
    #[derive(FromRow)]
    struct Rw {
        seq: i64,
        actor_seat: Option<i32>,
        action: Json<engine::Action>,
        events: Json<Vec<engine::Event>>,
        created_at: DateTime<Utc>,
    }
    let rows = sqlx::query_as::<_, Rw>(
        "SELECT seq, actor_seat, action, events, created_at FROM game_actions
         WHERE game_id=$1 AND seq > $2 ORDER BY seq LIMIT 1000",
    )
    .bind(id)
    .bind(since)
    .fetch_all(&state.db)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| GameActionEnvelope {
            seq: r.seq,
            actor: r.actor_seat.map(|s| s as u8),
            action: r.action.0,
            events: r.events.0,
            created_at: r.created_at,
        })
        .collect())
}

/// Everything in the engine state is public except the per-game dice seed, which would let a
/// client precompute every roll. Only the served copy is scrubbed; the stored state is untouched.
pub fn scrub_state(mut st: engine::GameState) -> engine::GameState {
    st.settings.seed = 0;
    st
}

pub async fn state_view(state: &AppState, user: Uuid, id: Uuid) -> AppResult<GameStateResponse> {
    reader_access(state, id, user).await?;
    let (seq, Json(st)): (i64, Json<engine::GameState>) =
        sqlx::query_as("SELECT seq, state FROM game_states WHERE game_id=$1")
            .bind(id)
            .fetch_optional(&state.db)
            .await?
            .ok_or_else(|| AppError::conflict("not_started", "game has not started"))?;
    Ok(GameStateResponse {
        seq,
        state: scrub_state(st),
    })
}

use rand::Rng;
use sqlx::types::Json;
use sqlx::PgConnection;
use uuid::Uuid;
use wartide_protocol::game::*;
use wartide_protocol::ws::ServerMessage;

use super::*;
use crate::error::{is_unique_violation, AppError, AppResult};
use crate::extract::CurrentUser;
use crate::services::{bots, groups, ratings, rules};
use crate::state::AppState;
use crate::util::{new_invite_code, normalize_invite_code};

// ---------- creation / joining ----------

#[allow(clippy::too_many_arguments)]
async fn insert_game(
    conn: &mut PgConnection,
    id: Uuid,
    name: &str,
    kind: GameKind,
    mode: GameMode,
    settings: &GameSettings,
    group_id: Option<Uuid>,
    host: Option<Uuid>,
    ladder: Option<&str>,
) -> AppResult<()> {
    let seed: i64 = rand::rng().random();
    sqlx::query(
        "INSERT INTO games (id, name, kind, mode, status, settings, group_id, host_id, ladder, seed)
         VALUES ($1,$9,$2,$3,$10,$4,$5,$6,$7,$8)",
    )
    .bind(id)
    .bind(kind)
    .bind(mode)
    .bind(serde_json::to_value(settings).map_err(|e| AppError::Internal(e.into()))?)
    .bind(group_id)
    .bind(host)
    .bind(ladder)
    .bind(seed)
    .bind(name)
    .bind(GameStatus::Lobby)
    .execute(&mut *conn)
    .await?;
    Ok(())
}

fn validate_settings(s: &GameSettings) -> AppResult<()> {
    if !(60..=60 * 60 * 24 * 14).contains(&s.turn_timeout_secs) {
        return Err(AppError::bad(
            "turn_timeout_secs must be between 60 seconds and 14 days",
        ));
    }
    for t in [s.axis_economic_victory, s.allied_economic_victory]
        .into_iter()
        .flatten()
    {
        if t == 0 || t > 500 {
            return Err(AppError::bad("economic victory threshold out of range"));
        }
    }
    Ok(())
}

/// Validate the seat specs of a create request (partition, one host, personas) and normalise them.
pub fn resolve_seats(req: &CreatePrivateGameRequest) -> AppResult<Vec<SeatSpec>> {
    let mut specs = req.seats.clone();
    let layout: Vec<Vec<Power>> = specs.iter().map(|s| s.powers.clone()).collect();
    check_partition(&layout).map_err(AppError::bad)?;
    if specs.iter().filter(|s| s.kind == SeatKind::Host).count() != 1 {
        return Err(AppError::bad("exactly one seat must be the host's"));
    }
    for (i, s) in specs.iter_mut().enumerate() {
        s.powers.sort();
        match (s.kind, s.persona.as_deref()) {
            (SeatKind::Bot, Some(p)) => s.persona = Some(bots::validate_id(p)?.to_string()),
            (SeatKind::Bot, None) => s.persona = Some(bots::default_for_seat(i).to_string()),
            (_, Some(_)) => return Err(AppError::bad("a persona was given for a non-bot seat")),
            (_, None) => {}
        }
    }
    Ok(specs)
}

pub async fn create_private(
    state: &AppState,
    host: &CurrentUser,
    req: CreatePrivateGameRequest,
) -> AppResult<GameSummary> {
    validate_settings(&req.settings)?;
    let specs = resolve_seats(&req)?;
    let layout: Vec<Vec<Power>> = specs.iter().map(|s| s.powers.clone()).collect();
    let mode = GameMode::derive(&layout);
    let name = req
        .name
        .as_deref()
        .map(clean_game_name)
        .transpose()?
        .flatten();
    let humans = specs.iter().filter(|s| s.kind != SeatKind::Bot).count();
    // Only the classic layouts are rated; custom layouts never are, not even in a group.
    let ladder = match req.group_id {
        Some(g) => {
            if groups::role_of(state, g, host.id).await?.is_none() {
                return Err(AppError::forbidden("you are not a member of that group"));
            }
            match mode {
                GameMode::Custom => None,
                m => Some(ratings::ladder_for(Some(g), m == GameMode::TwoPlayer)),
            }
        }
        None => None,
    };
    let id = Uuid::new_v4();
    let mut tx = state.db.begin().await?;
    let name = name.unwrap_or_else(|| default_name(GameKind::Private, mode, id, None));
    insert_game(
        &mut tx,
        id,
        &name,
        GameKind::Private,
        mode,
        &req.settings,
        req.group_id,
        Some(host.id),
        ladder.as_deref(),
    )
    .await?;
    for (i, spec) in specs.iter().enumerate() {
        let is_bot = spec.kind == SeatKind::Bot;
        sqlx::query("INSERT INTO game_seats (game_id, seat, powers, user_id, is_bot, bot_persona) VALUES ($1,$2,$3,$4,$5,$6)")
            .bind(id)
            .bind(i as i32)
            .bind(spec.powers.iter().map(|p| p.as_str().to_string()).collect::<Vec<_>>())
            .bind((spec.kind == SeatKind::Host).then_some(host.id))
            .bind(is_bot)
            .bind(if is_bot { spec.persona.clone() } else { None })
            .execute(&mut *tx)
            .await?;
    }
    // invite code (retry on the astronomically unlikely collision)
    for _ in 0..5 {
        match sqlx::query("INSERT INTO game_invites (code, game_id) VALUES ($1,$2)")
            .bind(new_invite_code())
            .bind(id)
            .execute(&mut *tx)
            .await
        {
            Ok(_) => break,
            Err(e) if is_unique_violation(&e) => continue,
            Err(e) => return Err(e.into()),
        }
    }
    tx.commit().await?;
    if humans == 1 {
        begin_game(state, id).await?;
    }
    summary(state, id, Some(host.id)).await
}

pub async fn join_by_invite(
    state: &AppState,
    user: &CurrentUser,
    code: &str,
) -> AppResult<GameSummary> {
    let row: Option<(Uuid,)> = sqlx::query_as("SELECT game_id FROM game_invites WHERE code=$1")
        .bind(normalize_invite_code(code))
        .fetch_optional(&state.db)
        .await?;
    let (gid,) = row.ok_or_else(|| AppError::not_found("invite code"))?;

    let mut tx = state.db.begin().await?;
    let game: GameRow = sqlx::query_as(&format!(
        "SELECT {GAME_COLS} FROM games WHERE id=$1 FOR UPDATE"
    ))
    .bind(gid)
    .fetch_one(&mut *tx)
    .await?;
    if game.status != GameStatus::Lobby {
        return Err(AppError::conflict(
            "game_started",
            "this game is no longer accepting players",
        ));
    }
    if let Some(g) = game.group_id {
        if groups::role_of(state, g, user.id).await?.is_none() {
            return Err(AppError::forbidden(
                "this game belongs to a group; join the group first",
            ));
        }
    }
    let seats: Vec<(i32, Option<Uuid>, bool)> = sqlx::query_as(
        "SELECT seat, user_id, is_bot FROM game_seats WHERE game_id=$1 ORDER BY seat",
    )
    .bind(gid)
    .fetch_all(&mut *tx)
    .await?;
    if seats.iter().any(|(_, u, _)| *u == Some(user.id)) {
        return Err(AppError::conflict(
            "already_joined",
            "you are already in this game",
        ));
    }
    let free = seats
        .iter()
        .find(|(_, u, bot)| u.is_none() && !bot)
        .map(|(s, _, _)| *s)
        .ok_or_else(|| AppError::conflict("game_full", "this game is full"))?;
    sqlx::query("UPDATE game_seats SET user_id=$3 WHERE game_id=$1 AND seat=$2")
        .bind(gid)
        .bind(free)
        .bind(user.id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("UPDATE games SET updated_at=now() WHERE id=$1")
        .bind(gid)
        .execute(&mut *tx)
        .await?;
    let full = !seats
        .iter()
        .any(|(s, u, bot)| *s != free && u.is_none() && !bot);
    tx.commit().await?;
    if full {
        begin_game(state, gid).await?;
    } else {
        state.hub.publish_game(
            gid,
            ServerMessage::GameUpdated {
                game_id: GameId(gid),
                seq: 0,
            },
        );
    }
    summary(state, gid, Some(user.id)).await
}

/// Host starts the game early; empty human seats become bots.
pub async fn start(state: &AppState, host: &CurrentUser, id: Uuid) -> AppResult<GameSummary> {
    let game = load_game(state, id).await?;
    if game.host_id != Some(host.id) {
        return Err(AppError::forbidden("only the host can start the game"));
    }
    if game.status != GameStatus::Lobby {
        return Err(AppError::conflict("game_started", "game already started"));
    }
    sqlx::query(
        "UPDATE game_seats SET is_bot=TRUE,
            bot_persona = (ARRAY['brass','frost','kestrel','volkov','tide','thistle'])[(seat % 6) + 1]
         WHERE game_id=$1 AND user_id IS NULL AND NOT is_bot",
    )
    .bind(id)
    .execute(&state.db)
    .await?;
    begin_game(state, id).await?;
    summary(state, id, Some(host.id)).await
}

/// Transition lobby -> in progress: initial engine state, first turn, notifications, bots.
pub async fn begin_game(state: &AppState, id: Uuid) -> AppResult<()> {
    let game = load_game(state, id).await?;
    let settings: GameSettings = serde_json::from_value(game.settings.clone()).unwrap_or_default();
    let initial = state.rules.initial_state(&settings, game.seed);
    let first = rules::pending_of(&initial).power.unwrap_or(Power::Ussr);
    let mut tx = state.db.begin().await?;
    let n = sqlx::query("UPDATE games SET status=$5, round=$2, current_power=$3, pending_power=$3, phase=$4, updated_at=now() WHERE id=$1 AND status=$6")
        .bind(id)
        .bind(initial.round as i32)
        .bind(initial.current_power.as_str())
        .bind(rules::phase_str(&initial))
        .bind(GameStatus::InProgress)
        .bind(GameStatus::Lobby)
        .execute(&mut *tx)
        .await?
        .rows_affected();
    if n == 0 {
        return Ok(()); // already started by a concurrent request
    }
    sqlx::query("INSERT INTO game_states (game_id, seq, state) VALUES ($1, 0, $2)")
        .bind(id)
        .bind(Json(&initial))
        .execute(&mut *tx)
        .await?;
    let first_is_bot = seat_is_bot_for(&mut tx, id, first).await?;
    set_deadline(&mut tx, id, settings.turn_timeout_secs, first_is_bot).await?;
    tx.commit().await?;
    state.hub.publish_game(
        id,
        ServerMessage::GameUpdated {
            game_id: GameId(id),
            seq: 0,
        },
    );
    if first_is_bot {
        bots::spawn_driver(state, id, None);
    } else {
        notify_turn(state, id, true).await;
    }
    Ok(())
}

// ---------- public games (matchmaking) ----------

/// Create and immediately start a public rated game with `players` (already shuffled into seat order).
pub async fn create_public(state: &AppState, mode: GameMode, players: &[Uuid]) -> AppResult<Uuid> {
    if mode == GameMode::Custom {
        return Err(AppError::bad("public games use a fixed layout"));
    }
    let layout = preset_layout(mode.seat_count());
    debug_assert_eq!(layout.len(), players.len());
    let id = Uuid::new_v4();
    let settings = GameSettings {
        turn_timeout_secs: state.config.default_turn_timeout_secs,
        ..GameSettings::default()
    };
    let ladder = ratings::ladder_for(None, mode == GameMode::TwoPlayer);
    let (number,): (i64,) = sqlx::query_as("SELECT nextval('public_game_number')")
        .fetch_one(&state.db)
        .await?;
    let name = default_name(GameKind::Public, mode, id, Some(number));
    let mut tx = state.db.begin().await?;
    insert_game(
        &mut tx,
        id,
        &name,
        GameKind::Public,
        mode,
        &settings,
        None,
        None,
        Some(&ladder),
    )
    .await?;
    for (i, (powers, user)) in layout.iter().zip(players).enumerate() {
        sqlx::query("INSERT INTO game_seats (game_id, seat, powers, user_id) VALUES ($1,$2,$3,$4)")
            .bind(id)
            .bind(i as i32)
            .bind(
                powers
                    .iter()
                    .map(|p| p.as_str().to_string())
                    .collect::<Vec<_>>(),
            )
            .bind(user)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    begin_game(state, id).await?;
    Ok(id)
}

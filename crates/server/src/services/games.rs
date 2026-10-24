use chrono::{DateTime, Duration, Utc};
use rand::Rng;
use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::Value;
use sqlx::{FromRow, PgConnection};
use uuid::Uuid;
use wartide_protocol::game::*;
use wartide_protocol::ws::ServerMessage;

use crate::error::{is_unique_violation, AppError, AppResult};
use crate::extract::CurrentUser;
use crate::services::{groups, ratings, rules};
use crate::state::AppState;
use crate::util::{new_invite_code, normalize_invite_code};

pub const BOT_PERSONAS: [&str; 5] = [
    "Colonel Brass",
    "General Frost",
    "Admiral Kestrel",
    "Marshal Volkov",
    "Commodore Tide",
];

// ---------- pure helpers ----------

pub fn enum_str<T: Serialize>(t: &T) -> String {
    match serde_json::to_value(t) {
        Ok(Value::String(s)) => s,
        _ => String::new(),
    }
}

pub fn parse_enum<T: DeserializeOwned>(s: &str) -> AppResult<T> {
    serde_json::from_value(Value::String(s.to_string()))
        .map_err(|_| AppError::Internal(anyhow::anyhow!("bad enum value in db: {s}")))
}

/// Which powers each seat controls. 2p: seat 0 = Axis, seat 1 = Allies. 5p: one power per seat
/// in turn order.
pub fn seat_layout(mode: GameMode) -> Vec<Vec<Power>> {
    match mode {
        GameMode::TwoPlayer => vec![
            vec![Power::Germany, Power::Japan],
            vec![Power::Ussr, Power::Uk, Power::Usa],
        ],
        GameMode::FivePlayer => Power::ALL.iter().map(|p| vec![*p]).collect(),
    }
}

/// Turn order of the powers (Classic: USSR, Germany, UK, Japan, USA).
pub fn next_power(current: Power) -> (Power, bool) {
    let i = Power::ALL.iter().position(|p| *p == current).unwrap_or(0);
    let n = (i + 1) % Power::ALL.len();
    (Power::ALL[n], n == 0)
}

pub fn game_over_winner(events: &Value) -> Option<Team> {
    events.as_array()?.iter().find_map(|e| {
        if e.get("type")?.as_str()? == "game_over" {
            match e.get("winner")?.as_str()? {
                "axis" => Some(Team::Axis),
                "allies" => Some(Team::Allies),
                _ => None,
            }
        } else {
            None
        }
    })
}

// ---------- rows ----------

#[derive(FromRow, Clone, Debug)]
pub struct GameRow {
    pub id: Uuid,
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
    pub turn_deadline: Option<DateTime<Utc>>,
    pub winner_team: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

const GAME_COLS: &str = "id, kind, mode, status, settings, group_id, host_id, ladder, seed, round, current_power, turn_deadline, winner_team, created_at, updated_at";

#[derive(FromRow, Clone, Debug)]
pub struct SeatRow {
    pub seat: i32,
    pub powers: Vec<String>,
    pub user_id: Option<Uuid>,
    pub is_bot: bool,
    pub bot_persona: Option<String>,
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
    fn to_assignment(&self) -> SeatAssignment {
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
        }
    }
}

const SEAT_SELECT: &str = "SELECT s.seat, s.powers, s.user_id, s.is_bot, s.bot_persona, u.username, u.display_name, u.avatar
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
    let invite_code = if is_host && g.status == "lobby" {
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
        kind: parse_enum(&g.kind)?,
        mode: parse_enum(&g.mode)?,
        status: parse_enum(&g.status)?,
        settings: serde_json::from_value(g.settings.clone()).unwrap_or_default(),
        seats: seats.iter().map(SeatRow::to_assignment).collect(),
        round: g.round,
        current_power: g.current_power.as_deref().and_then(Power::parse),
        turn_deadline: g.turn_deadline,
        group_id: g.group_id,
        host_id: g.host_id,
        invite_code,
        created_at: g.created_at,
        updated_at: g.updated_at,
        my_seat,
    })
}

/// Summary for a viewer; non-public games are only visible to participants.
pub async fn summary_for_viewer(
    state: &AppState,
    id: Uuid,
    viewer: Uuid,
) -> AppResult<GameSummary> {
    let s = summary(state, id, Some(viewer)).await?;
    if s.my_seat.is_none() && s.kind != GameKind::Public {
        return Err(AppError::forbidden("you are not a player in this game"));
    }
    Ok(s)
}

pub async fn list_mine(state: &AppState, user: Uuid, active: bool) -> AppResult<Vec<GameSummary>> {
    let statuses: &[&str] = if active {
        &["lobby", "in_progress"]
    } else {
        &["finished", "abandoned"]
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

// ---------- creation / joining ----------

#[allow(clippy::too_many_arguments)]
async fn insert_game(
    conn: &mut PgConnection,
    id: Uuid,
    kind: GameKind,
    mode: GameMode,
    settings: &GameSettings,
    group_id: Option<Uuid>,
    host: Option<Uuid>,
    ladder: Option<&str>,
) -> AppResult<()> {
    let seed: i64 = rand::rng().random();
    sqlx::query(
        "INSERT INTO games (id, kind, mode, status, settings, group_id, host_id, ladder, seed)
         VALUES ($1,$2,$3,'lobby',$4,$5,$6,$7,$8)",
    )
    .bind(id)
    .bind(enum_str(&kind))
    .bind(enum_str(&mode))
    .bind(serde_json::to_value(settings).map_err(|e| AppError::Internal(e.into()))?)
    .bind(group_id)
    .bind(host)
    .bind(ladder)
    .bind(seed)
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
    if s.economic_victory_threshold == 0 || s.economic_victory_threshold > 500 {
        return Err(AppError::bad("economic_victory_threshold out of range"));
    }
    Ok(())
}

pub async fn create_private(
    state: &AppState,
    host: &CurrentUser,
    req: CreatePrivateGameRequest,
) -> AppResult<GameSummary> {
    validate_settings(&req.settings)?;
    let layout = seat_layout(req.mode);
    let n = layout.len();
    let mut bots = req.bot_seats.clone();
    bots.sort_unstable();
    bots.dedup();
    if bots.iter().any(|b| *b as usize >= n) {
        return Err(AppError::bad("bot seat index out of range"));
    }
    let humans = n - bots.len();
    if humans == 0 {
        return Err(AppError::bad("a game needs at least one human seat"));
    }
    if let Some(h) = req.human_seats {
        if h as usize != humans {
            return Err(AppError::bad(
                "human_seats does not match the number of non-bot seats",
            ));
        }
    }
    let ladder = match req.group_id {
        Some(g) => {
            if groups::role_of(state, g, host.id).await?.is_none() {
                return Err(AppError::forbidden("you are not a member of that group"));
            }
            Some(ratings::ladder_for(
                Some(g),
                req.mode == GameMode::TwoPlayer,
            ))
        }
        None => None,
    };
    let id = Uuid::new_v4();
    let host_seat = (0..n as u8).find(|i| !bots.contains(i)).unwrap_or(0);
    let mut tx = state.db.begin().await?;
    insert_game(
        &mut tx,
        id,
        GameKind::Private,
        req.mode,
        &req.settings,
        req.group_id,
        Some(host.id),
        ladder.as_deref(),
    )
    .await?;
    for (i, powers) in layout.iter().enumerate() {
        let i = i as u8;
        let is_bot = bots.contains(&i);
        sqlx::query("INSERT INTO game_seats (game_id, seat, powers, user_id, is_bot, bot_persona) VALUES ($1,$2,$3,$4,$5,$6)")
            .bind(id)
            .bind(i as i32)
            .bind(powers.iter().map(|p| p.as_str().to_string()).collect::<Vec<_>>())
            .bind(if i == host_seat { Some(host.id) } else { None })
            .bind(is_bot)
            .bind(is_bot.then(|| BOT_PERSONAS[i as usize % BOT_PERSONAS.len()].to_string()))
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
    if game.status != "lobby" {
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
    if game.status != "lobby" {
        return Err(AppError::conflict("game_started", "game already started"));
    }
    sqlx::query(
        "UPDATE game_seats SET is_bot=TRUE,
            bot_persona = (ARRAY['Colonel Brass','General Frost','Admiral Kestrel','Marshal Volkov','Commodore Tide'])[(seat % 5) + 1]
         WHERE game_id=$1 AND user_id IS NULL AND NOT is_bot",
    )
    .bind(id)
    .execute(&state.db)
    .await?;
    begin_game(state, id).await?;
    summary(state, id, Some(host.id)).await
}

/// Transition lobby -> in progress: initial engine state, first turn, notifications.
pub async fn begin_game(state: &AppState, id: Uuid) -> AppResult<()> {
    let game = load_game(state, id).await?;
    let mode: GameMode = parse_enum(&game.mode)?;
    let settings: GameSettings = serde_json::from_value(game.settings.clone()).unwrap_or_default();
    let initial = state.rules.initial_state(mode, &settings, game.seed);
    let mut tx = state.db.begin().await?;
    let n = sqlx::query("UPDATE games SET status='in_progress', round=1, current_power='ussr', updated_at=now() WHERE id=$1 AND status='lobby'")
        .bind(id)
        .execute(&mut *tx)
        .await?
        .rows_affected();
    if n == 0 {
        return Ok(()); // already started by a concurrent request
    }
    sqlx::query("INSERT INTO game_states (game_id, seq, state) VALUES ($1, 0, $2)")
        .bind(id)
        .bind(initial)
        .execute(&mut *tx)
        .await?;
    let first_is_bot = seat_is_bot_for(&mut tx, id, Power::Ussr).await?;
    set_deadline(&mut tx, id, settings.turn_timeout_secs, first_is_bot).await?;
    tx.commit().await?;
    state.hub.publish_game(
        id,
        ServerMessage::GameUpdated {
            game_id: GameId(id),
            seq: 0,
        },
    );
    notify_turn(state, id).await;
    Ok(())
}

async fn seat_is_bot_for(conn: &mut PgConnection, game: Uuid, p: Power) -> AppResult<bool> {
    let r: Option<(bool,)> =
        sqlx::query_as("SELECT is_bot FROM game_seats WHERE game_id=$1 AND $2 = ANY(powers)")
            .bind(game)
            .bind(p.as_str())
            .fetch_optional(&mut *conn)
            .await?;
    Ok(r.map(|(b,)| b).unwrap_or(false))
}

/// Set `turn_deadline` for a freshly started turn. Bot turns are due immediately so the
/// background task picks them up.
async fn set_deadline(
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
pub async fn notify_turn(state: &AppState, id: Uuid) {
    let res: AppResult<()> = async {
        let g = load_game(state, id).await?;
        if g.status != "in_progress" {
            return Ok(());
        }
        let Some(power) = g.current_power.as_deref().and_then(Power::parse) else { return Ok(()) };
        let seats = load_seats(state, id).await?;
        let Some(seat) = seats.iter().find(|s| s.controls(power)) else { return Ok(()) };
        let Some(uid) = seat.user_id else { return Ok(()) };
        state.hub.publish_user(uid, ServerMessage::YourTurn { game_id: GameId(id), power: Some(power) });
        let u: Option<(String, String, bool, bool)> = sqlx::query_as(
            "SELECT email, username, email_turn_notifications, email_verified FROM users WHERE id=$1",
        )
        .bind(uid)
        .fetch_optional(&state.db)
        .await?;
        if let Some((email, username, true, true)) = u {
            state.mailer.send(
                &email,
                "It's your turn in Wartide",
                &format!(
                    "Hi {username},\n\nIt is your turn ({power:?}, round {}).\n{}/games/{id}\n\nYou can switch these emails off in your settings.",
                    g.round,
                    state.config.base_url.trim_end_matches('/'),
                ),
            );
        }
        Ok(())
    }
    .await;
    if let Err(e) = res {
        tracing::warn!(error = ?e, game = %id, "failed to notify turn");
    }
}

// ---------- actions ----------

pub async fn actions_since(
    state: &AppState,
    user: Uuid,
    id: Uuid,
    since: i64,
) -> AppResult<Vec<GameActionEnvelope>> {
    require_participant(state, id, user).await?;
    #[derive(FromRow)]
    struct Rw {
        seq: i64,
        actor_seat: Option<i32>,
        action: Value,
        events: Value,
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
            action: r.action,
            events: r.events,
            created_at: r.created_at,
        })
        .collect())
}

pub async fn state_view(state: &AppState, user: Uuid, id: Uuid) -> AppResult<GameStateResponse> {
    require_participant(state, id, user).await?;
    let (seq, st): (i64, Value) =
        sqlx::query_as("SELECT seq, state FROM game_states WHERE game_id=$1")
            .bind(id)
            .fetch_optional(&state.db)
            .await?
            .ok_or_else(|| AppError::conflict("not_started", "game has not started"))?;
    // TODO(engine): project per-viewer fog (hidden info) once the engine defines it.
    Ok(GameStateResponse { seq, state: st })
}

/// Submit an action as `user`. Validates seat/turn ownership, runs the rules hook and appends
/// to the log atomically.
pub async fn submit_action(
    state: &AppState,
    user: &CurrentUser,
    id: Uuid,
    action: Value,
) -> AppResult<GameActionEnvelope> {
    if !action.is_object() {
        return Err(AppError::bad("action must be a JSON object"));
    }
    let seats = load_seats(state, id).await?;
    let mine = seats
        .iter()
        .find(|s| s.user_id == Some(user.id))
        .ok_or_else(|| AppError::forbidden("you are not a player in this game"))?;
    apply_action(state, id, Some(mine.seat), action).await
}

/// Core action pipeline, shared by humans (`actor_seat = Some`) and system/AI play (`None` or a bot seat).
/// When `actor_seat` is `Some`, that seat must control the current power.
pub async fn apply_action(
    state: &AppState,
    id: Uuid,
    actor_seat: Option<i32>,
    action: Value,
) -> AppResult<GameActionEnvelope> {
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
    let current = game
        .current_power
        .as_deref()
        .and_then(Power::parse)
        .ok_or_else(|| {
            AppError::Internal(anyhow::anyhow!("in-progress game without current power"))
        })?;
    if let Some(seat) = actor_seat {
        let powers: (Vec<String>,) =
            sqlx::query_as("SELECT powers FROM game_seats WHERE game_id=$1 AND seat=$2")
                .bind(id)
                .bind(seat)
                .fetch_one(&mut *tx)
                .await?;
        if !powers.0.iter().any(|p| p == current.as_str()) {
            return Err(AppError::conflict(
                "not_your_turn",
                format!("it is {current:?}'s turn"),
            ));
        }
    }
    let (seq, cur_state): (i64, Value) =
        sqlx::query_as("SELECT seq, state FROM game_states WHERE game_id=$1 FOR UPDATE")
            .bind(id)
            .fetch_one(&mut *tx)
            .await?;
    let next_seq = seq + 1;

    // TODO(engine): the Rules impl should validate legality and return typed errors.
    let (new_state, events) = state
        .rules
        .apply(&cur_state, &action, rules::action_seed(game.seed, next_seq))
        .map_err(|e| AppError::BadRequest("illegal_action", e.to_string()))?;

    let created_at: DateTime<Utc> = sqlx::query_scalar(
        "INSERT INTO game_actions (game_id, seq, actor_seat, action, events) VALUES ($1,$2,$3,$4,$5) RETURNING created_at",
    )
    .bind(id)
    .bind(next_seq)
    .bind(actor_seat)
    .bind(&action)
    .bind(&events)
    .fetch_one(&mut *tx)
    .await?;
    sqlx::query("UPDATE game_states SET seq=$2, state=$3, updated_at=now() WHERE game_id=$1")
        .bind(id)
        .bind(next_seq)
        .bind(&new_state)
        .execute(&mut *tx)
        .await?;
    sqlx::query("UPDATE games SET updated_at=now() WHERE id=$1")
        .bind(id)
        .execute(&mut *tx)
        .await?;

    let winner = game_over_winner(&events);
    let ends_turn = action.get("type").and_then(Value::as_str) == Some("end_turn");
    if winner.is_none() && ends_turn {
        // TODO(engine): derive the next power/round from the engine state instead of this convention.
        advance_turn_tx(&mut tx, &game, current).await?;
    }
    tx.commit().await?;

    state.hub.publish_game(
        id,
        ServerMessage::GameUpdated {
            game_id: GameId(id),
            seq: next_seq,
        },
    );
    if let Some(team) = winner {
        finish_game(state, id, Some(team)).await?;
    } else if ends_turn {
        notify_turn(state, id).await;
    }
    Ok(GameActionEnvelope {
        seq: next_seq,
        actor: actor_seat.map(|s| s as u8),
        action,
        events,
        created_at,
    })
}

async fn advance_turn_tx(tx: &mut PgConnection, game: &GameRow, current: Power) -> AppResult<()> {
    let (next, new_round) = next_power(current);
    let settings: GameSettings = serde_json::from_value(game.settings.clone()).unwrap_or_default();
    sqlx::query("UPDATE games SET current_power=$2, round=round+$3 WHERE id=$1")
        .bind(game.id)
        .bind(next.as_str())
        .bind(i32::from(new_round))
        .execute(&mut *tx)
        .await?;
    let bot = seat_is_bot_for(tx, game.id, next).await?;
    set_deadline(tx, game.id, settings.turn_timeout_secs, bot).await
}

/// Skip the current power's turn (used by the timeout handler).
pub async fn advance_turn(state: &AppState, id: Uuid) -> AppResult<()> {
    let mut tx = state.db.begin().await?;
    let game: GameRow = sqlx::query_as(&format!(
        "SELECT {GAME_COLS} FROM games WHERE id=$1 FOR UPDATE"
    ))
    .bind(id)
    .fetch_one(&mut *tx)
    .await?;
    if game.status != "in_progress" {
        return Ok(());
    }
    let Some(current) = game.current_power.as_deref().and_then(Power::parse) else {
        return Ok(());
    };
    advance_turn_tx(&mut tx, &game, current).await?;
    sqlx::query("UPDATE games SET updated_at=now() WHERE id=$1")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    state.hub.publish_game(
        id,
        ServerMessage::GameUpdated {
            game_id: GameId(id),
            seq: -1,
        },
    );
    notify_turn(state, id).await;
    Ok(())
}

// ---------- ending ----------

/// Finish a game (idempotent). Applies ratings if the game is rated and a winner is known.
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
    if let (Some(ladder), Some(winner)) = (ladder, winner) {
        let seats = load_seats(state, id).await?;
        let mut win = Vec::new();
        let mut lose = Vec::new();
        for s in seats.iter().filter(|s| !s.is_bot) {
            if let Some(u) = s.user_id {
                if s.team() == winner {
                    win.push(u)
                } else {
                    lose.push(u)
                }
            }
        }
        // in 5p several seats can share a team; a user holds at most one seat
        ratings::apply_game_result(&state.db, &ladder, &[win, lose]).await?;
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

/// A seated player concedes: their team loses (rated if the game is rated).
pub async fn resign(state: &AppState, user: &CurrentUser, id: Uuid) -> AppResult<GameSummary> {
    let (game, _seats, mine) = require_participant(state, id, user.id).await?;
    if game.status != "in_progress" {
        return Err(AppError::conflict(
            "not_in_progress",
            "game is not in progress",
        ));
    }
    let winner = match mine.team() {
        Team::Axis => Team::Allies,
        Team::Allies => Team::Axis,
    };
    finish_game(state, id, Some(winner)).await?;
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
    state.hub.publish_game(
        id,
        ServerMessage::GameUpdated {
            game_id: GameId(id),
            seq: -1,
        },
    );
    summary(state, id, Some(user.id)).await
}

// ---------- public games (matchmaking) ----------

/// Create and immediately start a public rated game with `players` (already shuffled into seat order).
pub async fn create_public(state: &AppState, mode: GameMode, players: &[Uuid]) -> AppResult<Uuid> {
    let layout = seat_layout(mode);
    debug_assert_eq!(layout.len(), players.len());
    let id = Uuid::new_v4();
    let settings = GameSettings {
        turn_timeout_secs: state.config.default_turn_timeout_secs,
        ..GameSettings::default()
    };
    let ladder = ratings::ladder_for(None, mode == GameMode::TwoPlayer);
    let mut tx = state.db.begin().await?;
    insert_game(
        &mut tx,
        id,
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn layouts() {
        assert_eq!(seat_layout(GameMode::TwoPlayer).len(), 2);
        let five = seat_layout(GameMode::FivePlayer);
        assert_eq!(five.len(), 5);
        assert_eq!(five[0], vec![Power::Ussr]);
        let two = seat_layout(GameMode::TwoPlayer);
        let total: usize = two.iter().map(Vec::len).sum();
        assert_eq!(total, 5);
        assert!(two[0].iter().all(|p| p.team() == Team::Axis));
    }

    #[test]
    fn turn_order_wraps() {
        assert_eq!(next_power(Power::Ussr), (Power::Germany, false));
        assert_eq!(next_power(Power::Usa), (Power::Ussr, true));
    }

    #[test]
    fn detects_game_over() {
        assert_eq!(
            game_over_winner(&json!([{"type":"game_over","winner":"axis"}])),
            Some(Team::Axis)
        );
        assert_eq!(game_over_winner(&json!([])), None);
        assert_eq!(game_over_winner(&json!([{"type":"x"}])), None);
    }

    #[test]
    fn enum_roundtrip() {
        assert_eq!(enum_str(&GameMode::TwoPlayer), "two_player");
        assert_eq!(
            parse_enum::<GameStatus>("in_progress").unwrap(),
            GameStatus::InProgress
        );
        assert!(parse_enum::<GameStatus>("nope").is_err());
    }
}

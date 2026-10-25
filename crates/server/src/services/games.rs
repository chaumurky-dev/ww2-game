use chrono::{DateTime, Duration, Utc};
use rand::Rng;
use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::Value;
use sqlx::{FromRow, PgConnection};
use uuid::Uuid;
use wartide_engine as engine;
use wartide_protocol::game::*;
use wartide_protocol::ws::ServerMessage;

use crate::error::{is_unique_violation, AppError, AppResult};
use crate::extract::CurrentUser;
use crate::services::{bots, chat, groups, notify, ratings, rules};
use crate::state::AppState;
use crate::util::{new_invite_code, normalize_invite_code};

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
        // stored per game in `game_seats`; there is no fixed layout
        GameMode::Custom => Vec::new(),
    }
}

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

const GAME_COLS: &str = "id, name, kind, mode, status, settings, group_id, host_id, ladder, seed, round, current_power, phase, pending_power, turn_deadline, winner_team, created_at, updated_at";

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

const SEAT_SELECT: &str = "SELECT s.seat, s.powers, s.user_id, s.is_bot, s.bot_persona, s.resigned, s.timeouts, s.missed_in_row, u.username, u.display_name, u.avatar
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
        name: g.name.clone(),
        kind: parse_enum(&g.kind)?,
        mode: parse_enum(&g.mode)?,
        status: parse_enum(&g.status)?,
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
        winner: g.winner_team.as_deref().and_then(|t| parse_enum(t).ok()),
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

/// Public games and finished games may be read by any logged-in user (spectating).
pub fn is_spectatable(g: &GameRow) -> bool {
    g.kind == "public" || g.status == "finished"
}

/// What a user may read of a game.
#[derive(Debug, Clone)]
pub enum Access {
    Player(SeatRow),
    Spectator,
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
    let status = if finished { "finished" } else { "in_progress" };
    let (total,): (i64,) =
        sqlx::query_as("SELECT count(*) FROM games WHERE kind='public' AND status=$1")
            .bind(status)
            .fetch_one(&state.db)
            .await?;
    let ids: Vec<(Uuid,)> = sqlx::query_as(
        "SELECT id FROM games WHERE kind='public' AND status=$1
         ORDER BY created_at DESC, id LIMIT $2 OFFSET $3",
    )
    .bind(status)
    .bind(per_page)
    .bind((page - 1) * per_page)
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

fn clean_game_name(name: &str) -> AppResult<Option<String>> {
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
         VALUES ($1,$9,$2,$3,'lobby',$4,$5,$6,$7,$8)",
    )
    .bind(id)
    .bind(enum_str(&kind))
    .bind(enum_str(&mode))
    .bind(serde_json::to_value(settings).map_err(|e| AppError::Internal(e.into()))?)
    .bind(group_id)
    .bind(host)
    .bind(ladder)
    .bind(seed)
    .bind(name)
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

/// Turn a create request into validated seat specs: either the general `seats` model or the
/// legacy `mode/bot_seats/human_seats/bot_personas` fields mapped onto it.
pub fn resolve_seats(req: &CreatePrivateGameRequest) -> AppResult<Vec<SeatSpec>> {
    let mut specs = match &req.seats {
        Some(seats) => seats.clone(),
        None => legacy_seats(req)?,
    };
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
            (_, Some(_)) => return Err(AppError::bad("bot_personas given for a non-bot seat")),
            (_, None) => {}
        }
    }
    Ok(specs)
}

fn legacy_seats(req: &CreatePrivateGameRequest) -> AppResult<Vec<SeatSpec>> {
    if req.mode == GameMode::Custom {
        return Err(AppError::bad("custom games need an explicit seats list"));
    }
    let layout = seat_layout(req.mode);
    let n = layout.len();
    let mut bots = req.bot_seats.clone();
    bots.sort_unstable();
    bots.dedup();
    if bots.iter().any(|b| *b as usize >= n) {
        return Err(AppError::bad("bot seat index out of range"));
    }
    if req.bot_personas.len() > n {
        return Err(AppError::bad("bot_personas has more entries than seats"));
    }
    for (i, p) in req.bot_personas.iter().enumerate() {
        if p.is_some() && !bots.contains(&(i as u8)) {
            return Err(AppError::bad("bot_personas given for a non-bot seat"));
        }
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
    let host_seat = (0..n as u8).find(|i| !bots.contains(i)).unwrap_or(0);
    Ok(layout
        .into_iter()
        .enumerate()
        .map(|(i, powers)| {
            let i = i as u8;
            let kind = if bots.contains(&i) {
                SeatKind::Bot
            } else if i == host_seat {
                SeatKind::Host
            } else {
                SeatKind::Open
            };
            SeatSpec {
                powers,
                kind,
                persona: req.bot_personas.get(i as usize).cloned().flatten(),
            }
        })
        .collect())
}

pub async fn create_private(
    state: &AppState,
    host: &CurrentUser,
    req: CreatePrivateGameRequest,
) -> AppResult<GameSummary> {
    validate_settings(&req.settings)?;
    let specs = resolve_seats(&req)?;
    let layout: Vec<Vec<Power>> = specs.iter().map(|s| s.powers.clone()).collect();
    let mode = if req.seats.is_some() {
        GameMode::derive(&layout)
    } else {
        req.mode
    };
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
    let initial_json = rules::to_json(&initial)?;
    let mut tx = state.db.begin().await?;
    let n = sqlx::query("UPDATE games SET status='in_progress', round=$2, current_power=$3, pending_power=$3, phase=$4, updated_at=now() WHERE id=$1 AND status='lobby'")
        .bind(id)
        .bind(initial.round as i32)
        .bind(rules::power_from_engine(initial.current_power).as_str())
        .bind(rules::phase_str(&initial))
        .execute(&mut *tx)
        .await?
        .rows_affected();
    if n == 0 {
        return Ok(()); // already started by a concurrent request
    }
    sqlx::query("INSERT INTO game_states (game_id, seq, state) VALUES ($1, 0, $2)")
        .bind(id)
        .bind(initial_json)
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

pub async fn seat_is_bot_for(conn: &mut PgConnection, game: Uuid, p: Power) -> AppResult<bool> {
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
    reader_access(state, id, user).await?;
    let (seq, st): (i64, Value) =
        sqlx::query_as("SELECT seq, state FROM game_states WHERE game_id=$1")
            .bind(id)
            .fetch_optional(&state.db)
            .await?
            .ok_or_else(|| AppError::conflict("not_started", "game has not started"))?;
    // All information is public in this game: the stored engine state is returned as is.
    Ok(GameStateResponse { seq, state: st })
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
    action: Value,
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
    let (seq, cur_state): (i64, Value) =
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
    let mut gs = rules::parse_state(&cur_state)?;
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
    let parsed = rules::parse_action(&action)?;
    let events = rules::apply_typed(&mut gs, power, &parsed, game.seed, next_seq)?;
    let events = rules::to_json(&events)?;
    let new_state = rules::to_json(&gs)?;
    let after = rules::pending_of(&gs);

    let created_at: DateTime<Utc> = sqlx::query_scalar(
        "INSERT INTO game_actions (game_id, seq, actor_seat, action, events) VALUES ($1,$2,$3,$4,$5) RETURNING created_at",
    )
    .bind(id)
    .bind(next_seq)
    .bind(power_seat.seat)
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
    sqlx::query("UPDATE games SET round=$2, current_power=$3, phase=$4, pending_power=$5, updated_at=now() WHERE id=$1")
        .bind(id)
        .bind(gs.round as i32)
        .bind(rules::power_from_engine(gs.current_power).as_str())
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
    let mut notify = false;
    let mut notify_email = false;
    if let (Some(next), true) = (after.power, after.power != before.power) {
        let next_seat = seat_for_power(&seats, next);
        let bot = next_seat.is_some_and(|s| s.is_bot);
        // A human's defender decision is resolved by the driver within moments: no clock and no
        // "your turn" mail. Handing control back after one keeps the turn owner's running clock.
        let human_defender_next = after.defender && !bot;
        let human_defender_done = before.defender && !power_seat.is_bot;
        if !human_defender_next && !human_defender_done {
            let settings: GameSettings =
                serde_json::from_value(game.settings.clone()).unwrap_or_default();
            set_deadline(&mut tx, id, settings.turn_timeout_secs, bot).await?;
            notify = !bot;
            notify_email = notify && next_seat.map(|s| s.seat) != Some(power_seat.seat);
        }
    }
    tx.commit().await?;

    state.hub.publish_game(
        id,
        ServerMessage::GameUpdated {
            game_id: GameId(id),
            seq: next_seq,
        },
    );
    if let Some(winner) = after.game_over {
        finish_game(state, id, winner).await?;
    } else if notify {
        notify_turn(state, id, notify_email).await;
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

#[cfg(test)]
mod tests {
    use super::*;

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

    fn seats_for(mode: GameMode) -> Vec<SeatRow> {
        seat_layout(mode)
            .into_iter()
            .enumerate()
            .map(|(i, powers)| SeatRow {
                seat: i as i32,
                powers: powers.iter().map(|p| p.as_str().to_string()).collect(),
                ..SeatRow::default()
            })
            .collect()
    }

    #[test]
    fn seat_controls_powers_two_player() {
        let seats = seats_for(GameMode::TwoPlayer);
        for p in [Power::Germany, Power::Japan] {
            assert_eq!(seat_for_power(&seats, p).unwrap().seat, 0);
        }
        for p in [Power::Ussr, Power::Uk, Power::Usa] {
            assert_eq!(seat_for_power(&seats, p).unwrap().seat, 1);
        }
        assert_eq!(seats[0].team(), Team::Axis);
        assert_eq!(seats[1].team(), Team::Allies);
    }

    #[test]
    fn seat_controls_powers_five_player() {
        let seats = seats_for(GameMode::FivePlayer);
        for (i, p) in Power::ALL.into_iter().enumerate() {
            assert_eq!(seat_for_power(&seats, p).unwrap().seat, i as i32);
            assert!(seats.iter().filter(|s| s.controls(p)).count() == 1);
        }
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

use chrono::{DateTime, Utc};
use sqlx::FromRow;
use uuid::Uuid;
use wartide_protocol::chat::*;
use wartide_protocol::game::*;
use wartide_protocol::ws::ServerMessage;

use crate::error::{AppError, AppResult};
use crate::extract::CurrentUser;
use crate::services::games::{self, enum_str, SeatRow};
use crate::state::AppState;

const MAX_BODY: usize = 1000;

fn team_str(t: Team) -> String {
    enum_str(&t)
}

/// What a viewer (a seat) is allowed to see in a game's chat.
#[derive(Debug, Clone)]
pub struct Viewer {
    pub team: Team,
    pub powers: Vec<Power>,
    /// Spectators only see the `all` channel.
    pub spectator: bool,
}

impl Viewer {
    pub fn from_seat(s: &SeatRow) -> Self {
        Viewer {
            team: s.team(),
            powers: s.powers(),
            spectator: false,
        }
    }

    pub fn spectator() -> Self {
        Viewer {
            team: Team::Allies,
            powers: Vec::new(),
            spectator: true,
        }
    }

    pub fn from_access(a: &games::Access) -> Self {
        match a {
            games::Access::Player(s) => Viewer::from_seat(s),
            games::Access::Spectator => Viewer::spectator(),
        }
    }
}

/// Pure visibility rule used for REST reads and WS fan-out.
pub fn visible_to(viewer: &Viewer, channel: ChatChannel, scope: Option<&str>) -> bool {
    if viewer.spectator {
        return channel == ChatChannel::All;
    }
    match channel {
        ChatChannel::All | ChatChannel::Lobby => true,
        ChatChannel::Team => scope == Some(team_str(viewer.team).as_str()),
        ChatChannel::Power => scope.is_some_and(|s| viewer.powers.iter().any(|p| p.as_str() == s)),
    }
}

/// Resolve the scope string for a message sent by `seat`.
pub fn resolve_scope(
    seat: &SeatRow,
    channel: ChatChannel,
    power: Option<Power>,
) -> AppResult<Option<String>> {
    match channel {
        ChatChannel::All | ChatChannel::Lobby => Ok(None),
        ChatChannel::Team => Ok(Some(team_str(seat.team()))),
        ChatChannel::Power => {
            let p = match power {
                Some(p) if seat.controls(p) => p,
                Some(_) => return Err(AppError::forbidden("you do not control that power")),
                None => *seat
                    .powers()
                    .first()
                    .ok_or_else(|| AppError::bad("seat has no powers"))?,
            };
            Ok(Some(p.as_str().to_string()))
        }
    }
}

#[derive(FromRow)]
struct MsgRow {
    id: Uuid,
    game_id: Uuid,
    channel: String,
    scope: Option<String>,
    sender_kind: String,
    sender_name: Option<String>,
    body: String,
    created_at: DateTime<Utc>,
    u_id: Option<Uuid>,
    u_username: Option<String>,
    u_display_name: Option<String>,
    u_avatar: Option<String>,
}

impl MsgRow {
    fn into_message(self) -> AppResult<ChatMessage> {
        let sender = match (self.sender_kind.as_str(), self.u_id) {
            ("user", Some(id)) => ChatSender::User(UserRef {
                id,
                username: self.u_username.unwrap_or_default(),
                display_name: self.u_display_name.unwrap_or_default(),
                avatar: self.u_avatar,
            }),
            ("bot", _) => ChatSender::Bot {
                name: self.sender_name.unwrap_or_else(|| "Bot".into()),
            },
            _ => ChatSender::System,
        };
        Ok(ChatMessage {
            id: self.id,
            game_id: GameId(self.game_id),
            channel: games::parse_enum(&self.channel)?,
            scope: self.scope,
            sender,
            body: self.body,
            created_at: self.created_at,
        })
    }
}

pub async fn history(
    state: &AppState,
    user: Uuid,
    game: Uuid,
    channel: Option<ChatChannel>,
) -> AppResult<Vec<ChatMessage>> {
    let (_g, access) = games::reader_access(state, game, user).await?;
    let v = Viewer::from_access(&access);
    let channel = if v.spectator {
        match channel {
            None | Some(ChatChannel::All) => Some(ChatChannel::All),
            Some(_) => {
                return Err(AppError::forbidden(
                    "spectators can only read the public chat channel",
                ))
            }
        }
    } else {
        channel
    };
    let powers: Vec<String> = v.powers.iter().map(|p| p.as_str().to_string()).collect();
    let rows = sqlx::query_as::<_, MsgRow>(
        "SELECT * FROM (
           SELECT m.id, m.game_id, m.channel, m.scope, m.sender_kind, m.sender_name, m.body, m.created_at,
                  u.id AS u_id, u.username AS u_username, u.display_name AS u_display_name, u.avatar AS u_avatar
           FROM chat_messages m LEFT JOIN users u ON u.id = m.sender_user_id
           WHERE m.game_id = $1
             AND ($2::text IS NULL OR m.channel = $2)
             AND (m.channel IN ('all','lobby')
                  OR (m.channel = 'team' AND m.scope = $3)
                  OR (m.channel = 'power' AND m.scope = ANY($4)))
           ORDER BY m.created_at DESC LIMIT 200
         ) t ORDER BY created_at ASC",
    )
    .bind(game)
    .bind(channel.map(|c| c.as_str()))
    .bind(team_str(v.team))
    .bind(&powers)
    .fetch_all(&state.db)
    .await?;
    rows.into_iter().map(MsgRow::into_message).collect()
}

fn clean_body(body: &str) -> AppResult<String> {
    let b = body.trim();
    if b.is_empty() {
        return Err(AppError::bad("message is empty"));
    }
    if b.chars().count() > MAX_BODY {
        return Err(AppError::bad(format!(
            "message too long (max {MAX_BODY} characters)"
        )));
    }
    Ok(b.to_string())
}

#[allow(clippy::too_many_arguments)]
async fn insert(
    state: &AppState,
    game: Uuid,
    channel: ChatChannel,
    scope: Option<String>,
    sender: ChatSender,
    body: String,
) -> AppResult<ChatMessage> {
    let (kind, uid, name) = match &sender {
        ChatSender::User(u) => ("user", Some(u.id), None),
        ChatSender::Bot { name } => ("bot", None, Some(name.clone())),
        ChatSender::System => ("system", None, None),
    };
    let id = Uuid::new_v4();
    let created_at: DateTime<Utc> = sqlx::query_scalar(
        "INSERT INTO chat_messages (id, game_id, channel, scope, sender_kind, sender_user_id, sender_name, body)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8) RETURNING created_at",
    )
    .bind(id)
    .bind(game)
    .bind(channel.as_str())
    .bind(&scope)
    .bind(kind)
    .bind(uid)
    .bind(name)
    .bind(&body)
    .fetch_one(&state.db)
    .await?;
    let msg = ChatMessage {
        id,
        game_id: GameId(game),
        channel,
        scope,
        sender,
        body,
        created_at,
    };
    // WS handlers filter per-connection with `visible_to` before forwarding.
    state.hub.publish_game(
        game,
        ServerMessage::ChatReceived {
            message: msg.clone(),
        },
    );
    Ok(msg)
}

pub async fn send(
    state: &AppState,
    user: &CurrentUser,
    game: Uuid,
    req: SendChatRequest,
) -> AppResult<ChatMessage> {
    let (_g, _seats, mine) = games::require_participant(state, game, user.id).await?;
    let body = clean_body(&req.body)?;
    let scope = resolve_scope(&mine, req.channel, req.power)?;
    insert(
        state,
        game,
        req.channel,
        scope,
        ChatSender::User(user.to_user_ref()),
        body,
    )
    .await
}

/// Server-side helper for computer players (and the AI commentary hook): post as a named bot.
pub async fn post_bot_message(
    state: &AppState,
    game: Uuid,
    bot_name: &str,
    channel: ChatChannel,
    scope: Option<String>,
    body: &str,
) -> AppResult<ChatMessage> {
    insert(
        state,
        game,
        channel,
        scope,
        ChatSender::Bot {
            name: bot_name.to_string(),
        },
        clean_body(body)?,
    )
    .await
}

/// System notices (timeouts, resignations, ...), posted to the `all` channel.
pub async fn post_system_message(
    state: &AppState,
    game: Uuid,
    body: &str,
) -> AppResult<ChatMessage> {
    insert(
        state,
        game,
        ChatChannel::All,
        None,
        ChatSender::System,
        clean_body(body)?,
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seat(powers: &[&str]) -> SeatRow {
        SeatRow {
            seat: 0,
            powers: powers.iter().map(|s| s.to_string()).collect(),
            user_id: None,
            is_bot: false,
            bot_persona: None,
            username: None,
            display_name: None,
            avatar: None,
            ..SeatRow::default()
        }
    }

    #[test]
    fn visibility_rules() {
        let axis_seat = Viewer::from_seat(&seat(&["germany", "japan"]));
        let uk_seat = Viewer::from_seat(&seat(&["uk"]));
        assert!(visible_to(&axis_seat, ChatChannel::All, None));
        assert!(visible_to(&axis_seat, ChatChannel::Team, Some("axis")));
        assert!(!visible_to(&uk_seat, ChatChannel::Team, Some("axis")));
        assert!(visible_to(&uk_seat, ChatChannel::Team, Some("allies")));
        assert!(visible_to(&axis_seat, ChatChannel::Power, Some("japan")));
        assert!(!visible_to(&uk_seat, ChatChannel::Power, Some("japan")));
        assert!(!visible_to(&uk_seat, ChatChannel::Power, None));
    }

    #[test]
    fn spectators_only_see_all() {
        let v = Viewer::spectator();
        assert!(visible_to(&v, ChatChannel::All, None));
        assert!(!visible_to(&v, ChatChannel::Team, Some("allies")));
        assert!(!visible_to(&v, ChatChannel::Power, Some("uk")));
        assert!(!visible_to(&v, ChatChannel::Lobby, None));
    }

    #[test]
    fn scope_resolution() {
        let s = seat(&["germany", "japan"]);
        assert_eq!(
            resolve_scope(&s, ChatChannel::Team, None)
                .unwrap()
                .as_deref(),
            Some("axis")
        );
        assert_eq!(
            resolve_scope(&s, ChatChannel::Power, Some(Power::Japan))
                .unwrap()
                .as_deref(),
            Some("japan")
        );
        assert_eq!(
            resolve_scope(&s, ChatChannel::Power, None)
                .unwrap()
                .as_deref(),
            Some("germany")
        );
        assert!(resolve_scope(&s, ChatChannel::Power, Some(Power::Usa)).is_err());
        assert_eq!(resolve_scope(&s, ChatChannel::All, None).unwrap(), None);
    }

    #[test]
    fn body_validation() {
        assert!(clean_body("  ").is_err());
        assert_eq!(clean_body(" hi ").unwrap(), "hi");
        assert!(clean_body(&"x".repeat(1001)).is_err());
    }
}

//! `/ws`: authenticated by the session cookie. Clients subscribe to game ids; the server pushes
//! game updates and (visibility-filtered) chat, plus per-user `YourTurn` / `MatchFound`.
use std::collections::HashMap;
use std::time::Duration;

use axum::extract::ws::rejection::WebSocketUpgradeRejection;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::State;
use axum::http::HeaderMap;
use axum::response::Response;
use futures_util::{SinkExt, StreamExt};
use tokio::sync::{broadcast, mpsc};
use tokio::task::JoinHandle;
use uuid::Uuid;
use wartide_protocol::ws::{ClientMessage, ServerMessage};

use crate::error::{AppError, AppResult};
use crate::extract::{cookie_value, user_from_token, SESSION_COOKIE};
use crate::services::chat::{visible_to, Viewer};
use crate::services::games;
use crate::state::AppState;

/// Largest accepted client message / frame.
const MAX_WS_BYTES: usize = 16 * 1024;
/// Most game subscriptions one socket may hold.
const MAX_SUBSCRIPTIONS: usize = 50;
/// How often a socket re-checks that its session still exists.
const REVALIDATE_EVERY: Duration = Duration::from_secs(300);

/// `scheme://host[:port]` of a URL, lowercased (what a browser sends as `Origin`).
fn origin_of(url: &str) -> Option<String> {
    let (scheme, rest) = url.trim().split_once("://")?;
    let authority = rest.split(['/', '?', '#']).next()?;
    if scheme.is_empty() || authority.is_empty() {
        return None;
    }
    Some(format!("{scheme}://{authority}").to_ascii_lowercase())
}

/// Browsers always send `Origin` on a WebSocket handshake and cookies ride along, so without this
/// check any web page could open an authenticated socket (cross-site WebSocket hijacking).
fn origin_allowed(state: &AppState, headers: &HeaderMap) -> bool {
    let Some(origin) = headers
        .get(axum::http::header::ORIGIN)
        .and_then(|v| v.to_str().ok())
        .and_then(origin_of)
    else {
        return false;
    };
    origin_of(&state.config.base_url).is_some_and(|o| o == origin)
        || state
            .config
            .allowed_origins
            .iter()
            .filter_map(|o| origin_of(o))
            .any(|o| o == origin)
}

pub async fn handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    ws: Result<WebSocketUpgrade, WebSocketUpgradeRejection>,
) -> AppResult<Response> {
    if !origin_allowed(&state, &headers) {
        return Err(AppError::forbidden("origin not allowed"));
    }
    let token = cookie_value(&headers, SESSION_COOKIE).ok_or(AppError::Unauthorized)?;
    let user = user_from_token(&state, &token)
        .await?
        .ok_or(AppError::Unauthorized)?;
    let ws = match ws {
        Ok(ws) => ws,
        Err(rej) => return Ok(axum::response::IntoResponse::into_response(rej)),
    };
    Ok(ws
        .max_message_size(MAX_WS_BYTES)
        .max_frame_size(MAX_WS_BYTES)
        .on_upgrade(move |socket| run(state, user.id, token, socket)))
}

fn forward(
    mut rx: broadcast::Receiver<ServerMessage>,
    out: mpsc::Sender<ServerMessage>,
    viewer: Option<Viewer>,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        loop {
            match rx.recv().await {
                Ok(msg) => {
                    if let (ServerMessage::ChatReceived { message }, Some(v)) = (&msg, &viewer) {
                        if !visible_to(v, message.channel, message.scope.as_deref()) {
                            continue;
                        }
                    }
                    if out.send(msg).await.is_err() {
                        break;
                    }
                }
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    tracing::warn!(skipped = n, "ws subscriber lagged");
                }
                Err(broadcast::error::RecvError::Closed) => break,
            }
        }
    })
}

async fn run(state: AppState, user_id: Uuid, token: String, socket: WebSocket) {
    let (mut sink, mut stream) = socket.split();
    let (out_tx, mut out_rx) = mpsc::channel::<ServerMessage>(128);

    let writer = tokio::spawn(async move {
        while let Some(msg) = out_rx.recv().await {
            let Ok(text) = serde_json::to_string(&msg) else {
                continue;
            };
            if sink.send(Message::Text(text.into())).await.is_err() {
                break;
            }
        }
    });

    let mut subs: HashMap<Uuid, JoinHandle<()>> = HashMap::new();
    let user_task = forward(state.hub.subscribe_user(user_id), out_tx.clone(), None);

    let mut revalidate = tokio::time::interval(REVALIDATE_EVERY);
    revalidate.tick().await; // the first tick fires immediately
    loop {
        let frame = tokio::select! {
            f = stream.next() => match f {
                Some(Ok(f)) => f,
                _ => break,
            },
            _ = revalidate.tick() => {
                match user_from_token(&state, &token).await {
                    Ok(Some(u)) if u.id == user_id => continue,
                    Ok(_) => break, // logged out, expired or deleted
                    Err(_) => continue, // transient DB error: try again next time
                }
            }
        };
        let text = match frame {
            Message::Text(t) => t,
            Message::Close(_) => break,
            _ => continue,
        };
        let reply = match serde_json::from_str::<ClientMessage>(&text) {
            Ok(ClientMessage::Subscribe { game_id }) => {
                if subs.len() >= MAX_SUBSCRIPTIONS && !subs.contains_key(&game_id.0) {
                    let reply = ServerMessage::Error {
                        code: "too_many_subscriptions".into(),
                        message: format!(
                            "at most {MAX_SUBSCRIPTIONS} subscriptions per connection"
                        ),
                    };
                    if out_tx.send(reply).await.is_err() {
                        break;
                    }
                    continue;
                }
                match games::reader_access(&state, game_id.0, user_id).await {
                    Ok((_, access)) => {
                        // subscribe before acknowledging so no update is missed
                        let rx = state.hub.subscribe_game(game_id.0);
                        let h = forward(rx, out_tx.clone(), Some(Viewer::from_access(&access)));
                        if let Some(old) = subs.insert(game_id.0, h) {
                            old.abort();
                        }
                        ServerMessage::Subscribed { game_id }
                    }
                    Err(e) => ServerMessage::Error {
                        code: e.code().into(),
                        message: e.to_string(),
                    },
                }
            }
            Ok(ClientMessage::Unsubscribe { game_id }) => {
                if let Some(h) = subs.remove(&game_id.0) {
                    h.abort();
                }
                continue;
            }
            Ok(ClientMessage::Ping) => ServerMessage::Pong,
            Err(e) => ServerMessage::Error {
                code: "bad_message".into(),
                message: e.to_string(),
            },
        };
        if out_tx.send(reply).await.is_err() {
            break;
        }
    }

    user_task.abort();
    for (_, h) in subs {
        h.abort();
    }
    drop(out_tx);
    writer.abort();
    state.hub.gc();
}

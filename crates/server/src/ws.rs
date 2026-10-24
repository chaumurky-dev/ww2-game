//! `/ws`: authenticated by the session cookie. Clients subscribe to game ids; the server pushes
//! game updates and (visibility-filtered) chat, plus per-user `YourTurn` / `MatchFound`.
use std::collections::HashMap;

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

pub async fn handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    ws: WebSocketUpgrade,
) -> AppResult<Response> {
    let token = cookie_value(&headers, SESSION_COOKIE).ok_or(AppError::Unauthorized)?;
    let user = user_from_token(&state, &token)
        .await?
        .ok_or(AppError::Unauthorized)?;
    Ok(ws.on_upgrade(move |socket| run(state, user.id, socket)))
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

async fn run(state: AppState, user_id: Uuid, socket: WebSocket) {
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

    while let Some(Ok(frame)) = stream.next().await {
        let text = match frame {
            Message::Text(t) => t,
            Message::Close(_) => break,
            _ => continue,
        };
        let reply = match serde_json::from_str::<ClientMessage>(&text) {
            Ok(ClientMessage::Subscribe { game_id }) => {
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

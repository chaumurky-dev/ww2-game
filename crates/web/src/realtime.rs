//! WebSocket client (`/ws`): reconnects with back-off while logged in, resubscribes to game
//! channels, and turns `ServerMessage`s into signals / toasts.
use crate::state::{AuthState, Toasts};
use futures::channel::mpsc::{unbounded, UnboundedSender};
use futures::{FutureExt, SinkExt, StreamExt};
use gloo_net::websocket::{futures::WebSocket, Message};
use gloo_timers::future::TimeoutFuture;
use leptos::prelude::*;
use leptos::task::spawn_local;
use wartide_protocol::chat::ChatMessage;
use wartide_protocol::game::GameId;
use wartide_protocol::ws::{ClientMessage, ServerMessage};

#[derive(Clone, Copy)]
pub struct Realtime {
    tx: StoredValue<Option<UnboundedSender<ClientMessage>>>,
    subs: StoredValue<Vec<GameId>>,
    /// Bumped on anything that may change the lobby list (`game_updated`, `your_turn`, `match_found`).
    pub lobby_tick: RwSignal<u32>,
    /// Bumped on `game_updated` / `your_turn`; game pages refetch their summary.
    pub game_tick: RwSignal<u32>,
    /// Latest chat message pushed by the server.
    pub chat_in: RwSignal<Option<ChatMessage>>,
}

fn ws_url() -> String {
    let loc = web_sys::window().map(|w| w.location());
    let secure = loc.as_ref().and_then(|l| l.protocol().ok()).as_deref() == Some("https:");
    let host = loc.and_then(|l| l.host().ok()).unwrap_or_default();
    format!("{}://{host}/ws", if secure { "wss" } else { "ws" })
}

fn encode(m: &ClientMessage) -> Message {
    Message::Text(serde_json::to_string(m).unwrap_or_default())
}

impl Realtime {
    pub fn provide() -> Self {
        let rt = Realtime {
            tx: StoredValue::new(None),
            subs: StoredValue::new(vec![]),
            lobby_tick: RwSignal::new(0),
            game_tick: RwSignal::new(0),
            chat_in: RwSignal::new(None),
        };
        provide_context(rt);
        let auth = AuthState::expect();
        let started = StoredValue::new(false);
        Effect::new(move |_| {
            if auth.user.with(|u| u.is_some()) && !started.get_value() {
                started.set_value(true);
                rt.run(auth, Toasts::expect());
            }
        });
        rt
    }

    pub fn expect() -> Self {
        expect_context::<Realtime>()
    }

    pub fn subscribe(&self, id: GameId) {
        self.subs.update_value(|s| {
            if !s.contains(&id) {
                s.push(id)
            }
        });
        self.send(ClientMessage::Subscribe { game_id: id });
    }

    pub fn unsubscribe(&self, id: GameId) {
        self.subs.update_value(|s| s.retain(|g| *g != id));
        self.send(ClientMessage::Unsubscribe { game_id: id });
    }

    fn send(&self, m: ClientMessage) {
        if let Some(tx) = self.tx.get_value() {
            let _ = tx.unbounded_send(m);
        }
    }

    fn handle(&self, msg: ServerMessage, toasts: Toasts) {
        match msg {
            ServerMessage::GameUpdated { .. } => {
                self.lobby_tick.update(|n| *n += 1);
                self.game_tick.update(|n| *n += 1);
            }
            ServerMessage::ChatReceived { message } => self.chat_in.set(Some(message)),
            ServerMessage::YourTurn { game_id, power } => {
                self.lobby_tick.update(|n| *n += 1);
                self.game_tick.update(|n| *n += 1);
                let who = power
                    .map(|p| format!(" ({})", crate::model::PowerExt::name(p)))
                    .unwrap_or_default();
                toasts.link(format!("Your turn{who}"), format!("/game/{game_id}"));
            }
            ServerMessage::MatchFound { game_id } => {
                self.lobby_tick.update(|n| *n += 1);
                toasts.link("Match found! Open the game", format!("/game/{game_id}"));
            }
            ServerMessage::Error { message, .. } => leptos::logging::warn!("ws error: {message}"),
            ServerMessage::Subscribed { .. } | ServerMessage::Pong => {}
        }
    }

    fn run(self, auth: AuthState, toasts: Toasts) {
        spawn_local(async move {
            let mut backoff = 1000;
            loop {
                if auth.user.get_untracked().is_none() {
                    TimeoutFuture::new(1000).await;
                    continue;
                }
                if let Ok(ws) = WebSocket::open(&ws_url()) {
                    let (mut sink, mut stream) = ws.split();
                    let (tx, mut rx) = unbounded();
                    self.tx.set_value(Some(tx));
                    for g in self.subs.get_value() {
                        let _ = sink
                            .send(encode(&ClientMessage::Subscribe { game_id: g }))
                            .await;
                    }
                    backoff = 1000;
                    let mut ticks = 0u32;
                    loop {
                        if auth.user.get_untracked().is_none() {
                            break;
                        }
                        futures::select! {
                            m = stream.next().fuse() => match m {
                                Some(Ok(Message::Text(t))) => {
                                    if let Ok(sm) = serde_json::from_str::<ServerMessage>(&t) {
                                        self.handle(sm, toasts);
                                    }
                                }
                                Some(Ok(_)) => {}
                                _ => break,
                            },
                            o = rx.next().fuse() => if let Some(c) = o {
                                if sink.send(encode(&c)).await.is_err() { break; }
                            },
                            _ = TimeoutFuture::new(5000).fuse() => {
                                ticks += 1;
                                if ticks.is_multiple_of(5) {
                                    let _ = sink.send(encode(&ClientMessage::Ping)).await;
                                }
                            }
                        }
                    }
                    self.tx.set_value(None);
                    // Missed pushes while disconnected: refresh everything.
                    self.lobby_tick.update(|n| *n += 1);
                    self.game_tick.update(|n| *n += 1);
                }
                TimeoutFuture::new(backoff).await;
                backoff = (backoff * 2).min(15_000);
            }
        });
    }
}

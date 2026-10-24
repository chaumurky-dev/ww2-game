use crate::api;
use crate::model::LogEntry;
use crate::realtime::Realtime;
use crate::state::Toasts;
use leptos::prelude::*;
use leptos::task::spawn_local;
use wartide_protocol::chat::{ChatChannel, ChatMessage, ChatSender, SendChatRequest};
use wartide_protocol::game::{GameId, GameStatus, GameSummary};

fn channel_label(c: ChatChannel) -> &'static str {
    match c {
        ChatChannel::All => "All",
        ChatChannel::Team => "Team",
        ChatChannel::Power => "Power",
        ChatChannel::Lobby => "Lobby",
    }
}

fn push_unique(list: &mut Vec<ChatMessage>, m: ChatMessage) {
    if !list.iter().any(|x| x.id == m.id) {
        list.push(m);
        list.sort_by_key(|x| x.created_at);
    }
}

/// Chat drawer: history via `GET /games/{id}/chat`, live messages via the WebSocket,
/// sending via `POST /games/{id}/chat`.
#[component]
pub fn ChatDrawer(game_id: GameId, summary: Signal<GameSummary>) -> impl IntoView {
    let toasts = Toasts::expect();
    let rt = Realtime::expect();
    let messages = RwSignal::new(Vec::<ChatMessage>::new());
    let tab = RwSignal::new(ChatChannel::All);
    let draft = RwSignal::new(String::new());
    let lobby = Memo::new(move |_| summary.with(|s| s.status == GameStatus::Lobby));
    Effect::new(move |_| {
        tab.set(if lobby.get() {
            ChatChannel::Lobby
        } else {
            ChatChannel::All
        });
    });

    // History; refetched when the socket reconnects or the game changes.
    Effect::new(move |_| {
        rt.game_tick.track();
        spawn_local(async move {
            match api::chat(game_id).await {
                Ok(v) => messages.update(|l| v.into_iter().for_each(|m| push_unique(l, m))),
                Err(e) => toasts.api_error(&e),
            }
        });
    });
    Effect::new(move |_| {
        if let Some(m) = rt.chat_in.get() {
            if m.game_id == game_id {
                messages.update(|l| push_unique(l, m));
            }
        }
    });

    let send = move || {
        let body = draft.get_untracked();
        if body.trim().is_empty() {
            return;
        }
        let channel = tab.get_untracked();
        let power = summary.with_untracked(|s| {
            s.my_seat
                .and_then(|i| s.seats.iter().find(|x| x.seat == i))
                .and_then(|seat| {
                    seat.powers
                        .iter()
                        .copied()
                        .find(|p| Some(*p) == s.current_power)
                        .or_else(|| seat.powers.first().copied())
                })
        });
        let req = SendChatRequest {
            channel,
            body,
            power: matches!(channel, ChatChannel::Power | ChatChannel::Team)
                .then_some(power)
                .flatten(),
        };
        draft.set(String::new());
        spawn_local(async move {
            match api::send_chat(game_id, &req).await {
                Ok(m) => messages.update(|l| push_unique(l, m)),
                Err(e) => {
                    draft.set(req.body.clone());
                    toasts.api_error(&e)
                }
            }
        });
    };
    let send2 = send;
    let tabs = move || {
        let list: &[ChatChannel] = if lobby.get() {
            &[ChatChannel::Lobby, ChatChannel::All]
        } else {
            &[ChatChannel::All, ChatChannel::Team, ChatChannel::Power]
        };
        list.iter()
            .map(|c| {
                let c = *c;
                view! { <span class="tab" class:on=move || tab.get() == c on:click=move |_| tab.set(c)>{channel_label(c)}</span> }
            })
            .collect_view()
    };
    view! {
        <div class="chat">
            <div class="tabs">{tabs}</div>
            <div class="msgs">
                {move || {
                    messages
                        .get()
                        .into_iter()
                        .filter(|m| m.channel == tab.get())
                        .map(|m| {
                            let (who, class) = match &m.sender {
                                ChatSender::User(u) => (u.display_name.clone(), "msg"),
                                ChatSender::Bot { name } => (format!("{name} [bot]"), "msg bot"),
                                ChatSender::System => ("System".to_string(), "msg system"),
                            };
                            view! {
                                <div class=class>
                                    <strong>{who}</strong>
                                    ": " {m.body}
                                </div>
                            }
                        })
                        .collect_view()
                }}
            </div>
            <div class="row">
                <input
                    class="grow"
                    placeholder="Say something..."
                    prop:value=move || draft.get()
                    on:input:target=move |ev| draft.set(ev.target().value())
                    on:keydown=move |ev| if ev.key() == "Enter" { send2() }
                />
                <button class="small" on:click=move |_| send()>"Send"</button>
            </div>
        </div>
    }
}

#[component]
pub fn ActionLog(entries: RwSignal<Vec<LogEntry>>) -> impl IntoView {
    view! {
        <div>
            <h3>"Log"</h3>
            <div class="log">
                {move || {
                    entries
                        .get()
                        .into_iter()
                        .rev()
                        .map(|e| view! { <div><span class="muted">{format!("R{} ", e.round)}</span>{e.text}</div> })
                        .collect_view()
                }}
            </div>
        </div>
    }
}

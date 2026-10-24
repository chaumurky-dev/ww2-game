use super::create_game::CreateGameForm;
use crate::api;
use crate::components::board::MapPreview;
use crate::components::countdown::Countdown;
use crate::components::form::TextField;
use crate::components::modal::Modal;
use crate::components::power_badge::PowerBadge;
use crate::components::skeleton::{Empty, Pager, Skeleton};
use crate::model::*;
use crate::realtime::Realtime;
use crate::state::Toasts;
use crate::MapCtx;
use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::components::A;
use leptos_router::hooks::use_navigate;
use wartide_protocol::game::{GameMode, GameStatus, GameSummary};

fn game_row(g: GameSummary, watch: bool) -> impl IntoView {
    let mine = !watch && my_pending(&g);
    let note = pending_note(&g);
    let winner = g.winner;
    let waiting = g.status == GameStatus::Lobby;
    let (title, id, round, mode, current, status) = (
        game_title(&g),
        g.id,
        g.round,
        g.mode,
        g.current_power,
        g.status,
    );
    let deadline = deadline_ms(&g);
    let with = g
        .seats
        .iter()
        .filter(|s| Some(s.seat) != g.my_seat)
        .map(seat_label)
        .collect::<Vec<_>>()
        .join(", ");
    view! {
        <tr>
            <td>
                <A href=format!("/game/{id}")>{title}</A>
                {note.map(|n| view! { " " <span class="pill" title="The game waits for another power's decision">{n}</span> })}
                {mine.then(|| view! { " " <span class="pill hot">"Your turn"</span> })}
                {waiting.then(|| view! { " " <span class="pill">"Waiting"</span> })}
            </td>
            <td>{mode_label(mode)}</td>
            <td>{format!("R{round}")}</td>
            <td>{current.map(|p| view! { <PowerBadge power=p short=true /> })}</td>
            <td class="muted">{with}</td>
            <td>
                {match (status, deadline) {
                    (GameStatus::InProgress, Some(d)) => view! { <Countdown deadline_ms=d /> }.into_any(),
                    (GameStatus::Finished, _) => view! { <span class="muted">{winner.map(|t| format!("{} won", team_name(t))).unwrap_or_else(|| "Finished".into())}</span> }.into_any(),
                    (GameStatus::InProgress, None) => view! { <span class="muted">"Bot turn"</span> }.into_any(),
                    (s, _) => view! { <span class="muted">{status_label(s)}</span> }.into_any(),
                }}
            </td>
            <td>{watch.then(|| view! { <A href=format!("/game/{id}")><span class="btn small">"Watch"</span></A> })}</td>
        </tr>
    }
}

fn games_table(v: Vec<GameSummary>, empty: &'static str) -> AnyView {
    games_table_w(v, empty, false)
}

fn games_table_w(v: Vec<GameSummary>, empty: &'static str, watch: bool) -> AnyView {
    if v.is_empty() {
        return view! { <Empty text=empty /> }.into_any();
    }
    view! {
        <div style="overflow-x:auto"><table>
            <thead><tr><th>"Game"</th><th>"Mode"</th><th>"Round"</th><th>"To act"</th><th>"With"</th><th>"Deadline"</th><th></th></tr></thead>
            <tbody>{v.into_iter().map(|g| game_row(g, watch)).collect_view()}</tbody>
        </table></div>
    }
    .into_any()
}

/// Public (matchmade) games anyone can spectate: active or finished, paginated.
#[component]
fn WatchTab() -> impl IntoView {
    let rt = Realtime::expect();
    let status = RwSignal::new("active");
    let page = RwSignal::new(1u32);
    let list = LocalResource::new(move || async move {
        rt.lobby_tick.track();
        api::public_games(status.get(), page.get()).await
    });
    view! {
        <div class="card col">
            <div class="row">
                <h2 class="grow">"Watch public games"</h2>
                <button class=move || if status.get() == "active" { "small" } else { "small secondary" } on:click=move |_| { status.set("active"); page.set(1); }>"Live"</button>
                <button class=move || if status.get() == "finished" { "small" } else { "small secondary" } on:click=move |_| { status.set("finished"); page.set(1); }>"Finished"</button>
            </div>
            <p class="muted">"Spectators see the whole board and the all-chat, but cannot act or read team chat."</p>
            <Suspense fallback=|| view! { <Skeleton lines=5 /> }>
                {move || list.get().map(|r| match r {
                    Ok(p) => {
                        let (total, per) = (p.total, p.per_page);
                        view! {
                            {games_table_w(p.items, if status.get_untracked() == "active" { "No public games are running right now." } else { "No finished public games yet." }, true)}
                            <Pager page total per_page=per />
                        }.into_any()
                    }
                    Err(e) => view! { <p class="error">{e.message}</p> }.into_any(),
                })}
            </Suspense>
        </div>
    }
}

#[component]
pub fn Lobby() -> impl IntoView {
    let toasts = Toasts::expect();
    let rt = Realtime::expect();
    let navigate = use_navigate();
    let active = LocalResource::new(move || async move {
        rt.lobby_tick.track();
        api::games("active").await.map(|mut v| {
            v.sort_by(|a, b| {
                my_turn(b).cmp(&my_turn(a)).then(
                    deadline_ms(a)
                        .partial_cmp(&deadline_ms(b))
                        .unwrap_or(std::cmp::Ordering::Equal),
                )
            });
            v
        })
    });
    let finished = LocalResource::new(move || async move {
        rt.lobby_tick.track();
        api::games("finished").await
    });
    let create_open = RwSignal::new(false);
    let code = RwSignal::new(String::new());
    let queued = RwSignal::new(None::<GameMode>);
    let map = expect_context::<MapCtx>().0;

    spawn_local(async move {
        if let Ok(s) = api::queue_status().await {
            queued.set(s.queued.then_some(s.mode).flatten());
        }
    });
    // A found match removes us from the queue.
    Effect::new(move |prev: Option<u32>| {
        let t = rt.lobby_tick.get();
        if prev.is_some() && queued.get_untracked().is_some() {
            spawn_local(async move {
                if let Ok(s) = api::queue_status().await {
                    queued.set(s.queued.then_some(s.mode).flatten());
                }
            });
        }
        t
    });

    let join = {
        let navigate = navigate.clone();
        move |_| {
            let navigate = navigate.clone();
            spawn_local(async move {
                match api::join_game(&code.get_untracked()).await {
                    Ok(g) => navigate(&format!("/game/{}", g.id), Default::default()),
                    Err(e) => toasts.api_error(&e),
                }
            });
        }
    };
    let toggle_queue = move |mode: GameMode| {
        spawn_local(async move {
            if queued.get_untracked() == Some(mode) {
                match api::queue_leave().await {
                    Ok(()) => {
                        queued.set(None);
                        toasts.info("Left the queue");
                    }
                    Err(e) => toasts.api_error(&e),
                }
            } else {
                match api::queue_join(mode).await {
                    Ok(()) => {
                        queued.set(Some(mode));
                        toasts.info("Searching for an opponent...");
                    }
                    Err(e) => toasts.api_error(&e),
                }
            }
        });
    };
    let qbtn = move |mode: GameMode| {
        view! {
            <button class=move || if queued.get() == Some(mode) { "" } else { "secondary" } on:click=move |_| toggle_queue(mode)>
                {move || if queued.get() == Some(mode) { format!("Cancel {}", mode_label(mode)) } else { format!("Find {}", mode_label(mode)) }}
            </button>
        }
    };
    let tab = RwSignal::new("mine");
    view! {
        <div class="page col">
            <div class="tabs big-tabs">
                <span class="tab" class:on=move || tab.get() == "mine" on:click=move |_| tab.set("mine")>"My games"</span>
                <span class="tab" class:on=move || tab.get() == "watch" on:click=move |_| tab.set("watch")>"Watch"</span>
            </div>
            <Show when=move || tab.get() == "watch">
                <WatchTab />
            </Show>
            <div class="col" style:display=move || if tab.get() == "mine" { "flex" } else { "none" }>
            <div class="card">
                <div class="row"><h2 class="grow">"My games"</h2><button on:click=move |_| create_open.set(true)>"New private game"</button></div>
                <Suspense fallback=|| view! { <Skeleton /> }>
                    {move || active.get().map(|r| match r {
                        Ok(v) => games_table(v, "No active games. Create one or join the queue."),
                        Err(e) => view! { <p class="error">{e.message}</p> }.into_any(),
                    })}
                </Suspense>
            </div>
            <div class="grid2">
                <div class="card col">
                    <h3>"Public matchmaking"</h3>
                    <div class="row">{qbtn(GameMode::TwoPlayer)} {qbtn(GameMode::FivePlayer)}</div>
                    <p class="muted">
                        {move || match queued.get() {
                            Some(m) => format!("In the {} queue. You will get a notification when a match is found.", mode_label(m)),
                            None => "Rated games on the public ladders.".to_string(),
                        }}
                    </p>
                </div>
                <form class="card col" on:submit=|ev| ev.prevent_default()>
                    <h3>"Join by invite code"</h3>
                    <div class="row">
                        <div class="grow"><TextField label="Invite code" value=code placeholder="ABCD1234" /></div>
                        <button type="submit" on:click=join>"Join"</button>
                    </div>
                </form>
            </div>
            <div class="grid2">
                <div class="card">
                    <h3>"The world, before the first shot"</h3>
                    <div class="preview-wrap">
                        <Suspense fallback=|| view! { <p class="muted">"Loading map..."</p> }>
                            {move || map.get().map(|r| match r {
                                Ok(m) => view! { <MapPreview map=m /> }.into_any(),
                                Err(e) => view! { <p class="error">{e}</p> }.into_any(),
                            })}
                        </Suspense>
                    </div>
                </div>
                <div class="card">
                    <h3>"Finished games"</h3>
                    <Suspense fallback=|| view! { <Skeleton /> }>
                        {move || finished.get().map(|r| match r {
                            Ok(v) => games_table(v, "Nothing finished yet."),
                            Err(e) => view! { <p class="error">{e.message}</p> }.into_any(),
                        })}
                    </Suspense>
                </div>
            </div>
            </div>
            <Modal open=create_open title="New private game">
                <CreateGameForm open=create_open />
            </Modal>
        </div>
    }
}

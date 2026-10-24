use super::create_game::CreateGameForm;
use crate::api;
use crate::components::board::MapPreview;
use crate::components::countdown::Countdown;
use crate::components::form::TextField;
use crate::components::modal::Modal;
use crate::components::power_badge::PowerBadge;
use crate::model::*;
use crate::realtime::Realtime;
use crate::state::Toasts;
use crate::MapCtx;
use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::components::A;
use leptos_router::hooks::use_navigate;
use wartide_protocol::game::{GameMode, GameStatus, GameSummary};

fn game_row(g: GameSummary) -> impl IntoView {
    let mine = my_turn(&g);
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
                    (s, _) => view! { <span class="muted">{status_label(s)}</span> }.into_any(),
                }}
            </td>
        </tr>
    }
}

fn games_table(v: Vec<GameSummary>, empty: &'static str) -> AnyView {
    if v.is_empty() {
        return view! { <p class="muted">{empty}</p> }.into_any();
    }
    view! {
        <div style="overflow-x:auto"><table>
            <thead><tr><th>"Game"</th><th>"Mode"</th><th>"Round"</th><th>"To act"</th><th>"With"</th><th>"Deadline"</th></tr></thead>
            <tbody>{v.into_iter().map(game_row).collect_view()}</tbody>
        </table></div>
    }
    .into_any()
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
    view! {
        <div class="page col">
            <div class="card">
                <div class="row"><h2 class="grow">"My games"</h2><button on:click=move |_| create_open.set(true)>"New private game"</button></div>
                <Suspense fallback=|| view! { <p>"Loading..."</p> }>
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
                    <Suspense fallback=|| view! { <p>"Loading..."</p> }>
                        {move || finished.get().map(|r| match r {
                            Ok(v) => games_table(v, "Nothing finished yet."),
                            Err(e) => view! { <p class="error">{e.message}</p> }.into_any(),
                        })}
                    </Suspense>
                </div>
            </div>
            <Modal open=create_open title="New private game">
                <CreateGameForm open=create_open />
            </Modal>
        </div>
    }
}

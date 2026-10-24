use crate::api;
use crate::components::chat::ChatDrawer;
use crate::components::power_badge::PowerBadge;
use crate::game::live::LiveGame;
use crate::game::sandbox::sandbox_ctx;
use crate::game::screen::GameScreen;
use crate::map::SharedMap;
use crate::model::*;
use crate::realtime::Realtime;
use crate::state::Toasts;
use crate::MapCtx;
use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::hooks::use_params_map;
use uuid::Uuid;
use wartide_protocol::game::{GameId, GameStatus, GameSummary};

#[component]
pub fn GamePage() -> impl IntoView {
    let params = use_params_map();
    let rt = Realtime::expect();
    let map = expect_context::<MapCtx>().0;
    let id = Memo::new(move |_| {
        params
            .read()
            .get("id")
            .and_then(|s| Uuid::parse_str(&s).ok())
            .map(GameId)
    });
    let summary = RwSignal::new(None::<GameSummary>);
    let error = RwSignal::new(None::<String>);

    Effect::new(move |_| {
        rt.game_tick.track();
        let Some(id) = id.get() else {
            error.set(Some("Not a valid game id".into()));
            return;
        };
        spawn_local(async move {
            match api::game(id).await {
                Ok(g) => {
                    error.set(None);
                    summary.set(Some(g));
                }
                Err(e) => error.set(Some(e.message)),
            }
        });
    });
    Effect::new(move |prev: Option<Option<GameId>>| {
        let cur = id.get();
        if let Some(Some(old)) = prev {
            if Some(old) != cur {
                rt.unsubscribe(old);
            }
        }
        if let Some(g) = cur {
            rt.subscribe(g);
        }
        cur
    });
    on_cleanup(move || {
        if let Some(g) = id.get_untracked() {
            rt.unsubscribe(g);
        }
    });

    view! {
        {move || error.get().map(|e| view! { <div class="page"><p class="error">{e}</p></div> })}
        <Suspense fallback=|| view! { <div class="page"><p>"Loading game..."</p></div> }>
            {move || {
                let m = map.get();
                m.map(|r| match r {
                    Ok(m) => view! {
                        <Show when=move || summary.with(|s| s.is_some())>
                            <GameView summary map=m.clone() />
                        </Show>
                    }.into_any(),
                    Err(e) => view! { <div class="page"><p class="error">{e}</p></div> }.into_any(),
                })
            }}
        </Suspense>
    }
}

type GameAction =
    fn(GameId) -> std::pin::Pin<Box<dyn std::future::Future<Output = api::ApiResult<GameSummary>>>>;

#[component]
fn GameInfo(summary: RwSignal<Option<GameSummary>>, live: Memo<GameSummary>) -> impl IntoView {
    let toasts = Toasts::expect();
    let auth = crate::state::AuthState::expect();
    let sig = Signal::from(live);
    let act = move |f: GameAction, ok: &'static str| {
        let id = live.with_untracked(|s| s.id);
        spawn_local(async move {
            match f(id).await {
                Ok(g) => {
                    summary.set(Some(g));
                    toasts.info(ok);
                }
                Err(e) => toasts.api_error(&e),
            }
        });
    };
    let is_host = move || {
        let me = auth.user.with(|u| u.as_ref().map(|u| u.id));
        live.with(|s| me.is_some() && s.host_id == me)
    };
    view! {
        <div class="col game-info">
            <div class="row">
                <h3 class="grow" style="margin:0">{move || sig.with(game_title)}</h3>
                <span class="pill">{move || sig.with(|s| status_label(s.status))}</span>
            </div>
            <div class="seats">
                {move || sig.with(|s| s.seats.iter().map(|seat| {
                    let me = Some(seat.seat) == s.my_seat;
                    let cur = s.current_power.map(|p| seat.powers.contains(&p)).unwrap_or(false);
                    view! {
                        <div class="row seat" class:me=me class:cur=cur>
                            <span>{seat.powers.iter().map(|p| view! { <PowerBadge power=*p short=true /> " " }).collect_view()}</span>
                            <span class="grow">{seat_label(seat)}{me.then_some(" (you)")}</span>
                        </div>
                    }
                }).collect_view())}
            </div>
            {move || sig.with(|s| (s.status == GameStatus::Lobby).then(|| s.invite_code.clone()).flatten().map(|c| view! {
                <div class="invite">"Invite code: " <strong class="code">{c}</strong></div>
            }))}
            <div class="row">
                <Show when=move || is_host() && sig.with(|s| s.status == GameStatus::Lobby)>
                    <button on:click=move |_| act(|id| Box::pin(api::start_game(id)), "Game started")>"Start now (bots fill empty seats)"</button>
                </Show>
                <Show when=move || sig.with(|s| s.status == GameStatus::InProgress && s.my_seat.is_some())>
                    <button class="secondary small" on:click=move |_| {
                        let ok = web_sys::window().and_then(|w| w.confirm_with_message("Resign? Your team loses.").ok()).unwrap_or(false);
                        if ok { act(|id| Box::pin(api::resign_game(id)), "You resigned"); }
                    }>"Resign"</button>
                </Show>
                <Show when=move || is_host() && sig.with(|s| matches!(s.status, GameStatus::Lobby | GameStatus::InProgress))>
                    <button class="secondary small" on:click=move |_| {
                        let ok = web_sys::window().and_then(|w| w.confirm_with_message("Abandon this game for everyone? It will not be rated.").ok()).unwrap_or(false);
                        if ok { act(|id| Box::pin(api::abandon_game(id)), "Game abandoned"); }
                    }>"Abandon"</button>
                </Show>
            </div>
        </div>
    }
}

#[component]
fn GameView(summary: RwSignal<Option<GameSummary>>, map: SharedMap) -> impl IntoView {
    // `GameView` is only mounted while a summary exists.
    let Some(initial) = summary.get_untracked() else {
        return ().into_any();
    };
    let live = {
        let initial = initial.clone();
        Memo::new(move |prev: Option<&GameSummary>| {
            summary
                .get()
                .or_else(|| prev.cloned())
                .unwrap_or_else(|| initial.clone())
        })
    };
    let game_id = initial.id;
    let sig = Signal::from(live);
    let started = Memo::new(move |_| {
        live.with(|s| matches!(s.status, GameStatus::InProgress | GameStatus::Finished))
    });
    let side = move || {
        view! {
            <GameInfo summary live />
        }
    };
    let chat = move || view! { <ChatDrawer game_id summary=sig /> };
    let deadline = Signal::derive(move || live.with(deadline_ms));
    view! {
        <Show
            when=move || started.get()
            fallback=move || view! {
                <div class="page col">
                    <GameInfo summary live />
                    <ChatDrawer game_id summary=sig />
                </div>
            }
        >
            {
                let map = map.clone();
                if api::USE_MOCK_GAME_STATE {
                    provide_context(sandbox_ctx(crate::state::Toasts::expect()));
                    view! { <GameScreen map deadline_ms=deadline top=ViewFn::from(side) bottom=ViewFn::from(chat) /> }.into_any()
                } else {
                    view! { <LiveGame game_id live map deadline top=ViewFn::from(side) bottom=ViewFn::from(chat) /> }.into_any()
                }
            }
        </Show>
    }
    .into_any()
}

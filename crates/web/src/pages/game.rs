use crate::api;
use crate::components::action_panel::ActionPanel;
use crate::components::board::{Board, BoardModel, Stack};
use crate::components::chat::{ActionLog, ChatDrawer};
use crate::components::phase_bar::PhaseBar;
use crate::components::power_badge::PowerBadge;
use crate::map::SharedMap;
use crate::mock;
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
    let end_turn = move |_| {
        let id = live.with_untracked(|s| s.id);
        spawn_local(async move {
            match api::submit_action(id, serde_json::json!({"type": "end_turn"})).await {
                Ok(_) => toasts.info("Turn ended"),
                Err(e) => toasts.api_error(&e),
            }
            if let Ok(g) = api::game(id).await {
                summary.set(Some(g));
            }
        });
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
                <Show when=move || sig.with(my_turn)>
                    <button on:click=end_turn>"End turn"</button>
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
    let toasts = Toasts::expect();
    let phase = RwSignal::new(Phase::Purchase);
    let model = RwSignal::new(mock::initial_board(&map));
    let log = RwSignal::new(mock::log(initial.round.max(1) as u32));
    let collapsed = RwSignal::new(false);
    let treasury = mock::treasury();
    let game_id = initial.id;
    let sig = Signal::from(live);
    let power = Signal::derive(move || live.with(|s| s.current_power.unwrap_or(Power::Ussr)));
    let my_power = Signal::derive(move || {
        live.with(|s| {
            my_seat(s)
                .and_then(|seat| seat.powers.first().copied())
                .or(s.current_power)
                .unwrap_or(Power::Ussr)
        })
    });
    let names: StoredValue<Vec<(String, String)>> = StoredValue::new({
        let mut v: Vec<_> = map
            .data
            .info
            .iter()
            .map(|(id, i)| (id.clone(), i.name.clone()))
            .collect();
        v.sort_by(|a, b| a.1.cmp(&b.1));
        v
    });
    let selected = Signal::derive(move || {
        let id = model.with(|m| m.selected.clone())?;
        let name = names.with_value(|n| n.iter().find(|x| x.0 == id).map(|x| x.1.clone()))?;
        let me = my_power.get();
        let stacks: Vec<Stack> = model.with(|m| m.stacks.get(&id).cloned().unwrap_or_default());
        Some((name, stacks.into_iter().filter(|s| s.power == me).collect()))
    });
    let on_select =
        Callback::new(move |id: Option<String>| model.update(|m: &mut BoardModel| m.selected = id));
    let on_action = Callback::new(move |text: String| {
        // Mock only until the engine state view lands (api::USE_MOCK_GAME_STATE).
        toasts.info(text.clone());
        log.update(|l| {
            l.push(LogEntry {
                round: live.with_untracked(|s| s.round.max(1) as u32),
                text,
            })
        });
        phase.update(|p| *p = Phase::ALL[(p.index() + 1) % Phase::ALL.len()]);
    });
    let destinations = names.get_value();
    view! {
        <div class="game">
            {move || view! {
                <PhaseBar
                    round=live.with(|s| s.round.max(1) as u32)
                    power=power.get()
                    phase=phase.get()
                    treasury=treasury.clone()
                    deadline_ms=live.with(deadline_ms)
                />
            }}
            <div class="game-body">
                <Board map=map.clone() model=model on_select />
                <aside class="side" class:collapsed=move || collapsed.get()>
                    <button class="secondary small side-toggle" on:click=move |_| collapsed.update(|c| *c = !*c)>
                        {move || if collapsed.get() { "Show panel" } else { "Hide panel" }}
                    </button>
                    <GameInfo summary live />
                    <Show when=|| api::USE_MOCK_GAME_STATE>
                        <label>"Mock phase (board actions are local only)"
                            <select on:change:target=move |ev| {
                                if let Ok(i) = ev.target().value().parse::<usize>() { phase.set(Phase::ALL[i.min(6)]); }
                            }>
                                {Phase::ALL.iter().map(|p| view! {
                                    <option value=p.index().to_string() selected=move || phase.get() == *p>{p.label()}</option>
                                }).collect_view()}
                            </select>
                        </label>
                    </Show>
                    {
                        let destinations = destinations.clone();
                        move || view! {
                            <ActionPanel phase=phase.get() power=my_power.get() ipc=24 selected destinations=destinations.clone() on_action />
                        }
                    }
                    <ActionLog entries=log />
                    <ChatDrawer game_id summary=sig />
                </aside>
            </div>
        </div>
    }
    .into_any()
}

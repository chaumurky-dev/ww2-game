use crate::api;
use crate::components::action_panel::ActionPanel;
use crate::components::board::{Board, BoardModel, Stack};
use crate::components::chat::{ActionLog, ChatDrawer};
use crate::components::phase_bar::PhaseBar;
use crate::geometry::{self, Geometry};
use crate::mock::{GameDetail, LogEntry, Phase};
use crate::state::Toasts;
use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::hooks::use_params_map;

#[component]
pub fn GamePage() -> impl IntoView {
    let params = use_params_map();
    let data = LocalResource::new(move || {
        let id = params.read().get("id").unwrap_or_default();
        async move {
            let detail = api::game(&id).await?;
            let geo = geometry::load().await?;
            Ok::<_, String>((detail, geo))
        }
    });
    view! {
        <Suspense fallback=|| view! { <div class="page"><p>"Loading game..."</p></div> }>
            {move || data.get().map(|r| match r {
                Ok((detail, geo)) => view! { <GameView detail geometry=geo /> }.into_any(),
                Err(e) => view! { <div class="page"><p class="error">{e}</p></div> }.into_any(),
            })}
        </Suspense>
    }
}

#[component]
fn GameView(detail: GameDetail, geometry: Geometry) -> impl IntoView {
    let toasts = Toasts::expect();
    let phase = RwSignal::new(detail.phase);
    let model = RwSignal::new(detail.board.clone());
    let messages = RwSignal::new(detail.messages.clone());
    let log = RwSignal::new(detail.log.clone());
    let collapsed = RwSignal::new(false);
    let sum = detail.summary.clone();
    let power = sum.current_power;
    let my_power = sum.my_powers.first().copied().unwrap_or(power);
    let round = sum.round;
    let game_id = sum.id.clone();
    let treasury = detail.treasury.clone();
    let ipc = treasury.iter().find(|t| t.power == power).map(|t| t.ipc).unwrap_or(0);
    let names = StoredValue::new(geometry.locations.iter().map(|l| (l.id.clone(), l.name.clone())).collect::<Vec<_>>());

    let selected = Signal::derive(move || {
        let id = model.with(|m| m.selected.clone())?;
        let name = names.with_value(|n| n.iter().find(|x| x.0 == id).map(|x| x.1.clone()))?;
        let stacks: Vec<Stack> = model.with(|m| m.stacks.get(&id).cloned().unwrap_or_default());
        Some((name, stacks.into_iter().filter(|s| s.power == my_power).collect()))
    });
    let on_select = Callback::new(move |id: Option<String>| model.update(|m: &mut BoardModel| m.selected = id));
    let on_action = Callback::new(move |text: String| {
        let (gid, text2) = (game_id.clone(), text.clone());
        spawn_local(async move {
            match api::submit_action(&gid, &text2).await {
                Ok(()) => toasts.info(text2),
                Err(e) => toasts.error(e),
            }
        });
        log.update(|l| l.push(LogEntry { round, text }));
        // Mock only: advance the phase stepper. TODO(protocol): phase comes from server state.
        phase.update(|p| *p = Phase::ALL[(p.index() + 1) % Phase::ALL.len()]);
    });
    let destinations = names.get_value();
    let me = crate::state::Session::expect().user.get_untracked().map(|u| u.username).unwrap_or_else(|| "me".into());
    api::subscribe(&sum.id, move |t| toasts.info(t));

    view! {
        <div class="game">
            {move || view! {
                <PhaseBar round power phase=phase.get() treasury=treasury.clone() deadline_ms=sum.deadline_ms />
            }}
            <div class="game-body">
                <Board geometry model=model on_select />
                <aside class="side" class:collapsed=move || collapsed.get()>
                    <button class="secondary small side-toggle" on:click=move |_| collapsed.update(|c| *c = !*c)>
                        {move || if collapsed.get() { "Show panel" } else { "Hide panel" }}
                    </button>
                    <Show when=|| api::USE_MOCK>
                        <label>"Mock phase"
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
                            <ActionPanel phase=phase.get() power=my_power ipc selected destinations=destinations.clone() on_action />
                        }
                    }
                    <ActionLog entries=log />
                    <ChatDrawer messages me=me.clone() my_power />
                </aside>
            </div>
        </div>
    }
}

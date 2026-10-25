//! Server-driven game: loads `GameState`, submits engine actions, refetches on `GameUpdated`.
use super::conv::eng_power;
use super::ctx::GameCtx;
use super::log::{format_events, push_lines};
use super::screen::GameScreen;
use crate::api;
use crate::components::skeleton::Skeleton;
use crate::map::SharedMap;
use crate::realtime::Realtime;
use crate::state::Toasts;
use leptos::prelude::*;
use leptos::task::spawn_local;
use wartide_engine::{Action, Event, GameSettings, GameState, Power};
use wartide_protocol::game::{GameId, GameSummary};

#[derive(Clone, Copy)]
struct Sync {
    ctx: GameCtx,
    id: GameId,
    last_seq: StoredValue<i64>,
    loaded: RwSignal<bool>,
    running: StoredValue<bool>,
    again: StoredValue<bool>,
    toasts: Toasts,
}

fn parse_events(v: serde_json::Value) -> Vec<Event> {
    serde_json::from_value(v).unwrap_or_default()
}

impl Sync {
    /// Fetch the state; on first load also build the recent log, afterwards animate new events.
    async fn refresh(self) {
        if self.running.get_value() {
            self.again.set_value(true);
            return;
        }
        self.running.set_value(true);
        loop {
            self.again.set_value(false);
            self.once().await;
            if !self.again.get_value() {
                break;
            }
        }
        self.running.set_value(false);
    }

    async fn once(&self) {
        let resp = match api::game_state(self.id).await {
            Ok(r) => r,
            Err(e) => return self.toasts.api_error(&e),
        };
        let st: GameState = match serde_json::from_value(resp.state) {
            Ok(s) => s,
            Err(e) => return self.toasts.error(format!("Bad game state: {e}")),
        };
        let first = !self.loaded.get_untracked();
        let since = if first {
            (resp.seq - 60).max(0)
        } else {
            self.last_seq.get_value()
        };
        let envs = api::game_actions(self.id, since).await.unwrap_or_default();
        let mut events = vec![];
        for e in envs.into_iter().filter(|e| e.seq > since) {
            events.extend(parse_events(e.events));
        }
        if first {
            let lines = format_events(&events, st.round);
            self.ctx.log.update(|v| push_lines(v, lines));
            self.ctx.reset_state(st);
            self.loaded.set(true);
        } else {
            let before = self.ctx.state.get_untracked();
            if before != st {
                self.ctx.ingest(&before, st, &events, true);
            }
        }
        self.last_seq.set_value(resp.seq);
    }
}

#[component]
pub fn LiveGame(
    game_id: GameId,
    live: Memo<GameSummary>,
    map: SharedMap,
    #[prop(into)] deadline: Signal<Option<f64>>,
    top: ViewFn,
    bottom: ViewFn,
) -> impl IntoView {
    let toasts = Toasts::expect();
    let rt = Realtime::expect();
    let viewer = Signal::derive(move || -> Vec<Power> {
        live.with(|s| {
            crate::model::my_seat(s)
                .map(|seat| seat.powers.iter().map(|p| eng_power(*p)).collect())
                .unwrap_or_default()
        })
    });
    let mut ctx = GameCtx::new(GameState::new(GameSettings::default()), viewer);
    ctx.auto_defense = true;
    provide_context(ctx);
    let sync = Sync {
        ctx,
        id: game_id,
        last_seq: StoredValue::new(0),
        loaded: RwSignal::new(false),
        running: StoredValue::new(false),
        again: StoredValue::new(false),
        toasts,
    };
    ctx.set_backend(Callback::new(move |actions: Vec<Action>| {
        spawn_local(async move {
            ctx.busy.set(true);
            for a in actions {
                let body = serde_json::to_value(&a).unwrap_or_default();
                if let Err(e) = api::submit_action(game_id, body).await {
                    toasts.api_error(&e);
                    break;
                }
            }
            sync.refresh().await;
            ctx.busy.set(false);
        });
    }));
    Effect::new(move |_| {
        rt.game_tick.track();
        spawn_local(sync.refresh());
    });
    view! {
        <Show when=move || sync.loaded.get() fallback=|| view! { <div class="page"><Skeleton lines=6 /></div> }>
            <GameScreen map=map.clone() deadline_ms=deadline top=top.clone() bottom=bottom.clone() />
        </Show>
    }
}

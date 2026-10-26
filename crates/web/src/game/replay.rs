//! Client-side replay of a finished game: the engine re-applies the logged actions with the
//! per-action seeds the server used (`wartide/action` hash of the game seed and the sequence
//! number), so any step can be shown without extra endpoints.
use super::ctx::GameCtx;
use super::log::{format_events, push_lines};
use super::screen::GameScreen;
use crate::api;
use crate::map::SharedMap;
use crate::state::Toasts;
use gloo_timers::callback::Interval;
use leptos::prelude::*;
use leptos::task::spawn_local;
use wartide_engine::{apply, pending_decision, Action, Decision, Event, GameState, Power};
use wartide_protocol::game::{GameActionEnvelope, GameId};
use wartide_protocol::seed::action_seed;

const CHECKPOINT: usize = 25;

fn acting(st: &GameState) -> Option<Power> {
    match pending_decision(st) {
        Decision::Phase { power, .. }
        | Decision::SelectCasualties { power, .. }
        | Decision::RetreatOrPress { power, .. }
        | Decision::SubWithdrawal { power, .. } => Some(power),
        Decision::GameOver { .. } => None,
    }
}

pub struct Replay {
    steps: Vec<(Power, Action, u64)>,
    events: Vec<Vec<Event>>,
    rounds: Vec<u32>,
    checkpoints: Vec<GameState>,
}

impl Replay {
    /// Build from the final state (settings + seed) and the full action log. Errors when an
    /// action cannot be re-applied or the replayed end state differs from `last`.
    pub fn build(last: &GameState, envs: &[GameActionEnvelope]) -> Result<Replay, String> {
        let mut st = GameState::new(last.settings.clone());
        let mut r = Replay {
            steps: vec![],
            events: vec![],
            rounds: vec![],
            checkpoints: vec![st.clone()],
        };
        for e in envs {
            let action: Action = serde_json::from_value(e.action.clone())
                .map_err(|x| format!("unreadable action #{}: {x}", e.seq))?;
            let who = acting(&st).ok_or_else(|| format!("action #{} after game over", e.seq))?;
            let seed = action_seed(last.settings.seed, e.seq);
            r.rounds.push(st.round);
            let ev = apply(&mut st, who, &action, seed)
                .map_err(|x| format!("action #{} was rejected on replay: {x}", e.seq))?;
            r.steps.push((who, action, seed));
            r.events.push(ev);
            if r.steps.len().is_multiple_of(CHECKPOINT) {
                r.checkpoints.push(st.clone());
            }
        }
        if &st != last {
            return Err("replayed end state differs from the stored state".into());
        }
        Ok(r)
    }

    pub fn len(&self) -> usize {
        self.steps.len()
    }

    pub fn is_empty(&self) -> bool {
        self.steps.is_empty()
    }

    /// State after `i` actions.
    pub fn state_at(&self, i: usize) -> GameState {
        let i = i.min(self.len());
        let mut st = self.checkpoints[i / CHECKPOINT].clone();
        for (p, a, s) in &self.steps[i / CHECKPOINT * CHECKPOINT..i] {
            let _ = apply(&mut st, *p, a, *s);
        }
        st
    }

    pub fn events_at(&self, i: usize) -> &[Event] {
        if i == 0 {
            &[]
        } else {
            &self.events[i - 1]
        }
    }

    fn round_at(&self, i: usize) -> u32 {
        self.rounds.get(i.saturating_sub(1)).copied().unwrap_or(1)
    }
}

async fn load(id: GameId) -> Result<Replay, String> {
    let fin = api::game_state(id).await.map_err(|e| e.message)?;
    let last: GameState = serde_json::from_value(fin.state).map_err(|e| e.to_string())?;
    let mut envs: Vec<GameActionEnvelope> = vec![];
    let mut since = 0;
    loop {
        let page = api::game_actions(id, since).await.map_err(|e| e.message)?;
        let Some(tail) = page.last().map(|e| e.seq) else {
            break;
        };
        since = tail;
        envs.extend(page);
        if envs.len() > 50_000 {
            break;
        }
    }
    Replay::build(&last, &envs)
}

#[component]
pub fn ReplayGame(game_id: GameId, map: SharedMap, top: ViewFn, bottom: ViewFn) -> impl IntoView {
    let toasts = Toasts::expect();
    let replay = StoredValue::new(None::<Replay>);
    let ready = RwSignal::new(false);
    let failed = RwSignal::new(None::<String>);
    let step = RwSignal::new(0usize);
    let playing = RwSignal::new(false);
    let len = RwSignal::new(0usize);
    let ctx = GameCtx::new(GameState::new(Default::default()), Signal::derive(Vec::new));
    provide_context(ctx);
    spawn_local(async move {
        match load(game_id).await {
            Ok(r) => {
                len.set(r.len());
                ctx.reset_state(r.state_at(0));
                replay.set_value(Some(r));
                ready.set(true);
            }
            Err(e) => {
                toasts.error(format!("Replay unavailable: {e}"));
                failed.set(Some(e));
            }
        }
    });
    // Show `to`; stepping forward by one animates the dice, jumps just redraw.
    let show = move |to: usize| {
        let from = step.get_untracked();
        replay.with_value(|r| {
            let Some(r) = r else { return };
            let to = to.min(r.len());
            let after = r.state_at(to);
            if to == from + 1 {
                let before = ctx.state.get_untracked();
                ctx.ingest(&before, after, r.events_at(to), true);
            } else {
                ctx.reset_state(after);
                ctx.dice.set(vec![]);
                ctx.log.set(vec![]);
                ctx.battles.set_value(Default::default());
                let from_step = to.saturating_sub(12);
                for i in from_step + 1..=to {
                    let lines = ctx
                        .battles
                        .try_update_value(|a| format_events(r.events_at(i), r.round_at(i), a))
                        .unwrap_or_default();
                    ctx.log.update(|v| push_lines(v, lines));
                }
            }
            step.set(to);
        });
    };
    let timer = StoredValue::new_local(None::<Interval>);
    Effect::new(move |_| {
        if playing.get() {
            timer.set_value(Some(Interval::new(900, move || {
                let n = step.get_untracked();
                if n >= len.get_untracked() {
                    playing.set(false);
                } else {
                    show(n + 1);
                }
            })));
        } else {
            timer.set_value(None);
        }
    });
    on_cleanup(move || timer.set_value(None));
    let controls = move || {
        view! {
            <div class="replay card-ish">
                <strong>"Replay"</strong>
                {move || failed.get().map(|e| view! { <p class="error">{format!("Replay unavailable: {e}")}</p> })}
                <Show when=move || ready.get()>
                    <input type="range" min="0" aria-label="Replay position"
                        max=move || len.get().to_string()
                        prop:value=move || step.get().to_string()
                        on:input:target=move |ev| { playing.set(false); show(ev.target().value().parse().unwrap_or(0)) } />
                    <div class="row">
                        <button class="secondary small" on:click=move |_| { playing.set(false); show(0) }>"|<"</button>
                        <button class="secondary small" on:click=move |_| { playing.set(false); show(step.get_untracked().saturating_sub(1)) }>"<"</button>
                        <button class="small" on:click=move |_| {
                            if step.get_untracked() >= len.get_untracked() { show(0); }
                            playing.update(|p| *p = !*p)
                        }>{move || if playing.get() { "Pause" } else { "Play" }}</button>
                        <button class="secondary small" on:click=move |_| { playing.set(false); show(step.get_untracked() + 1) }>">"</button>
                        <button class="secondary small" on:click=move |_| { playing.set(false); show(len.get_untracked()) }>">|"</button>
                        <span class="muted">{move || format!("{} / {}", step.get(), len.get())}</span>
                    </div>
                </Show>
            </div>
        }
    };
    let top2 = top.clone();
    view! {
        <Show when=move || ready.get() fallback=move || view! { <div class="page"><p class="muted">{move || failed.get().map(|e| format!("Replay unavailable: {e}")).unwrap_or_else(|| "Replaying the game...".into())}</p></div> }>
            <GameScreen map=map.clone() deadline_ms=Signal::derive(|| None)
                top=ViewFn::from({ let top2 = top2.clone(); move || view! { {controls()} {top2.run()} } })
                bottom=bottom.clone() />
        </Show>
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wartide_engine::GameSettings;

    /// Replays a short engine game built with the same seed function and checks the end state.
    #[test]
    fn replay_matches_direct_play() {
        let settings = GameSettings {
            seed: 0x1234_5678_9abc_def0,
            ..Default::default()
        };
        let mut st = GameState::new(settings.clone());
        let mut envs = vec![];
        for seq in 1..=6 {
            let who = acting(&st).unwrap();
            let a = Action::EndPhase;
            apply(&mut st, who, &a, action_seed(settings.seed, seq)).unwrap();
            envs.push(GameActionEnvelope {
                seq,
                actor: Some(0),
                action: serde_json::to_value(&a).unwrap(),
                events: serde_json::Value::Null,
                created_at: chrono::DateTime::from_timestamp(0, 0).unwrap(),
            });
        }
        let r = Replay::build(&st, &envs).unwrap();
        assert_eq!(r.len(), 6);
        assert_eq!(r.state_at(6), st);
        assert_ne!(r.state_at(3), st);
        assert_eq!(r.state_at(0), GameState::new(settings));
    }

    #[test]
    fn seeds_differ_per_seq() {
        assert_ne!(action_seed(1, 1), action_seed(1, 2));
        assert_ne!(action_seed(1, 1), action_seed(2, 1));
    }
}

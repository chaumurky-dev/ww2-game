//! Offline sandbox: runs `wartide_engine::apply` locally in the browser, controlling all five
//! powers. Route `/sandbox`; also reachable for any game while `api::USE_MOCK_GAME_STATE` is on.
use super::ctx::GameCtx;
use super::screen::GameScreen;
use crate::state::Toasts;
use crate::MapCtx;
use leptos::prelude::*;
use wartide_engine::{apply, pending_decision, Action, Decision, GameSettings, GameState, Power};
use wasm_bindgen::prelude::*;

fn actor(st: &GameState) -> Option<Power> {
    match pending_decision(st) {
        Decision::Phase { power, .. }
        | Decision::SelectCasualties { power, .. }
        | Decision::RetreatOrPress { power, .. }
        | Decision::SubWithdrawal { power, .. } => Some(power),
        Decision::GameOver { .. } => None,
    }
}

/// Fresh sandbox context whose backend applies actions with the local engine.
pub fn sandbox_ctx(toasts: Toasts) -> GameCtx {
    let viewer = Signal::derive(|| Power::ALL.to_vec());
    let ctx = GameCtx::new(GameState::new(GameSettings::default()), viewer);
    ctx.set_backend(Callback::new(move |actions: Vec<Action>| {
        for a in actions {
            if let Err(e) = run_one(ctx, &a) {
                toasts.error(e);
                break;
            }
        }
    }));
    ctx
}

fn run_one(ctx: GameCtx, a: &Action) -> Result<(), String> {
    let mut st = ctx.state.get_untracked();
    let before = st.clone();
    let who = actor(&st).ok_or("the game is over")?;
    let seed = (js_sys::Math::random() * 9_007_199_254_740_991.0) as u64;
    let ev = apply(&mut st, who, a, seed).map_err(|e| e.to_string())?;
    ctx.ingest(&before, st, &ev, true);
    Ok(())
}

/// Test hooks on `window`: `wartideSubmit(json) -> error string ("" when ok)` and
/// `wartideState() -> json`.
fn install_hooks(ctx: GameCtx) {
    let Some(w) = web_sys::window() else { return };
    let submit = Closure::<dyn Fn(String) -> String>::new(move |s: String| {
        match serde_json::from_str::<Action>(&s) {
            Ok(a) => run_one(ctx, &a).err().unwrap_or_default(),
            Err(e) => e.to_string(),
        }
    });
    let state = Closure::<dyn Fn() -> String>::new(move || {
        ctx.state
            .with_untracked(|s| serde_json::to_string(s).unwrap_or_default())
    });
    let _ = js_sys::Reflect::set(&w, &"wartideSubmit".into(), submit.as_ref());
    let _ = js_sys::Reflect::set(&w, &"wartideState".into(), state.as_ref());
    submit.forget();
    state.forget();
}

#[component]
pub fn Sandbox() -> impl IntoView {
    let map = expect_context::<MapCtx>().0;
    let ctx = sandbox_ctx(Toasts::expect());
    provide_context(ctx);
    install_hooks(ctx);
    view! {
        <Suspense fallback=|| view! { <div class="page"><p>"Loading map..."</p></div> }>
            {move || map.get().map(|r| match r {
                Ok(m) => view! {
                    <GameScreen map=m deadline_ms=Signal::derive(|| None)
                        top=ViewFn::from(|| view! {
                            <div class="sandbox-note">
                                <strong>"Sandbox"</strong>
                                " - offline, you play all five powers with the local rules engine."
                            </div>
                        }) />
                }.into_any(),
                Err(e) => view! { <div class="page"><p class="error">{e}</p></div> }.into_any(),
            })}
        </Suspense>
    }
}

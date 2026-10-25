//! The interactive game screen shared by the server-driven game page and the sandbox.
use super::conv::loc_key;
use super::ctx::{GameCtx, Sel};
use super::log::ActionLog;
use super::model::{board_model, Overlay};
use super::panels::{place_at, SidePanel};
use super::topbar::TopBar;
use crate::components::board::{Board, BoardModel};
use crate::map::SharedMap;
use leptos::prelude::*;
use wartide_engine::query::{battles_pending, legal_placements};
use wartide_engine::{pending_decision, Decision, LocationId, Phase};
use wasm_bindgen::JsCast;

fn overlay(ctx: GameCtx) -> Overlay {
    let st = ctx.state.get();
    let sel = ctx.sel.get();
    let mut ov = Overlay {
        picked: sel.units.clone(),
        selected: sel.from,
        ..Default::default()
    };
    ov.flash = ctx.flash.get();
    ov.battles = battles_pending(&st);
    if let Some(b) = &st.battle {
        ov.battles.push(b.location);
    }
    if let Decision::Phase { power, phase } = pending_decision(&st) {
        if ctx.controls(power) && st.battle.is_none() {
            match phase {
                Phase::CombatMove | Phase::NonCombatMove => {
                    ov.movable = ctx.movable.get();
                    ov.dests = ctx.reach.get().iter().map(|r| (r.to, r.battle)).collect();
                }
                Phase::Mobilize => {
                    if let Some(k) = sel.place {
                        ov.placements = legal_placements(&st, k);
                    }
                }
                _ => {}
            }
        }
    }
    ov
}

fn board_click(ctx: GameCtx, loc: LocationId) {
    let st = ctx.state.get_untracked();
    let Decision::Phase { power, phase } = pending_decision(&st) else {
        return;
    };
    if !ctx.controls(power) || st.battle.is_some() {
        return;
    }
    match phase {
        Phase::CombatMove | Phase::NonCombatMove => {
            let sel = ctx.sel.get_untracked();
            let is_dest = !sel.units.is_empty()
                && ctx.reach.with_untracked(|r| r.iter().any(|x| x.to == loc));
            if is_dest {
                ctx.sel.update(|s| s.dest = Some(loc));
            } else if sel.from == Some(loc) {
                ctx.sel.set(Sel::default());
            } else if st.units_at(loc).any(|u| u.owner == power) {
                ctx.sel.set(Sel {
                    from: Some(loc),
                    ..Default::default()
                });
            } else {
                ctx.sel.update(|s| s.dest = None);
            }
        }
        Phase::Mobilize => place_at(ctx, loc),
        _ => {}
    }
}

/// Click the first enabled primary button of the side panel (Enter shortcut).
fn click_primary() -> bool {
    let btn = web_sys::window().and_then(|w| w.document()).and_then(|d| {
        d.query_selector(".sidepanel button.primary:not([disabled])")
            .ok()
            .flatten()
    });
    match btn.and_then(|b| b.dyn_into::<web_sys::HtmlElement>().ok()) {
        Some(b) => {
            b.click();
            true
        }
        None => false,
    }
}

/// Esc clears the selection; Enter presses the primary button of the side panel unless the
/// user is typing or focus sits on a link / a button outside the panel.
fn keyboard(ctx: GameCtx, e: web_sys::KeyboardEvent) {
    let el = e
        .target()
        .and_then(|t| t.dyn_into::<web_sys::Element>().ok());
    let tag = el
        .as_ref()
        .map(|el| el.tag_name().to_uppercase())
        .unwrap_or_default();
    if matches!(tag.as_str(), "INPUT" | "TEXTAREA" | "SELECT" | "A")
        || e.ctrl_key()
        || e.meta_key()
        || e.alt_key()
    {
        return;
    }
    match e.key().as_str() {
        "Escape" => ctx.sel.set(Sel::default()),
        "Enter" => {
            let in_panel = tag != "BUTTON"
                || el
                    .and_then(|el| el.closest(".sidepanel").ok().flatten())
                    .is_some();
            if in_panel && click_primary() {
                e.prevent_default();
            }
        }
        _ => {}
    }
}

#[component]
pub fn GameScreen(
    map: SharedMap,
    #[prop(into)] deadline_ms: Signal<Option<f64>>,
    #[prop(optional)] top: Option<ViewFn>,
    #[prop(optional)] bottom: Option<ViewFn>,
) -> impl IntoView {
    let ctx = expect_context::<GameCtx>();
    let model: Memo<BoardModel> = Memo::new(move |_| {
        let ov = overlay(ctx);
        ctx.state.with(|st| board_model(st, &ov))
    });
    let on_select = Callback::new(move |id: Option<String>| {
        if let Some(loc) = id.and_then(|i| LocationId::from_id(&i)) {
            board_click(ctx, loc);
        }
    });
    let focus = Signal::derive(move || {
        ctx.state
            .with(|st| st.battle.as_ref().map(|b| loc_key(b.location)))
    });
    // Mobile bottom sheet: which tab is open (desktop shows everything).
    let mtab = RwSignal::new("board");
    let needs_me = Memo::new(move |_| ctx.acting().is_some_and(|p| ctx.controls(p)));
    Effect::new(move |prev: Option<bool>| {
        let now = needs_me.get();
        if now && prev == Some(false) && mtab.get_untracked() == "board" {
            mtab.set("actions");
        }
        now
    });
    // Browser tab title flags a pending decision ("(!)") so a background tab is noticeable.
    Effect::new(move |_| {
        let title = if needs_me.get() {
            "(!) Your move - Wartide"
        } else {
            "Wartide"
        };
        if let Some(d) = web_sys::window().and_then(|w| w.document()) {
            d.set_title(title);
        }
    });
    on_cleanup(|| {
        if let Some(d) = web_sys::window().and_then(|w| w.document()) {
            d.set_title("Wartide");
        }
    });
    let handle = window_event_listener(leptos::ev::keydown, move |e| keyboard(ctx, e));
    on_cleanup(move || handle.remove());
    let tab = move |id: &'static str, label: &'static str| {
        view! {
            <button class="mtab" class:on=move || mtab.get() == id on:click=move |_| mtab.set(id)>
                {label}
                {(id == "actions").then(|| view! { <Show when=move || needs_me.get()><span class="dot" title="Your move"></span></Show> })}
            </button>
        }
    };
    view! {
        <div class="game" data-mtab=move || mtab.get()>
            <TopBar deadline_ms />
            <div class="game-body">
                <Board map model on_select focus />
                <aside class="side">
                    <section class="sec sec-actions">
                        {top.map(|t| t.run())}
                        <div class="sidepanel"><SidePanel /></div>
                    </section>
                    <section class="sec sec-log"><ActionLog entries=ctx.log /></section>
                    <section class="sec sec-chat">{bottom.map(|t| t.run())}</section>
                </aside>
            </div>
            <nav class="mtabs">
                {tab("board", "Board")} {tab("actions", "Actions")} {tab("chat", "Chat")} {tab("log", "Log")}
            </nav>
        </div>
    }
}

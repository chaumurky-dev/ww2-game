//! The interactive game screen shared by the server-driven game page and the sandbox.
use super::battle_board::{BattleBoardModal, BoardOpen};
use super::conv::{loc_key, loc_name, unit_name};
use super::ctx::{GameCtx, Sel};
use super::drag::{drop_plan, engine_kind, half, max_count, movable_ids, DropPlan};
use super::log::ActionLog;
use super::model::{board_model, Overlay};
use super::panels::{place_at, SidePanel};
use super::topbar::TopBar;
use crate::components::board::{Board, BoardModel, DragEnd};
use crate::map::SharedMap;
use crate::model::UnitKind;
use leptos::prelude::*;
use wartide_engine::query::{battles_pending, legal_placements};
use wartide_engine::{pending_decision, Action, Decision, LocationId, Phase, UnitType};
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

/// Count picker shown after dropping a multi-unit stack on a destination.
#[derive(Clone, Debug, PartialEq)]
struct Picker {
    from: LocationId,
    kind: UnitType,
    dest: LocationId,
    /// Movable units of that kind at `from`.
    total: usize,
    /// Most units this destination accepts (e.g. transport capacity).
    max: usize,
    n: usize,
    /// Pointer position in client pixels (the picker is anchored there).
    x: f64,
    y: f64,
}

/// Execute a drop plan, trimming `sel.units` to the planned units first.
fn run_plan(ctx: GameCtx, from: LocationId, plan: DropPlan) {
    match plan {
        DropPlan::Move { units, path } => {
            ctx.sel.update(|s| {
                s.from = Some(from);
                s.units = units.clone();
                s.dest = None;
            });
            ctx.send(Action::Move { units, path });
        }
        DropPlan::Load { units, transport } => {
            ctx.sel.update(|s| {
                s.from = Some(from);
                s.units = units.clone();
                s.dest = None;
            });
            ctx.send(Action::Load { units, transport });
        }
        // Several transports fit: keep the units selected; the side panel's transport section
        // lists one load button per adjacent transport.
        DropPlan::LoadChoice { units } => ctx.sel.update(|s| {
            s.from = Some(from);
            s.units = units;
            s.dest = None;
        }),
    }
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
        d.query_selector(
            ".battle-modal button.primary:not([disabled]), .sidepanel button.primary:not([disabled])",
        )
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
    provide_context(BoardOpen(RwSignal::new(true)));
    // (location, kind) pairs whose counters may be dragged: the viewer's own movable units.
    let draggable = Memo::new(move |_| {
        ctx.state.with(|st| {
            let Decision::Phase {
                power,
                phase: Phase::CombatMove | Phase::NonCombatMove,
            } = pending_decision(st)
            else {
                return vec![];
            };
            if !ctx.controls(power) || st.battle.is_some() {
                return vec![];
            }
            let mut out = vec![];
            for loc in ctx.movable.get() {
                let mut kinds: Vec<UnitType> = st
                    .units_at(loc)
                    .filter(|u| u.owner == power && u.loaded_on.is_none())
                    .map(|u| u.kind)
                    .collect();
                kinds.sort();
                kinds.dedup();
                for k in kinds {
                    if !movable_ids(st, loc, power, k).is_empty() {
                        out.push((loc, k));
                    }
                }
            }
            out
        })
    });
    let model: Memo<BoardModel> = Memo::new(move |_| {
        let ov = overlay(ctx);
        let drag_ok = draggable.get();
        ctx.state.with(|st| {
            let mut m = board_model(st, &ov);
            for (loc, kind) in &drag_ok {
                if let Some(v) = m.stacks.get_mut(&loc_key(*loc)) {
                    for s in v.iter_mut().filter(|s| engine_kind(s.kind) == *kind) {
                        s.draggable = s.power == st.current_power;
                    }
                }
            }
            m
        })
    });
    // Unit drag-and-drop: the selection before the drag is restored if the drop is cancelled.
    let prev_sel = StoredValue::new(Sel::default());
    let picker = RwSignal::new(None::<Picker>);
    let cancel_drag = move || {
        picker.set(None);
        let st = ctx.state.get_untracked();
        ctx.sel.set(prev_sel.get_value().sanitized(&st));
    };
    let on_drag_start = Callback::new(move |(id, kind): (String, UnitKind)| {
        let Some(from) = LocationId::from_id(&id) else {
            return;
        };
        picker.set(None);
        prev_sel.set_value(ctx.sel.get_untracked());
        let ids = ctx
            .state
            .with_untracked(|st| movable_ids(st, from, st.current_power, engine_kind(kind)));
        if !ids.is_empty() {
            ctx.sel.set(Sel {
                from: Some(from),
                units: ids,
                ..Default::default()
            });
        }
    });
    let on_drag_end = Callback::new(move |e: DragEnd| {
        let (Some(from), Some(dest)) = (
            LocationId::from_id(&e.from),
            e.dest.as_deref().and_then(LocationId::from_id),
        ) else {
            return cancel_drag();
        };
        let kind = engine_kind(e.kind);
        let (total, max) = ctx.state.with_untracked(|st| {
            (
                movable_ids(st, from, st.current_power, kind).len(),
                max_count(st, from, kind, dest),
            )
        });
        if max == 0 {
            return cancel_drag();
        }
        if total <= 1 || max == 1 {
            // Single unit: send straight away, same path as the side panel.
            match ctx
                .state
                .with_untracked(|st| drop_plan(st, from, kind, dest, 1))
            {
                Some(plan) => run_plan(ctx, from, plan),
                None => cancel_drag(),
            }
        } else {
            picker.set(Some(Picker {
                from,
                kind,
                dest,
                total,
                max,
                n: max,
                x: e.x,
                y: e.y,
            }));
        }
    });
    let confirm = move || {
        let Some(p) = picker.get_untracked() else {
            return;
        };
        picker.set(None);
        match ctx
            .state
            .with_untracked(|st| drop_plan(st, p.from, p.kind, p.dest, p.n))
        {
            Some(plan) => run_plan(ctx, p.from, plan),
            None => {
                let st = ctx.state.get_untracked();
                ctx.sel.set(prev_sel.get_value().sanitized(&st));
            }
        }
    };
    let h_pick = window_event_listener(leptos::ev::keydown, move |e| {
        if picker.get_untracked().is_none() {
            return;
        }
        match e.key().as_str() {
            "Enter" => {
                e.prevent_default();
                confirm();
            }
            "Escape" => {
                e.prevent_default();
                cancel_drag();
            }
            _ => {}
        }
    });
    on_cleanup(move || h_pick.remove());
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
    let handle = window_event_listener(leptos::ev::keydown, move |e| {
        if picker.get_untracked().is_none() {
            keyboard(ctx, e)
        }
    });
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
                <Board map model on_select focus on_drag_start on_drag_end />
                <aside class="side">
                    <section class="sec sec-actions">
                        {top.map(|t| t.run())}
                        <div class="sidepanel"><SidePanel /></div>
                    </section>
                    <section class="sec sec-log"><ActionLog entries=ctx.log /></section>
                    <section class="sec sec-chat">{bottom.map(|t| t.run())}</section>
                </aside>
            </div>
            <BattleBoardModal />
            {move || picker.get().map(|p| {
                let vw = web_sys::window()
                    .and_then(|w| w.inner_width().ok())
                    .and_then(|v| v.as_f64())
                    .unwrap_or(800.0);
                let left = p.x.clamp(130.0, (vw - 130.0).max(130.0));
                let n = p.n;
                let set = move |v: usize| picker.update(|q| {
                    if let Some(q) = q {
                        q.n = v.clamp(1, q.max);
                    }
                });
                view! {
                    <div class="count-picker" style=format!("left:{left}px;top:{}px", p.y + 14.0)>
                        <div class="cp-title">
                            {format!("Move {n} of {} {} to {}", p.total, unit_name(p.kind, p.total as u32), loc_name(p.dest))}
                        </div>
                        {(p.max < p.total).then(|| view! { <div class="muted">{format!("At most {} fit there.", p.max)}</div> })}
                        <div class="cp-row">
                            <button class="secondary small" on:click=move |_| set(n.saturating_sub(1))>"\u{2212}"</button>
                            <input type="range" min="1" max=p.max.to_string() prop:value=n.to_string()
                                on:input=move |e| set(event_target_value(&e).parse().unwrap_or(1)) />
                            <button class="secondary small" on:click=move |_| set(n + 1)>"+"</button>
                        </div>
                        <div class="cp-row">
                            <button class="secondary small" on:click=move |_| set(1)>"1"</button>
                            <button class="secondary small" on:click=move |_| set(half(p.max))>"Half"</button>
                            <button class="secondary small" on:click=move |_| set(p.max)>"All"</button>
                        </div>
                        <div class="cp-row">
                            <button class="primary small" on:click=move |_| confirm()>"Move"</button>
                            <button class="secondary small" on:click=move |_| cancel_drag()>"Cancel"</button>
                        </div>
                    </div>
                }
            })}
            <nav class="mtabs">
                {tab("board", "Board")} {tab("actions", "Actions")} {tab("chat", "Chat")} {tab("log", "Log")}
            </nav>
        </div>
    }
}

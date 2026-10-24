//! The interactive game screen shared by the server-driven game page and the sandbox.
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

fn overlay(ctx: GameCtx) -> Overlay {
    let st = ctx.state.get();
    let sel = ctx.sel.get();
    let mut ov = Overlay {
        picked: sel.units.clone(),
        selected: sel.from,
        ..Default::default()
    };
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
    let collapsed = RwSignal::new(false);
    view! {
        <div class="game">
            <TopBar deadline_ms />
            <div class="game-body">
                <Board map model on_select />
                <aside class="side" class:collapsed=move || collapsed.get()>
                    <button class="secondary small side-toggle" on:click=move |_| collapsed.update(|c| *c = !*c)>
                        {move || if collapsed.get() { "Show panel" } else { "Hide panel" }}
                    </button>
                    {top.map(|t| t.run())}
                    <SidePanel />
                    <ActionLog entries=ctx.log />
                    {bottom.map(|t| t.run())}
                </aside>
            </div>
        </div>
    }
}

use super::common::EndPhase;
use super::transport::TransportSection;
use super::undo::{undo_button, MovedSection, UndoAll};
use crate::components::unit_icon::UnitIcon;
use crate::game::conv::*;
use crate::game::ctx::GameCtx;
use crate::game::moves::{stranded_air, unit_groups, UnitGroup};
use leptos::prelude::*;
use wartide_engine::action::UndoScope;
use wartide_engine::undo::moved_this_phase;
use wartide_engine::{Action, Power, UnitId};

fn group_row(
    ctx: GameCtx,
    power: Power,
    g: UnitGroup,
    picked: usize,
    arrived: Vec<UnitId>,
) -> impl IntoView {
    let n = g.ids.len();
    let (ids_a, ids_s, ids_all) = (g.ids.clone(), g.ids.clone(), g.ids.clone());
    let add = move |_| {
        ctx.sel.update(|s| {
            if let Some(&id) = ids_a.iter().find(|i| !s.units.contains(i)) {
                s.units.push(id);
                s.dest = None;
            }
        })
    };
    let sub = move |_| {
        ctx.sel.update(|s| {
            if let Some(&id) = ids_s.iter().rev().find(|i| s.units.contains(i)) {
                s.units.retain(|x| *x != id);
                s.dest = None;
            }
        })
    };
    let all = move |_| {
        ctx.sel.update(|s| {
            let every = ids_all.iter().all(|i| s.units.contains(i));
            s.units.retain(|x| !ids_all.contains(x));
            if !every {
                s.units.extend(ids_all.iter().copied());
            }
            s.dest = None;
        })
    };
    let can = g.can_move;
    view! {
        <div class="urow" class:dim=!can>
            <UnitIcon kind=ui_kind(g.kind) power=power />
            <span class="grow">
                {format!("{} \u{d7}{n}", ui_kind(g.kind).name())}
                {g.aboard.then_some(" (on carrier)")}
                <span class="muted">{if can { format!(" \u{b7} {} move{} left", g.moves_left, if g.moves_left == 1 { "" } else { "s" }) } else { " \u{b7} cannot move".into() }}</span>
            </span>
            <button class="secondary small" disabled=!can || picked == 0 on:click=sub>"\u{2212}"</button>
            <strong class="stepval">{picked}</strong>
            <button class="secondary small" disabled=!can || (picked >= n) on:click=add>"+"</button>
            <button class="secondary small" disabled=!can on:click=all>"All"</button>
            {(!arrived.is_empty()).then(|| undo_button(ctx, UndoScope::Units { units: arrived }, "\u{21b6}", "secondary small undo-unit"))}
        </div>
    }
}

#[component]
pub fn MovePanel(power: Power, combat: bool) -> impl IntoView {
    let ctx = expect_context::<GameCtx>();
    let picker = move || {
        let st = ctx.state.get();
        let sel = ctx.sel.get();
        let Some(from) = sel.from else {
            return view! {
                <p class="muted">"Click a location with your units (outlined on the map)."</p>
            }
            .into_any();
        };
        let groups = unit_groups(&st, from, power);
        let moved: Vec<UnitId> = moved_this_phase(&st)
            .into_iter()
            .filter(|m| m.2 == from)
            .map(|m| m.0)
            .collect();
        let rows = groups
            .into_iter()
            .map(|g| {
                let picked = g.ids.iter().filter(|i| sel.units.contains(i)).count();
                let arrived: Vec<UnitId> = g
                    .ids
                    .iter()
                    .copied()
                    .filter(|i| moved.contains(i))
                    .collect();
                group_row(ctx, power, g, picked, arrived)
            })
            .collect_view();
        view! {
            <div class="row">
                <strong class="grow">{loc_name(from)}</strong>
                <button class="secondary small" on:click=move |_| ctx.sel.set(Default::default())>"Clear"</button>
            </div>
            {rows}
            <TransportSection from power />
        }.into_any()
    };
    let preview = move || {
        let sel = ctx.sel.get();
        if sel.units.is_empty() {
            return ().into_any();
        }
        let n = sel.units.len();
        let Some(dest) = sel.dest else {
            let any = ctx.reach.with(|r| !r.is_empty());
            return view! {
                <div class="hint">{if any { "Click a highlighted destination on the map." } else { "No common destination for this selection." }}</div>
            }.into_any();
        };
        let Some(r) = ctx.reach.get().into_iter().find(|r| r.to == dest) else {
            return ().into_any();
        };
        let from = sel.from;
        let air = ctx.state.with(|st| {
            sel.units
                .iter()
                .any(|&u| st.unit(u).is_some_and(|u| u.kind.is_air()))
        });
        let route = from
            .into_iter()
            .chain(r.path.iter().copied())
            .map(loc_name)
            .collect::<Vec<_>>()
            .join(" \u{2192} ");
        let (units, path) = (sel.units.clone(), r.path.clone());
        view! {
            <div class="preview">
                <div>{route}</div>
                {r.battle.then(|| view! { <div class="tag attack">"\u{2694} Starts a battle"</div> })}
                {(air && !combat).then(|| if r.landing {
                    view! { <div class="tag ok">"Landing spot available"</div> }.into_any()
                } else {
                    view! { <div class="tag bad">"No landing spot there"</div> }.into_any()
                })}
                <button class="primary" on:click=move |_| ctx.send(Action::Move { units: units.clone(), path: path.clone() })>
                    {format!("Move {n} unit{}", if n == 1 { "" } else { "s" })}
                </button>
            </div>
        }.into_any()
    };
    let warn = move || {
        let st = ctx.state.get();
        let v = stranded_air(&st, power, !combat);
        (!v.is_empty()).then(|| {
            let names: Vec<String> = v
                .iter()
                .filter_map(|&i| st.unit(i))
                .map(|u| format!("{} at {}", ui_kind(u.kind).name(), loc_name(u.location)))
                .collect();
            view! {
                <div class="warn">
                    "These aircraft will be lost unless they can land: " {names.join("; ")}
                    ". Move them to a highlighted landing spot first."
                </div>
            }
        })
    };
    view! {
        <h3>{if combat { "Combat movement" } else { "Non-combat movement" }}</h3>
        {picker}
        {preview}
        {warn}
        <MovedSection />
        <div class="row">
            <EndPhase label=if combat { "End combat move" } else { "End non-combat move" } secondary=true />
            <UndoAll />
        </div>
    }
}

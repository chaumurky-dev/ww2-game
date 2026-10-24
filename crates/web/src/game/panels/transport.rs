//! Load / unload controls for the selected location.
use crate::game::conv::*;
use crate::game::ctx::GameCtx;
use leptos::prelude::*;
use wartide_engine::transport::{used_slots, validate_load, validate_unload, UnloadMode};
use wartide_engine::{Action, LocationId, MapData, Power, UnitId, UnitType};

#[component]
pub fn TransportSection(from: LocationId, power: Power) -> impl IntoView {
    let ctx = expect_context::<GameCtx>();
    let map = MapData::get();
    let loads = move || {
        let st = ctx.state.get();
        let sel = ctx.sel.get();
        if sel.units.is_empty() || map.is_sea(from) {
            return ().into_any();
        }
        let btns = st
            .units
            .iter()
            .filter(|t| t.kind == UnitType::Transport && map.adjacent(t.location, from))
            .map(|t| {
                let ok = validate_load(&st, power, &sel.units, t.id);
                let id = t.id;
                let units = sel.units.clone();
                let free = 2 - used_slots(&st, id).min(2);
                (
                    format!(
                        "{} (#{id}, {free} slot{} free)",
                        loc_name(t.location),
                        if free == 1 { "" } else { "s" }
                    ),
                    ok.map_err(|e| e.to_string()),
                    Action::Load {
                        units,
                        transport: id,
                    },
                )
            })
            .collect::<Vec<_>>();
        if btns.is_empty() {
            return ().into_any();
        }
        view! {
            <div class="sub">
                <strong>"Load selected onto transport"</strong>
                {btns.into_iter().map(|(label, ok, act)| {
                    let title = ok.clone().err().unwrap_or_default();
                    view! {
                        <button class="secondary small" disabled=ok.is_err() title=title on:click=move |_| ctx.send(act.clone())>{label}</button>
                    }
                }).collect_view()}
            </div>
        }.into_any()
    };
    let unloads = move || {
        let st = ctx.state.get();
        st.units_at(from)
            .filter(|t| t.owner == power && t.kind == UnitType::Transport)
            .filter(|t| st.cargo_of(t.id).any(|c| c.owner == power))
            .map(|t| {
                let tid = t.id;
                let cargo: Vec<(UnitId, UnitType)> = st
                    .cargo_of(tid)
                    .filter(|c| c.owner == power)
                    .map(|c| (c.id, c.kind))
                    .collect();
                let chosen = RwSignal::new(cargo.iter().map(|c| c.0).collect::<Vec<_>>());
                let checks = cargo
                    .iter()
                    .map(|&(id, kind)| view! {
                        <label class="check">
                            <input type="checkbox" prop:checked=move || chosen.with(|c| c.contains(&id))
                                on:change=move |_| chosen.update(|c| if c.contains(&id) { c.retain(|x| *x != id) } else { c.push(id) }) />
                            {format!("{} #{id}", ui_kind(kind).name())}
                        </label>
                    })
                    .collect_view();
                let dests = map
                    .neighbours(from)
                    .iter()
                    .copied()
                    .filter(|&l| !map.is_sea(l))
                    .map(|to| {
                        let st2 = st.clone();
                        view! {
                            {move || {
                                let ids = chosen.get();
                                let r = if ids.is_empty() {
                                    Err("select cargo".to_string())
                                } else {
                                    validate_unload(&st2, power, tid, &ids, to).map_err(|e| e.to_string())
                                };
                                let assault = matches!(r, Ok(UnloadMode::Deferred));
                                let label = format!("\u{2192} {}{}", loc_name(to), if assault { " \u{2694} assault" } else { "" });
                                let title = r.clone().err().unwrap_or_default();
                                view! {
                                    <button class="secondary small" disabled=r.is_err() title=title
                                        on:click=move |_| ctx.send(Action::Unload { transport: tid, units: ids.clone(), to })>{label}</button>
                                }
                            }}
                        }
                    })
                    .collect_view();
                view! {
                    <div class="sub">
                        <strong>{format!("Transport #{tid}: unload")}</strong>
                        <div class="col">{checks}</div>
                        <div class="row">{dests}</div>
                    </div>
                }
            })
            .collect_view()
    };
    view! { {loads} {unloads} }
}

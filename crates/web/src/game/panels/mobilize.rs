use super::common::{EndPhase, Stepper};
use crate::components::unit_icon::UnitBadge;
use crate::game::conv::*;
use crate::game::ctx::GameCtx;
use leptos::prelude::*;
use wartide_engine::economy::{complex_capacity_left, usable_complex};
use wartide_engine::query::legal_placements;
use wartide_engine::{Action, GameState, LocationId, MapData, Power, UnitType};

/// Complexes that could supply `count` ships placed in `zone`.
fn suppliers(st: &GameState, p: Power, zone: LocationId, count: u32) -> Vec<LocationId> {
    MapData::get()
        .neighbours(zone)
        .iter()
        .copied()
        .filter(|&t| {
            !MapData::get().is_sea(t)
                && usable_complex(st, p, t)
                && complex_capacity_left(st, p, t) >= count
        })
        .collect()
}

/// Board click while a unit type is chosen for placement.
pub fn place_at(ctx: GameCtx, loc: LocationId) {
    let sel = ctx.sel.get_untracked();
    let Some(kind) = sel.place else { return };
    let st = ctx.state.get_untracked();
    let p = st.current_power;
    let count = sel.place_count.max(1);
    if !legal_placements(&st, kind).contains(&loc) {
        return;
    }
    let is_ship = !kind.is_land() && !kind.is_air() && kind != UnitType::IndustrialComplex;
    if is_ship && suppliers(&st, p, loc, count).len() > 1 {
        ctx.sel.update(|s| s.place_zone = Some(loc));
        return;
    }
    ctx.send(Action::Place {
        unit: kind,
        location: loc,
        count,
        from: None,
    });
}

#[component]
pub fn MobilizePanel(power: Power) -> impl IntoView {
    let ctx = expect_context::<GameCtx>();
    let count = RwSignal::new(1u32);
    Effect::new(move |_| {
        let c = count.get();
        ctx.sel.update(|s| s.place_count = c);
    });
    view! {
        <h3>"Place new units"</h3>
        {move || {
            let st = ctx.state.get();
            let sel = ctx.sel.get();
            if st.purchases.is_empty() {
                return view! { <p class="muted">"Everything is placed."</p> }.into_any();
            }
            let items = st.purchases.clone();
            let left = sel.place.and_then(|k| items.iter().find(|i| i.unit == k)).map_or(1, |i| i.count);
            let multi = !matches!(sel.place, Some(UnitType::AaGun | UnitType::IndustrialComplex));
            let zone_pick = sel.place_zone.map(|z| {
                let kind = sel.place.unwrap_or(UnitType::Transport);
                let opts = suppliers(&st, power, z, sel.place_count.max(1));
                view! {
                    <div class="hint">
                        {format!("Which complex supplies the {} in {}?", unit_name(kind, 2), loc_name(z))}
                        <div class="row">
                            {opts.into_iter().map(|o| view! {
                                <button class="secondary small" on:click=move |_| ctx.send(Action::Place {
                                    unit: kind, location: z, count: ctx.sel.get_untracked().place_count.max(1), from: Some(o),
                                })>{format!("{} ({} left)", loc_name(o), complex_capacity_left(&ctx.state.get_untracked(), power, o).min(99))}</button>
                            }).collect_view()}
                        </div>
                    </div>
                }
            });
            view! {
                <p class="muted">"Choose a unit type, then click a highlighted location."</p>
                <div class="row">
                    {items.into_iter().map(|i| {
                        let k = i.unit;
                        let on = sel.place == Some(k);
                        view! {
                            <button class="chip" class:on=on on:click=move |_| ctx.sel.update(|s| {
                                s.place = Some(k);
                                s.place_zone = None;
                                s.place_count = 1;
                                count.set(1);
                            })>
                                <UnitBadge kind=ui_kind(k) power=ui_power(power) count=i.count />
                            </button>
                        }
                    }).collect_view()}
                </div>
                {(sel.place.is_some() && multi).then(|| view! {
                    <div class="row">"How many per click " <Stepper value=count max=Signal::derive(move || left) /></div>
                })}
                {zone_pick}
            }.into_any()
        }}
        {move || {
            let n: u32 = ctx.state.with(|s| s.purchases.iter().map(|i| i.count).sum());
            (n > 0).then(|| view! { <div class="warn">{format!("{n} unplaced unit(s) will be lost if you end the phase.")}</div> })
        }}
        <EndPhase label="End placement" />
    }
}

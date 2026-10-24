use super::common::Stepper;
use crate::components::unit_icon::UnitIcon;
use crate::game::conv::*;
use crate::game::ctx::GameCtx;
use leptos::prelude::*;
use wartide_engine::economy::{purchase_cost, NEW_COMPLEX_CAP};
use wartide_engine::state::PurchaseItem;
use wartide_engine::tech::unit_cost;
use wartide_engine::{Action, Power, UnitType};

#[component]
pub fn PurchasePanel(power: Power) -> impl IntoView {
    let ctx = expect_context::<GameCtx>();
    let st = ctx.state.get_untracked();
    let budget = st.treasury[power] + purchase_cost(&st, power, &st.purchases);
    let complex_cap = if st.settings.original_complexes_only {
        0
    } else {
        NEW_COMPLEX_CAP.saturating_sub(st.new_complexes_placed)
    };
    let rows: Vec<(UnitType, u32, RwSignal<u32>)> = UnitType::ALL
        .into_iter()
        .map(|k| {
            let have = st
                .purchases
                .iter()
                .find(|i| i.unit == k)
                .map_or(0, |i| i.count);
            (k, unit_cost(&st, power, k), RwSignal::new(have))
        })
        .collect();
    let qty = rows.clone();
    let total = Memo::new(move |_| qty.iter().map(|(_, c, q)| c * q.get()).sum::<u32>());
    let trs = rows
        .iter()
        .map(|&(k, cost, q)| {
            let base = k.stats().cost;
            let rows2 = rows.clone();
            let max = Signal::derive(move || {
                let others: u32 = rows2.iter().filter(|r| r.0 != k).map(|r| r.1 * r.2.get()).sum();
                let by_money = budget.saturating_sub(others) / cost.max(1);
                if k == UnitType::IndustrialComplex {
                    by_money.min(complex_cap)
                } else {
                    by_money
                }
            });
            view! {
                <tr class:dim=move || max.get() == 0 && q.get() == 0>
                    <td><UnitIcon kind=ui_kind(k) power=ui_power(power) /></td>
                    <td>{ui_kind(k).name()}</td>
                    <td class="num">
                        {cost}
                        {(cost != base).then(|| view! { <span class="muted" title="Industrial technology">{format!(" ({base})")}</span> })}
                    </td>
                    <td><Stepper value=q max=max /></td>
                </tr>
            }
        })
        .collect_view();
    let rows3 = rows.clone();
    let items = move || -> Vec<PurchaseItem> {
        rows3
            .iter()
            .filter(|r| r.2.get_untracked() > 0)
            .map(|r| PurchaseItem {
                unit: r.0,
                count: r.2.get_untracked(),
            })
            .collect()
    };
    let items2 = items.clone();
    let over = move || total.get() > budget;
    view! {
        <h3>"Purchase units"</h3>
        <table class="ptable">
            <thead><tr><th></th><th>"Unit"</th><th class="num">"IPC"</th><th>"Qty"</th></tr></thead>
            <tbody>{trs}</tbody>
        </table>
        <div class="row">
            <span>{move || format!("Total {} / {} IPC", total.get(), budget)}</span>
            <span class="spacer"></span>
            <button class="primary" disabled=over on:click=move |_| {
                let it = items();
                if it.is_empty() {
                    ctx.send(Action::EndPhase);
                } else {
                    ctx.send_all(vec![Action::Purchase { items: it }, Action::EndPhase]);
                }
            }>{move || if total.get() == 0 { "Buy nothing" } else { "Confirm purchase" }}</button>
        </div>
        <div class="row">
            <button class="secondary small" disabled=move || (total.get() == 0) || (total.get() > budget)
                on:click=move |_| ctx.send(Action::Purchase { items: items2() })>"Save list (stay here)"</button>
            <button class="secondary small" on:click=move |_| ctx.send(Action::EndPhase)>"Continue with current list"</button>
        </div>
    }
}

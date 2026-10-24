//! Contextual right-hand panel driven by the current phase.
use super::battle::BattleBoard;
use super::board::Stack;
use super::unit_icon::{UnitBadge, UnitIcon};
use crate::mock::{Phase, Power, UnitKind};
use leptos::prelude::*;

#[component]
fn Stepper(value: RwSignal<u32>, #[prop(default = 99)] max: u32) -> impl IntoView {
    view! {
        <span class="row" style="gap:.2rem">
            <button class="secondary small" on:click=move |_| value.update(|v| *v = v.saturating_sub(1))>"\u{2212}"</button>
            <strong style="min-width:1.5rem;text-align:center">{move || value.get()}</strong>
            <button class="secondary small" on:click=move |_| value.update(|v| *v = (*v + 1).min(max))>"+"</button>
        </span>
    }
}

#[component]
fn PurchasePanel(ipc: u32, on_action: Callback<String>) -> impl IntoView {
    let qty: Vec<RwSignal<u32>> = UnitKind::ALL.iter().map(|_| RwSignal::new(0)).collect();
    let q2 = qty.clone();
    let total = Memo::new(move |_| -> u32 { q2.iter().zip(UnitKind::ALL).map(|(q, k)| q.get() * k.cost()).sum() });
    let rows = UnitKind::ALL
        .iter()
        .zip(qty.iter().copied())
        .map(|(k, q)| {
            view! {
                <tr>
                    <td><UnitIcon kind=*k /></td>
                    <td>{k.name()}</td>
                    <td class="num">{k.cost()}</td>
                    <td><Stepper value=q /></td>
                </tr>
            }
        })
        .collect_view();
    let over = move || total.get() > ipc;
    let confirm = move |_: web_sys::MouseEvent| on_action.run(format!("Purchased units for {} IPC", total.get()));
    view! {
        <h3>"Purchase units"</h3>
        <table>
            <thead><tr><th></th><th>"Unit"</th><th class="num">"IPC"</th><th>"Qty"</th></tr></thead>
            <tbody>{rows}</tbody>
        </table>
        <div class="row">
            <span>{move || format!("Total {} / {} IPC", total.get(), ipc)}</span>
            <span class="spacer"></span>
            <button disabled=over on:click=confirm>"Confirm purchase"</button>
        </div>
    }
}

#[component]
fn MovePanel(
    combat: bool,
    selected: Signal<Option<(String, Vec<Stack>)>>,
    destinations: Vec<(String, String)>,
    on_action: Callback<String>,
) -> impl IntoView {
    let dest = RwSignal::new(destinations.first().map(|d| d.0.clone()).unwrap_or_default());
    let opts = destinations
        .iter()
        .map(|(id, name)| view! { <option value=id.clone()>{name.clone()}</option> })
        .collect_view();
    let title = if combat { "Combat movement" } else { "Non-combat movement" };
    view! {
        <h3>{title}</h3>
        {move || match selected.get() {
            None => view! { <p class="muted">"Select one of your territories on the map."</p> }.into_any(),
            Some((name, stacks)) => {
                let name2 = name.clone();
                view! {
                    <div class="col">
                        <strong>{name}</strong>
                        <div class="row">
                            {stacks.into_iter().map(|s| view! { <UnitBadge kind=s.kind power=s.power count=s.count /> }).collect_view()}
                        </div>
                        <label>"Destination"
                            <select on:change:target=move |ev| dest.set(ev.target().value())>{opts.clone()}</select>
                        </label>
                        <button on:click=move |_| on_action.run(format!("Moved units from {name2} to {}", dest.get_untracked()))>
                            "Move selected units"
                        </button>
                    </div>
                }
                    .into_any()
            }
        }}
        <button class="secondary" on:click=move |_| on_action.run("Ended movement".into())>"End phase"</button>
    }
}

#[component]
fn PlacePanel(power: Power, on_action: Callback<String>) -> impl IntoView {
    view! {
        <h3>"Place new units"</h3>
        <div class="row">
            <UnitBadge kind=UnitKind::Infantry power=power count=3 />
            <UnitBadge kind=UnitKind::Armor power=power count=1 />
        </div>
        <label>"Industrial complex"
            <select><option>"Home territory"</option></select>
        </label>
        <button on:click=move |_| on_action.run("Placed purchased units".into())>"Place all"</button>
    }
}

#[component]
fn ResearchPanel(on_action: Callback<String>) -> impl IntoView {
    let dice = RwSignal::new(0_u32);
    view! {
        <h3>"Develop weapons"</h3>
        <p class="muted">"Each research die costs 5 IPC; a 6 breaks through."</p>
        <div class="row">"Dice " <Stepper value=dice max=5 /></div>
        <div class="row">
            <button on:click=move |_| on_action.run(format!("Bought {} research dice", dice.get_untracked()))>"Roll research"</button>
            <button class="secondary" on:click=move |_| on_action.run("Skipped research".into())>"Skip"</button>
        </div>
    }
}

#[component]
pub fn ActionPanel(
    phase: Phase,
    power: Power,
    ipc: u32,
    selected: Signal<Option<(String, Vec<Stack>)>>,
    destinations: Vec<(String, String)>,
    on_action: Callback<String>,
) -> impl IntoView {
    match phase {
        Phase::DevelopWeapons => view! { <ResearchPanel on_action /> }.into_any(),
        Phase::Purchase => view! { <PurchasePanel ipc on_action /> }.into_any(),
        Phase::CombatMove => view! { <MovePanel combat=true selected destinations on_action /> }.into_any(),
        Phase::Combat => view! {
            <BattleBoard place="Germany".to_string() attacker=power defender=Power::Germany on_resolved=on_action />
        }
            .into_any(),
        Phase::NonCombatMove => view! { <MovePanel combat=false selected destinations on_action /> }.into_any(),
        Phase::Mobilize => view! { <PlacePanel power on_action /> }.into_any(),
        Phase::CollectIncome => view! {
            <h3>"Collect income"</h3>
            <p>{format!("You collect {ipc} IPC this turn.")}</p>
            <button on:click=move |_| on_action.run("Ended turn".into())>"End turn"</button>
        }
            .into_any(),
    }
}

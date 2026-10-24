use super::countdown::Countdown;
use super::power_badge::PowerBadge;
use crate::mock::{Phase, Power, Treasury};
use leptos::prelude::*;

/// Top bar: round, current power, seven-phase stepper, treasury and deadline.
#[component]
pub fn PhaseBar(
    round: u32,
    power: Power,
    phase: Phase,
    treasury: Vec<Treasury>,
    deadline_ms: f64,
) -> impl IntoView {
    let steps = Phase::ALL
        .iter()
        .map(|p| {
            let class = if *p == phase {
                "step on"
            } else if p.index() < phase.index() {
                "step done"
            } else {
                "step"
            };
            view! { <span class=class>{p.label()}</span> }
        })
        .collect_view();
    let mine = treasury
        .iter()
        .find(|t| t.power == power)
        .map(|t| t.ipc)
        .unwrap_or(0);
    let all = treasury
        .into_iter()
        .map(|t| view! { <span class="pill" style=format!("border-color:{}", t.power.color())>{format!("{} {}", t.power.id().to_uppercase(), t.ipc)}</span> })
        .collect_view();
    view! {
        <div class="topbar">
            <h3 style="margin:0">{format!("Round {round}")}</h3>
            <PowerBadge power=power />
            <div class="stepper">{steps}</div>
            <span class="pill hot" title="Treasury of the acting power">{format!("{mine} IPC")}</span>
            <span class="row" style="gap:.2rem">{all}</span>
            <span class="spacer"></span>
            <span class="muted">"Deadline "</span>
            <Countdown deadline_ms=deadline_ms />
        </div>
    }
}

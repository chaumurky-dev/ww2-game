//! Phase panels: the side panel is chosen from `pending_decision(state)`.
mod combat;
mod common;
mod mobilize;
mod movement;
mod purchase;
mod research;
mod transport;

pub use mobilize::place_at;

use super::battle::BattleBoard;
use super::conv::*;
use super::ctx::GameCtx;
use crate::components::power_badge::PowerBadge;
use common::EndPhase;
use leptos::prelude::*;
use wartide_engine::{pending_decision, Decision, Phase, Power};

fn waiting(power: Power, what: &str) -> impl IntoView {
    let what = what.to_string();
    view! {
        <div class="wait">
            <PowerBadge power=ui_power(power) />
            <span>{format!(" is deciding ({what}) - waiting for them.")}</span>
        </div>
    }
}

#[component]
pub fn SidePanel() -> impl IntoView {
    let ctx = expect_context::<GameCtx>();
    view! {
        {move || {
            let st = ctx.state.get();
            let income = ctx.income.get();
            let income_note = income.map(|(p, amount, treasury)| view! {
                <div class="income-note">
                    {format!("{} collected {amount} IPC (treasury {treasury}).", p.name())}
                </div>
            });
            let body = match pending_decision(&st) {
                Decision::GameOver { winner } => view! {
                    <div class="gameover">
                        <h3>"Game over"</h3>
                        <p>{match winner { Some(t) => format!("{t:?} win{}.", st.winner.map(|w| format!(" ({:?})", w.reason)).unwrap_or_default()), None => "No winner.".into() }}</p>
                    </div>
                }.into_any(),
                Decision::Phase { power, phase } if st.battle.is_none() => {
                    if !ctx.controls(power) {
                        waiting(power, phase_label(phase)).into_any()
                    } else {
                        match phase {
                            Phase::DevelopWeapons => view! { <research::ResearchPanel power /> }.into_any(),
                            Phase::Purchase => view! { <purchase::PurchasePanel power /> }.into_any(),
                            Phase::CombatMove => view! { <movement::MovePanel power combat=true /> }.into_any(),
                            Phase::Combat => view! { <combat::CombatPanel power /> }.into_any(),
                            Phase::NonCombatMove => view! { <movement::MovePanel power combat=false /> }.into_any(),
                            Phase::Mobilize => view! { <mobilize::MobilizePanel power /> }.into_any(),
                            Phase::CollectIncome => view! {
                                <h3>"Collect income"</h3>
                                <p class="muted">"Income is collected automatically."</p>
                                <EndPhase label="Continue" />
                            }.into_any(),
                        }
                    }
                }
                Decision::Phase { .. } => view! { <BattleBoard /> }.into_any(),
                Decision::SelectCasualties { power, .. }
                | Decision::RetreatOrPress { power, .. }
                | Decision::SubWithdrawal { power, .. } => view! {
                    {(!ctx.controls(power)).then(|| waiting(power, "battle decision"))}
                    <BattleBoard />
                }.into_any(),
            };
            view! { {income_note} {body} }
        }}
    }
}

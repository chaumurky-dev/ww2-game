use super::common::{EndPhase, Stepper};
use crate::game::conv::*;
use crate::game::ctx::GameCtx;
use crate::game::dice::DiceBoard;
use leptos::prelude::*;
use wartide_engine::tech::RESEARCH_DIE_COST;
use wartide_engine::{Action, Power};

#[component]
pub fn ResearchPanel(power: Power) -> impl IntoView {
    let ctx = expect_context::<GameCtx>();
    let st = ctx.state.get_untracked();
    let max = st.treasury[power] / RESEARCH_DIE_COST;
    let researched = st.turn.researched;
    let dice = RwSignal::new(if researched { 0 } else { 1.min(max) });
    let techs = st.techs[power].clone();
    view! {
        <h3>"Develop weapons"</h3>
        <p class="muted">{format!("Each research die costs {RESEARCH_DIE_COST} IPC; any 6 is a breakthrough and a second die picks the technology.")}</p>
        <p>"Owned: " {if techs.is_empty() { "none".to_string() } else { techs.iter().map(|t| tech_name(*t)).collect::<Vec<_>>().join(", ") }}</p>
        {if researched {
            view! { <p>"Research already rolled this turn."</p> }.into_any()
        } else {
            view! {
                <div class="row">
                    "Dice " <Stepper value=dice max=max />
                    <span class="muted">{move || format!("= {} IPC of {}", dice.get() * RESEARCH_DIE_COST, st.treasury[power])}</span>
                </div>
                <div class="row">
                    <button disabled=move || dice.get() == 0
                        on:click=move |_| ctx.send(Action::BuyResearch { dice: dice.get_untracked() })>"Roll research"</button>
                </div>
            }.into_any()
        }}
        <DiceBoard />
        <div class="row"><EndPhase label=if researched { "Continue" } else { "Skip research" } /></div>
    }
}

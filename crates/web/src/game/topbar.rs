//! Top bar: round, acting power, phase stepper, treasuries, production, techs, deadline.
use super::conv::*;
use super::ctx::GameCtx;
use crate::components::countdown::Countdown;
use crate::components::power_badge::PowerBadge;
use crate::model::PowerExt;
use leptos::prelude::*;
use wartide_engine::{production_level, Phase, Power};

#[component]
pub fn TopBar(#[prop(into)] deadline_ms: Signal<Option<f64>>) -> impl IntoView {
    let ctx = expect_context::<GameCtx>();
    // Phone: the stepper and the treasury strip fold away behind the summary line.
    let open = RwSignal::new(false);
    view! {
        <div class="topbar" class:open=move || open.get()>
            <h3 style="margin:0">{move || format!("Round {}", ctx.state.with(|s| s.round))}</h3>
            {move || view! { <PowerBadge power=ctx.state.with(|s| s.current_power) /> }}
            <span class="phase-now">{move || phase_label(ctx.state.with(|s| s.phase))}</span>
            {move || deadline_ms.get().map(|d| view! {
                <span class="deadline" title="Time left to act">
                    <span class="muted dl-text">{format!("Deadline: {} ", crate::tz::deadline_text(d))}</span>
                    <span class="dl-count">"(" <Countdown deadline_ms=d />")"</span>
                </span>
            })}
            <button class="topbar-toggle secondary small" aria-label="Show phases and treasuries"
                aria-expanded=move || open.get().to_string()
                on:click=move |_| open.update(|o| *o = !*o)>
                {move || if open.get() { "\u{25B4}" } else { "\u{25BE}" }}
            </button>
            <div class="stepper">
                {move || {
                    let cur = ctx.state.with(|s| s.phase);
                    let idx = |p: Phase| PHASES.iter().position(|x| *x == p).unwrap_or(0);
                    PHASES.iter().map(|p| {
                        let class = if *p == cur { "step on" } else if idx(*p) < idx(cur) { "step done" } else { "step" };
                        view! { <span class=class>{phase_label(*p)}</span> }
                    }).collect_view()
                }}
            </div>
            <div class="row econ">
                {move || {
                    let st = ctx.state.get();
                    Power::ALL.into_iter().map(|p| {
                        let techs = st.techs[p].iter().map(|t| tech_short(*t)).collect::<Vec<_>>().join(" ");
                        let title = st.techs[p].iter().map(|t| tech_name(*t)).collect::<Vec<_>>().join(", ");
                        let cur = p == st.current_power;
                        view! {
                            <span class="pill econ-pill" class:hot=cur style=format!("border-color:{}", p.color())
                                title=format!("{}: treasury {}, production {}{}", p.name(), st.treasury[p], production_level(&st, p),
                                    if title.is_empty() { String::new() } else { format!(", tech: {title}") })>
                                <b>{p.id().to_uppercase()}</b>
                                {format!(" {} IPC (+{})", st.treasury[p], production_level(&st, p))}
                                {(!techs.is_empty()).then(|| view! { <span class="techs">{format!(" {techs}")}</span> })}
                            </span>
                        }
                    }).collect_view()
                }}
            </div>
        </div>
    }
}

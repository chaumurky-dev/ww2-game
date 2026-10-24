//! Small shared panel widgets.
use crate::game::ctx::GameCtx;
use leptos::prelude::*;
use wartide_engine::Action;

#[component]
pub fn Stepper(value: RwSignal<u32>, #[prop(into)] max: Signal<u32>) -> impl IntoView {
    view! {
        <span class="row stepper-s">
            <button class="secondary small" on:click=move |_| value.update(|v| *v = v.saturating_sub(1))>"\u{2212}"</button>
            <strong class="stepval">{move || value.get()}</strong>
            <button class="secondary small" on:click=move |_| value.update(|v| *v = (*v + 1).min(max.get()))>"+"</button>
        </span>
    }
}

/// Button sending `EndPhase`.
#[component]
pub fn EndPhase(#[prop(into)] label: String, #[prop(optional)] secondary: bool) -> impl IntoView {
    let ctx = expect_context::<GameCtx>();
    view! {
        <button class:secondary=secondary class:primary=!secondary on:click=move |_| ctx.send(Action::EndPhase)>{label}</button>
    }
}

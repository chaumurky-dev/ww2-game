use crate::mock::Power;
use leptos::prelude::*;

#[component]
pub fn PowerBadge(power: Power, #[prop(optional)] short: bool) -> impl IntoView {
    let label = if short {
        power.id().to_uppercase()
    } else {
        power.name().to_string()
    };
    view! {
        <span class=format!("power-badge power-{}", power.id()) title=power.name()>
            <span class="dot"></span>
            {label}
        </span>
    }
}

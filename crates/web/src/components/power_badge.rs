use crate::model::Power;
use leptos::prelude::*;

/// Power name pill with the power's emblem from the sprite (`power-<id>`).
#[component]
pub fn PowerBadge(power: Power, #[prop(optional)] short: bool) -> impl IntoView {
    let label = if short {
        power.id().to_uppercase()
    } else {
        power.name().to_string()
    };
    view! {
        <span class=format!("power-badge power-{}", power.id()) title=power.name()>
            <svg class="emblem" viewBox="0 0 64 64" aria-hidden="true">
                <use href=format!("/assets/icons/sprite.svg#power-{}", power.id()) />
            </svg>
            {label}
        </span>
    }
}

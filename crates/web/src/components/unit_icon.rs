use crate::model::{Power, PowerExt, UnitKind};
use leptos::prelude::*;

/// Unit icon from the shared SVG sprite (`/assets/icons/sprite.svg#unit-<id>`), drawn in
/// `currentColor`, which is set to the power colour when one is given.
#[component]
pub fn UnitIcon(kind: UnitKind, #[prop(optional)] power: Option<Power>) -> impl IntoView {
    let style = power
        .map(|p| format!("color:{}", p.color()))
        .unwrap_or_default();
    view! {
        <span class="unit-icon" style=style title=kind.name()>
            <svg viewBox="0 0 64 64" aria-hidden="true">
                <use href=format!("/assets/icons/sprite.svg#unit-{}", kind.id()) />
            </svg>
        </span>
    }
}

/// Small rounded badge: icon plus count.
#[component]
pub fn UnitBadge(kind: UnitKind, power: Power, count: u32) -> impl IntoView {
    view! {
        <span class="unit-badge" style=format!("--pc:{}", power.color())>
            <span class="on-badge"><UnitIcon kind=kind /></span>
            {format!("\u{d7}{count}")}
        </span>
    }
}

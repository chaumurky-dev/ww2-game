use crate::mock::{Power, UnitKind};
use leptos::prelude::*;

/// 24x24 fallback glyph paths (single path per unit), used when the PNG is missing.
fn glyph_path(kind: UnitKind) -> &'static str {
    match kind {
        UnitKind::Infantry => "M12 3a3 3 0 110 6 3 3 0 010-6zM7 21v-8l2-3h6l2 3v8h-3v-5h-4v5z",
        UnitKind::Armor => "M3 14h18l-2 5H5zM8 9h8v4H8zM15 10h7v2h-7z",
        UnitKind::Fighter => "M12 2l2 8 8 5v2l-8-2-1 5 3 2v1l-4-1-4 1v-1l3-2-1-5-8 2v-2l8-5z",
        UnitKind::Bomber => "M12 2l2 6 9 5v3l-9-2-1 4 4 3v1l-5-1-5 1v-1l4-3-1-4-9 2v-3l9-5z",
        UnitKind::Battleship => "M2 16l3 4h14l3-4zM6 11h12v4H6zM10 6h4v5h-4zM13 8h8v2h-8z",
        UnitKind::Carrier => "M1 17l3 3h16l3-3zM2 13h20v3H2zM15 8h4v5h-4z",
        UnitKind::Transport => "M2 16l3 4h14l3-4zM5 9h10v7H5zM16 11h3v5h-3z",
        UnitKind::Submarine => "M2 14a5 4 0 015-4h10a5 4 0 015 4 5 4 0 01-5 4H7a5 4 0 01-5-4zM10 5h4v5h-4z",
        UnitKind::AaGun => "M4 20h16v-3H4zM8 17l2-4h4l2 4zM12 13L19 4l2 1-6 9z",
        UnitKind::IndustrialComplex => "M2 21V10l6 4v-4l6 4v-4l6 4V5h3v16z",
    }
}

/// Unit icon: `/assets/units/<id>.png` as a CSS mask tinted with the power colour;
/// falls back to an inline SVG glyph until/unless the image loads.
#[component]
pub fn UnitIcon(kind: UnitKind, #[prop(optional)] power: Option<Power>) -> impl IntoView {
    let loaded = RwSignal::new(false);
    let src = format!("/assets/units/{}.png", kind.id());
    let tint = power.map(|p| format!("--pc:{};", p.color())).unwrap_or_default();
    let style = format!("{tint}--src:url({src})");
    view! {
        <span class="unit-icon" style=style title=kind.name()>
            <img src=src alt="" style="display:none" on:load=move |_| loaded.set(true) />
            <Show
                when=move || loaded.get()
                fallback=move || {
                    view! {
                        <svg viewBox="0 0 24 24" aria-hidden="true">
                            <path d=glyph_path(kind) />
                        </svg>
                    }
                }
            >
                <span class="mask"></span>
            </Show>
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

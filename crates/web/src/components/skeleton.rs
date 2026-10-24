use leptos::prelude::*;

/// Grey pulsing placeholder lines shown while a resource loads.
#[component]
pub fn Skeleton(#[prop(default = 3)] lines: usize) -> impl IntoView {
    view! {
        <div class="skeleton" aria-busy="true" aria-label="Loading">
            {(0..lines).map(|i| view! { <div class="sk-line" style=format!("width:{}%", 100 - (i * 17) % 45)></div> }).collect_view()}
        </div>
    }
}

/// Muted empty-state box with an optional call to action slot.
#[component]
pub fn Empty(#[prop(into)] text: String, #[prop(optional)] children: Option<Children>) -> impl IntoView {
    view! {
        <div class="empty">
            <p class="muted">{text}</p>
            {children.map(|c| c())}
        </div>
    }
}

/// Previous / next page controls for [`wartide_protocol::Paginated`] lists.
#[component]
pub fn Pager(page: RwSignal<u32>, total: i64, per_page: i64) -> impl IntoView {
    let pages = ((total + per_page.max(1) - 1) / per_page.max(1)).max(1) as u32;
    view! {
        <div class="row pager">
            <button class="secondary small" disabled=move || page.get() <= 1 on:click=move |_| page.update(|p| *p = p.saturating_sub(1).max(1))>"Previous"</button>
            <span class="muted">{move || format!("Page {} of {pages}", page.get())}</span>
            <button class="secondary small" disabled=move || page.get() >= pages on:click=move |_| page.update(|p| *p += 1)>"Next"</button>
        </div>
    }
}

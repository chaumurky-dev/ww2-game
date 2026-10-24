use crate::state::{ToastKind, Toasts};
use leptos::prelude::*;

#[component]
pub fn ToastHost() -> impl IntoView {
    let toasts = Toasts::expect();
    view! {
        <div class="toasts" aria-live="polite">
            <For
                each=move || toasts.items.get()
                key=|t| t.id
                children=move |t| {
                    let class = match t.kind {
                        ToastKind::Info => "toast",
                        ToastKind::Warn => "toast warn",
                        ToastKind::Error => "toast error",
                    };
                    view! { <div class=class>{t.text}</div> }
                }
            />
        </div>
    }
}

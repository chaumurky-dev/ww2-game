use crate::state::{ToastKind, Toasts};
use leptos::prelude::*;
use leptos_router::components::A;

#[component]
pub fn ToastHost() -> impl IntoView {
    let toasts = Toasts::expect();
    // A "your turn" link that points at the page we are already on is pointless: drop it.
    let path = leptos_router::hooks::use_location().pathname;
    Effect::new(move |_| {
        let here = path.get();
        toasts
            .items
            .update(|v| v.retain(|t| t.href.as_deref() != Some(here.as_str())));
    });
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
                    let id = t.id;
                    view! {
                        <div class=class on:click=move |_| toasts.items.update(|v| v.retain(|x| x.id != id))>
                            {t.text}
                            {t.href.map(|h| view! { " " <A href=h>"Open"</A> })}
                        </div>
                    }
                }
            />
        </div>
    }
}

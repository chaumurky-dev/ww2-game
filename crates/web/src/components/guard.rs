use crate::state::AuthState;
use leptos::prelude::*;
use leptos_router::hooks::use_navigate;

/// Renders its children only for logged-in users; once `/api/me` has answered "unauthenticated"
/// it redirects to `/`.
#[component]
pub fn Protected(children: ChildrenFn) -> impl IntoView {
    let auth = AuthState::expect();
    let navigate = use_navigate();
    Effect::new(move |_| {
        if auth.loaded.get() && auth.user.with(|u| u.is_none()) {
            navigate("/", Default::default());
        }
    });
    view! {
        <Show
            when=move || auth.user.with(|u| u.is_some())
            fallback=|| view! { <div class="page"><p class="muted">"Checking session..."</p></div> }
        >
            {children()}
        </Show>
    }
}

use crate::api;
use crate::state::{AuthState, Toasts};
use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::components::A;
use leptos_router::hooks::use_navigate;

#[component]
pub fn NavBar() -> impl IntoView {
    let auth = AuthState::expect();
    let toasts = Toasts::expect();
    let navigate = use_navigate();
    let menu = RwSignal::new(false);
    view! {
        <nav class="nav">
            <A href="/"><span class="brand"><img class="logo" src="/assets/brand/logo_128.png" alt="" /><span class="brand-text">"WARTIDE"</span></span></A>
            // Desktop: the links flow inline in the bar. Phone: they collapse into a dropdown
            // opened by the menu button (a real <button>, so keyboard and screen readers work).
            <button class="nav-toggle secondary" aria-controls="nav-links"
                aria-expanded=move || menu.get().to_string()
                on:click=move |_| menu.update(|m| *m = !*m)>
                {move || if menu.get() { "Close" } else { "Menu" }}
            </button>
            <div id="nav-links" class="nav-links" class:open=move || menu.get()
                on:click=move |_| menu.set(false)
                on:keydown=move |ev| if ev.key() == "Escape" { menu.set(false) }>
                <A href="/lobby">"Lobby"</A>
                <A href="/leaderboard">"Leaderboard"</A>
                <A href="/groups">"Groups"</A>
                <A href="/help">"How to play"</A>
                {
                    let navigate = navigate.clone();
                    move || {
                        let navigate = navigate.clone();
                        auth.user.get().map(move |_| view! {
                            <A href="/settings" attr:class="m-only">"Settings"</A>
                            <button class="secondary m-only" on:click=move |_| do_logout(navigate.clone(), auth, toasts)>"Log out"</button>
                        })
                    }
                }
            </div>
            <span class="spacer"></span>
            {move || match auth.user.get() {
                Some(u) => {
                    let navigate = navigate.clone();
                    view! {
                    <span class="user">
                        <A href=format!("/profile/{}", u.username)>{u.display_name.clone()}</A>
                        <A href="/settings" attr:class="d-only">"Settings"</A>
                        <button class="secondary small d-only" on:click=move |_| do_logout(navigate.clone(), auth, toasts)>"Log out"</button>
                    </span>
                }
                    .into_any()
                }
                None => view! { <A href="/">"Log in"</A> }.into_any(),
            }}
        </nav>
        {move || {
            auth.user
                .get()
                .filter(|u| !u.email_verified)
                .map(|_| view! {
                    <div class="banner">
                        "Your email address is not verified yet - check your inbox to create or join games. "
                        <button class="small secondary" on:click=move |_| {
                            spawn_local(async move {
                                match api::resend_verification().await {
                                    Ok(()) => toasts.info("Verification email sent"),
                                    Err(e) => toasts.api_error(&e),
                                }
                            });
                        }>"Resend email"</button>
                    </div>
                })
        }}
    }
}

fn do_logout<F: Fn(&str, leptos_router::NavigateOptions) + 'static>(
    navigate: F,
    auth: AuthState,
    toasts: Toasts,
) {
    spawn_local(async move {
        if let Err(e) = api::logout().await {
            toasts.api_error(&e);
        }
        auth.set(None);
        navigate("/", Default::default());
    });
}

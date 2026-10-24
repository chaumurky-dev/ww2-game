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
    view! {
        <nav class="nav">
            <A href="/"><span class="brand"><img class="logo" src="/assets/brand/logo_128.png" alt="" />"WARTIDE"</span></A>
            <A href="/lobby">"Lobby"</A>
            <A href="/leaderboard">"Leaderboard"</A>
            <A href="/groups">"Groups"</A>
            <A href="/help">"How to play"</A>
            <span class="spacer"></span>
            {move || match auth.user.get() {
                Some(u) => {
                    let navigate = navigate.clone();
                    view! {
                        <span class="user">
                            <A href=format!("/profile/{}", u.username)>{u.display_name.clone()}</A>
                            <A href="/settings">"Settings"</A>
                            <button class="secondary small" on:click=move |_| {
                                let navigate = navigate.clone();
                                spawn_local(async move {
                                    if let Err(e) = api::logout().await {
                                        toasts.api_error(&e);
                                    }
                                    auth.set(None);
                                    navigate("/", Default::default());
                                });
                            }>"Log out"</button>
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

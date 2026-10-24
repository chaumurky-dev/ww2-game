use crate::state::Session;
use leptos::prelude::*;
use leptos_router::components::A;
use leptos_router::hooks::use_navigate;

#[component]
pub fn NavBar() -> impl IntoView {
    let session = Session::expect();
    let navigate = use_navigate();
    view! {
        <nav class="nav">
            <A href="/"><span class="brand">"WARTIDE"</span></A>
            <A href="/lobby">"Lobby"</A>
            <A href="/leaderboard">"Leaderboard"</A>
            <A href="/groups">"Groups"</A>
            <span class="spacer"></span>
            {move || match session.user.get() {
                Some(u) => {
                    let navigate = navigate.clone();
                    view! {
                        <span class="user">
                            <A href=format!("/profile/{}", u.username)>{u.display_name.clone()}</A>
                            <A href="/settings">"Settings"</A>
                            <button class="secondary small" on:click=move |_| {
                                session.logout();
                                navigate("/", Default::default());
                            }>"Log out"</button>
                        </span>
                    }
                        .into_any()
                }
                None => view! { <A href="/">"Log in"</A> }.into_any(),
            }}
        </nav>
    }
}

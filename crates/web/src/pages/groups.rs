use super::leaderboard::LadderCard;
use crate::api;
use crate::components::form::TextField;
use crate::state::Toasts;
use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::components::A;
use leptos_router::hooks::{use_navigate, use_params_map};

#[component]
pub fn GroupList() -> impl IntoView {
    let toasts = Toasts::expect();
    let navigate = use_navigate();
    let groups = LocalResource::new(|| async { api::groups().await });
    let name = RwSignal::new(String::new());
    let create = move |_| {
        let navigate = navigate.clone();
        spawn_local(async move {
            match api::create_group(&name.get_untracked()).await {
                Ok(slug) => navigate(&format!("/groups/{slug}"), Default::default()),
                Err(e) => toasts.error(e),
            }
        });
    };
    view! {
        <div class="page grid2">
            <div class="card">
                <h2>"Groups"</h2>
                <Suspense fallback=|| view! { <p>"Loading..."</p> }>
                    {move || groups.get().map(|r| match r {
                        Ok(v) => v.into_iter().map(|g| view! {
                            <div class="row">
                                <A href=format!("/groups/{}", g.slug)><strong>{g.name.clone()}</strong></A>
                                <span class="muted grow">{g.description.clone()}</span>
                                <span class="pill">{format!("{} members", g.members.len())}</span>
                            </div>
                        }).collect_view().into_any(),
                        Err(e) => view! { <p class="error">{e}</p> }.into_any(),
                    })}
                </Suspense>
            </div>
            <form class="card col" on:submit=|ev| ev.prevent_default()>
                <h2>"Create a group"</h2>
                <TextField label="Group name" value=name />
                <button on:click=create>"Create"</button>
            </form>
        </div>
    }
}

#[component]
pub fn GroupDetail() -> impl IntoView {
    let toasts = Toasts::expect();
    let params = use_params_map();
    let data = LocalResource::new(move || {
        let slug = params.read().get("slug").unwrap_or_default();
        async move { api::group(&slug).await }
    });
    view! {
        <div class="page col">
            <Suspense fallback=|| view! { <p>"Loading..."</p> }>
                {move || data.get().map(|r| match r {
                    Ok(g) => {
                        let joined = RwSignal::new(g.joined);
                        view! {
                            <div class="card">
                                <div class="row">
                                    <h2 class="grow">{g.name.clone()}</h2>
                                    <button on:click=move |_| {
                                        joined.update(|j| *j = !*j);
                                        toasts.info(if joined.get_untracked() { "Joined group" } else { "Left group" });
                                    }>{move || if joined.get() { "Leave" } else { "Join" }}</button>
                                </div>
                                <p class="serif">{g.description.clone()}</p>
                                <h3>"Members"</h3>
                                <div class="row">
                                    {g.members.into_iter().map(|m| view! { <A href=format!("/profile/{m}")><span class="pill">{m.clone()}</span></A> }).collect_view()}
                                </div>
                            </div>
                            <div class="grid2">
                                <LadderCard title="Group 2-player ladder" ladder="2p" />
                                <LadderCard title="Group 5-player ladder" ladder="5p" />
                            </div>
                        }.into_any()
                    }
                    Err(e) => view! { <p class="error">{e}</p> }.into_any(),
                })}
            </Suspense>
        </div>
    }
}

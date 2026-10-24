use super::leaderboard::LadderCard;
use crate::api;
use crate::components::form::TextField;
use crate::state::{AuthState, Toasts};
use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::components::A;
use leptos_router::hooks::{use_navigate, use_params_map};
use wartide_protocol::game::GameMode;
use wartide_protocol::group::{Group, GroupRole};

fn role_value(r: GroupRole) -> &'static str {
    match r {
        GroupRole::Owner => "owner",
        GroupRole::Admin => "admin",
        GroupRole::Member => "member",
    }
}

#[component]
pub fn GroupList() -> impl IntoView {
    let toasts = Toasts::expect();
    let navigate = use_navigate();
    let groups = LocalResource::new(|| async { api::groups().await });
    let name = RwSignal::new(String::new());
    let desc = RwSignal::new(String::new());
    let code = RwSignal::new(String::new());
    let create = {
        let navigate = navigate.clone();
        move |_| {
            let navigate = navigate.clone();
            spawn_local(async move {
                let d = desc.get_untracked();
                let d = (!d.trim().is_empty()).then_some(d);
                match api::create_group(&name.get_untracked(), d).await {
                    Ok(g) => navigate(&format!("/groups/{}", g.slug), Default::default()),
                    Err(e) => toasts.api_error(&e),
                }
            });
        }
    };
    let join = move |_| {
        let navigate = navigate.clone();
        spawn_local(async move {
            match api::join_group(&code.get_untracked()).await {
                Ok(g) => navigate(&format!("/groups/{}", g.slug), Default::default()),
                Err(e) => toasts.api_error(&e),
            }
        });
    };
    view! {
        <div class="page grid2">
            <div class="card">
                <h2>"My groups"</h2>
                <Suspense fallback=|| view! { <p>"Loading..."</p> }>
                    {move || groups.get().map(|r| match r {
                        Ok(v) if v.is_empty() => view! { <p class="muted">"You are not in any group yet."</p> }.into_any(),
                        Ok(v) => v.into_iter().map(|g| view! {
                            <div class="row">
                                <A href=format!("/groups/{}", g.slug)><strong>{g.name.clone()}</strong></A>
                                <span class="muted grow">{g.description.clone()}</span>
                                <span class="pill">{format!("{} members", g.member_count)}</span>
                            </div>
                        }).collect_view().into_any(),
                        Err(e) => view! { <p class="error">{e.message}</p> }.into_any(),
                    })}
                </Suspense>
            </div>
            <div class="col">
                <form class="card col" on:submit=|ev| ev.prevent_default()>
                    <h2>"Create a group"</h2>
                    <TextField label="Group name" value=name />
                    <TextField label="Description (optional)" value=desc />
                    <button type="submit" on:click=create>"Create"</button>
                </form>
                <form class="card col" on:submit=|ev| ev.prevent_default()>
                    <h2>"Join with an invite code"</h2>
                    <TextField label="Invite code" value=code />
                    <button type="submit" on:click=join>"Join"</button>
                </form>
            </div>
        </div>
    }
}

#[component]
pub fn GroupDetail() -> impl IntoView {
    let params = use_params_map();
    let version = RwSignal::new(0_u32);
    let data = LocalResource::new(move || {
        version.track();
        let slug = params.read().get("slug").unwrap_or_default();
        async move { api::group_by_slug(&slug).await }
    });
    view! {
        <div class="page col">
            <Suspense fallback=|| view! { <p>"Loading..."</p> }>
                {move || data.get().map(|r| match r {
                    Ok(g) => view! { <GroupView group=g version /> }.into_any(),
                    Err(e) => view! { <p class="error">{e.message}</p> }.into_any(),
                })}
            </Suspense>
        </div>
    }
}

#[component]
fn GroupView(group: Group, version: RwSignal<u32>) -> impl IntoView {
    let toasts = Toasts::expect();
    let auth = AuthState::expect();
    let navigate = use_navigate();
    let gid = group.id;
    let role = group.my_role;
    let can_manage = role.map(|r| r.can_manage()).unwrap_or(false);
    let is_owner = role == Some(GroupRole::Owner);
    let members = LocalResource::new(move || {
        version.track();
        async move { api::group_members(gid).await }
    });
    let leave = move |_| {
        let navigate = navigate.clone();
        spawn_local(async move {
            match api::leave_group(gid).await {
                Ok(()) => navigate("/groups", Default::default()),
                Err(e) => toasts.api_error(&e),
            }
        });
    };
    let rotate = move |_| {
        spawn_local(async move {
            match api::rotate_invite(gid).await {
                Ok(_) => {
                    toasts.info("New invite code generated");
                    version.update(|v| *v += 1);
                }
                Err(e) => toasts.api_error(&e),
            }
        });
    };
    view! {
        <div class="card">
            <div class="row">
                <h2 class="grow">{group.name.clone()}</h2>
                <span class="pill">{format!("{} members", group.member_count)}</span>
                {(role.is_some() && !is_owner).then(|| view! { <button class="secondary" on:click=leave>"Leave"</button> })}
            </div>
            <p class="serif">{group.description.clone().unwrap_or_default()}</p>
            {group.invite_code.clone().filter(|_| can_manage).map(|c| view! {
                <div class="row">
                    "Invite code: " <strong class="code">{c}</strong>
                    <button class="secondary small" on:click=rotate>"Rotate"</button>
                </div>
            })}
            <h3>"Members"</h3>
            <Suspense fallback=|| view! { <p>"Loading..."</p> }>
                {move || members.get().map(|r| match r {
                    Ok(v) => v.into_iter().map(|m| {
                        let uid = m.user.id;
                        let is_me = auth.user.with(|u| u.as_ref().map(|u| u.id)) == Some(uid);
                        let m_role = m.role;
                        view! {
                            <div class="row">
                                <A href=format!("/profile/{}", m.user.username)><strong>{m.user.display_name.clone()}</strong></A>
                                {if is_owner && !is_me && m_role != GroupRole::Owner {
                                    view! {
                                        <select on:change:target=move |ev| {
                                            let r = if ev.target().value() == "admin" { GroupRole::Admin } else { GroupRole::Member };
                                            spawn_local(async move {
                                                match api::set_member_role(gid, uid, r).await {
                                                    Ok(()) => version.update(|v| *v += 1),
                                                    Err(e) => toasts.api_error(&e),
                                                }
                                            });
                                        }>
                                            <option value="member" selected=m_role == GroupRole::Member>"Member"</option>
                                            <option value="admin" selected=m_role == GroupRole::Admin>"Admin"</option>
                                        </select>
                                    }.into_any()
                                } else {
                                    view! { <span class="pill">{role_value(m_role)}</span> }.into_any()
                                }}
                                {(can_manage && !is_me && m_role != GroupRole::Owner).then(|| view! {
                                    <button class="secondary small" on:click=move |_| {
                                        spawn_local(async move {
                                            match api::remove_member(gid, uid).await {
                                                Ok(()) => version.update(|v| *v += 1),
                                                Err(e) => toasts.api_error(&e),
                                            }
                                        });
                                    }>"Remove"</button>
                                })}
                            </div>
                        }
                    }).collect_view().into_any(),
                    Err(e) => view! { <p class="error">{e.message}</p> }.into_any(),
                })}
            </Suspense>
        </div>
        <div class="grid2">
            <LadderCard title="Group 2-player ladder" mode=GameMode::TwoPlayer group=gid />
            <LadderCard title="Group 5-player ladder" mode=GameMode::FivePlayer group=gid />
        </div>
    }
}

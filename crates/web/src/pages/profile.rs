use crate::api;
use leptos::prelude::*;
use leptos_router::hooks::use_params_map;
use wartide_protocol::user::PublicProfile;

fn initials(name: &str) -> String {
    name.chars().take(2).collect::<String>().to_uppercase()
}

/// `public:2p` -> "Public 2-player", `group:<id>:5p` -> "Group 5-player".
pub fn ladder_label(l: &str) -> String {
    let mode = if l.ends_with("2p") {
        "2-player"
    } else {
        "5-player"
    };
    if l.starts_with("group:") {
        format!("Group {mode}")
    } else {
        format!("Public {mode}")
    }
}

fn profile_view(p: PublicProfile) -> impl IntoView {
    let name = p.display_name.clone();
    let meta = [p.country.clone(), p.timezone.clone()]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(" - ");
    view! {
        <div class="card row">
            <div class="avatar">{initials(&name)}</div>
            <div class="grow">
                <h2 style="margin:0">{name}</h2>
                <p class="muted" style="margin:0">{format!("@{}", p.username)} {(!meta.is_empty()).then(|| format!(" - {meta}"))}</p>
                <p class="serif" style="margin:.3rem 0 0">{p.bio.clone().unwrap_or_default()}</p>
            </div>
        </div>
        <div class="card">
            <h3>"Ratings"</h3>
            {if p.stats.is_empty() {
                view! { <p class="muted">"No rated games yet."</p> }.into_any()
            } else {
                view! {
                    <table>
                        <thead><tr><th>"Ladder"</th><th class="num">"Rating"</th><th class="num">"Rank"</th><th class="num">"Games"</th><th class="num">"W"</th><th class="num">"L"</th></tr></thead>
                        <tbody>{p.stats.into_iter().map(|r| view! {
                            <tr>
                                <td>{ladder_label(&r.ladder)}</td>
                                <td class="num">{format!("{:.0}", r.rating)}</td>
                                <td class="num">{r.rank_position.map(|n| format!("#{n}")).unwrap_or_else(|| "-".into())}</td>
                                <td class="num">{r.games_played}</td>
                                <td class="num">{r.wins}</td><td class="num">{r.losses}</td>
                            </tr>
                        }).collect_view()}</tbody>
                    </table>
                }.into_any()
            }}
        </div>
    }
}

#[component]
pub fn ProfilePage() -> impl IntoView {
    let params = use_params_map();
    let data = LocalResource::new(move || {
        let name = params.read().get("username").unwrap_or_default();
        async move { api::profile(&name).await }
    });
    view! {
        <div class="page col">
            <Suspense fallback=|| view! { <p>"Loading..."</p> }>
                {move || data.get().map(|r| match r {
                    Ok(p) => profile_view(p).into_any(),
                    Err(e) => view! { <p class="error">{e.message}</p> }.into_any(),
                })}
            </Suspense>
        </div>
    }
}

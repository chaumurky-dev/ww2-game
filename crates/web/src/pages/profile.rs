use crate::api;
use crate::components::skeleton::{Empty, Skeleton};
use crate::model::*;
use leptos::prelude::*;
use leptos_router::components::A;
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
    let recent = p.recent_games.clone();
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
                        <thead><tr><th>"Ladder"</th><th class="num">"Rating"</th><th class="num">"Rank"</th><th class="num">"Games"</th><th class="num">"W"</th><th class="num">"L"</th><th class="num" title="Current / best consecutive wins">"Streak"</th></tr></thead>
                        <tbody>{p.stats.into_iter().map(|r| view! {
                            <tr>
                                <td>{ladder_label(&r.ladder)}</td>
                                <td class="num">{format!("{:.0}", r.rating)}</td>
                                <td class="num">{r.rank_position.map(|n| format!("#{n}")).unwrap_or_else(|| "-".into())}</td>
                                <td class="num">{r.games_played}</td>
                                <td class="num">{r.wins}</td><td class="num">{r.losses}</td>
                                <td class="num" title="Current win streak / best win streak">{format!("{} / {}", r.current_win_streak, r.best_win_streak)}</td>
                            </tr>
                        }).collect_view()}</tbody>
                    </table>
                }.into_any()
            }}
        </div>
        <div class="card">
            <h3>"Recent games"</h3>
            {if recent.is_empty() {
                view! { <Empty text="No public or group games yet." /> }.into_any()
            } else {
                view! {
                    <div style="overflow-x:auto"><table>
                        <thead><tr><th>"Game"</th><th>"Mode"</th><th>"Status"</th><th>"Result"</th></tr></thead>
                        <tbody>{recent.into_iter().map(|g| {
                            let title = game_title(&g);
                            let open = g.group_id.is_none();
                            let id = g.id;
                            view! {
                                <tr>
                                    <td>{if open { view! { <A href=format!("/game/{id}")>{title}</A> }.into_any() } else { view! { <span>{title}" "<span class="pill">"group"</span></span> }.into_any() }}</td>
                                    <td>{mode_label(g.mode)}</td>
                                    <td>{status_label(g.status)}</td>
                                    <td>{match (g.status, g.winner) {
                                        (wartide_protocol::game::GameStatus::Finished, Some(t)) => format!("{} won", team_name(t)),
                                        (wartide_protocol::game::GameStatus::Finished, None) => "Draw".into(),
                                        _ => format!("Round {}", g.round),
                                    }}</td>
                                </tr>
                            }
                        }).collect_view()}</tbody>
                    </table></div>
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
            <Suspense fallback=|| view! { <Skeleton lines=4 /> }>
                {move || data.get().map(|r| match r {
                    Ok(p) => profile_view(p).into_any(),
                    Err(e) => view! { <p class="error">{e.message}</p> }.into_any(),
                })}
            </Suspense>
        </div>
    }
}

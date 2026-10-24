use crate::api;
use leptos::prelude::*;
use leptos_router::components::A;
use uuid::Uuid;
use wartide_protocol::game::GameMode;
use wartide_protocol::group::LeaderboardEntry;

fn table(rows: Vec<LeaderboardEntry>) -> impl IntoView {
    if rows.is_empty() {
        return view! { <p class="muted">"No rated games yet."</p> }.into_any();
    }
    view! {
        <table>
            <thead><tr><th class="num">"#"</th><th>"Player"</th><th class="num">"Rating"</th><th class="num">"W"</th><th class="num">"L"</th></tr></thead>
            <tbody>{rows.into_iter().map(|r| view! {
                <tr>
                    <td class="num">{r.rank}</td>
                    <td><A href=format!("/profile/{}", r.user.username)>{r.user.display_name.clone()}</A></td>
                    <td class="num">{format!("{:.0}", r.rating)}</td><td class="num">{r.wins}</td><td class="num">{r.losses}</td>
                </tr>
            }).collect_view()}</tbody>
        </table>
    }
    .into_any()
}

/// Ladder table: the public ladder, or a group's when `group` is given.
#[component]
pub fn LadderCard(
    #[prop(into)] title: String,
    mode: GameMode,
    #[prop(optional)] group: Option<Uuid>,
) -> impl IntoView {
    let data = LocalResource::new(move || async move {
        match group {
            Some(g) => api::group_leaderboard(g, mode).await,
            None => api::leaderboard(mode).await,
        }
    });
    view! {
        <div class="card">
            <h2>{title}</h2>
            <Suspense fallback=|| view! { <p>"Loading..."</p> }>
                {move || data.get().map(|r| match r {
                    Ok(rows) => table(rows).into_any(),
                    Err(e) => view! { <p class="error">{e.message}</p> }.into_any(),
                })}
            </Suspense>
        </div>
    }
}

#[component]
pub fn Leaderboard() -> impl IntoView {
    view! {
        <div class="page grid2">
            <LadderCard title="Public 2-player" mode=GameMode::TwoPlayer />
            <LadderCard title="Public 5-player" mode=GameMode::FivePlayer />
        </div>
    }
}

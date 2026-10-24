use crate::api;
use crate::mock::LeaderRow;
use leptos::prelude::*;
use leptos_router::components::A;

fn table(rows: Vec<LeaderRow>) -> impl IntoView {
    view! {
        <table>
            <thead><tr><th class="num">"#"</th><th>"Player"</th><th class="num">"Rating"</th><th class="num">"W"</th><th class="num">"L"</th></tr></thead>
            <tbody>{rows.into_iter().map(|r| view! {
                <tr>
                    <td class="num">{r.rank}</td>
                    <td><A href=format!("/profile/{}", r.username)>{r.username.clone()}</A></td>
                    <td class="num">{r.rating}</td><td class="num">{r.wins}</td><td class="num">{r.losses}</td>
                </tr>
            }).collect_view()}</tbody>
        </table>
    }
}

#[component]
pub fn LadderCard(#[prop(into)] title: String, ladder: &'static str) -> impl IntoView {
    let data = LocalResource::new(move || async move { api::leaderboard(ladder).await });
    view! {
        <div class="card">
            <h2>{title}</h2>
            <Suspense fallback=|| view! { <p>"Loading..."</p> }>
                {move || data.get().map(|r| match r {
                    Ok(rows) => table(rows).into_any(),
                    Err(e) => view! { <p class="error">{e}</p> }.into_any(),
                })}
            </Suspense>
        </div>
    }
}

#[component]
pub fn Leaderboard() -> impl IntoView {
    view! {
        <div class="page grid2">
            <LadderCard title="Public 2-player" ladder="2p" />
            <LadderCard title="Public 5-player" ladder="5p" />
        </div>
    }
}

use crate::api;
use crate::components::power_badge::PowerBadge;
use crate::mock::Profile;
use leptos::prelude::*;
use leptos_router::components::A;
use leptos_router::hooks::use_params_map;

fn initials(name: &str) -> String {
    name.chars().take(2).collect::<String>().to_uppercase()
}

fn profile_view(p: Profile) -> impl IntoView {
    let name = p.user.display_name.clone();
    view! {
        <div class="card row">
            <div style="width:72px;height:72px;border-radius:50%;background:var(--accent);color:var(--accent-ink);display:grid;place-items:center;font-family:var(--font-display);font-size:1.8rem">
                {initials(&name)}
            </div>
            <div class="grow"><h2 style="margin:0">{name}</h2><p class="serif muted" style="margin:0">{p.user.bio.clone()}</p></div>
        </div>
        <div class="grid2">
            <div class="card">
                <h3>"Ratings"</h3>
                <table>
                    <thead><tr><th>"Ladder"</th><th class="num">"Rating"</th><th class="num">"W"</th><th class="num">"L"</th></tr></thead>
                    <tbody>{p.ratings.into_iter().map(|r| view! {
                        <tr><td>{r.ladder}</td><td class="num">{r.rating}</td><td class="num">{r.wins}</td><td class="num">{r.losses}</td></tr>
                    }).collect_view()}</tbody>
                </table>
            </div>
            <div class="card">
                <h3>"Recent games"</h3>
                <table><tbody>{p.recent.into_iter().map(|g| view! {
                    <tr>
                        <td><A href=format!("/game/{}", g.id)>{g.name}</A></td>
                        <td><PowerBadge power=g.power short=true /></td>
                        <td>{g.result}</td>
                    </tr>
                }).collect_view()}</tbody></table>
            </div>
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
                    Err(e) => view! { <p class="error">{e}</p> }.into_any(),
                })}
            </Suspense>
        </div>
    }
}

use super::create_game::CreateGameForm;
use crate::api;
use crate::components::countdown::Countdown;
use crate::components::form::TextField;
use crate::components::modal::Modal;
use crate::components::power_badge::PowerBadge;
use crate::mock::GameSummary;
use crate::state::Toasts;
use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::components::A;
use leptos_router::hooks::use_navigate;

fn game_row(g: GameSummary) -> impl IntoView {
    let mine = g.my_turn();
    view! {
        <tr>
            <td>
                <A href=format!("/game/{}", g.id)>{g.name.clone()}</A>
                {mine.then(|| view! { " " <span class="pill hot">"Your turn"</span> })}
            </td>
            <td>{g.mode}</td>
            <td>{format!("R{}", g.round)}</td>
            <td><PowerBadge power=g.current_power short=true /></td>
            <td class="muted">{g.opponents.join(", ")}</td>
            <td><Countdown deadline_ms=g.deadline_ms /></td>
        </tr>
    }
}

#[component]
pub fn Lobby() -> impl IntoView {
    let toasts = Toasts::expect();
    let navigate = use_navigate();
    let games = LocalResource::new(|| async {
        api::my_games().await.map(|mut v| {
            v.sort_by(|a, b| b.my_turn().cmp(&a.my_turn()).then(a.deadline_ms.total_cmp(&b.deadline_ms)));
            v
        })
    });
    let create_open = RwSignal::new(false);
    let code = RwSignal::new(String::new());
    let queue = RwSignal::new(None::<u8>);
    let qstatus = RwSignal::new(String::new());

    let join = {
        let navigate = navigate.clone();
        move |_| {
            let navigate = navigate.clone();
            spawn_local(async move {
                match api::join_by_code(&code.get_untracked()).await {
                    Ok(id) => navigate(&format!("/game/{id}"), Default::default()),
                    Err(e) => toasts.error(e),
                }
            });
        }
    };
    let toggle_queue = move |n: u8| {
        spawn_local(async move {
            let joining = queue.get_untracked() != Some(n);
            if let Ok(s) = api::queue(n, joining).await {
                qstatus.set(s);
                queue.set(joining.then_some(n));
            }
        });
    };
    let qbtn = move |n: u8, label: &'static str| {
        view! {
            <button class=move || if queue.get() == Some(n) { "" } else { "secondary" } on:click=move |_| toggle_queue(n)>
                {move || if queue.get() == Some(n) { format!("Cancel {label}") } else { format!("Find {label}") }}
            </button>
        }
    };
    view! {
        <div class="page col">
            <div class="card">
                <div class="row"><h2 class="grow">"My games"</h2><button on:click=move |_| create_open.set(true)>"New private game"</button></div>
                <Suspense fallback=|| view! { <p>"Loading..."</p> }>
                    {move || games.get().map(|r| match r {
                        Ok(v) if v.is_empty() => view! { <p class="muted">"No games yet. Create one or join the queue."</p> }.into_any(),
                        Ok(v) => view! {
                            <div style="overflow-x:auto"><table>
                                <thead><tr><th>"Game"</th><th>"Mode"</th><th>"Round"</th><th>"To act"</th><th>"With"</th><th>"Deadline"</th></tr></thead>
                                <tbody>{v.into_iter().map(game_row).collect_view()}</tbody>
                            </table></div>
                        }.into_any(),
                        Err(e) => view! { <p class="error">{e}</p> }.into_any(),
                    })}
                </Suspense>
            </div>
            <div class="grid2">
                <div class="card col">
                    <h3>"Public matchmaking"</h3>
                    <div class="row">{qbtn(2, "2-player")} {qbtn(5, "5-player")}</div>
                    <p class="muted">{move || if qstatus.get().is_empty() { "Rated games on the public ladders.".to_string() } else { qstatus.get() }}</p>
                </div>
                <div class="card col">
                    <h3>"Join by invite code"</h3>
                    <div class="row">
                        <div class="grow"><TextField label="Invite code" value=code placeholder="ABCD-1234" /></div>
                        <button on:click=join>"Join"</button>
                    </div>
                </div>
            </div>
            <Modal open=create_open title="New private game">
                <CreateGameForm open=create_open />
            </Modal>
        </div>
    }
}

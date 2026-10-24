use crate::api;
use crate::components::form::{Check, TextField};
use crate::components::power_badge::PowerBadge;
use crate::model::*;
use crate::state::Toasts;
use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::hooks::use_navigate;
use uuid::Uuid;
use wartide_protocol::game::{CreatePrivateGameRequest, GameMode, GameSettings};

const PRESETS: [(u32, &str); 6] = [
    (3_600, "1 hour"),
    (43_200, "12 hours"),
    (86_400, "24 hours"),
    (172_800, "48 hours"),
    (259_200, "72 hours"),
    (604_800, "1 week"),
];

/// Seat configuration; seat 0 is always the host ("Me").
#[component]
fn SeatRow(index: usize, mode: RwSignal<GameMode>, kind: RwSignal<String>) -> impl IntoView {
    let label = move || match mode.get() {
        GameMode::FivePlayer => view! { <PowerBadge power=Power::ALL[index] /> }.into_any(),
        GameMode::TwoPlayer => {
            let (name, powers) = if index == 0 {
                ("Axis", "Germany + Japan")
            } else {
                ("Allies", "USSR + UK + USA")
            };
            view! { <span><strong>{name}</strong>" "<span class="muted">{powers}</span></span> }
                .into_any()
        }
    };
    view! {
        <div class="row">
            <div style="width:11rem">{label}</div>
            {if index == 0 {
                view! { <span class="pill">"Me"</span> }.into_any()
            } else {
                view! {
                    <select on:change:target=move |ev| kind.set(ev.target().value())>
                        <option value="open" selected=move || kind.get() == "open">"Open seat (human)"</option>
                        <option value="bot" selected=move || kind.get() == "bot">"Bot"</option>
                    </select>
                }.into_any()
            }}
        </div>
    }
}

#[component]
pub fn CreateGameForm(open: RwSignal<bool>) -> impl IntoView {
    let toasts = Toasts::expect();
    let navigate = use_navigate();
    let mode = RwSignal::new(GameMode::FivePlayer);
    let kinds: Vec<RwSignal<String>> = (0..5)
        .map(|i| RwSignal::new(if i == 0 { "me" } else { "bot" }.to_string()))
        .collect();
    let timeout = RwSignal::new(172_800_u32);
    let weapons = RwSignal::new(true);
    let econ = RwSignal::new(false);
    let threshold = RwSignal::new(String::from("84"));
    let group = RwSignal::new(String::new());
    let groups = LocalResource::new(|| async { api::groups().await.unwrap_or_default() });

    let kinds_c = kinds.clone();
    let submit = move |_| {
        let n = mode.get_untracked().seat_count();
        let bot_seats: Vec<u8> = (1..n)
            .filter(|i| kinds_c[*i].get_untracked() == "bot")
            .map(|i| i as u8)
            .collect();
        let req = CreatePrivateGameRequest {
            mode: mode.get_untracked(),
            name: None,
            bot_personas: Vec::new(),
            human_seats: Some((n - bot_seats.len()) as u8),
            bot_seats,
            settings: GameSettings {
                turn_timeout_secs: timeout.get_untracked(),
                weapons_development: weapons.get_untracked(),
                economic_victory: econ.get_untracked(),
                economic_victory_threshold: threshold.get_untracked().parse().unwrap_or(84),
            },
            group_id: group.get_untracked().parse::<Uuid>().ok(),
        };
        let navigate = navigate.clone();
        spawn_local(async move {
            match api::create_game(&req).await {
                Ok(g) => {
                    open.set(false);
                    toasts.info(if g.invite_code.is_some() {
                        "Game created. Share the invite code from the game page."
                    } else {
                        "Game created."
                    });
                    navigate(&format!("/game/{}", g.id), Default::default());
                }
                Err(e) => toasts.api_error(&e),
            }
        });
    };
    let seat_rows = {
        let kinds = kinds.clone();
        move || {
            (0..mode.get().seat_count())
                .map(|i| view! { <SeatRow index=i mode kind=kinds[i] /> })
                .collect_view()
        }
    };
    view! {
        <form class="col" on:submit=|ev| ev.prevent_default()>
            <label>"Mode"
                <select on:change:target=move |ev| {
                    mode.set(if ev.target().value() == "two" { GameMode::TwoPlayer } else { GameMode::FivePlayer })
                }>
                    <option value="five" selected=move || mode.get() == GameMode::FivePlayer>"5 players - one power per seat"</option>
                    <option value="two" selected=move || mode.get() == GameMode::TwoPlayer>"2 players - Axis vs Allies"</option>
                </select>
            </label>
            <h3>"Seats"</h3>
            {seat_rows}
            <p class="muted">"Open seats are filled by invite code; the game starts when every open seat is taken (or you start it early and bots fill in)."</p>
            <h3>"Settings"</h3>
            <label>"Turn timeout"
                <select on:change:target=move |ev| timeout.set(ev.target().value().parse().unwrap_or(172_800))>
                    {PRESETS.iter().map(|(s, l)| view! { <option value=s.to_string() selected=*s == 172_800>{*l}</option> }).collect_view()}
                </select>
            </label>
            <Check label="Weapons development" value=weapons />
            <Check label="Economic victory" value=econ />
            <Show when=move || econ.get()>
                <TextField label="Threshold (IPC per round for a side)" value=threshold kind="number" />
            </Show>
            <label>"Group (rated on its ladder)"
                <select on:change:target=move |ev| group.set(ev.target().value())>
                    <option value="">"None - unrated"</option>
                    {move || groups.get().unwrap_or_default().into_iter().map(|g| view! { <option value=g.id.to_string()>{g.name}</option> }).collect_view()}
                </select>
            </label>
            <button type="submit" on:click=submit>"Create game"</button>
        </form>
    }
}

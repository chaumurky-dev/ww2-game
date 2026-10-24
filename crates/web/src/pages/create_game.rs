use crate::api;
use crate::components::form::{Check, TextField};
use crate::components::power_badge::PowerBadge;
use crate::mock::{NewGame, Persona, Power, SeatKind};
use crate::state::Toasts;
use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::hooks::use_navigate;

const PRESETS: [(u32, &str); 6] = [
    (3_600, "1 hour"),
    (43_200, "12 hours"),
    (86_400, "24 hours"),
    (172_800, "48 hours"),
    (259_200, "72 hours"),
    (604_800, "1 week"),
];

#[derive(Clone, Copy)]
struct SeatCfg {
    power: Power,
    kind: RwSignal<String>,
    persona: RwSignal<usize>,
}

#[component]
fn SeatRow(seat: SeatCfg) -> impl IntoView {
    view! {
        <div class="row">
            <div style="width:11rem"><PowerBadge power=seat.power /></div>
            <select on:change:target=move |ev| seat.kind.set(ev.target().value())>
                <option value="me" selected=move || seat.kind.get() == "me">"Me"</option>
                <option value="open" selected=move || seat.kind.get() == "open">"Open seat"</option>
                <option value="bot" selected=move || seat.kind.get() == "bot">"Bot"</option>
            </select>
            <Show when=move || seat.kind.get() == "bot">
                <select on:change:target=move |ev| seat.persona.set(ev.target().value().parse().unwrap_or(0))>
                    {Persona::ALL.iter().enumerate().map(|(i, p)| view! { <option value=i.to_string()>{p.label()}</option> }).collect_view()}
                </select>
            </Show>
        </div>
    }
}

#[component]
pub fn CreateGameForm(open: RwSignal<bool>) -> impl IntoView {
    let toasts = Toasts::expect();
    let navigate = use_navigate();
    let name = RwSignal::new(String::from("My new campaign"));
    let seats: Vec<SeatCfg> = Power::ALL
        .iter()
        .map(|p| SeatCfg { power: *p, kind: RwSignal::new(if *p == Power::Ussr { "me" } else { "open" }.into()), persona: RwSignal::new(0) })
        .collect();
    let timeout = RwSignal::new(172_800_u32);
    let weapons = RwSignal::new(true);
    let econ = RwSignal::new(false);
    let threshold = RwSignal::new(String::from("84"));
    let group = RwSignal::new(String::new());

    let seats_c = seats.clone();
    let submit = move |_| {
        let cfg = NewGame {
            name: name.get_untracked(),
            seats: seats_c
                .iter()
                .map(|s| {
                    let kind = match s.kind.get_untracked().as_str() {
                        "me" => SeatKind::Me,
                        "bot" => SeatKind::Bot(Persona::ALL[s.persona.get_untracked().min(3)]),
                        _ => SeatKind::Open,
                    };
                    (s.power, kind)
                })
                .collect(),
            timeout_secs: timeout.get_untracked(),
            weapons_development: weapons.get_untracked(),
            economic_victory: econ.get_untracked(),
            economic_threshold: threshold.get_untracked().parse().unwrap_or(84),
            group: Some(group.get_untracked()).filter(|g| !g.trim().is_empty()),
        };
        if !cfg.seats.iter().any(|(_, k)| *k == SeatKind::Me) {
            toasts.error("Take at least one seat yourself.");
            return;
        }
        let navigate = navigate.clone();
        spawn_local(async move {
            match api::create_game(&cfg).await {
                Ok(id) => {
                    open.set(false);
                    toasts.info("Game created. Share the invite code from the game page.");
                    navigate(&format!("/game/{id}"), Default::default());
                }
                Err(e) => toasts.error(e),
            }
        });
    };
    view! {
        <form class="col" on:submit=|ev| ev.prevent_default()>
            <TextField label="Game name" value=name />
            <h3>"Seats"</h3>
            {seats.into_iter().map(|s| view! { <SeatRow seat=s /> }).collect_view()}
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
            <TextField label="Group (optional slug)" value=group placeholder="coffee-club" />
            <button on:click=submit>"Create game"</button>
        </form>
    }
}

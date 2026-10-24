use crate::api;
use crate::components::form::{Check, TextField};
use crate::components::power_badge::PowerBadge;
use crate::model::*;
use crate::state::Toasts;
use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::hooks::use_navigate;
use uuid::Uuid;
use wartide_protocol::bots::BotPersona;
use wartide_protocol::game::{CreatePrivateGameRequest, GameMode, GameSettings};

const PRESETS: [(u32, &str); 6] = [
    (3_600, "1 hour"),
    (43_200, "12 hours"),
    (86_400, "24 hours"),
    (172_800, "48 hours"),
    (259_200, "72 hours"),
    (604_800, "1 week"),
];

fn persona_label(p: &BotPersona) -> String {
    let style = format!("{:?}", p.style);
    format!("{} - {}", p.name, style.to_lowercase())
}

/// One seat: its kind (`me` / `open` / `bot`) and the chosen persona id (empty = default).
#[derive(Clone, Copy)]
struct Seat {
    kind: RwSignal<String>,
    persona: RwSignal<String>,
}

/// Seat configuration; seat 0 is always the host ("Me").
#[component]
fn SeatRow(
    index: usize,
    mode: RwSignal<GameMode>,
    seat: Seat,
    personas: Signal<Vec<BotPersona>>,
) -> impl IntoView {
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
    let blurb = move || {
        let id = seat.persona.get();
        personas.with(|v| {
            v.iter()
                .find(|p| p.id == id)
                .map(|p| format!("{} - {}", p.name, p.blurb))
        })
    };
    view! {
        <div class="seat-cfg">
            <div class="row">
                <div class="seat-label">{label}</div>
                {if index == 0 {
                    view! { <span class="pill">"Me"</span> }.into_any()
                } else {
                    view! {
                        <select aria-label="Seat type" on:change:target=move |ev| seat.kind.set(ev.target().value())>
                            <option value="open" selected=move || seat.kind.get() == "open">"Open seat (human)"</option>
                            <option value="bot" selected=move || seat.kind.get() == "bot">"Bot"</option>
                        </select>
                        <Show when=move || seat.kind.get() == "bot">
                            <select aria-label="Bot persona" on:change:target=move |ev| seat.persona.set(ev.target().value())>
                                <option value="" selected=move || seat.persona.get().is_empty()>"Default persona"</option>
                                {move || personas.get().into_iter().map(|p| {
                                    let id = p.id.clone();
                                    let id2 = p.id.clone();
                                    view! { <option value=id selected=move || seat.persona.get() == id2>{persona_label(&p)}</option> }
                                }).collect_view()}
                            </select>
                        </Show>
                    }.into_any()
                }}
            </div>
            <Show when=move || seat.kind.get() == "bot" && blurb().is_some()>
                <div class="muted small-text">{blurb}</div>
            </Show>
        </div>
    }
}

/// A checkbox with a one-line explanation underneath.
#[component]
fn Opt(
    #[prop(into)] label: String,
    #[prop(into)] help: String,
    value: RwSignal<bool>,
) -> impl IntoView {
    view! {
        <div class="opt">
            <Check label value />
            <div class="muted small-text opt-help">{help}</div>
        </div>
    }
}

fn threshold(on: RwSignal<bool>, val: RwSignal<String>, default: u32) -> Option<u32> {
    on.get_untracked()
        .then(|| val.get_untracked().trim().parse().unwrap_or(default))
}

#[component]
pub fn CreateGameForm(open: RwSignal<bool>) -> impl IntoView {
    let toasts = Toasts::expect();
    let navigate = use_navigate();
    let mode = RwSignal::new(GameMode::FivePlayer);
    let name = RwSignal::new(String::new());
    let seats: Vec<Seat> = (0..5)
        .map(|i| Seat {
            kind: RwSignal::new(if i == 0 { "me" } else { "bot" }.to_string()),
            persona: RwSignal::new(String::new()),
        })
        .collect();
    let timeout = RwSignal::new(172_800_u32);
    let weapons = RwSignal::new(true);
    let axis_econ = RwSignal::new(true);
    let axis_th = RwSignal::new(String::from("84"));
    let ally_econ = RwSignal::new(false);
    let ally_th = RwSignal::new(String::from("84"));
    let soviet = RwSignal::new(false);
    let orig = RwSignal::new(false);
    let total = RwSignal::new(false);
    let jet = RwSignal::new(false);
    let subs = RwSignal::new(false);
    let group = RwSignal::new(String::new());
    let groups = LocalResource::new(|| async { api::groups().await.unwrap_or_default() });
    let personas_res =
        LocalResource::new(|| async { api::bot_personas().await.unwrap_or_default() });
    let personas = Signal::derive(move || personas_res.get().unwrap_or_default());

    let seats_c = seats.clone();
    let submit = move |_| {
        let n = mode.get_untracked().seat_count();
        let bot_seats: Vec<u8> = (1..n)
            .filter(|i| seats_c[*i].kind.get_untracked() == "bot")
            .map(|i| i as u8)
            .collect();
        let bot_personas: Vec<Option<String>> = (0..n)
            .map(|i| {
                let p = seats_c[i].persona.get_untracked();
                (bot_seats.contains(&(i as u8)) && !p.is_empty()).then_some(p)
            })
            .collect();
        let title = name.get_untracked().trim().to_string();
        let req = CreatePrivateGameRequest {
            mode: mode.get_untracked(),
            name: (!title.is_empty()).then_some(title),
            bot_personas: if bot_personas.iter().any(|p| p.is_some()) {
                bot_personas
            } else {
                Vec::new()
            },
            human_seats: Some((n - bot_seats.len()) as u8),
            bot_seats,
            settings: GameSettings {
                turn_timeout_secs: timeout.get_untracked(),
                weapons_development: weapons.get_untracked(),
                axis_economic_victory: threshold(axis_econ, axis_th, 84),
                allied_economic_victory: threshold(ally_econ, ally_th, 84),
                soviet_first_turn_restriction: soviet.get_untracked(),
                original_complexes_only: orig.get_untracked(),
                total_victory: total.get_untracked(),
                germany_jet_power: jet.get_untracked(),
                japan_super_subs: subs.get_untracked(),
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
        let seats = seats.clone();
        move || {
            (0..mode.get().seat_count())
                .map(|i| view! { <SeatRow index=i mode seat=seats[i] personas /> })
                .collect_view()
        }
    };
    view! {
        <form class="col create-form" on:submit=|ev| ev.prevent_default()>
            <TextField label="Game name (optional, up to 60 characters)" value=name placeholder="Friday night showdown" />
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
            <p class="muted small-text">"Open seats are filled by invite code; the game starts when every open seat is taken (or you start it early and bots fill in)."</p>
            <h3>"Settings"</h3>
            <label>"Turn timeout"
                <select on:change:target=move |ev| timeout.set(ev.target().value().parse().unwrap_or(172_800))>
                    {PRESETS.iter().map(|(s, l)| view! { <option value=s.to_string() selected=*s == 172_800>{*l}</option> }).collect_view()}
                </select>
            </label>
            <Opt label="Weapons development" help="Adds a research phase at the start of each turn where powers can pay for technology breakthroughs." value=weapons />
            <Opt label="Axis economic victory" help="The Axis win at once when their combined production reaches the threshold at the end of a round." value=axis_econ />
            <Show when=move || axis_econ.get()>
                <TextField label="Axis threshold (IPC per round)" value=axis_th kind="number" />
            </Show>
            <Opt label="Allied economic victory" help="Same idea for the Allies; off by default because they start far behind." value=ally_econ />
            <Show when=move || ally_econ.get()>
                <TextField label="Allied threshold (IPC per round)" value=ally_th kind="number" />
            </Show>
            <Opt label="Soviet first-turn restriction" help="The Soviet Union may not attack on its first turn." value=soviet />
            <Opt label="Original complexes only" help="No new industrial complexes can be bought; only the starting ones produce." value=orig />
            <Opt label="Total victory" help="A military victory also requires the winners to hold every one of their own capitals." value=total />
            <Opt label="Germany jet power" help="Balance option: Germany begins with the jet-power technology." value=jet />
            <Opt label="Japan super submarines" help="Balance option: Japan begins with the super-submarine technology." value=subs />
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

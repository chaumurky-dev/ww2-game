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
use wartide_protocol::game::{
    check_partition, preset_layout, CreatePrivateGameRequest, GameMode, GameSettings, SeatKind,
    SeatSpec,
};

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

/// Powers of seat `index` under `assign` (power -> seat), in turn order.
fn seat_powers(assign: &[usize; 5], index: usize) -> Vec<Power> {
    Power::ALL
        .into_iter()
        .zip(assign)
        .filter(|(_, s)| **s == index)
        .map(|(p, _)| p)
        .collect()
}

fn assign_from_layout(layout: &[Vec<Power>]) -> [usize; 5] {
    let mut a = [0; 5];
    for (i, seat) in layout.iter().enumerate() {
        for p in seat {
            if let Some(k) = Power::ALL.iter().position(|x| x == p) {
                a[k] = i;
            }
        }
    }
    a
}

fn layout_of(assign: &[usize; 5], players: usize) -> Vec<Vec<Power>> {
    (0..players).map(|i| seat_powers(assign, i)).collect()
}

fn preset_blurb(n: usize) -> &'static str {
    match n {
        1 => "you play all five powers",
        2 => "Axis vs Allies",
        3 => "Axis / USSR / UK + USA",
        4 => "Germany / Japan / USSR / UK + USA",
        _ => "one power each",
    }
}

/// Seat configuration: the powers it controls, who plays it and (for bots) the persona.
#[component]
fn SeatRow(
    index: usize,
    powers: Signal<Vec<Power>>,
    seat: Seat,
    personas: Signal<Vec<BotPersona>>,
    on_kind: Callback<(usize, String)>,
) -> impl IntoView {
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
                <div class="seat-label">
                    <span class="muted">{format!("Seat {} ", index + 1)}</span>
                    {move || {
                        let ps = powers.get();
                        if ps.is_empty() {
                            view! { <span class="error">"no powers"</span> }.into_any()
                        } else {
                            ps.into_iter().map(|p| view! { <PowerBadge power=p short=true /> " " }).collect_view().into_any()
                        }
                    }}
                </div>
                <select aria-label="Seat type" on:change:target=move |ev| on_kind.run((index, ev.target().value()))>
                    <option value="me" selected=move || seat.kind.get() == "me">"Me"</option>
                    <option value="open" selected=move || seat.kind.get() == "open">"Open seat (friend joins)"</option>
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
    let players = RwSignal::new(5usize);
    let custom = RwSignal::new(false);
    let assign = RwSignal::new(assign_from_layout(&preset_layout(5)));
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

    let layout = Memo::new(move |_| layout_of(&assign.get(), players.get()));
    let mode = Memo::new(move |_| GameMode::derive(&layout.get()));
    // Why the current configuration cannot be created (None = fine).
    let problem = {
        let seats = seats.clone();
        Memo::new(move |_| {
            let n = players.get();
            if let Err(e) = check_partition(&layout.get()) {
                return Some(e.to_string());
            }
            let me = (0..n).filter(|i| seats[*i].kind.get() == "me").count();
            (me != 1).then(|| "Exactly one seat must be yours.".to_string())
        })
    };
    let set_players = {
        let seats = seats.clone();
        move |n: usize| {
            players.set(n);
            custom.set(false);
            assign.set(assign_from_layout(&preset_layout(n)));
            // keep exactly one "me" among the visible seats
            if !(0..n).any(|i| seats[i].kind.get_untracked() == "me") {
                seats[0].kind.set("me".into());
            }
        }
    };
    let on_kind = {
        let seats = seats.clone();
        Callback::new(move |(i, kind): (usize, String)| {
            if kind == "me" {
                for (j, s) in seats.iter().enumerate() {
                    if j != i && s.kind.get_untracked() == "me" {
                        s.kind.set("open".into());
                    }
                }
            }
            seats[i].kind.set(kind);
        })
    };
    let seats_c = seats.clone();
    let submit = move |_| {
        let n = players.get_untracked();
        let specs: Vec<SeatSpec> = layout
            .get_untracked()
            .into_iter()
            .enumerate()
            .map(|(i, powers)| {
                let (kind, persona) = match seats_c[i].kind.get_untracked().as_str() {
                    "me" => (SeatKind::Host, None),
                    "bot" => {
                        let p = seats_c[i].persona.get_untracked();
                        (SeatKind::Bot, (!p.is_empty()).then_some(p))
                    }
                    _ => (SeatKind::Open, None),
                };
                SeatSpec {
                    powers,
                    kind,
                    persona,
                }
            })
            .collect();
        debug_assert_eq!(specs.len(), n);
        let title = name.get_untracked().trim().to_string();
        let req = CreatePrivateGameRequest {
            mode: mode.get_untracked(),
            seats: Some(specs),
            name: (!title.is_empty()).then_some(title),
            bot_personas: Vec::new(),
            human_seats: None,
            bot_seats: Vec::new(),
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
            (0..players.get())
                .map(|i| {
                    let powers = Signal::derive(move || seat_powers(&assign.get(), i));
                    view! { <SeatRow index=i powers seat=seats[i] personas on_kind /> }
                })
                .collect_view()
        }
    };
    let power_pickers = move || {
        Power::ALL
            .into_iter()
            .enumerate()
            .map(|(k, p)| {
                view! {
                    <label class="power-pick"><PowerBadge power=p short=true />
                        <select aria-label=format!("Seat for {}", p.name()) on:change:target=move |ev| {
                            let seat: usize = ev.target().value().parse().unwrap_or(0);
                            assign.update(|a| a[k] = seat);
                            custom.set(true);
                        }>
                            {move || (0..players.get()).map(|i| view! {
                                <option value=i.to_string() selected=move || assign.with(|a| a[k] == i)>{format!("seat {}", i + 1)}</option>
                            }).collect_view()}
                        </select>
                    </label>
                }
            })
            .collect_view()
    };
    view! {
        <form class="col create-form" on:submit=|ev| ev.prevent_default()>
            <TextField label="Game name (optional, up to 60 characters)" value=name placeholder="Friday night showdown" />
            <label>"Players"
                <select aria-label="Players" on:change:target=move |ev| set_players(ev.target().value().parse().unwrap_or(5))>
                    {(1..=5usize).map(|n| view! {
                        <option value=n.to_string() selected=move || players.get() == n>
                            {format!("{n} player{} - {}", if n == 1 { "" } else { "s" }, preset_blurb(n))}
                        </option>
                    }).collect_view()}
                </select>
            </label>
            <label>"Split of the five powers"
                <select aria-label="Split" on:change:target=move |ev| {
                    if ev.target().value() == "classic" {
                        assign.set(assign_from_layout(&preset_layout(players.get_untracked())));
                        custom.set(false);
                    } else {
                        custom.set(true);
                    }
                }>
                    <option value="classic" selected=move || !custom.get()>{move || format!("Classic: {}", preset_blurb(players.get()))}</option>
                    <option value="custom" selected=move || custom.get()>"Custom"</option>
                </select>
            </label>
            <div class="power-picks">{power_pickers}</div>
            <h3>"Seats"</h3>
            {seat_rows}
            <Show when=move || problem.get().is_some()>
                <p class="error">{move || problem.get()}</p>
            </Show>
            <p class="muted small-text">"Open seats are filled by invite code; the game starts when every open seat is taken (or you start it early and bots fill in). With no open seats it starts at once."</p>
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
            <Show when=move || !group.get().is_empty() && mode.get() == GameMode::Custom>
                <p class="muted small-text">"This split is not rated: only the 2-player (Axis vs Allies) and 5-player (one power each) layouts count for a group ladder."</p>
            </Show>
            <button type="submit" disabled=move || problem.get().is_some() on:click=submit>"Create game"</button>
        </form>
    }
}

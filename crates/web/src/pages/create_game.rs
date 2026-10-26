mod layout;
mod request;

use crate::api;
use crate::components::form::{Check, TextField};
use crate::components::power_badge::PowerBadge;
use crate::model::*;
use crate::state::Toasts;
use layout::SetupForm;
use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::hooks::use_navigate;
use request::{build_request, Opts};
use wartide_protocol::bots::BotPersona;
use wartide_protocol::game::{GameMode, SeatKind, Team};

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

fn kind_value(kind: SeatKind) -> &'static str {
    match kind {
        SeatKind::Host => "me",
        SeatKind::Open => "open",
        SeatKind::Bot => "bot",
    }
}

fn kind_from_value(v: &str) -> SeatKind {
    match v {
        "me" => SeatKind::Host,
        "bot" => SeatKind::Bot,
        _ => SeatKind::Open,
    }
}

/// The classic split for `n` human players; one human plays a whole side against the computer.
fn preset_blurb(n: usize) -> &'static str {
    match n {
        1 => "you vs the computer",
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
    form: RwSignal<SetupForm>,
    personas: Signal<Vec<BotPersona>>,
) -> impl IntoView {
    let kind = move || form.with(|f| f.kinds[index]);
    let persona = move || form.with(|f| f.personas[index].clone());
    let is_bot = move || kind() == SeatKind::Bot;
    let blurb = move || {
        let id = persona();
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
                        let ps = form.with(|f| f.seat_powers(index));
                        if ps.is_empty() {
                            view! { <span class="error">"no powers"</span> }.into_any()
                        } else {
                            ps.into_iter().map(|p| view! { <PowerBadge power=p short=true /> " " }).collect_view().into_any()
                        }
                    }}
                </div>
                <select aria-label="Seat type" on:change:target=move |ev| form.update(|f| f.set_kind(index, kind_from_value(&ev.target().value())))>
                    {[SeatKind::Host, SeatKind::Open, SeatKind::Bot].map(|k| {
                        let label = match k {
                            SeatKind::Host => "Me",
                            SeatKind::Open => "Open seat (friend joins)",
                            SeatKind::Bot => "Bot",
                        };
                        view! { <option value=kind_value(k) selected=move || kind() == k>{label}</option> }
                    })}
                </select>
                <Show when=is_bot>
                    <select aria-label="Bot persona" on:change:target=move |ev| form.update(|f| f.set_persona(index, ev.target().value()))>
                        <option value="" selected=move || persona().is_empty()>"Default persona"</option>
                        {move || personas.get().into_iter().map(|p| {
                            let id = p.id.clone();
                            view! { <option value=id.clone() selected=move || persona() == id>{persona_label(&p)}</option> }
                        }).collect_view()}
                    </select>
                </Show>
            </div>
            <Show when=move || is_bot() && blurb().is_some()>
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

#[component]
pub fn CreateGameForm(open: RwSignal<bool>) -> impl IntoView {
    let toasts = Toasts::expect();
    let navigate = use_navigate();
    let form = RwSignal::new(SetupForm::default());
    // Memos so the views below only re-render when their own slice of the form changes.
    let humans = Memo::new(move |_| form.with(|f| f.humans));
    let side = Memo::new(move |_| form.with(|f| f.side));
    let seat_count = Memo::new(move |_| form.with(|f| f.seat_count));
    let custom = Memo::new(move |_| form.with(|f| f.custom));
    let mode = Memo::new(move |_| form.with(|f| f.mode()));
    // Why the current configuration cannot be created (None = fine).
    let problem = Memo::new(move |_| form.with(|f| f.problems().into_iter().next()));
    let name = RwSignal::new(String::new());
    let opts = Opts::new();
    let group = RwSignal::new(String::new());
    let groups = LocalResource::new(|| async { api::groups().await.unwrap_or_default() });
    let personas_res =
        LocalResource::new(|| async { api::bot_personas().await.unwrap_or_default() });
    let personas = Signal::derive(move || personas_res.get().unwrap_or_default());

    let submit = move |_| {
        let req = build_request(
            form.with_untracked(|f| f.to_seats()),
            mode.get_untracked(),
            &name.get_untracked(),
            opts.settings(),
            &group.get_untracked(),
        );
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
    let seat_rows = move || {
        (0..seat_count.get())
            .map(|i| view! { <SeatRow index=i form personas /> })
            .collect_view()
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
                            form.update(|f| f.assign_power(k, seat));
                        }>
                            {move || (0..seat_count.get()).map(|i| view! {
                                <option value=i.to_string() selected=move || form.with(|f| f.assign[k] == i)>{format!("seat {}", i + 1)}</option>
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
            <label>"Human players"
                <select aria-label="Human players" on:change:target=move |ev| {
                    form.update(|f| f.set_humans(ev.target().value().parse().unwrap_or(1)))
                }>
                    {(1..=5usize).map(|n| view! {
                        <option value=n.to_string() selected=move || humans.get() == n>
                            {format!("{n} player{} - {}", if n == 1 { "" } else { "s" }, preset_blurb(n))}
                        </option>
                    }).collect_view()}
                </select>
            </label>
            <Show when=move || humans.get() == 1>
                <label>"Your side"
                    <select aria-label="Your side" on:change:target=move |ev| {
                        let team = if ev.target().value() == "axis" { Team::Axis } else { Team::Allies };
                        form.update(|f| f.set_side(team));
                    }>
                        <option value="allies" selected=move || side.get() == Team::Allies>"Allies - Soviet Union, United Kingdom, United States"</option>
                        <option value="axis" selected=move || side.get() == Team::Axis>"Axis - Germany, Japan"</option>
                    </select>
                </label>
                <p class="muted small-text">"Each enemy power is played by its own computer general; pick their personas below."</p>
            </Show>
            <label>"Split of the five powers"
                <select aria-label="Split" on:change:target=move |ev| {
                    form.update(|f| f.set_classic(ev.target().value() == "classic"))
                }>
                    <option value="classic" selected=move || !custom.get()>{move || format!("Classic: {}", preset_blurb(humans.get()))}</option>
                    <option value="custom" selected=move || custom.get()>"Custom - any number of seats, any split, friends or bots in each"</option>
                </select>
            </label>
            <Show when=move || custom.get()>
                <label>"Seats"
                    <select aria-label="Seats" on:change:target=move |ev| {
                        form.update(|f| f.set_seat_count(ev.target().value().parse().unwrap_or(2)))
                    }>
                        {(1..=5usize).map(|n| view! {
                            <option value=n.to_string() selected=move || seat_count.get() == n>{format!("{n} seat{}", if n == 1 { "" } else { "s" })}</option>
                        }).collect_view()}
                    </select>
                </label>
                <div class="power-picks">{power_pickers}</div>
            </Show>
            <h3>"Seats"</h3>
            {seat_rows}
            <Show when=move || problem.get().is_some()>
                <p class="error">{move || problem.get()}</p>
            </Show>
            <p class="muted small-text">"Open seats are filled by invite code; the game starts when every open seat is taken (or you start it early and bots fill in). With no open seats it starts at once."</p>
            <h3>"Settings"</h3>
            <label>"Turn timeout"
                <select on:change:target=move |ev| opts.timeout.set(ev.target().value().parse().unwrap_or(172_800))>
                    {PRESETS.iter().map(|(s, l)| view! { <option value=s.to_string() selected=*s == 172_800>{*l}</option> }).collect_view()}
                </select>
            </label>
            <Opt label="Weapons development" help="Adds a research phase at the start of each turn where powers can pay for technology breakthroughs." value=opts.weapons />
            <Opt label="Axis economic victory" help="The Axis win at once when their combined production reaches the threshold at the end of a round." value=opts.axis_econ />
            <Show when=move || opts.axis_econ.get()>
                <TextField label="Axis threshold (IPC per round)" value=opts.axis_th kind="number" />
            </Show>
            <Opt label="Allied economic victory" help="Same idea for the Allies; off by default because they start far behind." value=opts.ally_econ />
            <Show when=move || opts.ally_econ.get()>
                <TextField label="Allied threshold (IPC per round)" value=opts.ally_th kind="number" />
            </Show>
            <Opt label="Soviet first-turn restriction" help="The Soviet Union may not attack on its first turn." value=opts.soviet />
            <Opt label="Original complexes only" help="No new industrial complexes can be bought; only the starting ones produce." value=opts.orig />
            <Opt label="Total victory" help="A military victory also requires the winners to hold every one of their own capitals." value=opts.total />
            <Opt label="Germany jet power" help="Balance option: Germany begins with the jet-power technology." value=opts.jet />
            <Opt label="Japan super submarines" help="Balance option: Japan begins with the super-submarine technology." value=opts.subs />
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

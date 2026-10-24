//! Human-readable action log built from engine events (formatting is pure and unit-tested).
use super::conv::*;
use super::logfmt_battle::battle_arm;
use crate::model::Power as UiPower;
use crate::model::PowerExt;
use leptos::prelude::*;
use wartide_engine::event::DiceContext;
use wartide_engine::units::UnitRef;
use wartide_engine::{Event, LocationId, Power};

/// Maximum number of lines kept.
pub const MAX_LINES: usize = 200;

#[derive(Clone, Debug, PartialEq)]
pub struct LogLine {
    pub round: u32,
    pub power: Option<UiPower>,
    pub text: String,
}

pub(super) fn refs(v: &[UnitRef]) -> String {
    describe_counts(v.iter().map(|r| (r.kind, 1)).collect())
}

fn dice_str(r: &[u8]) -> String {
    r.iter()
        .map(|d| d.to_string())
        .collect::<Vec<_>>()
        .join(" ")
}

pub(super) fn hits_str(n: u32) -> String {
    format!("{n} hit{}", if n == 1 { "" } else { "s" })
}

fn dice_label(c: &DiceContext) -> &'static str {
    match c {
        DiceContext::Research => "Research dice",
        DiceContext::ResearchTech => "Tech die",
        DiceContext::AaFire { .. } => "AA fire",
        DiceContext::Bombardment { .. } => "Bombardment",
        DiceContext::FirstStrike { .. } => "Submarine strike",
        DiceContext::Attack { .. } => "Attack dice",
        DiceContext::Defense { .. } => "Defense dice",
        DiceContext::StrategicBombing { .. } => "Bombing dice",
        DiceContext::Rocket { .. } => "Rocket die",
    }
}

fn l(x: LocationId) -> &'static str {
    loc_name(x)
}

/// Format a batch of events; `round` is the round in force before the batch.
pub fn format_events(events: &[Event], mut round: u32) -> Vec<LogLine> {
    let mut out: Vec<LogLine> = vec![];
    let mut i = 0;
    while i < events.len() {
        let mut who: Option<Power> = None;
        let mut text: Option<String> = None;
        match &events[i] {
            Event::TurnStarted { round: r, power } => {
                round = *r;
                who = Some(*power);
                text = Some(format!("Round {r}: {} begins", power.name()));
            }
            Event::DiceRolled {
                power,
                context,
                rolls,
                ..
            } => {
                who = Some(*power);
                let mut parts = vec![dice_str(rolls)];
                // Merge consecutive defense dice, then the hit tally that follows.
                if matches!(context, DiceContext::Defense { .. }) {
                    while let Some(Event::DiceRolled {
                        context: DiceContext::Defense { .. },
                        rolls,
                        ..
                    }) = events.get(i + 1)
                    {
                        parts.push(dice_str(rolls));
                        i += 1;
                    }
                }
                let mut t = format!("{}: {}", dice_label(context), parts.join(" | "));
                if let Some(Event::HitsScored { hits, .. }) = events.get(i + 1) {
                    if matches!(
                        context,
                        DiceContext::Attack { .. }
                            | DiceContext::Defense { .. }
                            | DiceContext::FirstStrike { .. }
                    ) {
                        t.push_str(&format!(" ({})", hits_str(*hits)));
                        i += 1;
                    }
                }
                text = Some(t);
            }
            Event::ResearchBought { power, dice, cost } => {
                who = Some(*power);
                text = Some(format!(
                    "{} buys {dice} research dice for {cost} IPC",
                    power.name()
                ));
            }
            Event::TechGained { power, tech } => {
                who = Some(*power);
                text = Some(format!("{} develops {}!", power.name(), tech_name(*tech)));
            }
            Event::Purchased { power, items, cost } => {
                who = Some(*power);
                let v = items.iter().map(|i| (i.unit, i.count)).collect();
                text = Some(format!(
                    "{} purchases {} for {cost} IPC",
                    power.name(),
                    describe_counts(v)
                ));
            }
            Event::UnitsMoved {
                power,
                units,
                from,
                path,
            } => {
                who = Some(*power);
                let to = path.last().copied().unwrap_or(*from);
                text = Some(format!(
                    "{} moves {} {} \u{2192} {}",
                    power.name(),
                    refs(units),
                    l(*from),
                    l(to)
                ));
            }
            Event::UnitsLoaded {
                power, units, at, ..
            } => {
                who = Some(*power);
                text = Some(format!(
                    "{} loads {} onto a transport in {}",
                    power.name(),
                    refs(units),
                    l(*at)
                ));
            }
            Event::UnitsUnloaded {
                power,
                units,
                to,
                assault,
                ..
            } => {
                who = Some(*power);
                text = Some(format!(
                    "{} unloads {} to {}{}",
                    power.name(),
                    refs(units),
                    l(*to),
                    if *assault {
                        " (amphibious assault)"
                    } else {
                        ""
                    }
                ));
            }
            Event::AircraftLanded { unit, .. } => {
                who = Some(unit.owner);
                text = Some(format!("{} lands on a carrier", refs(&[*unit])));
            }
            Event::AaFire {
                gun_owner,
                location,
                destroyed,
            } => {
                who = Some(*gun_owner);
                text = Some(if destroyed.is_empty() {
                    format!("AA fire at {} misses", l(*location))
                } else {
                    format!("AA fire at {} destroys {}", l(*location), refs(destroyed))
                });
            }
            Event::NeutralViolated {
                power,
                location,
                fee,
            } => {
                who = Some(*power);
                text = Some(format!(
                    "{} violates neutral {} (fee {fee} IPC)",
                    power.name(),
                    l(*location)
                ));
            }
            Event::TerritoryCaptured {
                power,
                location,
                from,
                liberated,
            } => {
                who = Some(*power);
                let src = from
                    .map(|f| format!(" from {}", f.name()))
                    .unwrap_or_default();
                text = Some(format!(
                    "{} {} {}{}",
                    power.name(),
                    if *liberated { "liberates" } else { "captures" },
                    l(*location),
                    src
                ));
            }
            Event::CapitalCaptured {
                power,
                capital_of,
                ipc_seized,
            } => {
                who = Some(*power);
                text = Some(format!(
                    "{} captures the capital of {} and seizes {ipc_seized} IPC",
                    power.name(),
                    capital_of.name()
                ));
            }
            Event::ControlRestored { power, locations } => {
                who = Some(*power);
                let n: Vec<&str> = locations.iter().map(|x| l(*x)).collect();
                text = Some(format!("{} regains {}", power.name(), n.join(", ")));
            }
            e @ (Event::BattleStarted { .. }
            | Event::BattleRound { .. }
            | Event::HitsScored { .. }
            | Event::CasualtiesRemoved { .. }
            | Event::Bombardment { .. }
            | Event::Retreated { .. }
            | Event::SubsWithdrew { .. }
            | Event::BattleEnded { .. }
            | Event::AssaultCancelled { .. }
            | Event::StrategicBombing { .. }
            | Event::RocketFired { .. }) => battle_arm(e, &mut who, &mut text),
            Event::UnitsPlaced {
                power,
                kind,
                location,
                ids,
            } => {
                who = Some(*power);
                text = Some(format!(
                    "{} places {} {} at {}",
                    power.name(),
                    ids.len(),
                    unit_name(*kind, ids.len() as u32),
                    l(*location)
                ));
            }
            Event::UnitsLost {
                power,
                units,
                reason,
            } => {
                who = Some(*power);
                text = Some(format!("{} loses {} ({reason})", power.name(), refs(units)));
            }
            Event::IncomeCollected {
                power,
                amount,
                treasury,
            } => {
                who = Some(*power);
                text = Some(format!(
                    "{} collects {amount} IPC (treasury {treasury})",
                    power.name()
                ));
            }
            Event::Conceded { power } => {
                who = Some(*power);
                text = Some(format!("{} concedes", power.name()));
            }
            Event::GameOver { winner, reason } => {
                text = Some(format!("Game over: {winner:?} win ({reason:?})"));
            }
            Event::PhaseChanged { .. } => {}
        }
        if let Some(text) = text {
            out.push(LogLine {
                round,
                power: who.map(ui_power),
                text,
            });
        }
        i += 1;
    }
    out
}

/// Append lines, keeping only the most recent [`MAX_LINES`].
pub fn push_lines(v: &mut Vec<LogLine>, new: Vec<LogLine>) {
    v.extend(new);
    if v.len() > MAX_LINES {
        let cut = v.len() - MAX_LINES;
        v.drain(..cut);
    }
}

#[component]
pub fn ActionLog(entries: RwSignal<Vec<LogLine>>) -> impl IntoView {
    let open = RwSignal::new(false);
    view! {
        <div class="actionlog">
            <div class="row">
                <h3 class="grow" style="margin:0">"Log"</h3>
                <button class="secondary small" on:click=move |_| open.update(|o| *o = !*o)>
                    {move || if open.get() { "Collapse" } else { "Expand" }}
                </button>
            </div>
            <div class="log" class:expanded=move || open.get()>
                {move || {
                    let v = entries.get();
                    if v.is_empty() {
                        return view! { <div class="muted">"Nothing has happened yet."</div> }.into_any();
                    }
                    v.into_iter()
                        .rev()
                        .map(|e| {
                            let style = e.power.map(|p| format!("border-left:3px solid var(--{})", p.id()));
                            view! { <div class="logline" style=style><span class="muted">{format!("R{} ", e.round)}</span>{e.text}</div> }
                        })
                        .collect_view()
                        .into_any()
                }}
            </div>
        </div>
    }
}

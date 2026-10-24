use super::common::EndPhase;
use crate::game::conv::*;
use crate::game::ctx::GameCtx;
use leptos::prelude::*;
use wartide_engine::battle::beginnable_battles;
use wartide_engine::bombing::{bombing_targets, rocket_targets};
use wartide_engine::query::{battle_odds, battles_pending};
use wartide_engine::{Action, Power, UnitType};

#[component]
pub fn CombatPanel(power: Power) -> impl IntoView {
    let ctx = expect_context::<GameCtx>();
    let list = move || {
        let st = ctx.state.get();
        let pending = battles_pending(&st);
        let can = beginnable_battles(&st);
        if pending.is_empty() {
            return view! { <p class="muted">"No battles to fight."</p> }.into_any();
        }
        pending
            .into_iter()
            .map(|loc| {
                let odds = battle_odds(&st, loc, 300, 0x5eed + loc.0 as u64);
                let ok = can.contains(&loc);
                let odds_txt = odds.map(|o| {
                    format!(
                        "Attacker wins {:.0}% \u{b7} defender {:.0}% \u{b7} stalemate {:.0}%",
                        o.attacker_win * 100.0,
                        o.defender_win * 100.0,
                        o.stalemate * 100.0
                    )
                });
                view! {
                    <div class="battlerow">
                        <div class="grow">
                            <strong>{loc_name(loc)}</strong>
                            <div class="muted">{odds_txt.unwrap_or_else(|| "No contest".into())}</div>
                        </div>
                        <button class="primary" disabled=!ok title=if ok { "" } else { "Resolve other battles first" }
                            on:click=move |_| ctx.send(Action::BeginBattle { location: loc })>"Begin battle"</button>
                    </div>
                }
            })
            .collect_view()
            .into_any()
    };
    let bombs = move || {
        let st = ctx.state.get();
        bombing_targets(&st, power)
            .into_iter()
            .map(|(loc, bombers)| {
                let n = bombers.len();
                view! {
                    <button class="secondary" on:click=move |_| ctx.send(Action::DeclareStrategicBombing { location: loc, bombers: bombers.clone() })>
                        {format!("Strategic bombing: {n} bomber{} \u{2192} {}", if n == 1 { "" } else { "s" }, loc_name(loc))}
                    </button>
                }
            })
            .collect_view()
    };
    let rockets = move || {
        let st = ctx.state.get();
        if st.turn.rocket_fired {
            return vec![];
        }
        st.units
            .iter()
            .filter(|u| u.owner == power && u.kind == UnitType::AaGun)
            .flat_map(|g| {
                let gid = g.id;
                let at = g.location;
                rocket_targets(&st, gid).into_iter().map(move |t| (gid, at, t))
            })
            .map(|(gid, at, t)| {
                view! {
                    <button class="secondary" on:click=move |_| ctx.send(Action::FireRocket { aa_gun: gid, target: t })>
                        {format!("Rocket from {} \u{2192} {}", loc_name(at), loc_name(t))}
                    </button>
                }
            })
            .collect::<Vec<_>>()
    };
    let n_pending = move || ctx.state.with(|s| battles_pending(s).len());
    view! {
        <h3>"Combat"</h3>
        {list}
        <div class="col">{bombs}{move || rockets().into_iter().collect_view()}</div>
        {move || (n_pending() > 0).then(|| view! { <div class="warn">{format!("{} battle(s) unresolved: attackers there stay put.", n_pending())}</div> })}
        <div class="row"><EndPhase label="End combat" secondary=true /></div>
    }
}

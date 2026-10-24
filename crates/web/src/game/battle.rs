//! Battle board: both sides, dice, and the decision controls of the current battle step.
use super::conv::*;
use super::ctx::GameCtx;
use super::dice::DiceBoard;
use crate::components::power_badge::PowerBadge;
use crate::components::unit_icon::UnitIcon;
use leptos::prelude::*;
use std::collections::BTreeMap;
use wartide_engine::battle::{defender_withdraw_options, sub_withdraw_options};
use wartide_engine::casualties::{candidates, default_pick, required_casualties};
use wartide_engine::query::retreat_destinations;
use wartide_engine::state::{Battle, BattleKind};
use wartide_engine::tech::{attack_value, defense_value};
use wartide_engine::{pending_decision, Action, Decision, GameState, Power, UnitId, UnitType};

/// (kind, alive, marked-as-casualty, hit number) rows for one side.
pub fn side_rows(
    st: &GameState,
    ids: &[UnitId],
    marked: &[UnitId],
    attacking: bool,
) -> Vec<(UnitType, u32, u32, u8)> {
    let mut m: BTreeMap<UnitType, (u32, u32, u8)> = BTreeMap::new();
    for &id in ids {
        let Some(u) = st.unit(id) else { continue };
        let hit = if attacking {
            attack_value(st, u)
        } else {
            defense_value(st, u)
        };
        let e = m.entry(u.kind).or_insert((0, 0, hit));
        e.0 += 1;
        if marked.contains(&id) {
            e.1 += 1;
        }
    }
    m.into_iter().map(|(k, (n, x, h))| (k, n, x, h)).collect()
}

fn side_view(
    power: Power,
    rows: Vec<(UnitType, u32, u32, u8)>,
    title: &'static str,
) -> impl IntoView {
    view! {
        <div class="bside">
            <div class="row"><strong>{title}</strong><PowerBadge power=ui_power(power) short=true /></div>
            {rows.into_iter().map(|(k, n, x, h)| view! {
                <div class="brow" class:dead=x == n>
                    <UnitIcon kind=ui_kind(k) power=ui_power(power) />
                    <span class="grow">{format!("{} \u{d7}{n}", ui_kind(k).name())}{(x > 0).then(|| format!(" ({x} lost)"))}</span>
                    <span class="hitnum" title="Hits on this number or lower">{if h == 0 { "\u{2013}".to_string() } else { format!("\u{2264}{h}") }}</span>
                </div>
            }).collect_view()}
        </div>
    }
}

#[component]
fn CasualtyPicker(hits: u32, sub_hits: u32, eligible: Vec<UnitId>) -> impl IntoView {
    let ctx = expect_context::<GameCtx>();
    let st = ctx.state.get_untracked();
    let Some(b) = st.battle.clone() else {
        return ().into_any();
    };
    let side = b
        .pending
        .as_ref()
        .map(|p| p.side)
        .unwrap_or(wartide_engine::state::Side::Attacker);
    let cand = candidates(&st, &b, side);
    let normal = hits - sub_hits;
    let (need, min_nonair) = required_casualties(&st, &cand, normal, sub_hits);
    let suggested = default_pick(&st, &cand, normal, sub_hits);
    let mut groups: BTreeMap<UnitType, Vec<UnitId>> = BTreeMap::new();
    for &id in &eligible {
        if let Some(u) = st.unit(id) {
            groups.entry(u.kind).or_default().push(id);
        }
    }
    let rows: Vec<(UnitType, Vec<UnitId>, RwSignal<u32>)> = groups
        .into_iter()
        .map(|(k, ids)| {
            let pre = ids.iter().filter(|i| suggested.contains(i)).count() as u32;
            (k, ids, RwSignal::new(pre))
        })
        .collect();
    let rows_total = rows.clone();
    let total = Memo::new(move |_| rows_total.iter().map(|r| r.2.get() as usize).sum::<usize>());
    let rows_air = rows.clone();
    let nonair = Memo::new(move |_| {
        rows_air
            .iter()
            .filter(|r| !r.0.is_air())
            .map(|r| r.2.get() as usize)
            .sum::<usize>()
    });
    let rows_btn = rows.clone();
    let ok = move || total.get() == need && nonair.get() >= min_nonair;
    view! {
        <div class="decision">
            <strong>{format!("Choose {need} casualt{} ({hits} hit{})", if need == 1 { "y" } else { "ies" }, if hits == 1 { "" } else { "s" })}</strong>
            {(sub_hits > 0).then(|| view! { <div class="muted">{format!("{sub_hits} hit(s) from submarines cannot be assigned to aircraft.")}</div> })}
            {rows.iter().map(|(k, ids, sig)| {
                let (k, n, sig) = (*k, ids.len() as u32, *sig);
                view! {
                    <div class="urow">
                        <UnitIcon kind=ui_kind(k) />
                        <span class="grow">{format!("{} (of {n})", ui_kind(k).name())}</span>
                        <button class="secondary small" on:click=move |_| sig.update(|v| *v = v.saturating_sub(1))>"\u{2212}"</button>
                        <strong class="stepval">{move || sig.get()}</strong>
                        <button class="secondary small" on:click=move |_| sig.update(|v| *v = (*v + 1).min(n))>"+"</button>
                    </div>
                }
            }).collect_view()}
            <div class="row">
                <span class="muted">{move || format!("{} / {need} selected", total.get())}</span>
                <span class="spacer"></span>
                <button disabled=move || !ok() on:click=move |_| {
                    let units: Vec<UnitId> = rows_btn.iter().flat_map(|(_, ids, s)| ids.iter().copied().take(s.get_untracked() as usize)).collect();
                    ctx.send(Action::SelectCasualties { units });
                }>"Remove casualties"</button>
            </div>
        </div>
    }.into_any()
}

fn controls(ctx: GameCtx, st: &GameState, b: &Battle) -> AnyView {
    let d = pending_decision(st);
    match d {
        Decision::SelectCasualties {
            power,
            hits,
            sub_hits,
            eligible,
            ..
        } if ctx.controls(power) => view! { <CasualtyPicker hits sub_hits eligible /> }.into_any(),
        Decision::RetreatOrPress { power, .. } if ctx.controls(power) => {
            let dests = retreat_destinations(st);
            let subs: Vec<UnitId> = b
                .attackers
                .iter()
                .copied()
                .filter(|&i| st.unit(i).is_some_and(|u| u.kind == UnitType::Submarine))
                .collect();
            let wopts = if subs.is_empty() {
                vec![]
            } else {
                sub_withdraw_options(st)
            };
            view! {
                <div class="decision">
                    <strong>"Press the attack or break off?"</strong>
                    <div class="row"><button on:click=move |_| ctx.send(Action::PressAttack)>"Press attack"</button></div>
                    {if b.amphibious {
                        view! { <div class="muted">"Amphibious assaults cannot retreat."</div> }.into_any()
                    } else if dests.is_empty() {
                        view! { <div class="muted">"No retreat destination available."</div> }.into_any()
                    } else {
                        view! { <div class="row">{dests.into_iter().map(|to| view! {
                            <button class="secondary" on:click=move |_| ctx.send(Action::Retreat { to })>{format!("Retreat to {}", loc_name(to))}</button>
                        }).collect_view()}</div> }.into_any()
                    }}
                    {(!wopts.is_empty()).then(|| view! { <div class="row">{wopts.into_iter().map(|to| { let subs = subs.clone(); view! {
                        <button class="secondary" on:click=move |_| ctx.send(Action::WithdrawSubs { subs: subs.clone(), to: Some(to) })>{format!("Withdraw submarines to {}", loc_name(to))}</button>
                    }}).collect_view()}</div> })}
                </div>
            }.into_any()
        }
        Decision::SubWithdrawal { power, subs, .. } if ctx.controls(power) => {
            let wopts = defender_withdraw_options(st, power);
            view! {
                <div class="decision">
                    <strong>"Withdraw your submarines?"</strong>
                    <div class="row">
                        <button on:click=move |_| ctx.send(Action::WithdrawSubs { subs: vec![], to: None })>"Stay and fight"</button>
                        {wopts.into_iter().map(|to| { let subs = subs.clone(); view! {
                            <button class="secondary" on:click=move |_| ctx.send(Action::WithdrawSubs { subs: subs.clone(), to: Some(to) })>{format!("Withdraw to {}", loc_name(to))}</button>
                        }}).collect_view()}
                    </div>
                </div>
            }.into_any()
        }
        _ => ().into_any(),
    }
}

#[component]
pub fn BattleBoard() -> impl IntoView {
    let ctx = expect_context::<GameCtx>();
    view! {
        {move || {
            let st = ctx.state.get();
            let Some(b) = st.battle.clone() else { return ().into_any() };
            let sea = b.kind == BattleKind::Sea;
            let def_power = st.units.iter().find(|u| b.defenders.contains(&u.id)).map(|u| u.owner)
                .unwrap_or_else(|| b.attacker.team().other().powers()[0]);
            let att = side_rows(&st, &b.attackers, &b.marked_att, true);
            let def = side_rows(&st, &b.defenders, &b.marked_def, false);
            let ctl = controls(ctx, &st, &b);
            view! {
                <div class="battle-board">
                    <h3>{format!("Battle: {} \u{b7} round {}", loc_name(b.location), b.round)}</h3>
                    <div class="muted">{format!("{}{}", if sea { "Sea battle" } else { "Land battle" }, if b.amphibious { " \u{b7} amphibious assault" } else { "" })}</div>
                    <div class="battle">
                        {side_view(b.attacker, att, "Attacker")}
                        {side_view(def_power, def, "Defender")}
                    </div>
                    <DiceBoard at=b.location />
                    {ctl}
                </div>
            }.into_any()
        }}
    }
}

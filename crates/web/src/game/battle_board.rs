//! Pop-up battle board: units laid out by combat value (columns 1-4, plus 5 with jet fighters and
//! a "0" column for units with no value on that side), the attacker on top, the defender below,
//! the dice and hit counts in the band between, and the decision controls of the current step.
use super::conv::*;
use super::ctx::GameCtx;
use super::dice::{DiceShow, Die};
use crate::components::power_badge::PowerBadge;
use crate::components::unit_icon::UnitIcon;
use leptos::prelude::*;
use std::collections::BTreeMap;
use wartide_engine::battle::{defender_withdraw_options, sub_withdraw_options};
use wartide_engine::casualties::{candidates, default_pick, required_casualties};
use wartide_engine::query::retreat_destinations;
use wartide_engine::state::{Battle, BattleKind, BattleStep, Side};
use wartide_engine::tech::{attack_value, defense_value};
use wartide_engine::{pending_decision, Action, Decision, GameState, Power, UnitId, UnitType};

/// Whether the battle board is expanded (shared between the modal, its mini bar and the side
/// panel's "Open battle board" button).
#[derive(Clone, Copy)]
pub struct BoardOpen(pub RwSignal<bool>);

/// Units of one kind and owner that share a combat value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Cell {
    pub owner: Power,
    pub kind: UnitType,
    pub count: u32,
    /// How many of them are marked as casualties this round.
    pub marked: u32,
}

/// The board grid: `cols` lists the combat values left to right (0 last); `att[i]` / `def[i]`
/// are the cells in column `cols[i]`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rows {
    pub cols: Vec<u8>,
    pub att: Vec<Vec<Cell>>,
    pub def: Vec<Vec<Cell>>,
}

type CellMap = BTreeMap<(u8, Power, UnitType), (u32, u32)>;

fn side_cells(st: &GameState, ids: &[UnitId], marked: &[UnitId], attacking: bool) -> CellMap {
    let mut m = CellMap::new();
    for &id in ids {
        let Some(u) = st.unit(id) else { continue };
        // The engine's own tech-aware hit numbers (jet fighters, super submarines).
        let v = if attacking {
            attack_value(st, u)
        } else {
            defense_value(st, u)
        };
        let e = m.entry((v, u.owner, u.kind)).or_default();
        e.0 += 1;
        if marked.contains(&id) {
            e.1 += 1;
        }
    }
    m
}

/// Lay a battle out by combat value. Columns 1-4 always exist; a higher column (5 for jets) only
/// when used; the 0 column comes last.
pub fn board_rows(st: &GameState, b: &Battle) -> Rows {
    let att = side_cells(st, &b.attackers, &b.marked_att, true);
    let def = side_cells(st, &b.defenders, &b.marked_def, false);
    let mut cols: Vec<u8> = (1..=4).collect();
    for v in att.keys().chain(def.keys()).map(|k| k.0) {
        if v > 4 && !cols.contains(&v) {
            cols.push(v);
        }
    }
    cols.sort_unstable();
    cols.push(0);
    let build = |m: &CellMap| -> Vec<Vec<Cell>> {
        cols.iter()
            .map(|&c| {
                m.iter()
                    .filter(|(k, _)| k.0 == c)
                    .map(|(k, v)| Cell {
                        owner: k.1,
                        kind: k.2,
                        count: v.0,
                        marked: v.1,
                    })
                    .collect()
            })
            .collect()
    };
    Rows {
        att: build(&att),
        def: build(&def),
        cols,
    }
}

/// Hits waiting to be assigned: (on the attacker, on the defender), pending plus queued.
pub fn hits_pending(b: &Battle) -> (u32, u32) {
    let (mut on_att, mut on_def) = (0, 0);
    for p in b.pending.iter().chain(b.queued.iter()) {
        let n = p.normal + p.sub;
        match p.side {
            Side::Attacker => on_att += n,
            Side::Defender => on_def += n,
        }
    }
    (on_att, on_def)
}

fn step_label(s: BattleStep) -> &'static str {
    match s {
        BattleStep::RoundStart => "Round start",
        BattleStep::FirstStrike => "Submarine first strike",
        BattleStep::MainFire => "Main fire",
        BattleStep::Resolve => "Removing casualties",
        BattleStep::Choice => "Press or retreat",
        BattleStep::DefenderSubChoice => "Defender submarine choice",
    }
}

fn defender_powers(st: &GameState, b: &Battle) -> Vec<Power> {
    let mut v: Vec<Power> = b
        .defenders
        .iter()
        .filter_map(|&i| st.unit(i))
        .map(|u| u.owner)
        .collect();
    v.sort();
    v.dedup();
    v
}

/// The viewer takes part in the battle (or watches without controlling any power).
pub fn viewer_sees(ctx: GameCtx, st: &GameState, b: &Battle) -> bool {
    ctx.viewer.with(|v| v.is_empty())
        || ctx.controls(b.attacker)
        || defender_powers(st, b).into_iter().any(|p| ctx.controls(p))
}

type Pick = RwSignal<BTreeMap<UnitType, u32>>;

#[component]
fn CasualtyPicker(hits: u32, sub_hits: u32, eligible: Vec<UnitId>, pick: Pick) -> impl IntoView {
    let ctx = expect_context::<GameCtx>();
    let st = ctx.state.get_untracked();
    let Some(b) = st.battle.clone() else {
        return ().into_any();
    };
    let side = b.pending.as_ref().map(|p| p.side).unwrap_or(Side::Attacker);
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
    let rows: Vec<(UnitType, Vec<UnitId>)> = groups.into_iter().collect();
    pick.set(
        rows.iter()
            .map(|(k, ids)| {
                (
                    *k,
                    ids.iter().filter(|i| suggested.contains(i)).count() as u32,
                )
            })
            .collect(),
    );
    let total = Memo::new(move |_| pick.with(|m| m.values().sum::<u32>() as usize));
    let nonair = Memo::new(move |_| {
        pick.with(|m| {
            m.iter()
                .filter(|(k, _)| !k.is_air())
                .map(|(_, n)| *n as usize)
                .sum::<usize>()
        })
    });
    let rows_btn = rows.clone();
    let ok = move || total.get() == need && nonair.get() >= min_nonair;
    view! {
        <div class="decision">
            <strong>{format!("Choose {need} casualt{} ({hits} hit{}): click units in your row to add one", if need == 1 { "y" } else { "ies" }, if hits == 1 { "" } else { "s" })}</strong>
            {(sub_hits > 0).then(|| view! { <div class="muted">{format!("{sub_hits} hit(s) from submarines cannot be assigned to aircraft.")}</div> })}
            {rows.iter().map(|(k, ids)| {
                let (k, n) = (*k, ids.len() as u32);
                let val = move || pick.with(|m| m.get(&k).copied().unwrap_or(0));
                view! {
                    <div class="urow">
                        <UnitIcon kind=ui_kind(k) />
                        <span class="grow">{format!("{} (of {n})", ui_kind(k).name())}</span>
                        <button class="secondary small" on:click=move |_| pick.update(|m| { let e = m.entry(k).or_default(); *e = e.saturating_sub(1); })>"\u{2212}"</button>
                        <strong class="stepval">{val}</strong>
                        <button class="secondary small" on:click=move |_| pick.update(|m| { let e = m.entry(k).or_default(); *e = (*e + 1).min(n); })>"+"</button>
                    </div>
                }
            }).collect_view()}
            <div class="row">
                <span class="muted">{move || format!("{} / {need} selected", total.get())}</span>
                <span class="spacer"></span>
                <button class="primary" disabled=move || !ok() on:click=move |_| {
                    let m = pick.get_untracked();
                    let units: Vec<UnitId> = rows_btn.iter().flat_map(|(k, ids)| ids.iter().copied().take(m.get(k).copied().unwrap_or(0) as usize)).collect();
                    ctx.send(Action::SelectCasualties { units });
                }>"Remove casualties"</button>
            </div>
        </div>
    }.into_any()
}

/// Decision controls of the current battle step (moved here from the old side-panel board).
fn controls(ctx: GameCtx, st: &GameState, b: &Battle, pick: Pick) -> AnyView {
    let d = pending_decision(st);
    // Online, defenders never answer: the server applies their saved battle preferences.
    if ctx.auto_defense && !ctx.controls(st.current_power) {
        if let Decision::SelectCasualties { power, .. } | Decision::SubWithdrawal { power, .. } = d
        {
            if ctx.controls(power) {
                return view! {
                    <div class="decision muted">"Your casualties are chosen automatically (Settings \u{2192} Battles)."</div>
                }
                .into_any();
            }
        }
    }
    match d {
        Decision::SelectCasualties {
            power,
            hits,
            sub_hits,
            eligible,
            ..
        } if ctx.controls(power) => {
            view! { <CasualtyPicker hits sub_hits eligible pick /> }.into_any()
        }
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
                    <div class="row"><button class="primary" on:click=move |_| ctx.send(Action::PressAttack)>"Press attack"</button></div>
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
                        <button class="primary" on:click=move |_| ctx.send(Action::WithdrawSubs { subs: vec![], to: None })>"Stay and fight"</button>
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

/// Casualty picking through the board: which side's row is clickable and how many per kind.
#[derive(Clone)]
struct Picking {
    side: Side,
    max: BTreeMap<UnitType, u32>,
    pick: Pick,
}

fn cell_view(cell: Cell, show_owner: bool, picking: Option<Picking>) -> impl IntoView {
    let dead = cell.marked == cell.count;
    let has_marked = cell.marked > 0;
    let kind = cell.kind;
    let max = picking
        .as_ref()
        .and_then(|p| p.max.get(&kind).copied())
        .unwrap_or(0);
    let clickable = picking.filter(|_| max > 0);
    let picked = clickable.as_ref().map(|p| p.pick);
    let name = ui_kind(kind).name();
    let is_clickable = clickable.is_some();
    let on_click = move |_| {
        if let Some(p) = &clickable {
            p.pick.update(|m| {
                let e = m.entry(kind).or_default();
                // Wrap around so a mis-click can be undone without the +/- list.
                *e = if *e >= max { 0 } else { *e + 1 };
            });
        }
    };
    view! {
        <div class="bb-cell" class:dead=dead class:marked=has_marked class:pickable=is_clickable
            title=name role=is_clickable.then_some("button") on:click=on_click>
            {show_owner.then(|| view! { <PowerBadge power=cell.owner short=true /> })}
            <UnitIcon kind=ui_kind(kind) power=cell.owner />
            <span class="bb-count">{format!("\u{d7}{}", cell.count)}</span>
            {(cell.marked > 0 && !dead).then(|| view! { <span class="bb-lost">{format!("{} lost", cell.marked)}</span> })}
            {picked.map(|m| view! {
                <Show when=move || m.with(|x| x.get(&kind).copied().unwrap_or(0)) != 0>
                    <span class="bb-pick">{move || format!("\u{2212}{}", m.with(|x| x.get(&kind).copied().unwrap_or(0)))}</span>
                </Show>
            })}
        </div>
    }
}

fn side_row(
    label: &'static str,
    power: Vec<Power>,
    cells: Vec<Vec<Cell>>,
    picking: Option<Picking>,
) -> impl IntoView {
    let multi = power.len() > 1
        || cells
            .iter()
            .flatten()
            .map(|c| c.owner)
            .collect::<std::collections::BTreeSet<_>>()
            .len()
            > 1;
    view! {
        <div class="bb-side">
            <strong>{label}</strong>
            {power.into_iter().map(|p| view! { <PowerBadge power=p short=true /> }).collect_view()}
        </div>
        {cells.into_iter().map(|col| view! {
            <div class="bb-col">
                {col.into_iter().map(|c| cell_view(c, multi, picking.clone())).collect_view()}
            </div>
        }).collect_view()}
    }
}

/// One roll of dice, spread over the board's columns by the number each die needed.
fn dice_group(show: DiceShow, cols: &[u8], fresh: bool) -> impl IntoView {
    let hits = show.dice.iter().filter(|d| d.hit == Some(true)).count();
    let known = show.dice.iter().any(|d| d.hit.is_some());
    let cells = cols
        .iter()
        .map(|&c| {
            let dice: Vec<_> = show
                .dice
                .iter()
                .filter(|d| d.needs.unwrap_or(0) == c)
                .copied()
                .collect();
            view! { <div class="bb-col bb-dicecol">{dice.into_iter().enumerate().map(|(i, d)| view! { <Die face=d index=i animate=fresh /> }).collect_view()}</div> }
        })
        .collect_view();
    view! {
        <div class="bb-dicelabel">{format!("{}: {}", show.power.id().to_uppercase(), show.label)}
            {known.then(|| view! { <strong>{format!(" \u{2013} {hits} hit{}", if hits == 1 { "" } else { "s" })}</strong> })}
        </div>
        <div class="bb-side"></div>
        {cells}
    }
}

type OpenKey = Option<(wartide_engine::LocationId, u32, Option<(Power, u8)>)>;

#[component]
pub fn BattleBoardModal() -> impl IntoView {
    let ctx = expect_context::<GameCtx>();
    let open = use_context::<BoardOpen>()
        .expect("BoardOpen provided by GameScreen")
        .0;
    let pick: Pick = RwSignal::new(BTreeMap::new());
    // Auto-open when a battle begins and whenever a new decision for the viewer arrives.
    let key = Memo::new(move |_| -> OpenKey {
        ctx.state.with(|st| {
            let b = st.battle.as_ref()?;
            let mine = match pending_decision(st) {
                Decision::SelectCasualties { power, .. } if ctx.controls(power) => Some((power, 1)),
                Decision::RetreatOrPress { power, .. } if ctx.controls(power) => Some((power, 2)),
                Decision::SubWithdrawal { power, .. } if ctx.controls(power) => Some((power, 3)),
                _ => None,
            };
            Some((b.location, b.round, mine))
        })
    });
    Effect::new(move |prev: Option<OpenKey>| {
        let now = key.get();
        if let Some(n) = &now {
            let prev = prev.flatten();
            let new_battle = prev.as_ref().is_none_or(|p| p.0 != n.0);
            let new_decision =
                n.2.is_some() && prev.as_ref().is_none_or(|p| p.1 != n.1 || p.2 != n.2);
            if new_battle || new_decision {
                open.set(true);
            }
        }
        now
    });
    view! {
        {move || {
            let st = ctx.state.get();
            let Some(b) = st.battle.clone() else { return ().into_any() };
            if !viewer_sees(ctx, &st, &b) {
                return ().into_any();
            }
            let sea = b.kind == BattleKind::Sea;
            let place = loc_name(b.location);
            if !open.get() {
                let pending = ctx.acting().is_some_and(|p| ctx.controls(p));
                return view! {
                    <div class="bb-mini" role="status">
                        <span>{format!("Battle at {place}, round {}", b.round)}</span>
                        {pending.then(|| view! { <span class="dot" title="Your decision"></span> })}
                        <button class="primary small" on:click=move |_| open.set(true)>"Open"</button>
                    </div>
                }.into_any();
            }
            let rows = board_rows(&st, &b);
            let defs = defender_powers(&st, &b);
            let (on_att, on_def) = hits_pending(&b);
            // Casualty picking through the unit cells.
            let picking = match pending_decision(&st) {
                Decision::SelectCasualties { power, eligible, .. }
                    if ctx.controls(power) && !(ctx.auto_defense && !ctx.controls(st.current_power)) =>
                {
                    let mut max: BTreeMap<UnitType, u32> = BTreeMap::new();
                    for u in eligible.iter().filter_map(|&i| st.unit(i)) {
                        *max.entry(u.kind).or_default() += 1;
                    }
                    let side = b.pending.as_ref().map(|p| p.side).unwrap_or(Side::Attacker);
                    Some(Picking { side, max, pick })
                }
                _ => None,
            };
            let pick_for = |s: Side| picking.clone().filter(|p| p.side == s);
            let ctl = controls(ctx, &st, &b, pick);
            let dice: Vec<DiceShow> = ctx
                .dice
                .get()
                .into_iter()
                .filter(|d| d.location == Some(b.location))
                .collect();
            let n = rows.cols.len();
            let cols = rows.cols.clone();
            let head = rows.cols.iter().map(|&c| view! { <div class="bb-colhead">{c.to_string()}</div> }).collect_view();
            view! {
                <div class="bb-back">
                    <div class="bb-modal battle-modal" role="dialog" aria-modal="true" aria-label="Battle board">
                        <div class="row">
                            <h2 class="grow">{format!("Battle at {place}")}</h2>
                            <button class="secondary small" title="Hide the board to look at the map" on:click=move |_| open.set(false)>"Minimise"</button>
                        </div>
                        <div class="muted">{format!("{}{} \u{b7} round {}", if sea { "Sea battle" } else { "Land battle" }, if b.amphibious { " \u{b7} amphibious assault" } else { "" }, b.round)}</div>
                        <div class="bb-scroll">
                            <div class="bb-grid" style=format!("--n:{n}")>
                                <div class="bb-corner muted">"Hit on"</div>
                                {head}
                                {side_row("Attacker", vec![b.attacker], rows.att.clone(), pick_for(Side::Attacker))}
                                <div class="bb-band">
                                    <div class="bb-step">
                                        <strong>{step_label(b.step)}</strong>
                                        <span class="muted">{format!(" \u{b7} round {}", b.round)}</span>
                                        <span class="bb-hits" title="Hits waiting to be assigned">
                                            {format!("Hits on attacker: {on_att} \u{b7} on defender: {on_def}")}
                                        </span>
                                    </div>
                                    <div class="bb-dicegrid" style=format!("--n:{n}")>
                                        {dice.into_iter().map(|d| { let fresh = ctx.first_view(d.id); dice_group(d, &cols, fresh) }).collect_view()}
                                    </div>
                                </div>
                                {side_row("Defender", defs, rows.def.clone(), pick_for(Side::Defender))}
                            </div>
                        </div>
                        {ctl}
                    </div>
                </div>
            }.into_any()
        }}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wartide_engine::state::PendingHits;
    use wartide_engine::{GameSettings, LocationId, Tech};

    fn battle(att: Vec<UnitId>, def: Vec<UnitId>) -> Battle {
        Battle {
            location: LocationId(0),
            kind: BattleKind::Land,
            attacker: Power::Germany,
            attackers: att,
            defenders: def,
            amphibious: false,
            round: 1,
            step: BattleStep::MainFire,
            pending: None,
            queued: None,
            marked_att: vec![],
            marked_def: vec![],
            bombarded: false,
            assault_zones: vec![],
        }
    }

    fn col(r: &Rows, side: &[Vec<Cell>], c: u8) -> Vec<Cell> {
        let i = r.cols.iter().position(|&x| x == c).expect("column");
        side[i].clone()
    }

    #[test]
    fn columns_follow_attack_and_defense_values() {
        let mut st = GameState::new(GameSettings::default());
        let loc = LocationId(0);
        let inf = st.spawn(UnitType::Infantry, Power::Germany, loc);
        let arm = st.spawn(UnitType::Armor, Power::Germany, loc);
        let inf2 = st.spawn(UnitType::Infantry, Power::Germany, loc);
        let aa = st.spawn(UnitType::AaGun, Power::Ussr, loc);
        let d_inf = st.spawn(UnitType::Infantry, Power::Ussr, loc);
        let mut b = battle(vec![inf, arm, inf2], vec![aa, d_inf]);
        b.marked_att = vec![inf2];
        let r = board_rows(&st, &b);
        assert_eq!(r.cols, vec![1, 2, 3, 4, 0]);
        let a1 = col(&r, &r.att, 1);
        assert_eq!(
            (a1[0].kind, a1[0].count, a1[0].marked),
            (UnitType::Infantry, 2, 1)
        );
        assert_eq!(col(&r, &r.att, 3)[0].kind, UnitType::Armor);
        assert_eq!(col(&r, &r.def, 2)[0].kind, UnitType::Infantry);
        let d0 = col(&r, &r.def, 0);
        assert_eq!(d0.len(), 1);
        assert_eq!(d0[0].kind, UnitType::AaGun);
    }

    #[test]
    fn tech_moves_units_between_columns() {
        let mut st = GameState::new(GameSettings::default());
        let loc = LocationId(0);
        let f = st.spawn(UnitType::Fighter, Power::Ussr, loc);
        let sub = st.spawn(UnitType::Submarine, Power::Germany, loc);
        let b = battle(vec![sub], vec![f]);
        let plain = board_rows(&st, &b);
        assert_eq!(col(&plain, &plain.att, 2)[0].kind, UnitType::Submarine);
        assert_eq!(col(&plain, &plain.def, 4)[0].kind, UnitType::Fighter);
        st.techs[Power::Ussr].push(Tech::JetPower);
        st.techs[Power::Germany].push(Tech::SuperSubmarines);
        let r = board_rows(&st, &b);
        assert_eq!(r.cols, vec![1, 2, 3, 4, 5, 0]);
        assert_eq!(col(&r, &r.def, 5)[0].kind, UnitType::Fighter);
        assert!(col(&r, &r.def, 4).is_empty());
        assert_eq!(col(&r, &r.att, 3)[0].kind, UnitType::Submarine);
    }

    #[test]
    fn allied_defenders_keep_their_owner() {
        let mut st = GameState::new(GameSettings::default());
        let loc = LocationId(0);
        let a = st.spawn(UnitType::Infantry, Power::Germany, loc);
        let d1 = st.spawn(UnitType::Infantry, Power::Ussr, loc);
        let d2 = st.spawn(UnitType::Infantry, Power::Uk, loc);
        let d3 = st.spawn(UnitType::Infantry, Power::Uk, loc);
        let b = battle(vec![a], vec![d1, d2, d3]);
        let r = board_rows(&st, &b);
        let cells = col(&r, &r.def, 2);
        assert_eq!(cells.len(), 2);
        let uk = cells.iter().find(|c| c.owner == Power::Uk).unwrap();
        assert_eq!(uk.count, 2);
        assert_eq!(defender_powers(&st, &b).len(), 2);
    }

    #[test]
    fn pending_hits_sum_per_side() {
        let mut b = battle(vec![], vec![]);
        assert_eq!(hits_pending(&b), (0, 0));
        b.pending = Some(PendingHits {
            side: Side::Defender,
            normal: 2,
            sub: 1,
            immediate: false,
        });
        b.queued = Some(PendingHits {
            side: Side::Attacker,
            normal: 1,
            sub: 0,
            immediate: false,
        });
        assert_eq!(hits_pending(&b), (1, 3));
    }
}

//! The battle state machine: rounds, casualties, retreats, withdrawals.
use crate::battle::{
    capture_after_win, defender_withdraw_options, defending_controller, retreat_options,
    sub_withdraw_options,
};
use crate::decision::Decision;
use crate::dice::{d6, Rng};
use crate::error::{illegal, Result, RuleError};
use crate::event::{BattleOutcome, DiceContext, Event};
use crate::map::LocationId;
use crate::power::Power;
use crate::state::{Battle, BattleKind, BattleStep, GameState, PendingHits, Side};
use crate::tech::{attack_dice, attack_value, defense_value};
use crate::units::{UnitId, UnitRef, UnitType};

fn refs(state: &GameState, ids: &[UnitId]) -> Vec<UnitRef> {
    ids.iter()
        .filter_map(|&i| state.unit(i))
        .map(UnitRef::from)
        .collect()
}

fn alive(state: &GameState, ids: &[UnitId]) -> Vec<UnitId> {
    ids.iter()
        .copied()
        .filter(|&i| state.unit(i).is_some())
        .collect()
}

/// Units of one side that can still be chosen as casualties.
pub fn candidates(state: &GameState, b: &Battle, side: Side) -> Vec<UnitId> {
    let (list, marked) = match side {
        Side::Attacker => (&b.attackers, &b.marked_att),
        Side::Defender => (&b.defenders, &b.marked_def),
    };
    list.iter()
        .copied()
        .filter(|i| state.unit(*i).is_some() && !marked.contains(i))
        .collect()
}

/// Number of casualties required for `normal`/`sub` hits against `cand`.
pub fn required_casualties(
    state: &GameState,
    cand: &[UnitId],
    normal: u32,
    sub: u32,
) -> (usize, usize) {
    let air = cand
        .iter()
        .filter(|&&i| state.unit(i).is_some_and(|u| u.kind.is_air()))
        .count();
    let nonair = cand.len() - air;
    let ms = (sub as usize).min(nonair);
    let cap = (normal as usize).min(nonair - ms + air);
    (ms + cap, ms)
}

/// Who chooses casualties for a side.
pub fn chooser(state: &GameState, b: &Battle, side: Side) -> Power {
    match side {
        Side::Attacker => b.attacker,
        Side::Defender => defending_controller(state, b),
    }
}

/// What input the current battle is waiting for.
pub fn battle_prompt(state: &GameState) -> Option<Decision> {
    let b = state.battle.as_ref()?;
    if let Some(ph) = &b.pending {
        let cand = candidates(state, b, ph.side);
        let eligible = if ph.normal == 0 {
            cand.iter()
                .copied()
                .filter(|&i| state.unit(i).is_some_and(|u| !u.kind.is_air()))
                .collect()
        } else {
            cand
        };
        return Some(Decision::SelectCasualties {
            power: chooser(state, b, ph.side),
            location: b.location,
            hits: ph.normal + ph.sub,
            sub_hits: ph.sub,
            eligible,
        });
    }
    match b.step {
        BattleStep::Choice => Some(Decision::RetreatOrPress {
            power: b.attacker,
            location: b.location,
        }),
        BattleStep::DefenderSubChoice => {
            let subs: Vec<UnitId> = b
                .defenders
                .iter()
                .copied()
                .filter(|&i| state.unit(i).is_some_and(|u| u.kind == UnitType::Submarine))
                .collect();
            Some(Decision::SubWithdrawal {
                power: defending_controller(state, b),
                location: b.location,
                subs,
            })
        }
        _ => None,
    }
}

/// Remove units from play (detaching aircraft from sunk carriers), returning what died.
pub fn kill_units(state: &mut GameState, ids: &[UnitId]) -> Vec<UnitRef> {
    let mut out = vec![];
    for &id in ids {
        let Some(u) = state.unit(id) else { continue };
        out.push(UnitRef::from(u));
        if u.kind == UnitType::Carrier {
            let air: Vec<UnitId> = state
                .cargo_of(id)
                .filter(|c| c.kind.is_air())
                .map(|c| c.id)
                .collect();
            for a in air {
                if let Some(au) = state.unit_mut(a) {
                    au.loaded_on = None;
                }
            }
        }
        state.remove_unit(id);
    }
    out
}

fn remove_now(state: &mut GameState, b: &mut Battle, ids: &[UnitId], ev: &mut Vec<Event>) {
    let r = kill_units(state, ids);
    b.attackers = alive(state, &b.attackers);
    b.defenders = alive(state, &b.defenders);
    if !r.is_empty() {
        ev.push(Event::CasualtiesRemoved {
            location: b.location,
            units: r,
        });
    }
}

/// Cheapest-first valid casualty choice (used for auto-resolution and as an AI fallback).
pub fn default_pick(state: &GameState, cand: &[UnitId], normal: u32, sub: u32) -> Vec<UnitId> {
    let mut sorted: Vec<&crate::units::Unit> = cand.iter().filter_map(|&i| state.unit(i)).collect();
    sorted.sort_by_key(|u| (u.kind.stats().cost, u.id));
    let mut out = vec![];
    let mut sub_left = sub;
    for u in sorted.iter().filter(|u| !u.kind.is_air()) {
        if sub_left == 0 {
            break;
        }
        out.push(u.id);
        sub_left -= 1;
    }
    let mut normal_left = normal;
    for u in &sorted {
        if normal_left == 0 {
            break;
        }
        if out.contains(&u.id) {
            continue;
        }
        out.push(u.id);
        normal_left -= 1;
    }
    out
}

/// Assign the casualties now if the choice is forced; otherwise record a pending selection.
fn request(
    state: &mut GameState,
    b: &mut Battle,
    side: Side,
    normal: u32,
    sub: u32,
    immediate: bool,
    ev: &mut Vec<Event>,
) {
    let cand = candidates(state, b, side);
    let (t, _) = required_casualties(state, &cand, normal, sub);
    if t == 0 {
        return;
    }
    let same = cand
        .iter()
        .filter_map(|&i| state.unit(i))
        .map(|u| (u.kind, u.owner))
        .collect::<std::collections::BTreeSet<_>>()
        .len()
        == 1;
    if t == cand.len() || same {
        let ids = if t == cand.len() {
            cand
        } else {
            default_pick(state, &cand, normal, sub)
        };
        assign(state, b, side, &ids, immediate, ev);
    } else {
        b.pending = Some(PendingHits {
            side,
            normal,
            sub,
            immediate,
        });
    }
}

fn assign(
    state: &mut GameState,
    b: &mut Battle,
    side: Side,
    ids: &[UnitId],
    immediate: bool,
    ev: &mut Vec<Event>,
) {
    if immediate {
        remove_now(state, b, ids, ev);
    } else {
        match side {
            Side::Attacker => b.marked_att.extend_from_slice(ids),
            Side::Defender => b.marked_def.extend_from_slice(ids),
        }
    }
}

fn after_selection(state: &mut GameState, b: &mut Battle, ev: &mut Vec<Event>) {
    b.pending = None;
    if let Some(q) = b.queued.take() {
        request(state, b, q.side, q.normal, q.sub, q.immediate, ev);
    }
}

fn can_hit(state: &GameState, shooters: &[UnitId], targets: &[UnitId], attacking: bool) -> bool {
    let any_nonair = targets
        .iter()
        .filter_map(|&i| state.unit(i))
        .any(|u| !u.kind.is_air());
    let any = targets.iter().any(|&i| state.unit(i).is_some());
    shooters.iter().filter_map(|&i| state.unit(i)).any(|u| {
        let dice = if attacking {
            attack_dice(state, u)
        } else {
            u32::from(defense_value(state, u) > 0)
        };
        dice > 0
            && if u.kind == UnitType::Submarine {
                any_nonair
            } else {
                any
            }
    })
}

fn roll_hits(rng: &mut Rng, n: u32, target: u8, rolls: &mut Vec<u8>) -> u32 {
    let mut h = 0;
    for _ in 0..n {
        let r = d6(rng);
        rolls.push(r);
        if r <= target {
            h += 1;
        }
    }
    h
}

fn bombard(state: &mut GameState, b: &mut Battle, rng: &mut Rng, ev: &mut Vec<Event>) {
    let ships: Vec<UnitId> = state
        .units
        .iter()
        .filter(|u| {
            u.owner == b.attacker
                && u.kind == UnitType::Battleship
                && b.assault_zones.contains(&u.location)
                && !u.fought
        })
        .map(|u| u.id)
        .collect();
    if ships.is_empty() {
        return;
    }
    let mut rolls = vec![];
    let hits = roll_hits(rng, ships.len() as u32, 4, &mut rolls);
    for s in ships {
        state.unit_mut(s).unwrap().fought = true;
    }
    ev.push(Event::DiceRolled {
        power: b.attacker,
        context: DiceContext::Bombardment {
            location: b.location,
        },
        rolls,
    });
    ev.push(Event::Bombardment {
        location: b.location,
        hits,
    });
    if hits > 0 {
        request(state, b, Side::Defender, hits, 0, false, ev);
    }
}

fn first_strike(state: &mut GameState, b: &mut Battle, rng: &mut Rng, ev: &mut Vec<Event>) {
    let subs: Vec<UnitId> = b
        .attackers
        .iter()
        .copied()
        .filter(|&i| state.unit(i).is_some_and(|u| u.kind == UnitType::Submarine))
        .collect();
    if subs.is_empty() || b.kind != BattleKind::Sea {
        return;
    }
    let mut rolls = vec![];
    let mut hits = 0;
    for s in subs {
        let u = state.unit(s).unwrap();
        hits += roll_hits(rng, 1, attack_value(state, u), &mut rolls);
    }
    ev.push(Event::DiceRolled {
        power: b.attacker,
        context: DiceContext::FirstStrike {
            location: b.location,
            round: b.round,
        },
        rolls,
    });
    ev.push(Event::HitsScored {
        location: b.location,
        by: b.attacker,
        hits,
    });
    if hits > 0 {
        request(state, b, Side::Defender, 0, hits, true, ev);
    }
}

fn main_fire(state: &mut GameState, b: &mut Battle, rng: &mut Rng, ev: &mut Vec<Event>) {
    let mut rolls = vec![];
    let mut a_hits = 0;
    for &id in &b.attackers {
        let Some(u) = state.unit(id) else { continue };
        if u.kind == UnitType::Submarine && b.kind == BattleKind::Sea {
            continue;
        }
        a_hits += roll_hits(
            rng,
            attack_dice(state, u),
            attack_value(state, u),
            &mut rolls,
        );
    }
    if !rolls.is_empty() {
        ev.push(Event::DiceRolled {
            power: b.attacker,
            context: DiceContext::Attack {
                location: b.location,
                round: b.round,
            },
            rolls,
        });
    }
    ev.push(Event::HitsScored {
        location: b.location,
        by: b.attacker,
        hits: a_hits,
    });
    let (mut d_norm, mut d_sub) = (0, 0);
    for pw in Power::ALL {
        let mut rolls = vec![];
        for &id in &b.defenders {
            let Some(u) = state.unit(id) else { continue };
            if u.owner != pw {
                continue;
            }
            let dv = defense_value(state, u);
            if dv == 0 {
                continue;
            }
            let h = roll_hits(rng, 1, dv, &mut rolls);
            if u.kind == UnitType::Submarine {
                d_sub += h;
            } else {
                d_norm += h;
            }
        }
        if !rolls.is_empty() {
            ev.push(Event::DiceRolled {
                power: pw,
                context: DiceContext::Defense {
                    location: b.location,
                    round: b.round,
                },
                rolls,
            });
        }
    }
    ev.push(Event::HitsScored {
        location: b.location,
        by: defending_controller(state, b),
        hits: d_norm + d_sub,
    });
    if a_hits > 0 {
        request(state, b, Side::Defender, a_hits, 0, false, ev);
    }
    if d_norm + d_sub > 0 {
        let q = PendingHits {
            side: Side::Attacker,
            normal: d_norm,
            sub: d_sub,
            immediate: false,
        };
        if b.pending.is_some() {
            b.queued = Some(q);
        } else {
            request(state, b, Side::Attacker, d_norm, d_sub, false, ev);
        }
    }
}

fn resolve(state: &mut GameState, b: &mut Battle, ev: &mut Vec<Event>) -> Option<BattleOutcome> {
    let mut dead = std::mem::take(&mut b.marked_def);
    dead.extend(std::mem::take(&mut b.marked_att));
    remove_now(state, b, &dead, ev);
    if b.attackers.is_empty() {
        return Some(BattleOutcome::DefenderWins);
    }
    if b.defenders.is_empty() {
        return Some(BattleOutcome::AttackerWins);
    }
    if !can_hit(state, &b.attackers, &b.defenders, true)
        && !can_hit(state, &b.defenders, &b.attackers, false)
    {
        return Some(BattleOutcome::Stalemate);
    }
    if b.amphibious {
        b.round += 1;
        b.step = BattleStep::RoundStart;
    } else {
        b.step = BattleStep::Choice;
    }
    None
}

/// Run the battle forward until input is needed or it ends.
pub fn advance(state: &mut GameState, rng: &mut Rng, ev: &mut Vec<Event>) {
    let Some(mut b) = state.battle.take() else {
        return;
    };
    for _ in 0..10_000 {
        if b.pending.is_some() {
            break;
        }
        match b.step {
            BattleStep::RoundStart => {
                ev.push(Event::BattleRound {
                    location: b.location,
                    round: b.round,
                });
                if b.round == 1 && !b.bombarded && b.kind == BattleKind::Land && b.amphibious {
                    bombard(state, &mut b, rng, ev);
                }
                b.bombarded = true;
                b.step = BattleStep::FirstStrike;
            }
            BattleStep::FirstStrike => {
                first_strike(state, &mut b, rng, ev);
                b.step = BattleStep::MainFire;
            }
            BattleStep::MainFire => {
                main_fire(state, &mut b, rng, ev);
                b.step = BattleStep::Resolve;
            }
            BattleStep::Resolve => {
                if let Some(o) = resolve(state, &mut b, ev) {
                    state.battle = Some(b);
                    finish_battle(state, o, ev);
                    return;
                }
            }
            BattleStep::Choice | BattleStep::DefenderSubChoice => break,
        }
    }
    state.battle = Some(b);
}

/// Conclude the battle in progress.
pub fn finish_battle(state: &mut GameState, outcome: BattleOutcome, ev: &mut Vec<Event>) {
    let Some(b) = state.battle.take() else { return };
    if b.kind == BattleKind::Land && outcome == BattleOutcome::AttackerWins {
        capture_after_win(state, &b, ev);
    }
    for id in &b.attackers {
        if let Some(u) = state.unit_mut(*id) {
            u.fought = true;
            u.combat_from = None;
        }
    }
    state.turn.resolved.push(b.location);
    if b.kind == BattleKind::Sea {
        rehome_orphans(state, b.location, b.attacker, ev);
    }
    ev.push(Event::BattleEnded {
        location: b.location,
        outcome,
    });
}

/// Defending fighters whose carrier sank land on another carrier in the zone or are lost.
fn rehome_orphans(state: &mut GameState, zone: LocationId, active: Power, ev: &mut Vec<Event>) {
    let orphans: Vec<UnitId> = state
        .units_at(zone)
        .filter(|u| {
            u.kind == UnitType::Fighter && u.loaded_on.is_none() && !u.owner.is_ally(active)
        })
        .map(|u| u.id)
        .collect();
    let mut lost = vec![];
    for id in orphans {
        let owner = state.unit(id).unwrap().owner;
        let carrier = state
            .units_at(zone)
            .filter(|c| {
                c.kind == UnitType::Carrier
                    && c.owner.is_ally(owner)
                    && state.carrier_free_slots(c.id) > 0
            })
            .map(|c| c.id)
            .next();
        match carrier {
            Some(c) => state.unit_mut(id).unwrap().loaded_on = Some(c),
            None => lost.extend(kill_units(state, &[id])),
        }
    }
    if !lost.is_empty() {
        let power = lost[0].owner;
        ev.push(Event::UnitsLost {
            power,
            units: lost,
            reason: "carrier sunk; fighters could not land".into(),
        });
    }
}

fn expect_battle(state: &GameState, actor: Power, step: BattleStep) -> Result<&Battle> {
    let b = state
        .battle
        .as_ref()
        .ok_or_else(|| RuleError::illegal("no battle in progress"))?;
    if b.pending.is_some() || b.step != step {
        return illegal("the battle is not waiting for that decision");
    }
    let who = if step == BattleStep::Choice {
        b.attacker
    } else {
        defending_controller(state, b)
    };
    if who != actor {
        return Err(RuleError::NotYourTurn {
            actor,
            expected: who,
        });
    }
    Ok(b)
}

/// Handle `SelectCasualties`.
pub fn select_casualties(
    state: &mut GameState,
    actor: Power,
    ids: &[UnitId],
    rng: &mut Rng,
    ev: &mut Vec<Event>,
) -> Result<()> {
    let b = state
        .battle
        .as_ref()
        .ok_or_else(|| RuleError::illegal("no battle in progress"))?;
    let ph = b
        .pending
        .clone()
        .ok_or_else(|| RuleError::illegal("no casualties to select"))?;
    let who = chooser(state, b, ph.side);
    if who != actor {
        return Err(RuleError::NotYourTurn {
            actor,
            expected: who,
        });
    }
    let cand = candidates(state, b, ph.side);
    let (t, ms) = required_casualties(state, &cand, ph.normal, ph.sub);
    let mut seen = ids.to_vec();
    seen.sort_unstable();
    seen.dedup();
    if seen.len() != ids.len() || ids.len() != t {
        return illegal(format!("you must choose exactly {t} distinct casualties"));
    }
    if ids.iter().any(|i| !cand.contains(i)) {
        return illegal("a chosen unit is not eligible");
    }
    let air_chosen = ids
        .iter()
        .filter(|&&i| state.unit(i).is_some_and(|u| u.kind.is_air()))
        .count();
    if air_chosen > (ph.normal as usize).min(t - ms) {
        return illegal("submarine hits cannot be assigned to aircraft");
    }
    let mut b = state.battle.take().unwrap();
    assign(state, &mut b, ph.side, ids, ph.immediate, ev);
    after_selection(state, &mut b, ev);
    state.battle = Some(b);
    advance(state, rng, ev);
    Ok(())
}

/// Handle `PressAttack`.
pub fn press(
    state: &mut GameState,
    actor: Power,
    rng: &mut Rng,
    ev: &mut Vec<Event>,
) -> Result<()> {
    expect_battle(state, actor, BattleStep::Choice)?;
    let mut b = state.battle.take().unwrap();
    let def_subs = b.kind == BattleKind::Sea
        && b.defenders
            .iter()
            .any(|&i| state.unit(i).is_some_and(|u| u.kind == UnitType::Submarine));
    if def_subs {
        b.step = BattleStep::DefenderSubChoice;
    } else {
        b.round += 1;
        b.step = BattleStep::RoundStart;
    }
    state.battle = Some(b);
    advance(state, rng, ev);
    Ok(())
}

/// Handle `Retreat`.
pub fn retreat(
    state: &mut GameState,
    actor: Power,
    to: LocationId,
    ev: &mut Vec<Event>,
) -> Result<()> {
    let b = expect_battle(state, actor, BattleStep::Choice)?;
    if b.amphibious {
        return illegal("no retreat from an amphibious assault");
    }
    let opts = retreat_options(state);
    if !opts.contains(&to) {
        return illegal("illegal retreat destination");
    }
    let b = state.battle.as_ref().unwrap();
    let from = b.location;
    let movers: Vec<UnitId> = b
        .attackers
        .iter()
        .copied()
        .filter(|&i| state.unit(i).is_some_and(|u| !u.kind.is_air()))
        .collect();
    let r = refs(state, &movers);
    if to != from {
        for &id in &movers {
            let cargo: Vec<UnitId> = state.cargo_of(id).map(|c| c.id).collect();
            state.unit_mut(id).unwrap().location = to;
            for c in cargo {
                state.unit_mut(c).unwrap().location = to;
            }
        }
        ev.push(Event::Retreated { units: r, from, to });
    }
    finish_battle(state, BattleOutcome::Retreated, ev);
    Ok(())
}

/// Handle `WithdrawSubs` (attacker at the Choice step, defender at the DefenderSubChoice step).
pub fn withdraw_subs(
    state: &mut GameState,
    actor: Power,
    subs: &[UnitId],
    to: Option<LocationId>,
    rng: &mut Rng,
    ev: &mut Vec<Event>,
) -> Result<()> {
    let b = state
        .battle
        .as_ref()
        .ok_or_else(|| RuleError::illegal("no battle in progress"))?;
    if b.pending.is_some() {
        return illegal("assign casualties first");
    }
    match b.step {
        BattleStep::Choice => {
            expect_battle(state, actor, BattleStep::Choice)?;
            if b.kind != BattleKind::Sea {
                return illegal("only sea battles allow withdrawal");
            }
            let to = to.ok_or_else(|| RuleError::illegal("destination required"))?;
            if subs.is_empty()
                || subs.iter().any(|i| {
                    !b.attackers.contains(i)
                        || state.unit(*i).is_none_or(|u| u.kind != UnitType::Submarine)
                })
            {
                return illegal("choose attacking submarines");
            }
            if !sub_withdraw_options(state).contains(&to) {
                return illegal("illegal withdrawal destination");
            }
            let from = b.location;
            let r = refs(state, subs);
            for &s in subs {
                state.unit_mut(s).unwrap().location = to;
                state.unit_mut(s).unwrap().fought = true;
                state.unit_mut(s).unwrap().combat_from = None;
            }
            let b = state.battle.as_mut().unwrap();
            b.attackers.retain(|i| !subs.contains(i));
            let empty = b.attackers.is_empty();
            ev.push(Event::SubsWithdrew { units: r, from, to });
            if empty {
                finish_battle(state, BattleOutcome::Withdrawn, ev);
            }
            Ok(())
        }
        BattleStep::DefenderSubChoice => {
            expect_battle(state, actor, BattleStep::DefenderSubChoice)?;
            let from = b.location;
            let defender = actor;
            if !subs.is_empty() {
                let to = to.ok_or_else(|| RuleError::illegal("destination required"))?;
                if subs.iter().any(|i| {
                    !b.defenders.contains(i)
                        || state.unit(*i).is_none_or(|u| u.kind != UnitType::Submarine)
                }) {
                    return illegal("choose defending submarines");
                }
                if !defender_withdraw_options(state, defender).contains(&to) {
                    return illegal("illegal withdrawal destination");
                }
                let r = refs(state, subs);
                for &s in subs {
                    state.unit_mut(s).unwrap().location = to;
                }
                state
                    .battle
                    .as_mut()
                    .unwrap()
                    .defenders
                    .retain(|i| !subs.contains(i));
                ev.push(Event::SubsWithdrew { units: r, from, to });
            }
            let b = state.battle.as_mut().unwrap();
            if b.defenders.is_empty() {
                finish_battle(state, BattleOutcome::AttackerWins, ev);
            } else {
                b.round += 1;
                b.step = BattleStep::RoundStart;
                advance(state, rng, ev);
            }
            Ok(())
        }
        _ => illegal("the battle is not waiting for a withdrawal decision"),
    }
}

/// Answer every prompt with the default policy until the battle ends (used by the odds simulator).
pub fn auto_resolve(state: &mut GameState, rng: &mut Rng, ev: &mut Vec<Event>) {
    for _ in 0..2000 {
        let Some(d) = battle_prompt(state) else {
            if state.battle.is_some() {
                advance(state, rng, ev);
                continue;
            }
            return;
        };
        let r = match d {
            Decision::SelectCasualties {
                power,
                hits,
                sub_hits,
                location: _,
                eligible: _,
            } => {
                let b = state.battle.as_ref().unwrap();
                let side = state
                    .battle
                    .as_ref()
                    .unwrap()
                    .pending
                    .as_ref()
                    .map(|p| p.side)
                    .unwrap_or(Side::Attacker);
                let cand = candidates(state, b, side);
                let ids = default_pick(state, &cand, hits - sub_hits, sub_hits);
                select_casualties(state, power, &ids, rng, ev)
            }
            Decision::RetreatOrPress { power, .. } => press(state, power, rng, ev),
            Decision::SubWithdrawal { power, .. } => {
                withdraw_subs(state, power, &[], None, rng, ev)
            }
            _ => return,
        };
        if r.is_err() {
            return;
        }
    }
}

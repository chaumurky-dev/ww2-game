//! Attacker/defender decisions between rounds: press, retreat, submarine withdrawal.
use crate::battle::{
    defender_withdraw_options, defending_controller, retreat_options, sub_withdraw_options,
};
use crate::casualties::refs;
use crate::combat::{advance, finish_battle};
use crate::dice::Rng;
use crate::error::{illegal, Result, RuleError};
use crate::event::{BattleOutcome, Event};
use crate::map::LocationId;
use crate::power::Power;
use crate::state::{Battle, BattleKind, BattleStep, GameState};
use crate::units::{UnitId, UnitType};

pub(crate) fn expect_battle(state: &GameState, actor: Power, step: BattleStep) -> Result<&Battle> {
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

/// Handle `PressAttack`.
pub fn press(
    state: &mut GameState,
    actor: Power,
    rng: &mut Rng,
    ev: &mut Vec<Event>,
) -> Result<()> {
    expect_battle(state, actor, BattleStep::Choice)?;
    let def_subs = {
        let b = state.battle.as_ref().unwrap();
        b.kind == BattleKind::Sea
            && b.defenders
                .iter()
                .any(|&i| state.unit(i).is_some_and(|u| u.kind == UnitType::Submarine))
            && !defender_withdraw_options(state, defending_controller(state, b)).is_empty()
    };
    let mut b = state.battle.take().unwrap();
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
                let cu = state.unit_mut(c).unwrap();
                cu.location = to;
                cu.assault_to = None;
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
            let mut uniq = subs.to_vec();
            uniq.sort_unstable();
            uniq.dedup();
            if subs.is_empty()
                || uniq.len() != subs.len()
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
                let mut uniq = subs.to_vec();
                uniq.sort_unstable();
                uniq.dedup();
                if uniq.len() != subs.len()
                    || subs.iter().any(|i| {
                        !b.defenders.contains(i)
                            || state.unit(*i).is_none_or(|u| u.kind != UnitType::Submarine)
                    })
                {
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

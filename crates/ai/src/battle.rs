//! Decisions made while battles are running: combat-phase flow, casualties, retreats, withdrawals.
use crate::eval::{loss_key, mix, piece, sim, Piece};
use crate::Style;
use wartide_engine::battle::{beginnable_battles, defender_withdraw_options, pending_battles};
use wartide_engine::bombing::{bombing_targets, rocket_targets};
use wartide_engine::casualties::{candidates, required_casualties};
use wartide_engine::query::retreat_destinations;
use wartide_engine::state::{BattleKind, Side};
use wartide_engine::{Action, GameState, MapData, Power, UnitId, UnitType};

/// What to do in the Combat phase between battles.
pub fn combat_phase_action(state: &GameState, me: Power, rejected: &[Action]) -> Action {
    let pending = pending_battles(state);
    for (loc, bombers) in bombing_targets(state, me) {
        let a = Action::DeclareStrategicBombing {
            location: loc,
            bombers,
        };
        if !pending.contains(&loc) && !rejected.contains(&a) {
            return a;
        }
    }
    if !state.turn.rocket_fired {
        for g in state
            .units
            .iter()
            .filter(|u| u.owner == me && u.kind == UnitType::AaGun)
        {
            if let Some(&t) = rocket_targets(state, g.id).first() {
                let a = Action::FireRocket {
                    aa_gun: g.id,
                    target: t,
                };
                if !rejected.contains(&a) {
                    return a;
                }
            }
        }
    }
    for l in beginnable_battles(state) {
        let a = Action::BeginBattle { location: l };
        if !rejected.contains(&a) {
            return a;
        }
    }
    Action::EndPhase
}

fn pieces(state: &GameState, ids: &[UnitId]) -> Vec<Piece> {
    ids.iter()
        .filter_map(|&i| state.unit(i))
        .filter(|u| u.kind != UnitType::AaGun && u.kind != UnitType::IndustrialComplex)
        .map(|u| piece(state, u))
        .collect()
}

/// Casualties to take: cheapest first, never the last land unit of a land assault when avoidable.
/// Returns the action and the kinds lost (for chat).
pub fn choose_casualties(
    state: &GameState,
    hits: u32,
    sub_hits: u32,
    eligible: &[UnitId],
) -> Option<(Action, Vec<UnitType>)> {
    let b = state.battle.as_ref()?;
    let side = b.pending.as_ref().map_or(Side::Attacker, |p| p.side);
    let cand = candidates(state, b, side);
    let normal = hits.saturating_sub(sub_hits);
    let (t, ms) = required_casualties(state, &cand, normal, sub_hits);
    let mut sorted: Vec<&wartide_engine::Unit> =
        cand.iter().filter_map(|&i| state.unit(i)).collect();
    sorted.sort_by_key(|u| (loss_key(state, u), u.id));
    let mut chosen: Vec<UnitId> = vec![];
    for u in sorted.iter().filter(|u| !u.kind.is_air()).take(ms) {
        chosen.push(u.id);
    }
    for u in &sorted {
        if chosen.len() >= t {
            break;
        }
        if !chosen.contains(&u.id) {
            chosen.push(u.id);
        }
    }
    // Keep a land unit alive to take the territory.
    if side == Side::Attacker && b.kind == BattleKind::Land {
        let land_left = |ch: &[UnitId]| {
            cand.iter().any(|i| {
                !ch.contains(i)
                    && state
                        .unit(*i)
                        .is_some_and(|u| matches!(u.kind, UnitType::Infantry | UnitType::Armor))
            })
        };
        if !land_left(&chosen) {
            let forced: Vec<UnitId> = chosen.iter().take(ms).copied().collect();
            let swap_out = chosen.iter().rev().copied().find(|i| {
                !forced.contains(i)
                    && state
                        .unit(*i)
                        .is_some_and(|u| matches!(u.kind, UnitType::Infantry | UnitType::Armor))
            });
            let swap_in = sorted
                .iter()
                .find(|u| u.kind.is_air() && !chosen.contains(&u.id))
                .map(|u| u.id);
            if let (Some(o), Some(i)) = (swap_out, swap_in) {
                chosen.retain(|&c| c != o);
                chosen.push(i);
            }
        }
    }
    let _ = eligible;
    let kinds = chosen
        .iter()
        .filter_map(|&i| state.unit(i))
        .map(|u| u.kind)
        .collect();
    Some((Action::SelectCasualties { units: chosen }, kinds))
}

/// Retreat, press on, or withdraw submarines.
pub fn retreat_or_press(state: &GameState, style: Style, seed: u64) -> Action {
    let Some(b) = state.battle.as_ref() else {
        return Action::PressAttack;
    };
    let att = pieces(state, &b.attackers);
    let def = pieces(state, &b.defenders);
    let land = b.kind == BattleKind::Land;
    let o = sim(
        &att,
        &def,
        false,
        land,
        150,
        mix(&[seed, u64::from(b.round)]),
    );
    let capital = MapData::get().capital_of(b.location).is_some();
    let floor = match style {
        Style::Aggressive => 0.28,
        Style::Balanced => 0.38,
        Style::Cautious => 0.5,
    } - if capital { 0.1 } else { 0.0 };
    if o.win < floor {
        let dests = retreat_destinations(state);
        if !b.amphibious {
            if let Some(&to) = dests.first() {
                return Action::Retreat { to };
            }
        }
        let subs: Vec<UnitId> = b
            .attackers
            .iter()
            .copied()
            .filter(|&i| state.unit(i).is_some_and(|u| u.kind == UnitType::Submarine))
            .collect();
        let opts = wartide_engine::battle::sub_withdraw_options(state);
        if !subs.is_empty() && b.kind == BattleKind::Sea {
            if let Some(&to) = opts.first() {
                return Action::WithdrawSubs { subs, to: Some(to) };
            }
        }
    }
    Action::PressAttack
}

/// Defender's decision whether to pull submarines out.
pub fn sub_withdrawal(state: &GameState, me: Power, subs: &[UnitId], seed: u64) -> Action {
    let stay = Action::WithdrawSubs {
        subs: vec![],
        to: None,
    };
    let Some(b) = state.battle.as_ref() else {
        return stay;
    };
    let att = pieces(state, &b.attackers);
    let def = pieces(state, &b.defenders);
    let o = sim(&att, &def, false, false, 120, mix(&[seed, 91]));
    let opts = defender_withdraw_options(state, me);
    match opts.first() {
        Some(&to) if o.win > 0.55 && !subs.is_empty() => Action::WithdrawSubs {
            subs: subs.to_vec(),
            to: Some(to),
        },
        _ => stay,
    }
}

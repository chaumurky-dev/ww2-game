//! The battle state machine: rounds, dice, resolution.
use crate::battle::{capture_after_win, defending_controller};
use crate::casualties::{chooser, kill_units, remove_now, request};
use crate::decision::Decision;
use crate::dice::{d6, Rng};
use crate::event::{BattleOutcome, DiceContext, DieDetail, Event};
use crate::map::LocationId;
use crate::power::Power;
use crate::state::{Battle, BattleKind, BattleStep, GameState, PendingHits, Side};
use crate::tech::{attack_dice, attack_value, defense_value};
use crate::units::{UnitId, UnitType};

pub use crate::casualties::{candidates, default_pick, required_casualties, select_casualties};
pub use crate::choices::{press, retreat, withdraw_subs};

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

/// Who rolled the dice being recorded.
struct Roller {
    unit: UnitId,
    kind: UnitType,
    owner: Power,
}

fn roll_hits(
    rng: &mut Rng,
    n: u32,
    target: u8,
    who: &Roller,
    rolls: &mut Vec<u8>,
    details: &mut Vec<DieDetail>,
) -> u32 {
    let mut h = 0;
    for _ in 0..n {
        let r = d6(rng);
        rolls.push(r);
        let hit = r <= target;
        details.push(DieDetail {
            unit: Some(who.unit),
            unit_type: Some(who.kind),
            owner: who.owner,
            hit_on: target,
            roll: r,
            hit,
        });
        if hit {
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
    let mut details = vec![];
    let mut hits = 0;
    for &s in &ships {
        let who = Roller {
            unit: s,
            kind: UnitType::Battleship,
            owner: b.attacker,
        };
        hits += roll_hits(rng, 1, 4, &who, &mut rolls, &mut details);
    }
    for s in ships {
        state.unit_mut(s).unwrap().fought = true;
    }
    ev.push(Event::DiceRolled {
        power: b.attacker,
        context: DiceContext::Bombardment {
            location: b.location,
        },
        rolls,
        details,
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
    let mut details = vec![];
    let mut hits = 0;
    for s in subs {
        let u = state.unit(s).unwrap();
        let who = Roller {
            unit: s,
            kind: u.kind,
            owner: u.owner,
        };
        hits += roll_hits(
            rng,
            1,
            attack_value(state, u),
            &who,
            &mut rolls,
            &mut details,
        );
    }
    ev.push(Event::DiceRolled {
        power: b.attacker,
        context: DiceContext::FirstStrike {
            location: b.location,
            round: b.round,
        },
        rolls,
        details,
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
    let mut details = vec![];
    let mut a_hits = 0;
    for &id in &b.attackers {
        let Some(u) = state.unit(id) else { continue };
        if u.kind == UnitType::Submarine && b.kind == BattleKind::Sea {
            continue;
        }
        let who = Roller {
            unit: id,
            kind: u.kind,
            owner: u.owner,
        };
        a_hits += roll_hits(
            rng,
            attack_dice(state, u),
            attack_value(state, u),
            &who,
            &mut rolls,
            &mut details,
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
            details,
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
        let mut details = vec![];
        for &id in &b.defenders {
            let Some(u) = state.unit(id) else { continue };
            if u.owner != pw {
                continue;
            }
            let dv = defense_value(state, u);
            if dv == 0 {
                continue;
            }
            let who = Roller {
                unit: id,
                kind: u.kind,
                owner: u.owner,
            };
            let h = roll_hits(rng, 1, dv, &who, &mut rolls, &mut details);
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
                details,
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
        // RULES 11: otherwise a friendly island in the zone (an island territory touching it).
        let map = crate::map::MapData::get();
        let island = map.neighbours(zone).iter().copied().find(|&t| {
            !map.is_sea(t)
                && map.loc(t).is_island
                && state.is_friendly_land(owner, t)
                && !state.enemy_occupied_land(owner, t)
        });
        match (carrier, island) {
            (Some(c), _) => state.unit_mut(id).unwrap().loaded_on = Some(c),
            (None, Some(t)) => state.unit_mut(id).unwrap().location = t,
            (None, None) => lost.extend(kill_units(state, &[id])),
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

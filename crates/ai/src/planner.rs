//! The turn planner: caches one plan per (turn, phase), re-validates each planned action against
//! the live state, remembers rejected actions and falls back to the random policy when stuck.
use crate::battle::{choose_casualties, combat_phase_action, retreat_or_press, sub_withdrawal};
use crate::combat::{plan_combat_move, AttackNote};
use crate::commentary::{Chatter, Kind, Voice};
use crate::economy::{plan_purchase, plan_research, BuyNote};
use crate::eval::{mix, Intel};
use crate::movement::plan_noncombat;
use crate::placement::next_place;
use crate::{Bot, Commentary};
use std::collections::VecDeque;
use wartide_engine::casualties::default_pick;
use wartide_engine::movement::validate_move;
use wartide_engine::transport::{validate_load, validate_unload};
use wartide_engine::{pending_decision, Action, Decision, GameState, Phase};

/// Everything the bot remembers between calls (all of it is optional: the bot also works if it
/// is recreated for every call).
#[derive(Default)]
pub struct Memory {
    key: Option<(u32, Phase)>,
    built: bool,
    count: u32,
    queue: VecDeque<Action>,
    intel: Option<Intel>,
    buy: Option<BuyNote>,
    notes: Vec<AttackNote>,
    hold: (Option<wartide_engine::LocationId>, u32),
    last: Option<(Action, u64)>,
    rejected: Vec<Action>,
    total_rejected: u32,
    chatter: Chatter,
}

/// Cheap fingerprint of everything an accepted action could change.
fn fingerprint(s: &GameState) -> u64 {
    let mut h = mix(&[
        u64::from(s.turn.number),
        s.phase as u64,
        s.current_power.index() as u64,
        s.treasury.0.iter().map(|&t| u64::from(t)).sum(),
        s.units.len() as u64,
        s.purchases
            .iter()
            .map(|p| u64::from(p.count) + 100 * p.unit as u64)
            .sum(),
        s.turn.resolved.len() as u64,
        u64::from(s.turn.rocket_fired),
        u64::from(s.turn.researched),
        s.turn.placed.iter().map(|p| u64::from(p.1)).sum(),
    ]);
    for u in &s.units {
        let bits = u64::from(u.moves_used)
            | u64::from(u.moved_cm) << 8
            | u64::from(u.fought) << 9
            | u64::from(u.bombing) << 10
            | u64::from(u.frozen) << 11
            | u64::from(u.combat_from.is_some()) << 12
            | u64::from(u.assault_to.is_some()) << 13;
        h = mix(&[
            h,
            u64::from(u.id),
            u64::from(u.location.0),
            u64::from(u.loaded_on.unwrap_or(0)),
            bits,
        ]);
    }
    if let Some(b) = &s.battle {
        h = mix(&[
            h,
            u64::from(b.round),
            b.step as u64,
            b.attackers.len() as u64,
            b.defenders.len() as u64,
            b.pending
                .as_ref()
                .map_or(0, |p| u64::from(p.normal + p.sub) + 1),
        ]);
    }
    h
}

fn still_valid(state: &GameState, me: wartide_engine::Power, a: &Action) -> bool {
    match a {
        Action::Move { units, path } => validate_move(state, me, units, path).is_ok(),
        Action::Load { units, transport } => validate_load(state, me, units, *transport).is_ok(),
        Action::Unload {
            transport,
            units,
            to,
        } => validate_unload(state, me, *transport, units, *to).is_ok(),
        _ => true,
    }
}

fn phase_cap(p: Phase) -> u32 {
    match p {
        Phase::CombatMove | Phase::NonCombatMove => 150,
        Phase::Combat => 120,
        Phase::Mobilize => 120,
        _ => 6,
    }
}

impl Bot {
    pub(crate) fn decide(&mut self, state: &GameState) -> Option<(Action, Vec<Commentary>)> {
        let decision = pending_decision(state);
        let me = self.power;
        let mine = match &decision {
            Decision::GameOver { .. } => false,
            Decision::SelectCasualties { power, .. }
            | Decision::RetreatOrPress { power, .. }
            | Decision::SubWithdrawal { power, .. }
            | Decision::Phase { power, .. } => *power == me,
        };
        if !mine {
            return None;
        }
        let fp = fingerprint(state);
        if let Some((a, f)) = self.mem.last.take() {
            if f == fp {
                self.mem.rejected.push(a);
                self.mem.total_rejected += 1;
            } else {
                self.mem.rejected.clear();
            }
        }
        let seed = mix(&[
            self.seed,
            u64::from(state.turn.number),
            self.mem.count.into(),
        ]);
        let (action, lines, kind) = if self.mem.rejected.len() >= 3 {
            (self.fallback_action(state, &decision), vec![], Kind::Plan)
        } else {
            match &decision {
                Decision::SelectCasualties {
                    hits,
                    sub_hits,
                    eligible,
                    location,
                    ..
                } => {
                    let r = choose_casualties(state, *hits, *sub_hits, eligible);
                    match r {
                        Some((a, kinds)) if !self.mem.rejected.contains(&a) => {
                            let v = self.voice();
                            let l = v.loss(state, &kinds, *location).into_iter().collect();
                            (a, l, Kind::Reaction)
                        }
                        _ => (self.fallback_action(state, &decision), vec![], Kind::Plan),
                    }
                }
                Decision::RetreatOrPress { location, .. } => {
                    let a = retreat_or_press(state, self.persona.style, seed);
                    let a = if self.mem.rejected.contains(&a) {
                        Action::PressAttack
                    } else {
                        a
                    };
                    let l = if matches!(a, Action::Retreat { .. }) {
                        vec![self.voice().retreat(state, *location)]
                    } else {
                        vec![]
                    };
                    (a, l, Kind::Reaction)
                }
                Decision::SubWithdrawal { subs, .. } => {
                    (sub_withdrawal(state, me, subs, seed), vec![], Kind::Plan)
                }
                Decision::Phase { phase, .. } => self.phase_action(state, *phase),
                Decision::GameOver { .. } => return None,
            }
        };
        let out = self
            .mem
            .chatter
            .filter(state, &self.persona, self.seed, kind, lines);
        self.mem.last = Some((action.clone(), fp));
        Some((action, out))
    }

    fn voice(&self) -> Voice<'_> {
        Voice {
            persona: &self.persona,
            me: self.power,
            seed: self.seed,
        }
    }

    /// Random-policy (or forced) action when our own choices keep being rejected.
    fn fallback_action(&mut self, state: &GameState, d: &Decision) -> Action {
        if self.mem.rejected.len() > 14 {
            // Last resort: the simplest thing that always moves the game along.
            return match d {
                Decision::SelectCasualties { hits, sub_hits, .. } => {
                    let b = state.battle.as_ref().unwrap();
                    let side = b
                        .pending
                        .as_ref()
                        .map_or(wartide_engine::state::Side::Attacker, |p| p.side);
                    let cand = wartide_engine::casualties::candidates(state, b, side);
                    Action::SelectCasualties {
                        units: default_pick(state, &cand, hits - sub_hits, *sub_hits),
                    }
                }
                Decision::RetreatOrPress { .. } => Action::PressAttack,
                Decision::SubWithdrawal { .. } => Action::WithdrawSubs {
                    subs: vec![],
                    to: None,
                },
                _ => Action::EndPhase,
            };
        }
        match self.fallback.next_action(state) {
            Some((_, a)) if !self.mem.rejected.contains(&a) => a,
            _ => Action::EndPhase,
        }
    }

    fn phase_action(&mut self, state: &GameState, phase: Phase) -> (Action, Vec<String>, Kind) {
        let me = self.power;
        let style = self.persona.style;
        let key = (state.turn.number, phase);
        if self.mem.key != Some(key) {
            self.mem.key = Some(key);
            self.mem.built = false;
            self.mem.count = 0;
            self.mem.queue.clear();
            self.mem.intel = None;
            self.mem.rejected.clear();
        }
        self.mem.count += 1;
        let end = |lines: Vec<String>| (Action::EndPhase, lines, Kind::Plan);
        if self.mem.count > phase_cap(phase) || self.mem.rejected.contains(&Action::EndPhase) {
            return if self.mem.rejected.contains(&Action::EndPhase) {
                (
                    self.fallback_action(state, &Decision::Phase { power: me, phase }),
                    vec![],
                    Kind::Plan,
                )
            } else {
                end(vec![])
            };
        }
        let round_seed = mix(&[self.seed, u64::from(state.turn.number), phase as u64]);
        match phase {
            Phase::DevelopWeapons => {
                if !self.mem.built {
                    self.mem.built = true;
                    if let Some(a) = plan_research(state, me, style) {
                        let l = self.voice().research(state);
                        return (a, l, Kind::Plan);
                    }
                }
                end(vec![])
            }
            Phase::Purchase => {
                if !self.mem.built && state.purchases.is_empty() {
                    self.mem.built = true;
                    let intel = Intel::new(state, me);
                    let (a, note) = plan_purchase(state, me, style, &intel);
                    if matches!(a, Action::Purchase { .. }) {
                        let l = self.voice().purchase(state, &note);
                        self.mem.buy = Some(note);
                        return (a, l, Kind::Plan);
                    }
                }
                end(vec![])
            }
            Phase::CombatMove => {
                if !self.mem.built {
                    self.mem.built = true;
                    let intel = Intel::new(state, me);
                    let plan = plan_combat_move(state, me, style, &intel, round_seed);
                    self.mem.queue = plan.actions.into();
                    self.mem.notes = plan.notes;
                }
                if let Some(a) = self.pop_valid(state) {
                    return (a, vec![], Kind::Plan);
                }
                let l = self.voice().attacks(state, &self.mem.notes);
                end(l)
            }
            Phase::Combat => {
                let a = combat_phase_action(state, me, &self.mem.rejected);
                if a == Action::EndPhase {
                    let attempted = !self.mem.notes.is_empty();
                    return end(self.voice().results(state, attempted));
                }
                (a, vec![], Kind::Plan)
            }
            Phase::NonCombatMove => {
                if !self.mem.built {
                    self.mem.built = true;
                    let intel = Intel::new(state, me);
                    let plan = plan_noncombat(state, me, style, &intel, round_seed);
                    self.mem.hold = (plan.hold, plan.advanced + plan.ferry);
                    self.mem.queue = plan.actions.into();
                }
                if let Some(a) = self.pop_valid(state) {
                    return (a, vec![], Kind::Plan);
                }
                let (h, n) = self.mem.hold;
                end(self.voice().hold(state, h, n))
            }
            Phase::Mobilize => {
                if state.purchases.is_empty() {
                    return end(vec![]);
                }
                if self.mem.intel.is_none() {
                    self.mem.intel = Some(Intel::new(state, me));
                }
                let intel = self.mem.intel.as_ref().unwrap();
                let a = next_place(state, me, intel, &self.mem.rejected);
                (a, vec![], Kind::Plan)
            }
            Phase::CollectIncome => end(vec![]),
        }
    }

    fn pop_valid(&mut self, state: &GameState) -> Option<Action> {
        while let Some(a) = self.mem.queue.pop_front() {
            if still_valid(state, self.power, &a) {
                return Some(a);
            }
        }
        None
    }
}

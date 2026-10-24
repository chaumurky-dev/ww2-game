//! A simple random legal-action policy: drives a game using only `pending_decision` and the
//! `query` helpers. Used by the fuzz tests and as a fallback "bot" by other crates.
use crate::action::Action;
use crate::battle::{defender_withdraw_options, pending_battles, sub_withdraw_options};
use crate::bombing::{bombing_targets, rocket_targets};
use crate::combat::{candidates, required_casualties};
use crate::decision::{pending_decision, Decision};
use crate::dice::{rng_from_seed, Rng, RngCore};
use crate::movement::is_landing_spot;
use crate::power::Power;
use crate::query::{
    legal_placements, load_options, purchase_options, reachable, retreat_destinations,
    unload_options,
};
use crate::state::{GameState, Phase, PurchaseItem, Side};
use crate::units::{UnitId, UnitType};

/// Random player. Deterministic for a given seed and sequence of states.
pub struct RandomPolicy {
    rng: Rng,
    key: (u32, Phase),
    budget: u32,
    /// Probability (0..=1) of preferring an attacking move when one exists.
    pub aggression: f64,
}

impl RandomPolicy {
    /// New policy with the given seed.
    pub fn new(seed: u64) -> Self {
        RandomPolicy {
            rng: rng_from_seed(seed),
            key: (0, Phase::CollectIncome),
            budget: 0,
            aggression: 0.6,
        }
    }

    fn below(&mut self, n: usize) -> usize {
        if n <= 1 {
            0
        } else {
            (self.rng.next_u64() % n as u64) as usize
        }
    }

    fn chance(&mut self, p: f64) -> bool {
        (self.rng.next_u32() as f64 / u32::MAX as f64) < p
    }

    fn pick<T: Clone>(&mut self, v: &[T]) -> Option<T> {
        if v.is_empty() {
            None
        } else {
            Some(v[self.below(v.len())].clone())
        }
    }

    /// The next (actor, action) for the state, or `None` if the game is over.
    pub fn next_action(&mut self, state: &GameState) -> Option<(Power, Action)> {
        match pending_decision(state) {
            Decision::GameOver { .. } => None,
            Decision::SelectCasualties {
                power,
                hits,
                sub_hits,
                eligible,
                ..
            } => Some((power, self.casualties(state, hits, sub_hits, &eligible))),
            Decision::RetreatOrPress { power, .. } => Some((power, self.retreat_or_press(state))),
            Decision::SubWithdrawal { power, subs, .. } => {
                let opts = defender_withdraw_options(state, power);
                let act = match self.pick(&opts) {
                    Some(to) if self.chance(0.3) => Action::WithdrawSubs { subs, to: Some(to) },
                    _ => Action::WithdrawSubs {
                        subs: vec![],
                        to: None,
                    },
                };
                Some((power, act))
            }
            Decision::Phase { power, phase } => {
                let key = (state.turn.number, phase);
                if key != self.key {
                    self.key = key;
                    self.budget = match phase {
                        Phase::CombatMove => 6 + self.below(14) as u32,
                        Phase::NonCombatMove => 3 + self.below(10) as u32,
                        Phase::Combat => 40,
                        Phase::Mobilize => 60,
                        _ => 1,
                    };
                }
                Some((power, self.phase_action(state, power, phase)))
            }
        }
    }

    fn casualties(
        &mut self,
        state: &GameState,
        hits: u32,
        sub_hits: u32,
        eligible: &[UnitId],
    ) -> Action {
        let b = state.battle.as_ref().expect("battle");
        let side = b.pending.as_ref().map_or(Side::Attacker, |p| p.side);
        let cand = candidates(state, b, side);
        let normal = hits - sub_hits;
        let (t, ms) = required_casualties(state, &cand, normal, sub_hits);
        let mut nonair: Vec<UnitId> = cand
            .iter()
            .copied()
            .filter(|i| state.unit(*i).is_some_and(|u| !u.kind.is_air()))
            .collect();
        let mut rest: Vec<UnitId> = cand
            .iter()
            .copied()
            .filter(|i| state.unit(*i).is_some_and(|u| u.kind.is_air()))
            .collect();
        let mut chosen = vec![];
        for _ in 0..ms {
            let i = self.below(nonair.len());
            chosen.push(nonair.remove(i));
        }
        rest.extend(nonair);
        for _ in 0..(t - ms) {
            let i = self.below(rest.len());
            chosen.push(rest.remove(i));
        }
        debug_assert!(chosen.iter().all(|c| eligible.contains(c)) || normal == 0);
        Action::SelectCasualties { units: chosen }
    }

    fn retreat_or_press(&mut self, state: &GameState) -> Action {
        let subs: Vec<UnitId> = state.battle.as_ref().map_or(vec![], |b| {
            b.attackers
                .iter()
                .copied()
                .filter(|i| {
                    state
                        .unit(*i)
                        .is_some_and(|u| u.kind == UnitType::Submarine)
                })
                .collect()
        });
        let wopts = sub_withdraw_options(state);
        if !subs.is_empty() && !wopts.is_empty() && self.chance(0.15) {
            let to = self.pick(&wopts);
            return Action::WithdrawSubs {
                subs: vec![subs[0]],
                to,
            };
        }
        let opts = retreat_destinations(state);
        if !opts.is_empty() && self.chance(0.25) {
            return Action::Retreat {
                to: self.pick(&opts).unwrap(),
            };
        }
        Action::PressAttack
    }

    fn phase_action(&mut self, state: &GameState, p: Power, phase: Phase) -> Action {
        match phase {
            Phase::DevelopWeapons => {
                if !state.turn.researched && state.treasury[p] >= 5 && self.chance(0.25) {
                    Action::BuyResearch {
                        dice: 1 + self.below(2).min((state.treasury[p] / 5 - 1) as usize) as u32,
                    }
                } else {
                    Action::EndPhase
                }
            }
            Phase::Purchase => {
                if state.purchases.is_empty() && state.treasury[p] >= 3 {
                    let items = self.shopping(state);
                    if !items.is_empty() {
                        return Action::Purchase { items };
                    }
                }
                Action::EndPhase
            }
            Phase::CombatMove => self.movement(state, p, true),
            Phase::NonCombatMove => self.movement(state, p, false),
            Phase::Combat => self.combat(state, p),
            Phase::Mobilize => self.mobilize(state),
            Phase::CollectIncome => Action::EndPhase,
        }
    }

    fn shopping(&mut self, state: &GameState) -> Vec<PurchaseItem> {
        let opts = purchase_options(state);
        let mut left = state.treasury[state.current_power];
        let target = left * (40 + self.below(61) as u32) / 100;
        let mut spent = 0;
        let mut items: Vec<PurchaseItem> = vec![];
        let mut ic_bought = 0;
        for _ in 0..40 {
            if spent >= target {
                break;
            }
            let affordable: Vec<_> = opts
                .iter()
                .filter(|o| {
                    o.cost <= left
                        && (o.unit != UnitType::IndustrialComplex
                            || (o.max_more > ic_bought && self.chance(0.1)))
                })
                .collect();
            let Some(o) = self.pick(&affordable) else {
                break;
            };
            if o.unit == UnitType::IndustrialComplex {
                ic_bought += 1;
            }
            match items.iter_mut().find(|i| i.unit == o.unit) {
                Some(i) => i.count += 1,
                None => items.push(PurchaseItem {
                    unit: o.unit,
                    count: 1,
                }),
            }
            left -= o.cost;
            spent += o.cost;
        }
        items
    }

    fn movement(&mut self, state: &GameState, p: Power, cm: bool) -> Action {
        if self.budget == 0 {
            if !cm {
                if let Some(a) = self.land_stragglers(state, p) {
                    return a;
                }
            }
            return Action::EndPhase;
        }
        self.budget -= 1;
        let mine: Vec<UnitId> = state
            .units
            .iter()
            .filter(|u| u.owner == p && u.kind != UnitType::IndustrialComplex)
            .map(|u| u.id)
            .collect();
        if mine.is_empty() {
            return Action::EndPhase;
        }
        // transports
        if self.chance(0.2) {
            let u = mine[self.below(mine.len())];
            let unit = state.unit(u).unwrap();
            if unit.kind.transport_slots().is_some() && unit.loaded_on.is_none() {
                if let Some(t) = self.pick(&load_options(state, u)) {
                    return Action::Load {
                        units: vec![u],
                        transport: t,
                    };
                }
            }
            if unit.kind == UnitType::Transport && state.cargo_of(u).any(|c| c.owner == p) {
                if let Some(to) = self.pick(&unload_options(state, u)) {
                    let cargo: Vec<UnitId> = state
                        .cargo_of(u)
                        .filter(|c| c.owner == p)
                        .map(|c| c.id)
                        .collect();
                    return Action::Unload {
                        transport: u,
                        units: cargo,
                        to,
                    };
                }
            }
        }
        for _ in 0..10 {
            let id = mine[self.below(mine.len())];
            let reach = reachable(state, id);
            if reach.is_empty() {
                continue;
            }
            let u = state.unit(id).unwrap();
            let battles: Vec<_> = reach.iter().filter(|r| r.battle).cloned().collect();
            let r = if cm && !battles.is_empty() && self.chance(self.aggression) {
                self.pick(&battles).unwrap()
            } else if !cm && u.kind.is_air() {
                let landing: Vec<_> = reach.iter().filter(|r| r.landing).cloned().collect();
                match self.pick(&landing) {
                    Some(r) => r,
                    None => continue,
                }
            } else {
                self.pick(&reach).unwrap()
            };
            return Action::Move {
                units: vec![id],
                path: r.path,
            };
        }
        if !cm {
            if let Some(a) = self.land_stragglers(state, p) {
                return a;
            }
        }
        Action::EndPhase
    }

    /// Non-combat: bring home aircraft that are not on a landing spot.
    fn land_stragglers(&mut self, state: &GameState, p: Power) -> Option<Action> {
        for u in state
            .units
            .iter()
            .filter(|u| u.owner == p && u.kind.is_air() && u.loaded_on.is_none())
        {
            if is_landing_spot(state, u, u.location) {
                continue;
            }
            let landing: Vec<_> = reachable(state, u.id)
                .into_iter()
                .filter(|r| r.landing)
                .collect();
            if let Some(r) = self.pick(&landing) {
                return Some(Action::Move {
                    units: vec![u.id],
                    path: r.path,
                });
            }
        }
        None
    }

    fn combat(&mut self, state: &GameState, p: Power) -> Action {
        if let Some(l) = self.pick(&pending_battles(state)) {
            return Action::BeginBattle { location: l };
        }
        if self.budget > 0 {
            self.budget -= 1;
            if let Some((loc, bombers)) = self.pick(&bombing_targets(state, p)) {
                if self.chance(0.7) {
                    return Action::DeclareStrategicBombing {
                        location: loc,
                        bombers,
                    };
                }
            }
            if !state.turn.rocket_fired {
                for g in state
                    .units
                    .iter()
                    .filter(|u| u.owner == p && u.kind == UnitType::AaGun)
                {
                    if let Some(t) = self.pick(&rocket_targets(state, g.id)) {
                        return Action::FireRocket {
                            aa_gun: g.id,
                            target: t,
                        };
                    }
                }
            }
        }
        Action::EndPhase
    }

    fn mobilize(&mut self, state: &GameState) -> Action {
        let kinds: Vec<UnitType> = state.purchases.iter().map(|i| i.unit).collect();
        if self.budget == 0 || kinds.is_empty() {
            return Action::EndPhase;
        }
        self.budget -= 1;
        for _ in 0..4 {
            let k = kinds[self.below(kinds.len())];
            if let Some(l) = self.pick(&legal_placements(state, k)) {
                return Action::Place {
                    unit: k,
                    location: l,
                    count: 1,
                    from: None,
                };
            }
        }
        Action::EndPhase
    }
}

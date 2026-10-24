//! Action dispatcher and the phase machine.
use crate::action::Action;
use crate::battle::{begin_battle, pending_battles};
use crate::bombing;
use crate::combat;
use crate::decision::{pending_decision, Decision};
use crate::dice::{rng_from_seed, Rng};
use crate::economy;
use crate::error::{illegal, Result, RuleError};
use crate::event::Event;
use crate::map::LocationId;
use crate::movement::{execute_move, resolve_landings, validate_move};
use crate::power::{Power, Team};
use crate::state::{GameState, Phase, TurnState, VictoryReason, Winner};
use crate::tech;
use crate::transport::{execute_load, execute_unload, validate_load, validate_unload};
use crate::victory;

/// Apply `action` by `actor`. On success the state is updated and the events returned; on
/// failure the state is untouched. `seed` drives all dice rolled by this action.
pub fn apply(
    state: &mut GameState,
    actor: Power,
    action: &Action,
    seed: u64,
) -> Result<Vec<Event>> {
    let mut work = state.clone();
    let mut rng = rng_from_seed(seed);
    let mut ev = vec![];
    dispatch(&mut work, actor, action, &mut rng, &mut ev)?;
    *state = work;
    Ok(ev)
}

/// Replay a recorded game from its settings and action log.
pub fn replay(
    settings: crate::state::GameSettings,
    log: &[(Power, Action, u64)],
) -> Result<GameState> {
    let mut st = GameState::new(settings);
    for (p, a, s) in log {
        apply(&mut st, *p, a, *s)?;
    }
    Ok(st)
}

fn wrong_phase<T>(state: &GameState, why: &str) -> Result<T> {
    Err(RuleError::WrongPhase {
        phase: state.phase,
        reason: why.to_string(),
    })
}

fn dispatch(
    state: &mut GameState,
    actor: Power,
    action: &Action,
    rng: &mut Rng,
    ev: &mut Vec<Event>,
) -> Result<()> {
    if state.winner.is_some() {
        return Err(RuleError::GameOver);
    }
    if let Action::Concede = action {
        return concede(state, actor, ev);
    }
    let expected = match pending_decision(state) {
        Decision::Phase { power, .. }
        | Decision::SelectCasualties { power, .. }
        | Decision::RetreatOrPress { power, .. }
        | Decision::SubWithdrawal { power, .. } => power,
        Decision::GameOver { .. } => return Err(RuleError::GameOver),
    };
    if actor != expected {
        return Err(RuleError::NotYourTurn { actor, expected });
    }
    if state.battle.is_some() {
        return match action {
            Action::SelectCasualties { units } => {
                combat::select_casualties(state, actor, units, rng, ev)
                    .map(|_| after_combat(state, ev))
            }
            Action::Retreat { to } => {
                combat::retreat(state, actor, *to, ev).map(|_| after_combat(state, ev))
            }
            Action::PressAttack => {
                combat::press(state, actor, rng, ev).map(|_| after_combat(state, ev))
            }
            Action::WithdrawSubs { subs, to } => {
                combat::withdraw_subs(state, actor, subs, *to, rng, ev)
                    .map(|_| after_combat(state, ev))
            }
            _ => wrong_phase(state, "a battle is in progress"),
        };
    }
    let p = state.current_power;
    match (state.phase, action) {
        (_, Action::EndPhase) => end_phase(state, ev),
        (Phase::DevelopWeapons, Action::BuyResearch { dice }) => {
            tech::research(state, p, *dice, rng, ev)
        }
        (Phase::Purchase, Action::Purchase { items }) => economy::purchase(state, p, items, ev),
        (Phase::CombatMove | Phase::NonCombatMove, Action::Move { units, path }) => {
            let plan = validate_move(state, p, units, path)?;
            execute_move(state, p, units, path, &plan, rng, ev);
            Ok(())
        }
        (Phase::CombatMove | Phase::NonCombatMove, Action::Load { units, transport }) => {
            validate_load(state, p, units, *transport)?;
            execute_load(state, p, units, *transport, ev);
            Ok(())
        }
        (
            Phase::CombatMove | Phase::NonCombatMove,
            Action::Unload {
                transport,
                units,
                to,
            },
        ) => {
            let mode = validate_unload(state, p, *transport, units, *to)?;
            execute_unload(state, p, *transport, units, *to, mode, ev);
            Ok(())
        }
        (Phase::Combat, Action::BeginBattle { location }) => {
            begin_battle(state, *location, ev)?;
            combat::advance(state, rng, ev);
            after_combat(state, ev);
            Ok(())
        }
        (Phase::Combat, Action::DeclareStrategicBombing { location, bombers }) => {
            bombing::declare(state, p, *location, bombers, rng, ev).map(|_| after_combat(state, ev))
        }
        (Phase::Combat, Action::FireRocket { aa_gun, target }) => {
            bombing::fire_rocket(state, p, *aa_gun, *target, rng, ev)
                .map(|_| after_combat(state, ev))
        }
        (
            Phase::Mobilize,
            Action::Place {
                unit,
                location,
                count,
                from,
            },
        ) => economy::place(state, p, *unit, *location, *count, *from, ev),
        _ => wrong_phase(state, "that action is not available in this phase"),
    }
}

fn concede(state: &mut GameState, actor: Power, ev: &mut Vec<Event>) -> Result<()> {
    if !state.conceded.contains(&actor) {
        state.conceded.push(actor);
        ev.push(Event::Conceded { power: actor });
    }
    if actor
        .team()
        .powers()
        .iter()
        .all(|p| state.conceded.contains(p))
    {
        let team = actor.team().other();
        finish_game(
            state,
            Winner {
                team,
                reason: VictoryReason::Concession,
            },
            ev,
        );
    }
    Ok(())
}

fn finish_game(state: &mut GameState, w: Winner, ev: &mut Vec<Event>) {
    state.winner = Some(w);
    state.battle = None;
    ev.push(Event::GameOver {
        winner: w.team,
        reason: w.reason,
    });
}

fn set_phase(state: &mut GameState, phase: Phase, ev: &mut Vec<Event>) {
    state.phase = phase;
    ev.push(Event::PhaseChanged {
        power: state.current_power,
        phase,
    });
}

/// After combat steps: leave the Combat phase automatically when nothing remains to resolve.
fn after_combat(state: &mut GameState, ev: &mut Vec<Event>) {
    if state.phase == Phase::Combat && state.battle.is_none() && combat_finished(state) {
        enter_noncombat(state, ev);
    }
}

fn combat_finished(state: &GameState) -> bool {
    let p = state.current_power;
    pending_battles(state).is_empty()
        && bombing::bombing_targets(state, p).is_empty()
        && !bombing::rocket_available(state, p)
}

fn enter_noncombat(state: &mut GameState, ev: &mut Vec<Event>) {
    let p = state.current_power;
    for u in state.units.iter_mut().filter(|u| u.owner == p) {
        u.assault_to = None;
        u.combat_from = None;
    }
    set_phase(state, Phase::NonCombatMove, ev);
}

fn end_phase(state: &mut GameState, ev: &mut Vec<Event>) -> Result<()> {
    let p = state.current_power;
    match state.phase {
        Phase::DevelopWeapons => set_phase(state, Phase::Purchase, ev),
        Phase::Purchase => set_phase(state, Phase::CombatMove, ev),
        Phase::CombatMove => {
            set_phase(state, Phase::Combat, ev);
            after_combat(state, ev);
        }
        Phase::Combat => {
            if !pending_battles(state).is_empty() {
                return illegal("all battles must be resolved before ending combat");
            }
            enter_noncombat(state, ev);
        }
        Phase::NonCombatMove => {
            resolve_landings(state, p, ev);
            set_phase(state, Phase::Mobilize, ev);
        }
        Phase::Mobilize => {
            economy::discard_unplaced(state, p, ev);
            set_phase(state, Phase::CollectIncome, ev);
            economy::collect_income(state, p, ev);
            finish_turn(state, ev);
        }
        Phase::CollectIncome => return illegal("income is collected automatically"),
    }
    Ok(())
}

/// Power with no units and no territories.
fn eliminated(state: &GameState, p: Power) -> bool {
    !state.units.iter().any(|u| u.owner == p)
        && economy::production_level(state, p) == 0
        && !crate::map::MapData::get()
            .territories()
            .any(|t| state.control[t] == Some(p))
}

fn finish_turn(state: &mut GameState, ev: &mut Vec<Event>) {
    let p = state.current_power;
    for u in state.units.iter_mut().filter(|u| u.owner == p) {
        u.reset_turn();
    }
    if let Some(w) = victory::check_military(state) {
        return finish_game(state, w, ev);
    }
    if p == Power::Usa {
        if let Some(w) = victory::check_economic(state) {
            return finish_game(state, w, ev);
        }
    }
    for _ in 0..5 {
        start_next_turn(state, ev);
        let cur = state.current_power;
        if !eliminated(state, cur) {
            return;
        }
        // Fully eliminated: run its turn trivially to keep the order stable.
        economy::collect_income(state, cur, ev);
        for u in state.units.iter_mut().filter(|u| u.owner == cur) {
            u.reset_turn();
        }
        if cur == Power::Usa {
            if let Some(w) = victory::check_economic(state) {
                return finish_game(state, w, ev);
            }
        }
    }
}

fn start_next_turn(state: &mut GameState, ev: &mut Vec<Event>) {
    let next = state.current_power.next();
    if next == Power::Ussr {
        state.round += 1;
    }
    state.current_power = next;
    state.turn = TurnState {
        number: state.turn.number + 1,
        start_control: state.control.clone(),
        resolved: vec![],
        researched: false,
        rocket_fired: false,
        placed: vec![],
    };
    state.purchases.clear();
    for u in state.units.iter_mut().filter(|u| u.owner == next) {
        u.reset_turn();
    }
    ev.push(Event::TurnStarted {
        round: state.round,
        power: next,
    });
    let phase = if state.settings.weapons_development {
        Phase::DevelopWeapons
    } else {
        Phase::Purchase
    };
    set_phase(state, phase, ev);
}

/// The team (if any) that has won.
pub fn winner(state: &GameState) -> Option<Team> {
    state.winner.map(|w| w.team)
}

/// Locations of pending battles (convenience re-export).
pub fn battles(state: &GameState) -> Vec<LocationId> {
    pending_battles(state)
}

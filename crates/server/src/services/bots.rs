//! Fixed catalog of computer-player personas.
use std::time::Duration;

use serde_json::json;
use uuid::Uuid;
use wartide_ai::{Bot, Commentary, Persona, Style, Verbosity};
use wartide_engine as engine;
use wartide_protocol::bots::{BotPersona, PersonaStyle, PersonaVerbosity};
use wartide_protocol::chat::ChatChannel;
use wartide_protocol::game::Power;
use wartide_protocol::user::UserSettings;

use crate::error::{AppError, AppResult};
use crate::services::games::{self, Actor, SeatRow};
use crate::services::{chat, rules, users};
use crate::state::{AppState, DriverSlot};

type Row = (
    &'static str,
    &'static str,
    PersonaStyle,
    PersonaVerbosity,
    &'static str,
);

const CATALOG: [Row; 6] = [
    (
        "brass",
        "Colonel Brass",
        PersonaStyle::Aggressive,
        PersonaVerbosity::Chatty,
        "Charges first and explains later. Loves a bold opening and says so, loudly.",
    ),
    (
        "frost",
        "General Frost",
        PersonaStyle::Cautious,
        PersonaVerbosity::Quiet,
        "Patient and unhurried. Digs in, builds up, and rarely says a word.",
    ),
    (
        "kestrel",
        "Admiral Kestrel",
        PersonaStyle::Balanced,
        PersonaVerbosity::Normal,
        "Keeps the sea lanes open and weighs every trade. A steady, fair opponent.",
    ),
    (
        "volkov",
        "Marshal Volkov",
        PersonaStyle::Aggressive,
        PersonaVerbosity::Normal,
        "Masses armor and pushes on the main front until something breaks.",
    ),
    (
        "tide",
        "Commodore Tide",
        PersonaStyle::Balanced,
        PersonaVerbosity::Chatty,
        "Friendly commentator who narrates every move and every dice roll.",
    ),
    (
        "thistle",
        "Major Thistle",
        PersonaStyle::Cautious,
        PersonaVerbosity::Chatty,
        "Careful planner who double-checks the odds out loud before committing.",
    ),
];

pub fn catalog() -> Vec<BotPersona> {
    CATALOG
        .iter()
        .map(|(id, name, style, verbosity, blurb)| BotPersona {
            id: (*id).into(),
            name: (*name).into(),
            style: *style,
            verbosity: *verbosity,
            blurb: (*blurb).into(),
        })
        .collect()
}

pub fn persona_name(id: &str) -> Option<&'static str> {
    CATALOG.iter().find(|r| r.0 == id).map(|r| r.1)
}

/// Default persona id for a seat that did not choose one.
pub fn default_for_seat(seat: usize) -> &'static str {
    CATALOG[seat % CATALOG.len()].0
}

pub fn validate_id(id: &str) -> AppResult<&'static str> {
    CATALOG
        .iter()
        .find(|r| r.0 == id)
        .map(|r| r.0)
        .ok_or_else(|| {
            AppError::BadRequest("invalid_persona", format!("unknown bot persona '{id}'"))
        })
}

/// Computer-player persona (as the AI crate sees it) for a catalog id; unknown ids get the default seat persona.
pub fn ai_persona(id: &str) -> Persona {
    let row = CATALOG.iter().find(|r| r.0 == id).unwrap_or(&CATALOG[0]);
    Persona {
        id: row.0.into(),
        name: row.1.into(),
        style: match row.2 {
            PersonaStyle::Aggressive => Style::Aggressive,
            PersonaStyle::Balanced => Style::Balanced,
            PersonaStyle::Cautious => Style::Cautious,
        },
        verbosity: match row.3 {
            PersonaVerbosity::Chatty => Verbosity::Chatty,
            PersonaVerbosity::Normal => Verbosity::Normal,
            PersonaVerbosity::Quiet => Verbosity::Quiet,
        },
    }
}

// ---------- bot driver ----------

/// Most actions one driver pass applies before yielding (the sweep resumes unfinished games).
pub const MAX_ACTIONS_PER_PASS: usize = 5000;
/// Rejected bot actions for one decision before the random policy takes over.
const BOT_ATTEMPTS: u32 = 3;
/// Rejected random-policy actions before falling back to `EndPhase` / `Concede`.
const RANDOM_ATTEMPTS: u32 = 20;

/// Start a driver for every game in progress. Called once at startup so games interrupted by a
/// restart (a bot mid-turn, or a defender decision awaiting automatic resolution) resume at once
/// instead of waiting for a deadline to expire. Cheap: a pass with nothing to do returns quickly.
pub async fn resume_all(state: &AppState) -> AppResult<usize> {
    let ids: Vec<(Uuid,)> = sqlx::query_as("SELECT id FROM games WHERE status='in_progress'")
        .fetch_all(&state.db)
        .await?;
    for (id,) in &ids {
        spawn_driver(state, *id, None);
    }
    Ok(ids.len())
}

/// Make sure a task is playing every bot-controlled decision of `game`. Cheap and idempotent: if a
/// task is already running it is asked to look again when it finishes. `takeover` additionally
/// makes the task play that (human, timed-out) power until the engine moves on to another power.
pub fn spawn_driver(state: &AppState, game: Uuid, takeover: Option<Power>) {
    {
        let mut drivers = state.runtime.drivers.lock().unwrap();
        if let Some(slot) = drivers.get_mut(&game) {
            slot.rerun = true;
            if takeover.is_some() {
                slot.takeover = takeover;
            }
            return;
        }
        drivers.insert(
            game,
            DriverSlot {
                rerun: false,
                takeover,
            },
        );
    }
    let state = state.clone();
    tokio::spawn(async move {
        loop {
            let takeover = state
                .runtime
                .drivers
                .lock()
                .unwrap()
                .get(&game)
                .and_then(|s| s.takeover);
            let keep = match run_pass(&state, game, takeover).await {
                Ok(t) => t,
                Err(e) => {
                    tracing::error!(error = ?e, %game, "bot driver failed");
                    None
                }
            };
            let mut drivers = state.runtime.drivers.lock().unwrap();
            let slot = drivers.get_mut(&game).expect("driver slot");
            slot.takeover = keep;
            if slot.rerun {
                slot.rerun = false;
                continue;
            }
            drivers.remove(&game);
            break;
        }
    });
}

/// Is the power controlled by the server right now (bot seat, or timeout takeover)?
fn server_plays(seats: &[SeatRow], power: Power, takeover: Option<Power>) -> bool {
    games::seat_for_power(seats, power).is_some_and(|s| s.is_bot) || takeover == Some(power)
}

/// One driver pass: play decisions while they belong to the server. Returns the takeover that is
/// still in force (None once the engine moved on from the power).
async fn run_pass(
    state: &AppState,
    game: Uuid,
    mut takeover: Option<Power>,
) -> AppResult<Option<Power>> {
    let delay = state.config.bot_action_delay_ms;
    let mut applied = 0usize;
    // (seq the failures belong to, failed attempts)
    let mut failures: (i64, u32) = (-1, 0);
    loop {
        if applied >= MAX_ACTIONS_PER_PASS {
            tracing::warn!(%game, "bot driver hit the per-pass action limit; the sweep will resume");
            return Ok(takeover);
        }
        let g = games::load_game(state, game).await?;
        if g.status != "in_progress" {
            return Ok(None);
        }
        let (seq, st): (i64, serde_json::Value) =
            sqlx::query_as("SELECT seq, state FROM game_states WHERE game_id=$1")
                .bind(game)
                .fetch_one(&state.db)
                .await?;
        let gs = rules::parse_state(&st)?;
        let pending = rules::pending_of(&gs);
        let Some(power) = pending.power else {
            if let Some(w) = pending.game_over {
                games::finish_game(state, game, w).await?;
            }
            return Ok(None);
        };
        // The takeover lasts until the engine moves on to a different power.
        if takeover.is_some_and(|t| t != power) {
            takeover = None;
        }
        let seats = games::load_seats(state, game).await?;
        // Defenders never wait on a human: resolve their decisions from their battle preferences.
        if pending.defender && !server_plays(&seats, power, takeover) {
            let seat = games::seat_for_power(&seats, power).expect("seat for power");
            let settings = match seat.user_id {
                Some(uid) => users::get_settings(state, uid).await?,
                None => Default::default(),
            };
            let Some(action) = defender_action(&gs, &settings, g.seed, seq) else {
                return Ok(takeover);
            };
            let value = rules::to_json(&action)?;
            let res = {
                let _guard = state.runtime.lock(game).await;
                games::apply_locked(state, game, Actor::Auto, value, Some(seq)).await
            };
            match res {
                Ok(_) => applied += 1,
                Err(AppError::Conflict("stale_state", _)) => {}
                Err(AppError::BadRequest("illegal_action", msg)) => {
                    // Should be unreachable (the policy always yields a legal selection); never
                    // leave the game stuck behind a human who is not being asked.
                    tracing::error!(%game, ?power, %msg, "automatic defender action rejected");
                    return Ok(takeover);
                }
                Err(e) => return Err(e),
            }
            continue;
        }
        if !server_plays(&seats, power, takeover) {
            return Ok(takeover);
        }
        let seat = games::seat_for_power(&seats, power).expect("seat for power");
        let persona_id = seat
            .bot_persona
            .clone()
            .unwrap_or_else(|| default_for_seat(seat.seat as usize).to_string());
        let persona = ai_persona(&persona_id);
        let bot_name = persona.name.clone();
        let team = power.team();

        if failures.0 != seq {
            failures = (seq, 0);
        }
        let attempt = failures.1;
        let (action, commentary): (engine::Action, Vec<Commentary>) = if attempt < BOT_ATTEMPTS {
            match plan(state, game, g.seed, &persona_id, persona, power, gs.clone()).await {
                Some(r) => r,
                None => {
                    failures.1 += 1;
                    continue;
                }
            }
        } else if attempt < BOT_ATTEMPTS + RANDOM_ATTEMPTS {
            let mut rp = engine::random_policy::RandomPolicy::new(
                wartide_protocol::seed::fallback_seed(g.seed as u64, seq, attempt),
            );
            match rp.next_action(&gs) {
                Some((p, a)) if rules::power_from_engine(p) == power => (a, Vec::new()),
                _ => {
                    failures.1 += 1;
                    continue;
                }
            }
        } else {
            let first_resort = BOT_ATTEMPTS + RANDOM_ATTEMPTS;
            if attempt > first_resort + 1 {
                tracing::error!(%game, ?power, seq, "bot is stuck: even EndPhase/Concede failed");
                return Ok(takeover);
            }
            let last = match engine::pending_decision(&gs) {
                engine::Decision::Phase { .. } if attempt == first_resort => {
                    engine::Action::EndPhase
                }
                _ => engine::Action::Concede,
            };
            (last, Vec::new())
        };

        let value = rules::to_json(&action)?;
        let res = {
            let _guard = state.runtime.lock(game).await;
            games::apply_locked(state, game, Actor::Bot, value, Some(seq)).await
        };
        match res {
            Ok(_) => {
                applied += 1;
                for c in commentary {
                    if let Err(e) = chat::post_bot_message(
                        state,
                        game,
                        &bot_name,
                        ChatChannel::Team,
                        Some(games::enum_str(&team)),
                        &c.text,
                    )
                    .await
                    {
                        tracing::warn!(error = ?e, %game, "could not post bot commentary");
                    }
                }
            }
            Err(AppError::BadRequest("illegal_action", msg)) => {
                tracing::warn!(%game, ?power, attempt, %msg, action = %json!(action), "bot action rejected");
                failures.1 += 1;
            }
            Err(AppError::Conflict("stale_state", _)) => {} // someone else moved; look again
            Err(e) => return Err(e),
        }
        if delay > 0 {
            tokio::time::sleep(Duration::from_millis(delay)).await;
        }
    }
}

/// The action that resolves the pending defender decision under the player's battle preferences.
fn defender_action(
    gs: &engine::GameState,
    settings: &UserSettings,
    game_seed: i64,
    seq: i64,
) -> Option<engine::Action> {
    use engine::casualty_policy::{choose_casualties, SubWithdrawalPolicy};
    match engine::pending_decision(gs) {
        engine::Decision::SelectCasualties {
            hits,
            sub_hits,
            eligible,
            ..
        } => Some(engine::Action::SelectCasualties {
            units: choose_casualties(gs, settings.casualty_policy, hits, sub_hits, &eligible),
        }),
        engine::Decision::SubWithdrawal { power, subs, .. } => {
            let stay = engine::Action::WithdrawSubs {
                subs: vec![],
                to: None,
            };
            let to = engine::battle::defender_withdraw_options(gs, power)
                .first()
                .copied();
            Some(match (settings.sub_withdrawal, to) {
                (SubWithdrawalPolicy::Never, _) | (_, None) => stay,
                (SubWithdrawalPolicy::Always, Some(to)) => {
                    engine::Action::WithdrawSubs { subs, to: Some(to) }
                }
                (SubWithdrawalPolicy::WhenOutgunned, Some(_)) => {
                    wartide_ai::battle::sub_withdrawal(
                        gs,
                        power,
                        &subs,
                        wartide_protocol::seed::fallback_seed(game_seed as u64, seq, 0),
                    )
                }
            })
        }
        _ => None,
    }
}

/// Ask the cached (or freshly created) bot for its next action. The planner may be slow, so it
/// runs on the blocking pool.
async fn plan(
    state: &AppState,
    game: Uuid,
    game_seed: i64,
    persona_id: &str,
    persona: Persona,
    power: Power,
    gs: engine::GameState,
) -> Option<(engine::Action, Vec<Commentary>)> {
    let cached = {
        let mut bots = state.runtime.bots.lock().unwrap();
        match bots.remove(&(game, power)) {
            Some((pid, bot)) if pid == persona_id => Some(bot),
            _ => None,
        }
    };
    // Bots are deterministic functions of (persona, power, seed); a missing one (restart) is rebuilt.
    let mut bot = cached.unwrap_or_else(|| {
        Bot::new(
            persona,
            rules::power_to_engine(power),
            wartide_protocol::seed::bot_seed(game_seed as u64, power),
        )
    });
    let (bot, out) = tokio::task::spawn_blocking(move || {
        let out = bot.next_action(&gs);
        (bot, out)
    })
    .await
    .ok()?;
    state
        .runtime
        .bots
        .lock()
        .unwrap()
        .insert((game, power), (persona_id.to_string(), bot));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_is_consistent() {
        let c = catalog();
        assert_eq!(c.len(), 6);
        let mut ids: Vec<_> = c.iter().map(|p| p.id.clone()).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), 6);
        assert_eq!(persona_name("brass"), Some("Colonel Brass"));
        assert!(validate_id("nope").is_err());
        assert_eq!(default_for_seat(7), "frost");
    }
}

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use sqlx::PgPool;
use uuid::Uuid;
use wartide_protocol::game::Power;

use crate::config::Config;
use crate::email::Mailer;
use crate::hub::Hub;
use crate::ratelimit::RateLimiter;
use crate::services::rules::EngineRules;
use crate::services::timeouts::TurnTimeoutHandler;

/// Bookkeeping for one running bot driver task (see `services::bots`).
#[derive(Debug, Default, Clone)]
pub struct DriverSlot {
    /// Someone asked for another pass while the task was running.
    pub rerun: bool,
    /// Human power being auto-played after a timeout.
    pub takeover: Option<Power>,
}

/// In-memory, per-process game runtime state.
#[derive(Default)]
pub struct GameRuntime {
    /// One async mutex per game: only one action is applied to a game at a time.
    locks: Mutex<HashMap<Uuid, Arc<tokio::sync::Mutex<()>>>>,
    /// Games that currently have a bot driver task.
    pub drivers: Mutex<HashMap<Uuid, DriverSlot>>,
    /// Computer players, keyed by game and power, with the persona id they were built for.
    pub bots: Mutex<HashMap<(Uuid, Power), (String, wartide_ai::Bot)>>,
}

impl GameRuntime {
    /// Acquire the per-game lock.
    pub async fn lock(&self, game: Uuid) -> tokio::sync::OwnedMutexGuard<()> {
        let m = self.locks.lock().unwrap().entry(game).or_default().clone();
        m.lock_owned().await
    }

    /// Forget everything cached for a finished game.
    pub fn forget(&self, game: Uuid) {
        self.bots.lock().unwrap().retain(|(g, _), _| *g != game);
        let mut locks = self.locks.lock().unwrap();
        if locks.get(&game).is_some_and(|m| Arc::strong_count(m) == 1) {
            locks.remove(&game);
        }
    }
}

#[derive(Clone)]
pub struct AppState {
    pub db: PgPool,
    pub config: Arc<Config>,
    pub mailer: Mailer,
    pub hub: Hub,
    pub limiter: RateLimiter,
    pub rules: EngineRules,
    pub runtime: Arc<GameRuntime>,
    pub timeout_handler: Arc<dyn TurnTimeoutHandler>,
}

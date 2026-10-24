use std::sync::Arc;

use sqlx::PgPool;

use crate::config::Config;
use crate::email::Mailer;
use crate::hub::Hub;
use crate::ratelimit::RateLimiter;
use crate::services::rules::Rules;
use crate::services::timeouts::TurnTimeoutHandler;

#[derive(Clone)]
pub struct AppState {
    pub db: PgPool,
    pub config: Arc<Config>,
    pub mailer: Mailer,
    pub hub: Hub,
    pub limiter: RateLimiter,
    pub rules: Arc<dyn Rules>,
    pub timeout_handler: Arc<dyn TurnTimeoutHandler>,
}

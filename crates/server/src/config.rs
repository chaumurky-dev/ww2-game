use std::env;

#[derive(Debug, Clone)]
pub struct Config {
    pub port: u16,
    pub database_url: String,
    pub base_url: String,
    pub smtp_host: Option<String>,
    pub smtp_port: u16,
    pub smtp_user: Option<String>,
    pub smtp_pass: Option<String>,
    pub smtp_from: String,
    pub session_secret: String,
    pub web_dist_dir: String,
    pub default_turn_timeout_secs: u32,
    pub cookie_secure: bool,
    pub rate_limit: bool,
}

fn opt(key: &str) -> Option<String> {
    env::var(key).ok().filter(|v| !v.trim().is_empty())
}

impl Config {
    pub fn from_env() -> anyhow::Result<Self> {
        let database_url =
            opt("DATABASE_URL").ok_or_else(|| anyhow::anyhow!("DATABASE_URL must be set"))?;
        Ok(Self {
            port: opt("PORT").map(|p| p.parse()).transpose()?.unwrap_or(8080),
            database_url,
            base_url: opt("BASE_URL").unwrap_or_else(|| "http://localhost:8080".into()),
            smtp_host: opt("SMTP_HOST"),
            smtp_port: opt("SMTP_PORT")
                .map(|p| p.parse())
                .transpose()?
                .unwrap_or(1025),
            smtp_user: opt("SMTP_USER"),
            smtp_pass: opt("SMTP_PASS"),
            smtp_from: opt("SMTP_FROM").unwrap_or_else(|| "Wartide <noreply@localhost>".into()),
            session_secret: opt("SESSION_SECRET").unwrap_or_else(|| {
                tracing::warn!("SESSION_SECRET not set; using an insecure development default");
                "dev-insecure-session-secret".into()
            }),
            web_dist_dir: opt("WEB_DIST_DIR").unwrap_or_else(|| "crates/web/dist".into()),
            default_turn_timeout_secs: opt("DEFAULT_TURN_TIMEOUT_SECS")
                .map(|p| p.parse())
                .transpose()?
                .unwrap_or(172_800),
            cookie_secure: opt("COOKIE_SECURE").map(|v| v != "false").unwrap_or(true),
            rate_limit: opt("RATE_LIMIT").map(|v| v != "false").unwrap_or(true),
        })
    }

    /// Config for tests: no SMTP, no rate limit.
    pub fn for_test(database_url: String) -> Self {
        Self {
            port: 0,
            database_url,
            base_url: "http://localhost:8080".into(),
            smtp_host: None,
            smtp_port: 1025,
            smtp_user: None,
            smtp_pass: None,
            smtp_from: "Wartide <noreply@localhost>".into(),
            session_secret: "test-secret".into(),
            web_dist_dir: "does-not-exist".into(),
            default_turn_timeout_secs: 172_800,
            cookie_secure: false,
            rate_limit: false,
        }
    }
}

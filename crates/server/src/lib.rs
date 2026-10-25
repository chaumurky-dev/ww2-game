//! Wartide server: axum REST + WebSocket API over Postgres.
pub mod config;
pub mod email;
pub mod email_templates;
pub mod error;
pub mod extract;
pub mod hub;
pub mod ratelimit;
pub mod routes;
pub mod security;
pub mod services;
pub mod state;
pub mod util;
pub mod ws;

use std::path::Path;
use std::sync::Arc;

use axum::extract::{DefaultBodyLimit, State};
use axum::http::{HeaderName, Request};
use axum::routing::get;
use axum::{Json, Router};
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;
use tower_http::compression::CompressionLayer;
use tower_http::request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer};
use tower_http::services::{ServeDir, ServeFile};
use tower_http::trace::TraceLayer;

pub use config::Config;
pub use state::AppState;

use crate::email::Mailer;
use crate::services::rules::EngineRules;
use crate::services::timeouts::DefaultTimeoutHandler;

pub static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!();

pub async fn connect_db(url: &str) -> anyhow::Result<PgPool> {
    Ok(PgPoolOptions::new()
        .max_connections(10)
        .connect(url)
        .await?)
}

pub fn build_state(db: PgPool, config: Config, mailer: Mailer) -> AppState {
    let rate_limit = config.rate_limit;
    AppState {
        db,
        config: Arc::new(config),
        mailer,
        hub: hub::Hub::default(),
        limiter: ratelimit::RateLimiter::new(rate_limit),
        rules: EngineRules,
        runtime: Arc::new(state::GameRuntime::default()),
        timeout_handler: Arc::new(DefaultTimeoutHandler),
    }
}

async fn healthz(State(s): State<AppState>) -> Result<Json<serde_json::Value>, error::AppError> {
    sqlx::query("SELECT 1").execute(&s.db).await?;
    Ok(Json(serde_json::json!({ "status": "ok" })))
}

pub fn build_router(state: AppState) -> Router {
    let x_request_id = HeaderName::from_static("x-request-id");
    let dist = state.config.web_dist_dir.clone();
    let index = Path::new(&dist).join("index.html");
    // Unknown non-API paths serve the SPA shell so client-side routes work.
    let spa = ServeDir::new(&dist).fallback(ServeFile::new(index.clone()));
    let hardening = security::Hardening::new(index);

    Router::new()
        .route("/healthz", get(healthz))
        .route("/ws", get(ws::handler))
        .nest("/api", routes::api_router(&state))
        .fallback_service(spa)
        .with_state(state)
        .layer(DefaultBodyLimit::max(security::BODY_LIMIT))
        .layer(axum::middleware::from_fn_with_state(
            hardening,
            security::harden,
        ))
        .layer(CompressionLayer::new())
        .layer(
            TraceLayer::new_for_http().make_span_with(|req: &Request<_>| {
                let id = req
                    .headers()
                    .get("x-request-id")
                    .and_then(|v| v.to_str().ok())
                    .unwrap_or("-");
                tracing::info_span!("http", method = %req.method(), path = %req.uri().path(), request_id = %id)
            }),
        )
        .layer(PropagateRequestIdLayer::new(x_request_id.clone()))
        .layer(SetRequestIdLayer::new(x_request_id, MakeRequestUuid))
}

/// Run migrations, start background tasks and serve until SIGINT/SIGTERM.
pub async fn run(config: Config) -> anyhow::Result<()> {
    let db = connect_db(&config.database_url).await?;
    MIGRATOR.run(&db).await?;
    let mailer = Mailer::from_config(&config)?;
    let port = config.port;
    let state = build_state(db, config, mailer);

    services::matchmaking::spawn(state.clone());
    services::timeouts::spawn(state.clone());
    match services::bots::resume_all(&state).await {
        Ok(n) if n > 0 => tracing::info!(games = n, "resumed drivers for games in progress"),
        Ok(_) => {}
        Err(e) => tracing::error!(error = ?e, "could not resume game drivers"),
    }

    let app = build_router(state);
    let listener = tokio::net::TcpListener::bind(("0.0.0.0", port)).await?;
    tracing::info!("listening on {}", listener.local_addr()?);
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown_signal())
    .await?;
    tracing::info!("shut down cleanly");
    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };
    #[cfg(unix)]
    let term = async {
        if let Ok(mut s) = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        {
            s.recv().await;
        }
    };
    #[cfg(not(unix))]
    let term = std::future::pending::<()>();
    tokio::select! { _ = ctrl_c => {}, _ = term => {} }
    tracing::info!("shutdown signal received");
}

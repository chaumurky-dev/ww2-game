//! HTTP handlers. Thin: extract, call a service, shape the response.
use axum::routing::{delete, get, patch, post, put};
use axum::Router;

use axum::response::IntoResponse;

use crate::error::AppError;
use crate::state::AppState;

mod admin;
mod auth;
mod bots;
mod chat;
mod games;
mod groups;
mod matchmaking;
mod users;

pub fn api_router(state: &AppState) -> Router<AppState> {
    // Credential-handling endpoints get per-IP rate limiting.
    let mut auth_limited = Router::new()
        .route("/auth/register", post(auth::register))
        .route("/auth/login", post(auth::login))
        .route("/auth/forgot-password", post(auth::forgot_password))
        .route("/auth/reset-password", post(auth::reset_password))
        .route("/auth/verify-email", post(auth::verify_email))
        .route("/auth/resend-verification", post(auth::resend_verification));
    if state.config.rate_limit {
        use tower_governor::governor::GovernorConfigBuilder;
        use tower_governor::GovernorError;
        use tower_governor::GovernorLayer;
        let conf = GovernorConfigBuilder::default()
            .per_second(2)
            .burst_size(10)
            .key_extractor(crate::ratelimit::TrustedProxyKey {
                hops: state.config.trusted_proxy_hops,
            })
            .finish()
            .expect("valid governor config");
        // 429s use the standard ApiError JSON body (`code: "rate_limited"`).
        auth_limited = auth_limited.layer(GovernorLayer::new(conf).error_handler(|e| {
            match e {
                GovernorError::TooManyRequests { wait_time, .. } => {
                    AppError::TooManyRequests(wait_time).into_response()
                }
                GovernorError::UnableToExtractKey => {
                    AppError::bad("could not determine client address").into_response()
                }
                GovernorError::Other { code, msg, .. } => (
                    code,
                    axum::Json(wartide_protocol::ApiError {
                        code: "bad_request".into(),
                        message: msg.unwrap_or_default(),
                    }),
                )
                    .into_response(),
            }
        }));
    }

    Router::new()
        .merge(auth_limited)
        .route("/auth/logout", post(auth::logout))
        .route("/me", get(auth::me).delete(auth::delete_me))
        .route("/me/password", post(auth::change_password))
        .route("/bots/personas", get(bots::personas))
        .route("/admin/stats", get(admin::stats))
        .route("/admin/games/{id}/abandon", post(admin::abandon_game))
        .route("/games/public", get(games::list_public))
        .route("/users/{username}", get(users::profile))
        .route("/me/profile", patch(users::update_profile))
        .route(
            "/me/settings",
            get(users::get_settings).patch(users::update_settings),
        )
        .route("/leaderboard", get(users::public_leaderboard))
        .route("/groups", post(groups::create).get(groups::list_mine))
        .route("/groups/join", post(groups::join))
        .route("/groups/by-slug/{slug}", get(groups::by_slug))
        .route("/groups/{id}", get(groups::get))
        .route("/groups/{id}/leave", post(groups::leave))
        .route("/groups/{id}/invite/rotate", post(groups::rotate_invite))
        .route("/groups/{id}/members", get(groups::members))
        .route("/groups/{id}/members/{user_id}/role", put(groups::set_role))
        .route(
            "/groups/{id}/members/{user_id}",
            delete(groups::remove_member),
        )
        .route("/groups/{id}/leaderboard", get(groups::leaderboard))
        .route("/games", post(games::create).get(games::list_mine))
        .route("/games/join", post(games::join))
        .route("/games/{id}", get(games::get))
        .route("/games/{id}/state", get(games::state))
        .route(
            "/games/{id}/actions",
            get(games::actions).post(games::submit_action),
        )
        .route("/games/{id}/start", post(games::start))
        .route("/games/{id}/resign", post(games::resign))
        .route("/games/{id}/abandon", post(games::abandon))
        .route("/games/{id}/chat", get(chat::history).post(chat::send))
        .route(
            "/matchmaking/queue",
            post(matchmaking::enqueue)
                .delete(matchmaking::dequeue)
                .get(matchmaking::status),
        )
        .fallback(|| async { crate::error::AppError::not_found("endpoint") })
}

use axum::Json;
use wartide_protocol::bots::BotPersona;

use crate::services::bots;

/// The fixed persona catalog hosts can pick from when creating a game.
pub async fn personas() -> Json<Vec<BotPersona>> {
    Json(bots::catalog())
}

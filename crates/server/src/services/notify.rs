//! Email notifications. Every send here honours the recipient's settings and account state.
use uuid::Uuid;
use wartide_protocol::game::{GameMode, Power};

use crate::email_templates;
use crate::error::AppResult;
use crate::state::AppState;

#[derive(sqlx::FromRow)]
struct Recipient {
    email: String,
    username: String,
    email_turn_notifications: bool,
    email_match_found: bool,
    email_verified: bool,
}

async fn recipient(state: &AppState, user: Uuid) -> AppResult<Option<Recipient>> {
    Ok(sqlx::query_as::<_, Recipient>(
        "SELECT email, username, email_turn_notifications, email_match_found, email_verified
         FROM users WHERE id=$1 AND deleted_at IS NULL",
    )
    .bind(user)
    .fetch_optional(&state.db)
    .await?)
}

fn base(state: &AppState) -> &str {
    state.config.base_url.trim_end_matches('/')
}

pub async fn email_your_turn(
    state: &AppState,
    user: Uuid,
    game_name: &str,
    game: Uuid,
    power: Power,
    round: i32,
) -> AppResult<()> {
    let Some(r) = recipient(state, user).await? else {
        return Ok(());
    };
    if !(r.email_turn_notifications && r.email_verified) {
        return Ok(());
    }
    let b = base(state);
    state.mailer.send_rendered(
        &r.email,
        email_templates::your_turn(
            &r.username,
            game_name,
            &format!("{power:?}"),
            round,
            &format!("{b}/games/{game}"),
            &format!("{b}/settings"),
        ),
    );
    Ok(())
}

pub async fn email_match_found(
    state: &AppState,
    user: Uuid,
    game_name: &str,
    game: Uuid,
    mode: GameMode,
) -> AppResult<()> {
    let Some(r) = recipient(state, user).await? else {
        return Ok(());
    };
    if !(r.email_match_found && r.email_verified) {
        return Ok(());
    }
    let b = base(state);
    let mode = match mode {
        GameMode::TwoPlayer => "2-player",
        GameMode::FivePlayer => "5-player",
    };
    state.mailer.send_rendered(
        &r.email,
        email_templates::match_found(
            &r.username,
            game_name,
            mode,
            &format!("{b}/games/{game}"),
            &format!("{b}/settings"),
        ),
    );
    Ok(())
}

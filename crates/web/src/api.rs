//! Backend client: every REST call the UI makes lives here (see `crates/server/README.md`).
//! Cookies are sent with `credentials: include`; failures are parsed into [`ApiError`].
use gloo_net::http::{Method, RequestBuilder, Response};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use uuid::Uuid;
use wartide_protocol::auth::*;
use wartide_protocol::bots::BotPersona;
use wartide_protocol::chat::*;
use wartide_protocol::game::*;
use wartide_protocol::group::*;
use wartide_protocol::user::*;
use wartide_protocol::{ApiError, Paginated};
use web_sys::RequestCredentials;

pub type ApiResult<T> = Result<T, ApiError>;

/// Offline escape hatch: when true, `/game/:id` runs the same game screen on a local sandbox
/// state (`wartide_engine::apply` in the browser) instead of `GET /games/{id}/state`. The
/// sandbox is always available at `/sandbox` regardless of this flag.
pub const USE_MOCK_GAME_STATE: bool = false;

fn net_err(e: impl std::fmt::Display) -> ApiError {
    ApiError {
        code: "network".into(),
        message: format!("Network error: {e}"),
    }
}

async fn raw(method: Method, path: &str, body: Option<serde_json::Value>) -> ApiResult<Response> {
    let b = RequestBuilder::new(&format!("/api{path}"))
        .method(method)
        .credentials(RequestCredentials::Include);
    let req = match body {
        Some(v) => b.json(&v).map_err(net_err)?,
        None => b.build().map_err(net_err)?,
    };
    let resp = req.send().await.map_err(net_err)?;
    if resp.ok() {
        return Ok(resp);
    }
    let status = resp.status();
    let text = resp.text().await.unwrap_or_default();
    Err(
        serde_json::from_str::<ApiError>(&text).unwrap_or_else(|_| ApiError {
            code: format!("http_{status}"),
            message: if status == 429 {
                "Too many attempts, please wait a moment.".into()
            } else {
                format!("Request failed ({status})")
            },
        }),
    )
}

fn to_value<B: Serialize>(b: &B) -> serde_json::Value {
    serde_json::to_value(b).unwrap_or(serde_json::Value::Null)
}

async fn json<T: DeserializeOwned>(
    method: Method,
    path: &str,
    body: Option<serde_json::Value>,
) -> ApiResult<T> {
    raw(method, path, body).await?.json().await.map_err(net_err)
}

async fn unit(method: Method, path: &str, body: Option<serde_json::Value>) -> ApiResult<()> {
    raw(method, path, body).await.map(|_| ())
}

fn enc(s: &str) -> String {
    js_sys::encode_uri_component(s).into()
}

fn mode_str(m: GameMode) -> &'static str {
    match m {
        GameMode::TwoPlayer => "two_player",
        GameMode::FivePlayer => "five_player",
        GameMode::Custom => "custom",
    }
}

// ---- auth -------------------------------------------------------------------------------

pub async fn register(username: &str, email: &str, password: &str) -> ApiResult<SessionUser> {
    let body = RegisterRequest {
        email: email.trim().into(),
        username: username.trim().into(),
        password: password.into(),
    };
    json(Method::POST, "/auth/register", Some(to_value(&body))).await
}

pub async fn login(login: &str, password: &str) -> ApiResult<SessionUser> {
    let body = LoginRequest {
        login: login.trim().into(),
        password: password.into(),
    };
    json(Method::POST, "/auth/login", Some(to_value(&body))).await
}

pub async fn logout() -> ApiResult<()> {
    unit(Method::POST, "/auth/logout", None).await
}

pub async fn verify_email(token: &str) -> ApiResult<()> {
    let body = VerifyEmailRequest {
        token: token.into(),
    };
    unit(Method::POST, "/auth/verify-email", Some(to_value(&body))).await
}

pub async fn resend_verification() -> ApiResult<()> {
    unit(Method::POST, "/auth/resend-verification", None).await
}

pub async fn forgot_password(email: &str) -> ApiResult<()> {
    let body = ForgotPasswordRequest {
        email: email.trim().into(),
    };
    unit(Method::POST, "/auth/forgot-password", Some(to_value(&body))).await
}

pub async fn reset_password(token: &str, new_password: &str) -> ApiResult<()> {
    let body = ResetPasswordRequest {
        token: token.into(),
        new_password: new_password.into(),
    };
    unit(Method::POST, "/auth/reset-password", Some(to_value(&body))).await
}

pub async fn change_password(current: &str, new: &str) -> ApiResult<()> {
    let body = ChangePasswordRequest {
        current_password: current.into(),
        new_password: new.into(),
    };
    unit(Method::POST, "/me/password", Some(to_value(&body))).await
}

pub async fn delete_account(password: &str) -> ApiResult<()> {
    let body = DeleteAccountRequest {
        password: password.into(),
    };
    unit(Method::DELETE, "/me", Some(to_value(&body))).await
}

pub async fn bot_personas() -> ApiResult<Vec<BotPersona>> {
    json(Method::GET, "/bots/personas", None).await
}

// ---- me / users -------------------------------------------------------------------------

pub async fn me() -> ApiResult<SessionUser> {
    json(Method::GET, "/me", None).await
}

pub async fn update_profile(req: &UpdateProfileRequest) -> ApiResult<PublicProfile> {
    json(Method::PATCH, "/me/profile", Some(to_value(req))).await
}

pub async fn settings() -> ApiResult<UserSettings> {
    json(Method::GET, "/me/settings", None).await
}

pub async fn update_settings(req: &UpdateSettingsRequest) -> ApiResult<UserSettings> {
    json(Method::PATCH, "/me/settings", Some(to_value(req))).await
}

pub async fn profile(username: &str) -> ApiResult<PublicProfile> {
    json(Method::GET, &format!("/users/{}", enc(username)), None).await
}

pub async fn leaderboard(mode: GameMode) -> ApiResult<Vec<LeaderboardEntry>> {
    json(
        Method::GET,
        &format!("/leaderboard?mode={}", mode_str(mode)),
        None,
    )
    .await
}

// ---- groups -----------------------------------------------------------------------------

pub async fn groups() -> ApiResult<Vec<Group>> {
    json(Method::GET, "/groups", None).await
}

pub async fn group_by_slug(slug: &str) -> ApiResult<Group> {
    json(Method::GET, &format!("/groups/by-slug/{}", enc(slug)), None).await
}

pub async fn create_group(name: &str, description: Option<String>) -> ApiResult<Group> {
    let body = CreateGroupRequest {
        name: name.trim().into(),
        description,
    };
    json(Method::POST, "/groups", Some(to_value(&body))).await
}

pub async fn join_group(invite_code: &str) -> ApiResult<Group> {
    let body = JoinGroupRequest {
        invite_code: invite_code.trim().into(),
    };
    json(Method::POST, "/groups/join", Some(to_value(&body))).await
}

pub async fn leave_group(id: Uuid) -> ApiResult<()> {
    unit(Method::POST, &format!("/groups/{id}/leave"), None).await
}

pub async fn rotate_invite(id: Uuid) -> ApiResult<Group> {
    json(Method::POST, &format!("/groups/{id}/invite/rotate"), None).await
}

pub async fn group_members(id: Uuid) -> ApiResult<Vec<GroupMember>> {
    json(Method::GET, &format!("/groups/{id}/members"), None).await
}

pub async fn set_member_role(id: Uuid, user_id: Uuid, role: GroupRole) -> ApiResult<()> {
    let body = SetMemberRoleRequest { role };
    unit(
        Method::PUT,
        &format!("/groups/{id}/members/{user_id}/role"),
        Some(to_value(&body)),
    )
    .await
}

pub async fn remove_member(id: Uuid, user_id: Uuid) -> ApiResult<()> {
    unit(
        Method::DELETE,
        &format!("/groups/{id}/members/{user_id}"),
        None,
    )
    .await
}

pub async fn group_leaderboard(id: Uuid, mode: GameMode) -> ApiResult<Vec<LeaderboardEntry>> {
    json(
        Method::GET,
        &format!("/groups/{id}/leaderboard?mode={}", mode_str(mode)),
        None,
    )
    .await
}

// ---- games ------------------------------------------------------------------------------

pub async fn games(status: &str) -> ApiResult<Vec<GameSummary>> {
    json(Method::GET, &format!("/games?status={status}"), None).await
}

/// Matchmade games anyone may watch (`status` = `active` | `finished`), 20 per page.
pub async fn public_games(status: &str, page: u32) -> ApiResult<Paginated<GameSummary>> {
    json(
        Method::GET,
        &format!("/games/public?status={status}&page={page}"),
        None,
    )
    .await
}

pub async fn game(id: GameId) -> ApiResult<GameSummary> {
    json(Method::GET, &format!("/games/{id}"), None).await
}

pub async fn game_state(id: GameId) -> ApiResult<GameStateResponse> {
    json(Method::GET, &format!("/games/{id}/state"), None).await
}

pub async fn game_actions(id: GameId, since: i64) -> ApiResult<Vec<GameActionEnvelope>> {
    json(
        Method::GET,
        &format!("/games/{id}/actions?since={since}"),
        None,
    )
    .await
}

pub async fn submit_action(
    id: GameId,
    action: wartide_engine::Action,
) -> ApiResult<GameActionEnvelope> {
    let body = SubmitActionRequest { action };
    json(
        Method::POST,
        &format!("/games/{id}/actions"),
        Some(to_value(&body)),
    )
    .await
}

pub async fn create_game(req: &CreatePrivateGameRequest) -> ApiResult<GameSummary> {
    json(Method::POST, "/games", Some(to_value(req))).await
}

pub async fn join_game(code: &str) -> ApiResult<GameSummary> {
    let body = JoinByInviteRequest {
        code: code.trim().into(),
    };
    json(Method::POST, "/games/join", Some(to_value(&body))).await
}

pub async fn start_game(id: GameId) -> ApiResult<GameSummary> {
    json(Method::POST, &format!("/games/{id}/start"), None).await
}

pub async fn resign_game(id: GameId) -> ApiResult<GameSummary> {
    json(Method::POST, &format!("/games/{id}/resign"), None).await
}

pub async fn abandon_game(id: GameId) -> ApiResult<GameSummary> {
    json(Method::POST, &format!("/games/{id}/abandon"), None).await
}

// ---- chat -------------------------------------------------------------------------------

pub async fn chat(id: GameId) -> ApiResult<Vec<ChatMessage>> {
    json(Method::GET, &format!("/games/{id}/chat"), None).await
}

pub async fn send_chat(id: GameId, req: &SendChatRequest) -> ApiResult<ChatMessage> {
    json(
        Method::POST,
        &format!("/games/{id}/chat"),
        Some(to_value(req)),
    )
    .await
}

// ---- matchmaking ------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct QueueStatus {
    pub queued: bool,
    #[serde(default)]
    pub mode: Option<GameMode>,
}

pub async fn queue_join(mode: GameMode) -> ApiResult<()> {
    let body = MatchmakingRequest { mode };
    unit(Method::POST, "/matchmaking/queue", Some(to_value(&body))).await
}

pub async fn queue_leave() -> ApiResult<()> {
    unit(Method::DELETE, "/matchmaking/queue", None).await
}

pub async fn queue_status() -> ApiResult<QueueStatus> {
    json(Method::GET, "/matchmaking/queue", None).await
}

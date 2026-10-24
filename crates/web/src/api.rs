//! Backend client. Everything the UI needs from the server goes through here so the switch to
//! `wartide-protocol` DTOs (and real endpoints) is confined to this file.
//! While `USE_MOCK` is true, functions return canned data from `crate::mock`.
use crate::mock::{self, *};
use futures::StreamExt;
use gloo_net::http::Request;
use gloo_net::websocket::{futures::WebSocket, Message};
use serde::{de::DeserializeOwned, Serialize};

// TODO(protocol): flip to false once the server and `wartide-protocol` are wired in.
pub const USE_MOCK: bool = true;

#[allow(dead_code)]
async fn get_json<T: DeserializeOwned>(path: &str) -> Result<T, String> {
    let resp = Request::get(path).send().await.map_err(|e| e.to_string())?;
    if !resp.ok() {
        return Err(format!("{} {}", resp.status(), resp.status_text()));
    }
    resp.json().await.map_err(|e| e.to_string())
}

#[allow(dead_code)]
async fn post_json<B: Serialize, T: DeserializeOwned>(path: &str, body: &B) -> Result<T, String> {
    let req = Request::post(path).json(body).map_err(|e| e.to_string())?;
    let resp = req.send().await.map_err(|e| e.to_string())?;
    if !resp.ok() {
        return Err(resp.text().await.unwrap_or_else(|_| resp.status_text()));
    }
    resp.json().await.map_err(|e| e.to_string())
}

// ---- auth -------------------------------------------------------------------------------

pub async fn login(username: &str, password: &str) -> Result<User, String> {
    if username.trim().is_empty() || password.is_empty() {
        return Err("Enter a username and password".into());
    }
    // TODO(protocol): POST /api/auth/login
    Ok(user_named(username.trim()))
}

pub async fn register(username: &str, email: &str, password: &str) -> Result<(), String> {
    if username.trim().len() < 3 || !email.contains('@') || password.len() < 8 {
        return Err("Username 3+ chars, valid email, password 8+ chars".into());
    }
    // TODO(protocol): POST /api/auth/register
    Ok(())
}

pub async fn verify_email(token: &str) -> Result<(), String> {
    // TODO(protocol): POST /api/auth/verify
    if token.is_empty() {
        Err("Missing token".into())
    } else {
        Ok(())
    }
}

pub async fn forgot_password(email: &str) -> Result<(), String> {
    // TODO(protocol): POST /api/auth/forgot
    if email.contains('@') {
        Ok(())
    } else {
        Err("Enter a valid email".into())
    }
}

pub async fn reset_password(token: &str, password: &str) -> Result<(), String> {
    // TODO(protocol): POST /api/auth/reset
    if token.is_empty() || password.len() < 8 {
        Err("Token and 8+ char password required".into())
    } else {
        Ok(())
    }
}

// ---- lobby ------------------------------------------------------------------------------

pub async fn my_games() -> Result<Vec<GameSummary>, String> {
    // TODO(protocol): GET /api/games?mine=1
    Ok(mock::my_games())
}

pub async fn create_game(g: &NewGame) -> Result<String, String> {
    // TODO(protocol): POST /api/games
    let _ = g;
    Ok("g1".into())
}

pub async fn join_by_code(code: &str) -> Result<String, String> {
    // TODO(protocol): POST /api/games/join {code}
    if code.trim().len() < 4 {
        Err("Invite codes are at least 4 characters".into())
    } else {
        Ok("g3".into())
    }
}

/// Join (`true`) or leave (`false`) the public matchmaking queue for `players` (2 or 5).
pub async fn queue(players: u8, join: bool) -> Result<String, String> {
    // TODO(protocol): POST/DELETE /api/matchmaking
    Ok(if join {
        format!("Queued for {players}-player: 3 waiting")
    } else {
        "Left queue".into()
    })
}

// ---- game -------------------------------------------------------------------------------

pub async fn game(id: &str) -> Result<GameDetail, String> {
    // TODO(protocol): GET /api/games/{id} -> GameState + seats
    Ok(mock::game_detail(id))
}

pub async fn submit_action(game_id: &str, summary: &str) -> Result<(), String> {
    // TODO(protocol): POST /api/games/{id}/actions {action}
    let _ = (game_id, summary);
    Ok(())
}

/// Subscribe to server pushes for a game. TODO(protocol): decode `ServerMsg` frames.
pub fn subscribe(game_id: &str, on_text: impl Fn(String) + 'static) {
    if USE_MOCK {
        return;
    }
    let proto = if web_sys::window()
        .and_then(|w| w.location().protocol().ok())
        .as_deref()
        == Some("https:")
    {
        "wss"
    } else {
        "ws"
    };
    let host = web_sys::window()
        .and_then(|w| w.location().host().ok())
        .unwrap_or_default();
    let url = format!("{proto}://{host}/ws/games/{game_id}");
    wasm_bindgen_futures::spawn_local(async move {
        let Ok(ws) = WebSocket::open(&url) else {
            return;
        };
        let (_tx, mut rx) = ws.split();
        while let Some(Ok(Message::Text(t))) = rx.next().await {
            on_text(t);
        }
    });
}

// ---- social -----------------------------------------------------------------------------

pub async fn profile(name: &str) -> Result<Profile, String> {
    // TODO(protocol): GET /api/users/{name}
    Ok(mock::profile(name))
}

pub async fn leaderboard(ladder: &str) -> Result<Vec<LeaderRow>, String> {
    // TODO(protocol): GET /api/ladders/{ladder}
    Ok(mock::leaderboard(ladder))
}

pub async fn groups() -> Result<Vec<Group>, String> {
    // TODO(protocol): GET /api/groups
    Ok(mock::groups())
}

pub async fn group(slug: &str) -> Result<Group, String> {
    // TODO(protocol): GET /api/groups/{slug}
    mock::groups()
        .into_iter()
        .find(|g| g.slug == slug)
        .ok_or_else(|| "Group not found".into())
}

pub async fn create_group(name: &str) -> Result<String, String> {
    // TODO(protocol): POST /api/groups
    if name.trim().is_empty() {
        Err("Name required".into())
    } else {
        Ok(name.trim().to_lowercase().replace(' ', "-"))
    }
}

pub async fn save_settings(_user: &User, _email_notifications: bool) -> Result<(), String> {
    // TODO(protocol): PUT /api/me
    Ok(())
}

pub async fn change_password(old: &str, new: &str) -> Result<(), String> {
    // TODO(protocol): POST /api/me/password
    if old.is_empty() || new.len() < 8 {
        Err("New password must be 8+ characters".into())
    } else {
        Ok(())
    }
}

//! Integration tests against a real Postgres. Skipped (with a notice) when DATABASE_URL is unset.
use std::sync::{Arc, Mutex};

use axum::body::Body;
use axum::http::{header, Method, Request, StatusCode};
use axum::Router;
use serde_json::{json, Value};
use tower::ServiceExt;
use uuid::Uuid;
use wartide_server::email::{Email, Mailer};
use wartide_server::{build_router, build_state, connect_db, AppState, Config, MIGRATOR};

struct Ctx {
    app: Router,
    state: AppState,
    outbox: Arc<Mutex<Vec<Email>>>,
}

async fn setup() -> Option<Ctx> {
    setup_with(false).await
}

async fn setup_with(rate_limit: bool) -> Option<Ctx> {
    let _ = dotenvy::dotenv();
    let Ok(url) = std::env::var("DATABASE_URL") else {
        eprintln!("NOTICE: DATABASE_URL not set; skipping DB integration test");
        return None;
    };
    let db = connect_db(&url).await.expect("connect");
    MIGRATOR.run(&db).await.expect("migrate");
    let (mailer, outbox) = Mailer::capture();
    let mut cfg = Config::for_test(url);
    cfg.rate_limit = rate_limit;
    let state = build_state(db, cfg, mailer);
    Some(Ctx {
        app: build_router(state.clone()),
        state,
        outbox,
    })
}

async fn call(
    app: &Router,
    method: Method,
    uri: &str,
    cookie: Option<&str>,
    body: Option<Value>,
) -> (StatusCode, Value, Option<String>) {
    let mut b = Request::builder().method(method).uri(uri);
    if let Some(c) = cookie {
        b = b.header(header::COOKIE, format!("wt_session={c}"));
    }
    let req = if let Some(body) = body {
        b.header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(body.to_string()))
            .unwrap()
    } else {
        b.body(Body::empty()).unwrap()
    };
    let res = app.clone().oneshot(req).await.unwrap();
    let status = res.status();
    let set_cookie = res
        .headers()
        .get(header::SET_COOKIE)
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);
    let bytes = axum::body::to_bytes(res.into_body(), 1 << 20)
        .await
        .unwrap();
    let json = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, json, set_cookie)
}

/// Like [`call`] with extra request headers.
async fn call_h(
    app: &Router,
    method: Method,
    uri: &str,
    cookie: Option<&str>,
    body: Option<Value>,
    headers: &[(&str, &str)],
) -> (StatusCode, Value, Option<String>) {
    let mut b = Request::builder().method(method).uri(uri);
    if let Some(c) = cookie {
        b = b.header(header::COOKIE, format!("wt_session={c}"));
    }
    for (k, v) in headers {
        b = b.header(*k, *v);
    }
    let req = if let Some(body) = body {
        b.header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(body.to_string()))
            .unwrap()
    } else {
        b.body(Body::empty()).unwrap()
    };
    let res = app.clone().oneshot(req).await.unwrap();
    let status = res.status();
    let set_cookie = res
        .headers()
        .get(header::SET_COOKIE)
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);
    let bytes = axum::body::to_bytes(res.into_body(), 1 << 20)
        .await
        .unwrap();
    let json = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, json, set_cookie)
}

/// A second app over the same database with rate limiting on (and its own mail outbox).
fn limited(c: &Ctx, tweak: impl FnOnce(&mut Config)) -> Ctx {
    let (mailer, outbox) = Mailer::capture();
    let mut cfg = Config::for_test(String::new());
    cfg.rate_limit = true;
    tweak(&mut cfg);
    let state = build_state(c.state.db.clone(), cfg, mailer);
    Ctx {
        app: build_router(state.clone()),
        state,
        outbox,
    }
}

fn token_from_mail(outbox: &Arc<Mutex<Vec<Email>>>, to: &str, marker: &str) -> String {
    let o = outbox.lock().unwrap();
    let mail = o
        .iter()
        .rev()
        .find(|m| m.to == to && m.body.contains(marker))
        .expect("mail");
    let start = mail.body.find("token=").unwrap() + 6;
    mail.body[start..]
        .split_whitespace()
        .next()
        .unwrap()
        .to_string()
}

struct TestUser {
    cookie: String,
    id: Uuid,
    username: String,
}

async fn register(c: &Ctx, verify: bool) -> TestUser {
    let tag = Uuid::new_v4().simple().to_string();
    let username = format!("u{}", &tag[..10]);
    let email = format!("{username}@example.com");
    let (st, body, _) = call(
        &c.app,
        Method::POST,
        "/api/auth/register",
        None,
        Some(json!({"email": email, "username": username, "password": "hunter2hunter2"})),
    )
    .await;
    assert_eq!(st, StatusCode::CREATED, "{body}");
    let id: Uuid = body["id"].as_str().unwrap().parse().unwrap();
    if verify {
        let t = token_from_mail(&c.outbox, &email, "/verify?token=");
        let (st, _, _) = call(
            &c.app,
            Method::POST,
            "/api/auth/verify-email",
            None,
            Some(json!({"token": t})),
        )
        .await;
        assert_eq!(st, StatusCode::NO_CONTENT);
    }
    let (st, _, cookie) = call(
        &c.app,
        Method::POST,
        "/api/auth/login",
        None,
        Some(json!({"login": email, "password": "hunter2hunter2"})),
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    let cookie = cookie.unwrap();
    assert!(cookie.contains("HttpOnly") && cookie.contains("SameSite=Lax"));
    let token = cookie
        .split(';')
        .next()
        .unwrap()
        .trim_start_matches("wt_session=")
        .to_string();
    TestUser {
        cookie: token,
        id,
        username,
    }
}

/// Wait until no bot driver task runs for the game.
async fn wait_idle(c: &Ctx, id: Uuid) {
    for _ in 0..1200 {
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        if !c.state.runtime.drivers.lock().unwrap().contains_key(&id) {
            return;
        }
    }
    panic!("bot driver never finished");
}

/// End phases for `u` until `power` is the current power.
async fn pass_turn(c: &Ctx, u: &TestUser, game: &str, power: &str) {
    for _ in 0..40 {
        let (_, sum, _) = call(
            &c.app,
            Method::GET,
            &format!("/api/games/{game}"),
            Some(&u.cookie),
            None,
        )
        .await;
        if sum["current_power"] == power {
            return;
        }
        let (st, env, _) = call(
            &c.app,
            Method::POST,
            &format!("/api/games/{game}/actions"),
            Some(&u.cookie),
            Some(json!({"action": {"type": "end_phase"}})),
        )
        .await;
        assert_eq!(st, StatusCode::OK, "{env}");
    }
    panic!("never reached {power}");
}

#[tokio::test]
async fn auth_flow_and_unverified_gate() {
    let Some(c) = setup().await else { return };
    let u = register(&c, false).await;
    let (st, me, _) = call(&c.app, Method::GET, "/api/me", Some(&u.cookie), None).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(me["email_verified"], false);

    let (st, err, _) = call(
        &c.app,
        Method::POST,
        "/api/games",
        Some(&u.cookie),
        Some(json!({"mode": "two_player", "bot_seats": [1]})),
    )
    .await;
    assert_eq!(st, StatusCode::FORBIDDEN);
    assert_eq!(err["code"], "email_unverified");

    // no cookie
    let (st, err, _) = call(&c.app, Method::GET, "/api/me", None, None).await;
    assert_eq!(st, StatusCode::UNAUTHORIZED);
    assert_eq!(err["code"], "unauthorized");

    // wrong password
    let (st, err, _) = call(
        &c.app,
        Method::POST,
        "/api/auth/login",
        None,
        Some(json!({"login": u.username, "password": "nope-nope-nope"})),
    )
    .await;
    assert_eq!(st, StatusCode::BAD_REQUEST);
    assert_eq!(err["code"], "invalid_credentials");

    // password reset
    let email = format!("{}@example.com", u.username);
    let (st, _, _) = call(
        &c.app,
        Method::POST,
        "/api/auth/forgot-password",
        None,
        Some(json!({"email": email})),
    )
    .await;
    assert_eq!(st, StatusCode::NO_CONTENT);
    let t = token_from_mail(&c.outbox, &email, "/reset-password?token=");
    let (st, _, _) = call(
        &c.app,
        Method::POST,
        "/api/auth/reset-password",
        None,
        Some(json!({"token": t, "new_password": "brand-new-pass"})),
    )
    .await;
    assert_eq!(st, StatusCode::NO_CONTENT);
    // old session is gone, new password works
    let (st, _, _) = call(&c.app, Method::GET, "/api/me", Some(&u.cookie), None).await;
    assert_eq!(st, StatusCode::UNAUTHORIZED);
    let (st, _, cookie) = call(
        &c.app,
        Method::POST,
        "/api/auth/login",
        None,
        Some(json!({"login": email, "password": "brand-new-pass"})),
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    let tok = cookie
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .trim_start_matches("wt_session=")
        .to_string();
    let (st, _, _) = call(&c.app, Method::POST, "/api/auth/logout", Some(&tok), None).await;
    assert_eq!(st, StatusCode::NO_CONTENT);
    let (st, _, _) = call(&c.app, Method::GET, "/api/me", Some(&tok), None).await;
    assert_eq!(st, StatusCode::UNAUTHORIZED);

    // unknown api path is JSON 404
    let (st, err, _) = call(&c.app, Method::GET, "/api/nope", None, None).await;
    assert_eq!(st, StatusCode::NOT_FOUND);
    assert_eq!(err["code"], "not_found");
    let (st, _, _) = call(&c.app, Method::GET, "/healthz", None, None).await;
    assert_eq!(st, StatusCode::OK);
}

#[tokio::test]
async fn group_game_chat_and_ratings() {
    let Some(c) = setup().await else { return };
    let a = register(&c, true).await;
    let b = register(&c, true).await;

    // profile + settings
    let (st, p, _) = call(
        &c.app,
        Method::PATCH,
        "/api/me/profile",
        Some(&a.cookie),
        Some(json!({"bio": "hi", "country": "NZ"})),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{p}");
    assert_eq!(p["bio"], "hi");
    let (st, s, _) = call(
        &c.app,
        Method::PATCH,
        "/api/me/settings",
        Some(&a.cookie),
        Some(json!({"email_turn_notifications": false})),
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(s["email_turn_notifications"], false);
    let (st, _, _) = call(
        &c.app,
        Method::GET,
        &format!("/api/users/{}", a.username),
        None,
        None,
    )
    .await;
    assert_eq!(st, StatusCode::OK);

    // group
    let (st, g, _) = call(
        &c.app,
        Method::POST,
        "/api/groups",
        Some(&a.cookie),
        Some(json!({"name": "Test Club", "description": "x"})),
    )
    .await;
    assert_eq!(st, StatusCode::CREATED, "{g}");
    let gid = g["id"].as_str().unwrap().to_string();
    let code = g["invite_code"].as_str().unwrap().to_string();
    let (st, g2, _) = call(
        &c.app,
        Method::POST,
        "/api/groups/join",
        Some(&b.cookie),
        Some(json!({"invite_code": code})),
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(
        g2["invite_code"],
        Value::Null,
        "plain members must not see the invite code"
    );
    assert_eq!(g2["member_count"], 2);
    let (st, _, _) = call(
        &c.app,
        Method::POST,
        &format!("/api/groups/{gid}/invite/rotate"),
        Some(&b.cookie),
        None,
    )
    .await;
    assert_eq!(st, StatusCode::FORBIDDEN);
    let (st, g3, _) = call(
        &c.app,
        Method::POST,
        &format!("/api/groups/{gid}/invite/rotate"),
        Some(&a.cookie),
        None,
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    assert_ne!(g3["invite_code"].as_str().unwrap(), code);
    let (_, mine, _) = call(&c.app, Method::GET, "/api/groups", Some(&b.cookie), None).await;
    assert_eq!(mine.as_array().unwrap().len(), 1);

    // group game, 2 humans
    let (st, game, _) = call(
        &c.app,
        Method::POST,
        "/api/games",
        Some(&a.cookie),
        Some(json!({"mode": "two_player", "bot_seats": [], "group_id": gid})),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{game}");
    assert_eq!(game["status"], "lobby");
    let gameid = game["id"].as_str().unwrap().to_string();
    let invite = game["invite_code"].as_str().unwrap().to_string();
    let (st, joined, _) = call(
        &c.app,
        Method::POST,
        "/api/games/join",
        Some(&b.cookie),
        Some(json!({"code": invite})),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{joined}");
    assert_eq!(joined["status"], "in_progress");
    assert_eq!(joined["current_power"], "ussr");
    assert_eq!(joined["my_seat"], 1);

    // a non-member cannot join a group game
    let outsider = register(&c, true).await;
    let (st, _, _) = call(
        &c.app,
        Method::GET,
        &format!("/api/games/{gameid}"),
        Some(&outsider.cookie),
        None,
    )
    .await;
    assert_eq!(st, StatusCode::FORBIDDEN);

    // chat: all vs team
    let chat = format!("/api/games/{gameid}/chat");
    let (st, _, _) = call(
        &c.app,
        Method::POST,
        &chat,
        Some(&a.cookie),
        Some(json!({"channel": "all", "body": "good luck"})),
    )
    .await;
    assert_eq!(st, StatusCode::CREATED);
    let (st, _, _) = call(
        &c.app,
        Method::POST,
        &chat,
        Some(&a.cookie),
        Some(json!({"channel": "team", "body": "axis secret"})),
    )
    .await;
    assert_eq!(st, StatusCode::CREATED);
    let (_, ha, _) = call(&c.app, Method::GET, &chat, Some(&a.cookie), None).await;
    let (_, hb, _) = call(&c.app, Method::GET, &chat, Some(&b.cookie), None).await;
    assert_eq!(ha.as_array().unwrap().len(), 2);
    assert_eq!(hb.as_array().unwrap().len(), 1);
    assert_eq!(hb[0]["body"], "good luck");
    let (st, _, _) = call(&c.app, Method::GET, &chat, Some(&outsider.cookie), None).await;
    assert_eq!(st, StatusCode::FORBIDDEN);

    // bot message helper
    let gid_uuid: Uuid = gameid.parse().unwrap();
    wartide_server::services::chat::post_bot_message(
        &c.state,
        gid_uuid,
        "General Frost",
        wartide_protocol::chat::ChatChannel::All,
        None,
        "Hmm.",
    )
    .await
    .unwrap();
    let (_, hb, _) = call(
        &c.app,
        Method::GET,
        &format!("{chat}?channel=all"),
        Some(&b.cookie),
        None,
    )
    .await;
    assert_eq!(hb.as_array().unwrap().len(), 2);
    assert_eq!(hb[1]["sender"]["kind"], "bot");

    // actions: turn ownership. USSR (allies, b) moves first.
    let actions = format!("/api/games/{gameid}/actions");
    let end_phase = json!({"action": {"type": "end_phase"}});
    let (st, err, _) = call(
        &c.app,
        Method::POST,
        &actions,
        Some(&a.cookie),
        Some(end_phase.clone()),
    )
    .await;
    assert_eq!(st, StatusCode::CONFLICT);
    assert_eq!(err["code"], "not_your_turn");
    // garbage and illegal actions are 400 illegal_action with the engine's message
    let (st, err, _) = call(
        &c.app,
        Method::POST,
        &actions,
        Some(&b.cookie),
        Some(json!({"action": {"type": "noop"}})),
    )
    .await;
    assert_eq!(st, StatusCode::BAD_REQUEST);
    assert_eq!(err["code"], "illegal_action");
    let (st, env, _) = call(
        &c.app,
        Method::POST,
        &actions,
        Some(&b.cookie),
        Some(end_phase.clone()),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{env}");
    assert_eq!(env["seq"], 1);
    assert!(env["events"].is_array());
    let (_, sum, _) = call(
        &c.app,
        Method::GET,
        &format!("/api/games/{gameid}"),
        Some(&a.cookie),
        None,
    )
    .await;
    assert_eq!(sum["phase"], "purchase");
    assert_eq!(sum["pending_power"], "ussr");
    // b ends the rest of the USSR turn; Germany (a) is next
    pass_turn(&c, &b, &gameid, "germany").await;
    let (_, log, _) = call(
        &c.app,
        Method::GET,
        &format!("{actions}?since=1"),
        Some(&a.cookie),
        None,
    )
    .await;
    assert!(!log.as_array().unwrap().is_empty());
    let (_, stv, _) = call(
        &c.app,
        Method::GET,
        &format!("/api/games/{gameid}/state"),
        Some(&a.cookie),
        None,
    )
    .await;
    assert_eq!(stv["state"]["current_power"], "germany");
    pass_turn(&c, &a, &gameid, "uk").await;

    // active list
    let (_, active, _) = call(&c.app, Method::GET, "/api/games", Some(&a.cookie), None).await;
    assert_eq!(active.as_array().unwrap().len(), 1);

    // a resigns -> b (allies) wins, group ladder updated; public ladder untouched
    let (st, done, _) = call(
        &c.app,
        Method::POST,
        &format!("/api/games/{gameid}/resign"),
        Some(&a.cookie),
        None,
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{done}");
    // the game goes on with a computer player on a's seat; a counts as resigned
    assert_eq!(done["status"], "in_progress");
    assert_eq!(done["seats"][0]["resigned"], true);
    assert_eq!(done["seats"][0]["is_bot"], true);
    // b's team wins in the end; a still loses even if a's own team had won
    wartide_server::services::games::finish_game(
        &c.state,
        gid_uuid,
        Some(wartide_protocol::game::Team::Allies),
    )
    .await
    .unwrap();
    let (st, lb, _) = call(
        &c.app,
        Method::GET,
        &format!("/api/groups/{gid}/leaderboard?mode=two_player"),
        Some(&a.cookie),
        None,
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    let lb = lb.as_array().unwrap();
    assert_eq!(lb.len(), 2);
    assert_eq!(lb[0]["user"]["id"], json!(b.id));
    assert!(lb[0]["rating"].as_f64().unwrap() > 1500.0);
    assert!(lb[1]["rating"].as_f64().unwrap() < 1500.0);
    let (_, prof, _) = call(
        &c.app,
        Method::GET,
        &format!("/api/users/{}", b.username),
        None,
        None,
    )
    .await;
    assert_eq!(prof["stats"].as_array().unwrap().len(), 0);
    let (_, fin, _) = call(
        &c.app,
        Method::GET,
        "/api/games?status=finished",
        Some(&a.cookie),
        None,
    )
    .await;
    assert_eq!(fin.as_array().unwrap().len(), 1);

    // private non-group games never rate
    let (_, pg, _) = call(
        &c.app,
        Method::POST,
        "/api/games",
        Some(&a.cookie),
        Some(json!({"mode": "five_player", "bot_seats": [1,2,3,4]})),
    )
    .await;
    assert_eq!(pg["status"], "in_progress");
    let pid = pg["id"].as_str().unwrap();
    let (st, _, _) = call(
        &c.app,
        Method::POST,
        &format!("/api/games/{pid}/resign"),
        Some(&a.cookie),
        None,
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    let (n,): (i64,) = sqlx::query_as("SELECT count(*) FROM ratings WHERE user_id=$1")
        .bind(a.id)
        .fetch_one(&c.state.db)
        .await
        .unwrap();
    assert_eq!(n, 1, "only the group ladder row");
}

#[tokio::test]
async fn lobby_start_with_bots_and_timeouts() {
    let Some(c) = setup().await else { return };
    let a = register(&c, true).await;
    let (st, game, _) = call(&c.app, Method::POST, "/api/games", Some(&a.cookie),
        Some(json!({"mode": "five_player", "bot_seats": [3], "settings": {"turn_timeout_secs": 3600}}))).await;
    assert_eq!(st, StatusCode::OK, "{game}");
    assert_eq!(game["status"], "lobby");
    let id = game["id"].as_str().unwrap().to_string();
    let (st, started, _) = call(
        &c.app,
        Method::POST,
        &format!("/api/games/{id}/start"),
        Some(&a.cookie),
        None,
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{started}");
    assert_eq!(started["status"], "in_progress");
    assert!(started["turn_deadline"].is_string());
    assert!(started["seats"]
        .as_array()
        .unwrap()
        .iter()
        .all(|s| s["user"] != Value::Null || s["is_bot"] == true));

    // expire the deadline and sweep
    let uid: Uuid = id.parse().unwrap();
    sqlx::query("UPDATE games SET turn_deadline = now() - interval '1 minute' WHERE id=$1")
        .bind(uid)
        .execute(&c.state.db)
        .await
        .unwrap();
    let n = wartide_server::services::timeouts::sweep(&c.state)
        .await
        .unwrap();
    assert!(n >= 1);
    // a computer player takes over USSR and then the bot seats play on until the human is up again
    wait_idle(&c, uid).await;
    let (_, sum, _) = call(
        &c.app,
        Method::GET,
        &format!("/api/games/{id}"),
        Some(&a.cookie),
        None,
    )
    .await;
    assert_eq!(sum["pending_power"], "ussr");
    let n: i64 = sqlx::query_scalar("SELECT count(*) FROM game_actions WHERE game_id=$1")
        .bind(uid)
        .fetch_one(&c.state.db)
        .await
        .unwrap();
    assert!(n > 3, "bots played, got {n} actions");
    assert_eq!(sum["seats"][0]["timeouts"], 1);
    assert_eq!(sum["seats"][0]["resigned"], false);
    let (_, chat, _) = call(
        &c.app,
        Method::GET,
        &format!("/api/games/{id}/chat"),
        Some(&a.cookie),
        None,
    )
    .await;
    assert!(chat
        .as_array()
        .unwrap()
        .iter()
        .any(|m| m["body"].as_str().unwrap().starts_with("timeout")));

    // two more misses in a row: the seat is handed to a computer player for good
    for i in 0..2 {
        sqlx::query("UPDATE games SET turn_deadline = now() - interval '1 minute' WHERE id=$1")
            .bind(uid)
            .execute(&c.state.db)
            .await
            .unwrap();
        wartide_server::services::timeouts::sweep(&c.state)
            .await
            .unwrap();
        if i == 0 {
            wait_idle(&c, uid).await;
        } // after the third miss only bots remain and would play the whole game
    }
    let (_, sum, _) = call(
        &c.app,
        Method::GET,
        &format!("/api/games/{id}"),
        Some(&a.cookie),
        None,
    )
    .await;
    assert_eq!(sum["seats"][0]["timeouts"], 3);
    assert_eq!(sum["seats"][0]["resigned"], true);
    assert_eq!(sum["seats"][0]["is_bot"], true);

    // abandon
    let (st, ab, _) = call(
        &c.app,
        Method::POST,
        &format!("/api/games/{id}/abandon"),
        Some(&a.cookie),
        None,
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(ab["status"], "abandoned");
    wait_idle(&c, uid).await; // the driver stops once the game is no longer in progress
}

#[tokio::test]
async fn matchmaking_creates_public_game() {
    let Some(c) = setup().await else { return };
    // clean slate so leftovers from other runs can't be paired in
    sqlx::query("DELETE FROM matchmaking_queue")
        .execute(&c.state.db)
        .await
        .unwrap();
    let a = register(&c, true).await;
    let b = register(&c, true).await;
    let mut rx = c.state.hub.subscribe_user(a.id);
    // a opts out of match-found emails
    let (st, settings, _) = call(
        &c.app,
        Method::PATCH,
        "/api/me/settings",
        Some(&a.cookie),
        Some(json!({"email_match_found": false})),
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(settings["email_match_found"], false);
    assert_eq!(settings["email_turn_notifications"], true);
    for u in [&a, &b] {
        let (st, _, _) = call(
            &c.app,
            Method::POST,
            "/api/matchmaking/queue",
            Some(&u.cookie),
            Some(json!({"mode": "two_player"})),
        )
        .await;
        assert_eq!(st, StatusCode::ACCEPTED);
    }
    let (st, err, _) = call(
        &c.app,
        Method::POST,
        "/api/matchmaking/queue",
        Some(&a.cookie),
        Some(json!({"mode": "two_player"})),
    )
    .await;
    assert_eq!(
        (st, err["code"].as_str()),
        (StatusCode::CONFLICT, Some("already_queued"))
    );
    let made = wartide_server::services::matchmaking::run_once(&c.state)
        .await
        .unwrap();
    assert_eq!(made.len(), 1);
    let mut found = false;
    while let Ok(msg) = rx.try_recv() {
        found |= matches!(msg, wartide_protocol::ws::ServerMessage::MatchFound { .. });
    }
    assert!(found, "MatchFound should be delivered on the user channel");
    let (_, game, _) = call(
        &c.app,
        Method::GET,
        &format!("/api/games/{}", made[0]),
        Some(&a.cookie),
        None,
    )
    .await;
    assert_eq!(game["kind"], "public");
    assert_eq!(game["status"], "in_progress");
    assert!(
        game["name"].as_str().unwrap().starts_with("Ranked 2p #"),
        "{game}"
    );
    {
        let o = c.outbox.lock().unwrap();
        let found = |u: &TestUser| {
            o.iter().any(|m| {
                m.to == format!("{}@example.com", u.username)
                    && m.subject == "Your Wartide match is ready"
                    && m.html.is_some()
            })
        };
        assert!(found(&b), "b should get a match-found email");
        assert!(!found(&a), "a opted out of match-found emails");
    }
    let (_, q, _) = call(
        &c.app,
        Method::GET,
        "/api/matchmaking/queue",
        Some(&a.cookie),
        None,
    )
    .await;
    assert_eq!(q["queued"], false);

    // public result updates the public ladder
    let (st, _, _) = call(
        &c.app,
        Method::POST,
        &format!("/api/games/{}/resign", made[0]),
        Some(&a.cookie),
        None,
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    // b is still playing, so the game goes on; end it with the other team winning
    let (_, g, _) = call(
        &c.app,
        Method::GET,
        &format!("/api/games/{}", made[0]),
        Some(&a.cookie),
        None,
    )
    .await;
    assert_eq!(g["status"], "in_progress");
    let a_axis = g["seats"][g["my_seat"].as_u64().unwrap() as usize]["powers"][0] == "germany";
    wartide_server::services::games::finish_game(
        &c.state,
        made[0],
        Some(if a_axis {
            wartide_protocol::game::Team::Allies
        } else {
            wartide_protocol::game::Team::Axis
        }),
    )
    .await
    .unwrap();
    let (_, prof, _) = call(
        &c.app,
        Method::GET,
        &format!("/api/users/{}", a.username),
        None,
        None,
    )
    .await;
    assert_eq!(prof["stats"][0]["ladder"], "public:2p");
    assert_eq!(prof["stats"][0]["losses"], 1);
    assert_eq!(prof["stats"][0]["current_win_streak"], 0);
    let games = prof["recent_games"].as_array().unwrap();
    assert_eq!(games.len(), 1);
    assert_eq!(games[0]["status"], "finished");
    assert!(games[0]["winner"].is_string());
    let (_, prof_b, _) = call(
        &c.app,
        Method::GET,
        &format!("/api/users/{}", b.username),
        None,
        None,
    )
    .await;
    assert_eq!(prof_b["stats"][0]["wins"], 1);
    assert_eq!(prof_b["stats"][0]["current_win_streak"], 1);
    assert_eq!(prof_b["stats"][0]["best_win_streak"], 1);
}

async fn create_solo_game(c: &Ctx, u: &TestUser, extra: Value) -> Value {
    let mut body = json!({"mode": "five_player", "bot_seats": [1, 2, 3, 4]});
    for (k, v) in extra.as_object().unwrap() {
        body[k] = v.clone();
    }
    let (st, game, _) = call(
        &c.app,
        Method::POST,
        "/api/games",
        Some(&u.cookie),
        Some(body),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{game}");
    game
}

#[tokio::test]
async fn names_personas_and_turn_emails() {
    let Some(c) = setup().await else { return };
    let a = register(&c, true).await;
    let (st, personas, _) = call(&c.app, Method::GET, "/api/bots/personas", None, None).await;
    assert_eq!(st, StatusCode::OK);
    let personas = personas.as_array().unwrap();
    assert_eq!(personas.len(), 6);
    assert!(personas.iter().all(|p| p["id"].is_string()
        && p["name"].is_string()
        && p["blurb"].is_string()
        && ["aggressive", "balanced", "cautious"].contains(&p["style"].as_str().unwrap())
        && ["chatty", "normal", "quiet"].contains(&p["verbosity"].as_str().unwrap())));

    let g = create_solo_game(
        &c,
        &a,
        json!({"name": "  Eastern   Front ", "bot_personas": [null, "thistle", null, "frost"]}),
    )
    .await;
    assert_eq!(g["name"], "Eastern Front");
    let seats = g["seats"].as_array().unwrap();
    assert_eq!(seats[1]["bot_persona"], "thistle");
    assert_eq!(seats[1]["bot_name"], "Major Thistle");
    assert_eq!(seats[3]["bot_persona"], "frost");
    assert!(seats[2]["bot_persona"].is_string());
    assert!(seats[0]["bot_persona"].is_null());

    let g2 = create_solo_game(&c, &a, json!({})).await;
    assert!(g2["name"].as_str().unwrap().starts_with("Private game #"));

    for (extra, what) in [
        (json!({"bot_personas": [null, "nobody"]}), "unknown persona"),
        (json!({"bot_personas": ["frost"]}), "persona on human seat"),
        (json!({"name": "x".repeat(61)}), "long name"),
    ] {
        let mut body = json!({"mode": "five_player", "bot_seats": [1, 2, 3, 4]});
        for (k, v) in extra.as_object().unwrap() {
            body[k] = v.clone();
        }
        let (st, err, _) = call(
            &c.app,
            Method::POST,
            "/api/games",
            Some(&a.cookie),
            Some(body),
        )
        .await;
        assert_eq!(st, StatusCode::BAD_REQUEST, "{what}: {err}");
    }

    // your-turn email is HTML+text and honours the opt-out
    let b = register(&c, true).await;
    let (st, _, _) = call(
        &c.app,
        Method::PATCH,
        "/api/me/settings",
        Some(&b.cookie),
        Some(json!({"email_turn_notifications": false})),
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    create_solo_game(&c, &b, json!({})).await;
    let o = c.outbox.lock().unwrap();
    let turn = |u: &TestUser| {
        o.iter()
            .filter(|m| {
                m.to == format!("{}@example.com", u.username)
                    && m.subject == "It's your turn in Wartide"
            })
            .count()
    };
    assert_eq!(turn(&a), 2);
    assert_eq!(turn(&b), 0);
    let mail = o
        .iter()
        .find(|m| m.subject == "It's your turn in Wartide" && m.to.starts_with(&a.username))
        .unwrap();
    assert!(mail.html.as_ref().unwrap().contains("WARTIDE"));
    assert!(mail.body.contains("/games/"));
    // verification mails are multipart too
    assert!(o
        .iter()
        .any(|m| m.subject == "Verify your Wartide email" && m.html.is_some()));
}

#[tokio::test]
async fn password_change_and_account_deletion() {
    let Some(c) = setup().await else { return };
    let a = register(&c, true).await;
    // second session
    let (st, _, cookie2) = call(
        &c.app,
        Method::POST,
        "/api/auth/login",
        None,
        Some(json!({"login": a.username, "password": "hunter2hunter2"})),
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    let second = cookie2
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .trim_start_matches("wt_session=")
        .to_string();

    let (st, err, _) = call(
        &c.app,
        Method::POST,
        "/api/me/password",
        Some(&a.cookie),
        Some(json!({"current_password": "wrong-one", "new_password": "brand-new-pass"})),
    )
    .await;
    assert_eq!(
        (st, err["code"].as_str()),
        (StatusCode::BAD_REQUEST, Some("invalid_credentials"))
    );
    let (st, _, _) = call(
        &c.app,
        Method::POST,
        "/api/me/password",
        Some(&a.cookie),
        Some(json!({"current_password": "hunter2hunter2", "new_password": "short"})),
    )
    .await;
    assert_eq!(st, StatusCode::BAD_REQUEST);
    let (st, _, _) = call(
        &c.app,
        Method::POST,
        "/api/me/password",
        Some(&a.cookie),
        Some(json!({"current_password": "hunter2hunter2", "new_password": "brand-new-pass"})),
    )
    .await;
    assert_eq!(st, StatusCode::NO_CONTENT);
    assert_eq!(
        call(&c.app, Method::GET, "/api/me", Some(&a.cookie), None)
            .await
            .0,
        StatusCode::OK
    );
    assert_eq!(
        call(&c.app, Method::GET, "/api/me", Some(&second), None)
            .await
            .0,
        StatusCode::UNAUTHORIZED
    );
    let login = |pw: &'static str| {
        let app = c.app.clone();
        let u = a.username.clone();
        async move {
            call(
                &app,
                Method::POST,
                "/api/auth/login",
                None,
                Some(json!({"login": u, "password": pw})),
            )
            .await
            .0
        }
    };
    assert_eq!(login("hunter2hunter2").await, StatusCode::BAD_REQUEST);
    assert_eq!(login("brand-new-pass").await, StatusCode::OK);

    // deletion keeps games intact
    let game = create_solo_game(&c, &a, json!({})).await;
    let gid: Uuid = game["id"].as_str().unwrap().parse().unwrap();
    let (st, err, _) = call(
        &c.app,
        Method::DELETE,
        "/api/me",
        Some(&a.cookie),
        Some(json!({"password": "not-my-password"})),
    )
    .await;
    assert_eq!(
        (st, err["code"].as_str()),
        (StatusCode::FORBIDDEN, Some("invalid_credentials"))
    );
    let (st, _, _) = call(&c.app, Method::DELETE, "/api/me", Some(&a.cookie), None).await;
    assert!(st.is_client_error(), "a body is required: {st}");
    assert_eq!(
        call(&c.app, Method::GET, "/api/me", Some(&a.cookie), None)
            .await
            .0,
        StatusCode::OK,
        "failed deletions must not log out"
    );
    let (st, _, cleared) = call(
        &c.app,
        Method::DELETE,
        "/api/me",
        Some(&a.cookie),
        Some(json!({"password": "brand-new-pass"})),
    )
    .await;
    assert_eq!(st, StatusCode::NO_CONTENT);
    assert!(cleared.unwrap().contains("Max-Age=0"));
    assert_eq!(
        call(&c.app, Method::GET, "/api/me", Some(&a.cookie), None)
            .await
            .0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(login("brand-new-pass").await, StatusCode::BAD_REQUEST);
    let (st, _, _) = call(
        &c.app,
        Method::GET,
        &format!("/api/users/{}", a.username),
        None,
        None,
    )
    .await;
    assert_eq!(st, StatusCode::NOT_FOUND);
    let (uname, email, seat_user): (String, String, Option<Uuid>) = sqlx::query_as(
        "SELECT u.username, u.email, s.user_id FROM users u JOIN game_seats s ON s.user_id=u.id WHERE s.game_id=$1 AND s.seat=0",
    )
    .bind(gid)
    .fetch_one(&c.state.db)
    .await
    .unwrap();
    assert_eq!(uname, format!("deleted-{}", a.id));
    assert!(email.starts_with(&format!("deleted-{}", a.id)));
    assert_eq!(seat_user, Some(a.id));
    // the game itself is still readable by a spectator once finished; here: still exists
    let b = register(&c, true).await;
    let (st, _, _) = call(
        &c.app,
        Method::GET,
        &format!("/api/games/{gid}"),
        Some(&b.cookie),
        None,
    )
    .await;
    assert_eq!(st, StatusCode::FORBIDDEN); // private & unfinished
}

#[tokio::test]
async fn spectating_and_public_listing() {
    let Some(c) = setup().await else { return };
    let a = register(&c, true).await;
    let b = register(&c, true).await;
    let watcher = register(&c, true).await;
    let gid = wartide_server::services::games::create_public(
        &c.state,
        wartide_protocol::game::GameMode::TwoPlayer,
        &[a.id, b.id],
    )
    .await
    .unwrap();
    // players write to all + team chat
    for (ch, body) in [("all", "hello everyone"), ("team", "secret plan")] {
        let (st, _, _) = call(
            &c.app,
            Method::POST,
            &format!("/api/games/{gid}/chat"),
            Some(&a.cookie),
            Some(json!({"channel": ch, "body": body})),
        )
        .await;
        assert_eq!(st, StatusCode::CREATED);
    }
    let get = |p: String| {
        let app = c.app.clone();
        let ck = watcher.cookie.clone();
        async move { call(&app, Method::GET, &p, Some(&ck), None).await }
    };
    assert_eq!(get(format!("/api/games/{gid}")).await.0, StatusCode::OK);
    assert_eq!(
        get(format!("/api/games/{gid}/state")).await.0,
        StatusCode::OK
    );
    assert_eq!(
        get(format!("/api/games/{gid}/actions")).await.0,
        StatusCode::OK
    );
    let (st, chat, _) = get(format!("/api/games/{gid}/chat?channel=all")).await;
    assert_eq!(st, StatusCode::OK);
    assert!(chat
        .as_array()
        .unwrap()
        .iter()
        .any(|m| m["body"] == "hello everyone"));
    assert!(chat
        .as_array()
        .unwrap()
        .iter()
        .all(|m| m["channel"] == "all"));
    let (st, chat, _) = get(format!("/api/games/{gid}/chat")).await;
    assert_eq!(st, StatusCode::OK);
    assert!(chat
        .as_array()
        .unwrap()
        .iter()
        .all(|m| m["channel"] == "all"));
    assert_eq!(
        get(format!("/api/games/{gid}/chat?channel=team")).await.0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        get(format!("/api/games/{gid}/chat?channel=power")).await.0,
        StatusCode::FORBIDDEN
    );
    // spectators cannot post or act
    let (st, _, _) = call(
        &c.app,
        Method::POST,
        &format!("/api/games/{gid}/chat"),
        Some(&watcher.cookie),
        Some(json!({"channel": "all", "body": "hi"})),
    )
    .await;
    assert_eq!(st, StatusCode::FORBIDDEN);
    let (st, _, _) = call(
        &c.app,
        Method::POST,
        &format!("/api/games/{gid}/actions"),
        Some(&watcher.cookie),
        Some(json!({"action": {"type": "end_turn"}})),
    )
    .await;
    assert_eq!(st, StatusCode::FORBIDDEN);
    // logged-out users cannot spectate
    assert_eq!(
        call(
            &c.app,
            Method::GET,
            &format!("/api/games/{gid}"),
            None,
            None
        )
        .await
        .0,
        StatusCode::UNAUTHORIZED
    );

    // listing
    let (st, list, _) = get("/api/games/public?status=active".into()).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(list["page"], 1);
    assert_eq!(list["per_page"], 20);
    let items = list["items"].as_array().unwrap();
    assert!(items.iter().any(|g| g["id"] == gid.to_string()));
    assert!(items
        .iter()
        .all(|g| g["kind"] == "public" && g["status"] == "in_progress"));
    let times: Vec<&str> = items
        .iter()
        .map(|g| g["created_at"].as_str().unwrap())
        .collect();
    assert!(times.windows(2).all(|w| w[0] >= w[1]), "newest first");
    let (_, beyond, _) = get("/api/games/public?page=100000".into()).await;
    assert!(beyond["items"].as_array().unwrap().is_empty());

    // private games: hidden until finished
    let private = create_solo_game(&c, &a, json!({})).await;
    let pid = private["id"].as_str().unwrap().to_string();
    assert_eq!(
        get(format!("/api/games/{pid}")).await.0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        get(format!("/api/games/{pid}/state")).await.0,
        StatusCode::FORBIDDEN
    );
    let (st, _, _) = call(
        &c.app,
        Method::POST,
        &format!("/api/games/{pid}/resign"),
        Some(&a.cookie),
        None,
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    // ... and a finished private game stays private to its players
    for suffix in ["", "/state", "/actions", "/chat?channel=all"] {
        assert_eq!(
            get(format!("/api/games/{pid}{suffix}")).await.0,
            StatusCode::FORBIDDEN,
            "{suffix}"
        );
    }
    let (st, _, _) = call(
        &c.app,
        Method::GET,
        &format!("/api/games/{pid}"),
        Some(&a.cookie),
        None,
    )
    .await;
    assert_eq!(st, StatusCode::OK);

    // profile redacts group/private details, shows public games
    let (_, prof, _) = call(
        &c.app,
        Method::GET,
        &format!("/api/users/{}", a.username),
        None,
        None,
    )
    .await;
    assert!(prof["recent_games"]
        .as_array()
        .unwrap()
        .iter()
        .any(|g| g["id"] == gid.to_string()));
    assert!(prof["recent_games"]
        .as_array()
        .unwrap()
        .iter()
        .all(|g| g["kind"] == "public" && g["id"] != pid.as_str()));
}

#[tokio::test]
async fn rate_limits_return_api_errors() {
    let Some(c) = setup().await else { return };
    let a = register(&c, true).await;
    let limited = Ctx {
        app: build_router(build_state(
            c.state.db.clone(),
            {
                let mut cfg = Config::for_test(String::new());
                cfg.rate_limit = true;
                cfg
            },
            Mailer::capture().0,
        )),
        state: c.state.clone(),
        outbox: c.outbox.clone(),
    };
    // chat: 20 / minute
    let game = create_solo_game(&c, &a, json!({})).await;
    let gid = game["id"].as_str().unwrap();
    let mut last = (StatusCode::OK, Value::Null);
    for i in 0..21 {
        let (st, body, _) = call(
            &limited.app,
            Method::POST,
            &format!("/api/games/{gid}/chat"),
            Some(&a.cookie),
            Some(json!({"channel": "all", "body": format!("m{i}")})),
        )
        .await;
        if i < 20 {
            assert_eq!(st, StatusCode::CREATED, "{body}");
        }
        last = (st, body);
    }
    assert_eq!(last.0, StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(last.1["code"], "rate_limited");
    // game creation: 10 / hour (one game already made through the unlimited app)
    let mut codes = Vec::new();
    for _ in 0..11 {
        let (st, body, _) = call(
            &limited.app,
            Method::POST,
            "/api/games",
            Some(&a.cookie),
            Some(json!({"mode": "five_player", "bot_seats": [1, 2, 3, 4]})),
        )
        .await;
        codes.push((st, body["code"].as_str().map(str::to_string)));
    }
    assert!(codes[..10].iter().all(|(s, _)| *s == StatusCode::OK));
    assert_eq!(
        codes[10],
        (StatusCode::TOO_MANY_REQUESTS, Some("rate_limited".into()))
    );
    // per-IP limit on credential endpoints also speaks ApiError
    let mut seen = None;
    for _ in 0..15 {
        let req = Request::builder()
            .method(Method::POST)
            .uri("/api/auth/login")
            .header(header::CONTENT_TYPE, "application/json")
            .header("x-forwarded-for", "203.0.113.77")
            .body(Body::from(
                json!({"login": "nobody", "password": "x"}).to_string(),
            ))
            .unwrap();
        let res = limited.app.clone().oneshot(req).await.unwrap();
        if res.status() == StatusCode::TOO_MANY_REQUESTS {
            assert!(res.headers().contains_key(header::RETRY_AFTER));
            let bytes = axum::body::to_bytes(res.into_body(), 1 << 16)
                .await
                .unwrap();
            let v: Value = serde_json::from_slice(&bytes).unwrap();
            seen = Some(v);
            break;
        }
    }
    assert_eq!(
        seen.expect("governor should trigger")["code"],
        "rate_limited"
    );
}

#[tokio::test]
async fn admin_endpoints() {
    let Some(c) = setup().await else { return };
    let admin = register(&c, true).await;
    let user = register(&c, true).await;
    sqlx::query("UPDATE users SET is_admin=TRUE WHERE id=$1")
        .bind(admin.id)
        .execute(&c.state.db)
        .await
        .unwrap();
    assert_eq!(
        call(
            &c.app,
            Method::GET,
            "/api/admin/stats",
            Some(&user.cookie),
            None
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        call(&c.app, Method::GET, "/api/admin/stats", None, None)
            .await
            .0,
        StatusCode::UNAUTHORIZED
    );
    let (st, stats, _) = call(
        &c.app,
        Method::GET,
        "/api/admin/stats",
        Some(&admin.cookie),
        None,
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{stats}");
    assert!(stats["users"].as_i64().unwrap() >= 2);
    assert!(stats["games_lobby"].is_i64() && stats["queue_two_player"].is_i64());
    let (_, me, _) = call(&c.app, Method::GET, "/api/me", Some(&admin.cookie), None).await;
    assert_eq!(me["is_admin"], true);

    let game = create_solo_game(&c, &user, json!({})).await;
    assert_eq!(game["status"], "in_progress");
    let uri = format!("/api/admin/games/{}/abandon", game["id"].as_str().unwrap());
    assert_eq!(
        call(&c.app, Method::POST, &uri, Some(&user.cookie), None)
            .await
            .0,
        StatusCode::FORBIDDEN
    );
    let (st, ab, _) = call(&c.app, Method::POST, &uri, Some(&admin.cookie), None).await;
    assert_eq!(st, StatusCode::OK, "{ab}");
    assert_eq!(ab["status"], "abandoned");
    assert_eq!(
        call(&c.app, Method::POST, &uri, Some(&admin.cookie), None)
            .await
            .0,
        StatusCode::CONFLICT
    );
    let nope = format!("/api/admin/games/{}/abandon", Uuid::new_v4());
    assert_eq!(
        call(&c.app, Method::POST, &nope, Some(&admin.cookie), None)
            .await
            .0,
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn hardening_headers_and_body_limit() {
    let Some(c) = setup().await else { return };
    let res = c
        .app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/me")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.headers()[header::CACHE_CONTROL], "no-store");
    assert_eq!(res.headers()[header::X_CONTENT_TYPE_OPTIONS], "nosniff");
    let res = c
        .app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/some/spa/route")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.headers()[header::X_FRAME_OPTIONS], "DENY");
    let csp = res.headers()[header::CONTENT_SECURITY_POLICY]
        .to_str()
        .unwrap();
    assert!(csp.contains("fonts.googleapis.com") && csp.contains("connect-src 'self';"));
    assert!(!csp.contains("ws:"));
    assert_eq!(res.headers()["cross-origin-opener-policy"], "same-origin");
    assert!(
        !res.headers().contains_key("strict-transport-security"),
        "no HSTS when cookies are not Secure (plain-http dev)"
    );
    assert!(!res.headers().contains_key(header::CACHE_CONTROL));
    // production (COOKIE_SECURE on): HSTS on every response
    let prod = limited(&c, |cfg| cfg.cookie_secure = true);
    for uri in ["/api/me", "/some/spa/route", "/healthz"] {
        let res = prod
            .app
            .clone()
            .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(
            res.headers()["strict-transport-security"],
            "max-age=63072000; includeSubDomains",
            "{uri}"
        );
    }
    // > 1 MB bodies are rejected
    let big = "x".repeat(1_100_000);
    let req = Request::builder()
        .method(Method::POST)
        .uri("/api/auth/login")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(format!(
            "{{\"login\":\"{big}\",\"password\":\"x\"}}"
        )))
        .unwrap();
    let res = c.app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::PAYLOAD_TOO_LARGE);
}

/// Host plays USSR with the random policy through the HTTP API against four computer players.
#[tokio::test]
async fn full_stack_game_against_bots() {
    use wartide_engine::{random_policy::RandomPolicy, GameState};
    let Some(c) = setup().await else { return };
    let u = register(&c, true).await;
    let game = create_solo_game(&c, &u, json!({})).await;
    let gid = game["id"].as_str().unwrap().to_string();
    let guid: Uuid = gid.parse().unwrap();
    let started = std::time::Instant::now();

    let mut policy = RandomPolicy::new(7);
    let mut posted = 0;
    let mut rejected = 0;
    let mut final_round = 0;
    let mut over = false;
    for step in 0..20_000 {
        let (st, sv, _) = call(
            &c.app,
            Method::GET,
            &format!("/api/games/{gid}/state"),
            Some(&u.cookie),
            None,
        )
        .await;
        assert_eq!(st, StatusCode::OK, "{sv}");
        let gs: GameState = serde_json::from_value(sv["state"].clone()).expect("engine state");
        final_round = gs.round;
        over = gs.is_over();
        if gs.round >= 3 || over {
            break;
        }
        assert!(
            started.elapsed() < std::time::Duration::from_secs(280),
            "too slow: step {step}, round {}",
            gs.round
        );
        let (_, sum, _) = call(
            &c.app,
            Method::GET,
            &format!("/api/games/{gid}"),
            Some(&u.cookie),
            None,
        )
        .await;
        if sum["pending_power"] == "ussr" {
            let (_, action) = policy.next_action(&gs).expect("action");
            let (st, env, _) = call(
                &c.app,
                Method::POST,
                &format!("/api/games/{gid}/actions"),
                Some(&u.cookie),
                Some(json!({ "action": action })),
            )
            .await;
            if st == StatusCode::OK {
                posted += 1;
            } else {
                rejected += 1;
                // Racing the server's automatic defender resolution can also surface as
                // "not your turn" once control has moved on.
                assert!(
                    env["code"] == "illegal_action" || env["code"] == "not_your_turn",
                    "{env}"
                );
            }
        } else {
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    }
    eprintln!(
        "full game: reached round {final_round} in {:?} ({posted} human actions, {rejected} rejected)",
        started.elapsed()
    );
    // (the dice seed is random per game, so a lucky bot can end it early)
    assert!(
        final_round >= 3 || over,
        "game stalled at round {final_round}"
    );
    assert!(posted > 10 || over);

    // team chat: bot commentary lands in the Allies channel (empty while the AI is silent)
    let (_, chat, _) = call(
        &c.app,
        Method::GET,
        &format!("/api/games/{gid}/chat?channel=team"),
        Some(&u.cookie),
        None,
    )
    .await;
    let bot_msgs: Vec<&Value> = chat
        .as_array()
        .unwrap()
        .iter()
        .filter(|m| m["sender"]["kind"] == "bot")
        .collect();
    if bot_msgs.is_empty() {
        eprintln!("NOTICE: the AI produced no commentary in this game");
    }
    assert!(bot_msgs.iter().all(|m| m["scope"] == "allies"));

    // let the bots settle, then replay the log and compare with the stored state
    wait_idle(&c, guid).await;
    let mut log = Vec::new();
    let mut since = 0;
    loop {
        let (_, batch, _) = call(
            &c.app,
            Method::GET,
            &format!("/api/games/{gid}/actions?since={since}"),
            Some(&u.cookie),
            None,
        )
        .await;
        let batch = batch.as_array().unwrap().clone();
        if batch.is_empty() {
            break;
        }
        since = batch.last().unwrap()["seq"].as_i64().unwrap();
        log.extend(batch);
    }
    let (seed,): (i64,) = sqlx::query_as("SELECT seed FROM games WHERE id=$1")
        .bind(guid)
        .fetch_one(&c.state.db)
        .await
        .unwrap();
    let settings = wartide_server::services::rules::engine_settings(
        &wartide_protocol::game::GameSettings::default(),
        seed,
    );
    let mut stepper = GameState::new(settings.clone());
    let mut entries = Vec::new();
    for e in &log {
        let seq = e["seq"].as_i64().unwrap();
        let action: wartide_engine::Action = serde_json::from_value(e["action"].clone()).unwrap();
        let power = match wartide_engine::pending_decision(&stepper) {
            wartide_engine::Decision::Phase { power, .. }
            | wartide_engine::Decision::SelectCasualties { power, .. }
            | wartide_engine::Decision::RetreatOrPress { power, .. }
            | wartide_engine::Decision::SubWithdrawal { power, .. } => power,
            other => panic!("log continues after {other:?}"),
        };
        let seed = wartide_protocol::seed::action_seed(seed as u64, seq);
        wartide_engine::apply(&mut stepper, power, &action, seed).unwrap();
        entries.push((power, action, seed));
    }
    assert_eq!(entries.len() as i64, since, "log is gapless from seq 1");
    let replayed = wartide_engine::replay(settings, &entries).unwrap();
    let (seq, stored): (i64, Value) =
        sqlx::query_as("SELECT seq, state FROM game_states WHERE game_id=$1")
            .bind(guid)
            .fetch_one(&c.state.db)
            .await
            .unwrap();
    assert_eq!(seq, since);
    assert_eq!(serde_json::to_value(&replayed).unwrap(), stored);
    assert!(log
        .iter()
        .all(|e| e["events"].is_array() && e["actor"].is_number()));
}

trait HostPower {
    fn is_host_power(&self) -> bool;
}
impl HostPower for wartide_engine::Power {
    fn is_host_power(&self) -> bool {
        matches!(self.id(), "uk" | "usa")
    }
}

#[tokio::test]
async fn three_player_custom_layout_runs_to_round_two() {
    let Some(c) = setup().await else { return };
    let a = register(&c, true).await;
    let seats = json!([
        {"powers": ["germany", "japan"], "kind": "bot", "persona": "frost"},
        {"powers": ["ussr"], "kind": "bot"},
        {"powers": ["uk", "usa"], "kind": "host"},
    ]);
    let (st, game, _) = call(
        &c.app,
        Method::POST,
        "/api/games",
        Some(&a.cookie),
        Some(json!({"seats": seats})),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{game}");
    assert_eq!(game["mode"], "custom");
    assert_eq!(game["status"], "in_progress");
    assert_eq!(game["my_seat"], 2);
    assert_eq!(game["seats"][2]["powers"], json!(["uk", "usa"]));
    assert_eq!(game["seats"][0]["bot_persona"], "frost");
    assert_eq!(game["seats"][1]["is_bot"], true);
    let gid = game["id"].as_str().unwrap().to_string();
    let guid: Uuid = gid.parse().unwrap();

    // The host controls UK and then USA; bots drive USSR and Germany+Japan. The host ONLY ever
    // handles `Decision::Phase`: when a bot attacks the host's territory the server resolves the
    // host's casualties and submarine withdrawals itself, so the host is never asked.
    use wartide_engine::{Decision, GameState};
    let mut acted = std::collections::BTreeSet::new();
    let mut round = 0;
    let mut waiting_on_host_defender = 0;
    for _ in 0..600 {
        let (st, sv, _) = call(
            &c.app,
            Method::GET,
            &format!("/api/games/{gid}/state"),
            Some(&a.cookie),
            None,
        )
        .await;
        assert_eq!(st, StatusCode::OK, "{sv}");
        let gs: GameState = serde_json::from_value(sv["state"].clone()).expect("engine state");
        round = gs.round as i64;
        if round >= 2 {
            break;
        }
        let (power, action) = match wartide_engine::pending_decision(&gs) {
            Decision::Phase { power, .. } if power.is_host_power() => {
                waiting_on_host_defender = 0;
                (power, json!({"type": "end_phase"}))
            }
            d @ (Decision::SelectCasualties { .. }
            | Decision::RetreatOrPress { .. }
            | Decision::SubWithdrawal { .. }) => {
                let p = match &d {
                    Decision::SelectCasualties { power, .. }
                    | Decision::RetreatOrPress { power, .. }
                    | Decision::SubWithdrawal { power, .. } => *power,
                    _ => unreachable!(),
                };
                if p.is_host_power() {
                    // The driver resolves this within milliseconds; the host never answers it.
                    waiting_on_host_defender += 1;
                    assert!(
                        waiting_on_host_defender < 100,
                        "host was left holding a defender decision: {d:?}"
                    );
                }
                tokio::time::sleep(std::time::Duration::from_millis(30)).await;
                continue;
            }
            _ => {
                tokio::time::sleep(std::time::Duration::from_millis(30)).await;
                continue;
            }
        };
        let (st, env, _) = call(
            &c.app,
            Method::POST,
            &format!("/api/games/{gid}/actions"),
            Some(&a.cookie),
            Some(json!({"action": action})),
        )
        .await;
        assert_eq!(st, StatusCode::OK, "{env}");
        acted.insert(power.id().to_string());
    }
    assert!(round >= 2, "stalled in round {round}");
    assert!(acted.contains("uk") && acted.contains("usa"), "{acted:?}");
    let (n,): (i64,) =
        sqlx::query_as("SELECT count(DISTINCT actor_seat) FROM game_actions WHERE game_id=$1")
            .bind(guid)
            .fetch_one(&c.state.db)
            .await
            .unwrap();
    assert_eq!(n, 3, "all three seats acted");
    let (g_acts,): (i64,) =
        sqlx::query_as("SELECT count(*) FROM game_actions WHERE game_id=$1 AND actor_seat=0")
            .bind(guid)
            .fetch_one(&c.state.db)
            .await
            .unwrap();
    assert!(g_acts > 0);
}

#[tokio::test]
async fn battle_preferences_round_trip_and_reject_garbage() {
    let Some(c) = setup().await else { return };
    let a = register(&c, true).await;
    let (st, s, _) = call(
        &c.app,
        Method::GET,
        "/api/me/settings",
        Some(&a.cookie),
        None,
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{s}");
    assert_eq!(s["casualty_policy"], "cheapest_first");
    assert_eq!(s["sub_withdrawal"], "when_outgunned");
    let (st, s, _) = call(
        &c.app,
        Method::PATCH,
        "/api/me/settings",
        Some(&a.cookie),
        Some(json!({"casualty_policy": "protect_aircraft", "sub_withdrawal": "always"})),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{s}");
    assert_eq!(s["casualty_policy"], "protect_aircraft");
    assert_eq!(s["sub_withdrawal"], "always");
    // an unrelated patch leaves them alone
    let (_, s, _) = call(
        &c.app,
        Method::PATCH,
        "/api/me/settings",
        Some(&a.cookie),
        Some(json!({"email_match_found": false})),
    )
    .await;
    assert_eq!(s["casualty_policy"], "protect_aircraft");
    for bad in [
        json!({"casualty_policy": "nonsense"}),
        json!({"sub_withdrawal": "sometimes"}),
        json!({"casualty_policy": 7}),
    ] {
        let (st, e, _) = call(
            &c.app,
            Method::PATCH,
            "/api/me/settings",
            Some(&a.cookie),
            Some(bad.clone()),
        )
        .await;
        assert_eq!(st, StatusCode::BAD_REQUEST, "{bad}: {e}");
    }
    let (_, s, _) = call(
        &c.app,
        Method::GET,
        "/api/me/settings",
        Some(&a.cookie),
        None,
    )
    .await;
    assert_eq!(s["casualty_policy"], "protect_aircraft");
}

#[tokio::test]
async fn seat_layout_validation_and_open_seats() {
    let Some(c) = setup().await else { return };
    let a = register(&c, true).await;
    let b = register(&c, true).await;
    let post = |body: Value| {
        let c = &c;
        let cookie = a.cookie.clone();
        async move {
            call(
                &c.app,
                Method::POST,
                "/api/games",
                Some(&cookie),
                Some(body),
            )
            .await
        }
    };
    let host = |p: Value| json!({"powers": p, "kind": "host"});
    let bad = [
        // missing power
        json!([host(json!(["germany", "japan", "uk", "usa"]))]),
        // duplicate power
        json!([host(json!(["germany", "japan", "uk", "usa", "ussr"])), {"powers": ["usa"], "kind": "bot"}]),
        // empty seat
        json!([host(json!(["germany", "japan", "uk", "usa", "ussr"])), {"powers": [], "kind": "bot"}]),
        // no host
        json!([{"powers": ["germany", "japan", "uk", "usa", "ussr"], "kind": "bot"}]),
        // two hosts
        json!([
            host(json!(["germany", "japan"])),
            host(json!(["uk", "usa", "ussr"]))
        ]),
        // persona on a non-bot seat
        json!([{"powers": ["germany", "japan", "uk", "usa", "ussr"], "kind": "host", "persona": "frost"}]),
        // unknown persona
        json!([host(json!(["germany", "japan"])), {"powers": ["uk", "usa", "ussr"], "kind": "bot", "persona": "nobody"}]),
    ];
    for seats in bad {
        let (st, err, _) = post(json!({"seats": seats})).await;
        assert_eq!(st, StatusCode::BAD_REQUEST, "{seats}: {err}");
    }

    // one player controlling everything vs nobody: starts at once, mode custom
    let (st, g, _) =
        post(json!({"seats": [host(json!(["ussr", "germany", "uk", "japan", "usa"]))]})).await;
    assert_eq!(st, StatusCode::OK, "{g}");
    assert_eq!(g["status"], "in_progress");
    assert_eq!(g["mode"], "custom");

    // 2 seats split by team derive two_player; open seat waits for a friend
    let (st, g, _) = post(json!({"seats": [
        {"powers": ["germany", "japan"], "kind": "open"},
        host(json!(["ussr", "uk", "usa"])),
    ]}))
    .await;
    assert_eq!(st, StatusCode::OK, "{g}");
    assert_eq!(g["mode"], "two_player");
    assert_eq!(g["status"], "lobby");
    assert_eq!(g["my_seat"], 1);
    let code = g["invite_code"].as_str().unwrap().to_string();
    let (st, joined, _) = call(
        &c.app,
        Method::POST,
        "/api/games/join",
        Some(&b.cookie),
        Some(json!({"code": code})),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{joined}");
    assert_eq!(joined["status"], "in_progress");
    assert_eq!(joined["my_seat"], 0);
    assert_eq!(joined["seats"][0]["powers"], json!(["germany", "japan"]));

    // 5 single-power seats in a different order are still five_player
    let (st, g, _) = post(json!({"seats": [
        host(json!(["usa"])),
        {"powers": ["ussr"], "kind": "bot"}, {"powers": ["germany"], "kind": "bot"},
        {"powers": ["uk"], "kind": "bot"}, {"powers": ["japan"], "kind": "bot"},
    ]}))
    .await;
    assert_eq!(st, StatusCode::OK, "{g}");
    assert_eq!(g["mode"], "five_player");

    // timezone validation
    for (tz, ok) in [
        ("Europe/Berlin", true),
        ("Etc/GMT+5", true),
        ("not a zone!", false),
        ("../x", false),
    ] {
        let (st, _, _) = call(
            &c.app,
            Method::PATCH,
            "/api/me/profile",
            Some(&a.cookie),
            Some(json!({"timezone": tz})),
        )
        .await;
        assert_eq!(st == StatusCode::OK, ok, "{tz}");
    }
    let (_, me, _) = call(&c.app, Method::GET, "/api/me", Some(&a.cookie), None).await;
    assert_eq!(me["timezone"], "Etc/GMT+5");
}

// ---------------------------------------------------------------------------------------------
// Security hardening
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn rate_limit_key_ignores_spoofed_forwarded_for() {
    let Some(c) = setup().await else { return };
    // one trusted proxy: only the LAST X-Forwarded-For entry counts
    let l = limited(&c, |_| {});
    let mut codes = Vec::new();
    for i in 0..40 {
        let xff = format!("1.2.{i}.4, 9.9.9.9");
        let (st, _, _) = call_h(
            &l.app,
            Method::POST,
            "/api/auth/verify-email",
            None,
            Some(json!({"token": format!("nope{i}")})),
            &[
                ("x-forwarded-for", &xff),
                ("x-real-ip", &format!("5.5.5.{i}")),
            ],
        )
        .await;
        codes.push(st);
    }
    assert!(
        codes.contains(&StatusCode::TOO_MANY_REQUESTS),
        "rotating the leftmost entry must not dodge the limiter: {codes:?}"
    );
    // a different real client (last entry) has its own bucket
    let (st, _, _) = call_h(
        &l.app,
        Method::POST,
        "/api/auth/verify-email",
        None,
        Some(json!({"token": "nope"})),
        &[("x-forwarded-for", "1.2.3.4, 8.8.8.8")],
    )
    .await;
    assert_eq!(st, StatusCode::BAD_REQUEST);

    // no trusted proxy: the header is ignored, and without a peer address there is no key
    let l0 = limited(&c, |cfg| cfg.trusted_proxy_hops = 0);
    let (st, body, _) = call_h(
        &l0.app,
        Method::POST,
        "/api/auth/verify-email",
        None,
        Some(json!({"token": "nope"})),
        &[("x-forwarded-for", "1.2.3.4")],
    )
    .await;
    assert_eq!(st, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], "bad_request");
}

#[tokio::test]
async fn per_account_limits_and_uniform_responses() {
    let Some(c) = setup().await else { return };
    let u = register(&c, false).await;
    let email = format!("{}@example.com", u.username);
    let l = limited(&c, |_| {});
    let n = std::sync::atomic::AtomicU32::new(0);
    let post = |uri: &'static str, body: Value, cookie: Option<String>| {
        let app = l.app.clone();
        let ip = format!(
            "10.1.0.{}",
            n.fetch_add(1, std::sync::atomic::Ordering::SeqCst)
        );
        async move {
            call_h(
                &app,
                Method::POST,
                uri,
                cookie.as_deref(),
                Some(body),
                &[("x-forwarded-for", &ip)],
            )
            .await
        }
    };

    // register: 3 per address per hour (the 4th is limited before touching the database)
    let mut codes = Vec::new();
    for i in 0..4 {
        let (st, _, _) = post(
            "/api/auth/register",
            json!({"email": email.to_uppercase(), "username": format!("dup{i}{}", &u.username[1..8]), "password": "hunter2hunter2"}),
            None,
        )
        .await;
        codes.push(st);
    }
    assert_eq!(
        codes,
        [
            StatusCode::CONFLICT,
            StatusCode::CONFLICT,
            StatusCode::CONFLICT,
            StatusCode::TOO_MANY_REQUESTS
        ]
    );

    // forgot-password: always 204, but only 3 mails per address per hour
    for _ in 0..5 {
        let (st, _, _) = post("/api/auth/forgot-password", json!({"email": email}), None).await;
        assert_eq!(st, StatusCode::NO_CONTENT);
    }
    let resets = |l: &Ctx| {
        l.outbox
            .lock()
            .unwrap()
            .iter()
            .filter(|m| m.to == email && m.body.contains("/reset-password?token="))
            .count()
    };
    assert_eq!(resets(&l), 3);
    // unknown addresses get the same answer
    let (st, _, _) = post(
        "/api/auth/forgot-password",
        json!({"email": "nobody-here@example.com"}),
        None,
    )
    .await;
    assert_eq!(st, StatusCode::NO_CONTENT);

    // resend-verification: uniform 204, 3 mails per address per hour
    for _ in 0..5 {
        let (st, _, _) = post(
            "/api/auth/resend-verification",
            json!({}),
            Some(u.cookie.clone()),
        )
        .await;
        assert_eq!(st, StatusCode::NO_CONTENT);
    }
    let verifies = l
        .outbox
        .lock()
        .unwrap()
        .iter()
        .filter(|m| m.to == email && m.body.contains("/verify?token="))
        .count();
    assert_eq!(verifies, 3);

    // login failures: 10 per identifier per 15 minutes, then even the right password is refused
    for i in 0..10 {
        let (st, body, _) = post(
            "/api/auth/login",
            json!({"login": u.username.to_uppercase(), "password": format!("wrong-{i}")}),
            None,
        )
        .await;
        assert_eq!(st, StatusCode::BAD_REQUEST, "{body}");
        assert_eq!(body["code"], "invalid_credentials");
    }
    let (st, body, _) = post(
        "/api/auth/login",
        json!({"login": u.username, "password": "hunter2hunter2"}),
        None,
    )
    .await;
    assert_eq!(st, StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(body["code"], "rate_limited");
    // other accounts are unaffected, and unknown identifiers fail identically (and are limited too)
    let (st, body, _) = post(
        "/api/auth/login",
        json!({"login": "no-such-person", "password": "whatever1"}),
        None,
    )
    .await;
    assert_eq!(st, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], "invalid_credentials");
}

#[tokio::test]
async fn state_endpoint_hides_dice_seed() {
    let Some(c) = setup().await else { return };
    let u = register(&c, true).await;
    let game = create_solo_game(&c, &u, json!({})).await;
    let gid: Uuid = game["id"].as_str().unwrap().parse().unwrap();
    let (st, sv, _) = call(
        &c.app,
        Method::GET,
        &format!("/api/games/{gid}/state"),
        Some(&u.cookie),
        None,
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(sv["state"]["settings"]["seed"], 0);
    let (seed,): (i64,) = sqlx::query_as("SELECT seed FROM games WHERE id=$1")
        .bind(gid)
        .fetch_one(&c.state.db)
        .await
        .unwrap();
    assert_ne!(seed, 0);
    // the stored state (used for replay) still carries the real seed
    let (stored,): (String,) =
        sqlx::query_as("SELECT state->'settings'->>'seed' FROM game_states WHERE game_id=$1")
            .bind(gid)
            .fetch_one(&c.state.db)
            .await
            .unwrap();
    assert_eq!(stored, (seed as u64).to_string());
}

#[tokio::test]
async fn finished_group_game_read_access() {
    let Some(c) = setup().await else { return };
    let a = register(&c, true).await;
    let b = register(&c, true).await;
    let member = register(&c, true).await;
    let outsider = register(&c, true).await;
    let (st, g, _) = call(
        &c.app,
        Method::POST,
        "/api/groups",
        Some(&a.cookie),
        Some(json!({"name": "Readers Club", "description": "x"})),
    )
    .await;
    assert_eq!(st, StatusCode::CREATED, "{g}");
    let group = g["id"].as_str().unwrap().to_string();
    let code = g["invite_code"].as_str().unwrap().to_string();
    for u in [&b, &member] {
        let (st, _, _) = call(
            &c.app,
            Method::POST,
            "/api/groups/join",
            Some(&u.cookie),
            Some(json!({"invite_code": code})),
        )
        .await;
        assert_eq!(st, StatusCode::OK);
    }
    let (_, game, _) = call(
        &c.app,
        Method::POST,
        "/api/games",
        Some(&a.cookie),
        Some(json!({"mode": "two_player", "bot_seats": [], "group_id": group})),
    )
    .await;
    let gid = game["id"].as_str().unwrap().to_string();
    let (st, _, _) = call(
        &c.app,
        Method::POST,
        "/api/games/join",
        Some(&b.cookie),
        Some(json!({"code": game["invite_code"]})),
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    let (st, _, _) = call(
        &c.app,
        Method::POST,
        &format!("/api/games/{gid}/resign"),
        Some(&a.cookie),
        None,
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    // a 2-human game does not end on one resignation; finish it directly
    sqlx::query("UPDATE games SET status='finished' WHERE id=$1")
        .bind(gid.parse::<Uuid>().unwrap())
        .execute(&c.state.db)
        .await
        .unwrap();

    for suffix in ["", "/state", "/actions", "/chat?channel=all"] {
        let uri = format!("/api/games/{gid}{suffix}");
        for reader in [&a, &b, &member] {
            let (st, _, _) = call(&c.app, Method::GET, &uri, Some(&reader.cookie), None).await;
            assert_eq!(st, StatusCode::OK, "{uri}");
        }
        let (st, _, _) = call(&c.app, Method::GET, &uri, Some(&outsider.cookie), None).await;
        assert_eq!(st, StatusCode::FORBIDDEN, "{uri}");
    }
    // the game id is not advertised on the players' public profiles
    for p in [&a, &b] {
        let (_, prof, _) = call(
            &c.app,
            Method::GET,
            &format!("/api/users/{}", p.username),
            Some(&outsider.cookie),
            None,
        )
        .await;
        assert!(prof["recent_games"]
            .as_array()
            .unwrap()
            .iter()
            .all(|x| x["id"] != gid.as_str()));
    }
}

#[tokio::test]
async fn verification_revokes_sessions() {
    let Some(c) = setup().await else { return };
    // the (attacker) registrant holds a live session before the mailbox owner verifies
    let u = register(&c, false).await;
    let email = format!("{}@example.com", u.username);
    assert_eq!(
        call(&c.app, Method::GET, "/api/me", Some(&u.cookie), None)
            .await
            .0,
        StatusCode::OK
    );
    let t = token_from_mail(&c.outbox, &email, "/verify?token=");
    let (st, _, _) = call(
        &c.app,
        Method::POST,
        "/api/auth/verify-email",
        None,
        Some(json!({"token": t})),
    )
    .await;
    assert_eq!(st, StatusCode::NO_CONTENT);
    assert_eq!(
        call(&c.app, Method::GET, "/api/me", Some(&u.cookie), None)
            .await
            .0,
        StatusCode::UNAUTHORIZED
    );
    // logging in again works and shows a verified account
    let (st, me, _) = call(
        &c.app,
        Method::POST,
        "/api/auth/login",
        None,
        Some(json!({"login": email, "password": "hunter2hunter2"})),
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(me["email_verified"], true);
}

#[tokio::test]
async fn websocket_requires_matching_origin() {
    let Some(c) = setup().await else { return };
    let u = register(&c, true).await;
    let ws = |origin: Option<&str>, cookie: Option<&str>| {
        let app = c.app.clone();
        let mut b = Request::builder()
            .uri("/ws")
            .header(header::CONNECTION, "upgrade")
            .header(header::UPGRADE, "websocket")
            .header("sec-websocket-version", "13")
            .header("sec-websocket-key", "dGhlIHNhbXBsZSBub25jZQ==");
        if let Some(o) = origin {
            b = b.header(header::ORIGIN, o);
        }
        if let Some(k) = cookie {
            b = b.header(header::COOKIE, format!("wt_session={k}"));
        }
        let req = b.body(Body::empty()).unwrap();
        async move { app.oneshot(req).await.unwrap().status() }
    };
    for bad in [
        Some("http://evil.example"),
        Some("http://localhost:8081"),
        Some("https://localhost:8080"),
        Some("null"),
        None,
    ] {
        assert_eq!(
            ws(bad, Some(&u.cookie)).await,
            StatusCode::FORBIDDEN,
            "{bad:?}"
        );
    }
    // matching origin: still needs a session ...
    assert_eq!(
        ws(Some("http://localhost:8080"), None).await,
        StatusCode::UNAUTHORIZED
    );
    // ... and with one the origin gate no longer blocks (the oneshot harness cannot complete the
    // protocol upgrade itself, so anything but 403/401 means we got past our checks)
    let st = ws(Some("HTTP://LOCALHOST:8080"), Some(&u.cookie)).await;
    assert!(
        st != StatusCode::FORBIDDEN && st != StatusCode::UNAUTHORIZED,
        "{st}"
    );
}

#[tokio::test]
async fn display_name_and_description_hygiene() {
    let Some(c) = setup().await else { return };
    let u = register(&c, true).await;
    let patch = |cookie: String, name: String| {
        let app = c.app.clone();
        async move {
            call(
                &app,
                Method::PATCH,
                "/api/me/profile",
                Some(&cookie),
                Some(json!({"display_name": name})),
            )
            .await
            .0
        }
    };
    for bad in [
        "Ad\u{202E}min",
        "zero\u{200B}width",
        "a\u{2066}b",
        "bell\u{7}",
        "tab\there",
        "joiner\u{200D}x",
        "word\u{2060}joiner",
        "bom\u{FEFF}",
        "Admin",
        " WARTIDE ",
        "Support",
        "system",
        "Moderator",
        "administrator",
    ] {
        assert_eq!(
            patch(u.cookie.clone(), bad.to_string()).await,
            StatusCode::BAD_REQUEST,
            "{bad:?}"
        );
    }
    assert_eq!(
        patch(u.cookie.clone(), "Général Ünïcode 将军".into()).await,
        StatusCode::OK
    );
    // admins may use the reserved names
    sqlx::query("UPDATE users SET is_admin=TRUE WHERE id=$1")
        .bind(u.id)
        .execute(&c.state.db)
        .await
        .unwrap();
    assert_eq!(
        patch(u.cookie.clone(), "Admin".into()).await,
        StatusCode::OK
    );
    // nobody can register a reserved username
    let (st, body, _) = call(
        &c.app,
        Method::POST,
        "/api/auth/register",
        None,
        Some(json!({"email": format!("{}@example.org", u.username), "username": "Admin", "password": "hunter2hunter2"})),
    )
    .await;
    assert_eq!(
        (st, body["code"].as_str()),
        (StatusCode::BAD_REQUEST, Some("invalid_username"))
    );

    // group description: 500 characters max
    let mk = |desc: String, name: &'static str| {
        let app = c.app.clone();
        let ck = u.cookie.clone();
        async move {
            call(
                &app,
                Method::POST,
                "/api/groups",
                Some(&ck),
                Some(json!({"name": name, "description": desc})),
            )
            .await
            .0
        }
    };
    assert_eq!(
        mk("é".repeat(501), "Long Desc One").await,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        mk("é".repeat(500), "Long Desc Two").await,
        StatusCode::CREATED
    );
}

#[tokio::test]
async fn reset_tokens_are_single_outstanding_and_expired_rows_purged() {
    let Some(c) = setup().await else { return };
    let u = register(&c, true).await;
    let email = format!("{}@example.com", u.username);
    let forgot = || async {
        let (st, _, _) = call(
            &c.app,
            Method::POST,
            "/api/auth/forgot-password",
            None,
            Some(json!({"email": email})),
        )
        .await;
        assert_eq!(st, StatusCode::NO_CONTENT);
        token_from_mail(&c.outbox, &email, "/reset-password?token=")
    };
    let count = || async {
        let (n,): (i64,) = sqlx::query_as(
            "SELECT count(*) FROM email_tokens WHERE user_id=$1 AND purpose='reset'",
        )
        .bind(u.id)
        .fetch_one(&c.state.db)
        .await
        .unwrap();
        n
    };
    let t1 = forgot().await;
    let t2 = forgot().await;
    assert_ne!(t1, t2);
    assert_eq!(
        count().await,
        1,
        "issuing a new token drops the earlier one"
    );
    let app = c.app.clone();
    let reset = |t: String| {
        let app = app.clone();
        async move {
            call(
                &app,
                Method::POST,
                "/api/auth/reset-password",
                None,
                Some(json!({"token": t, "new_password": "another-pass-1"})),
            )
            .await
            .0
        }
    };
    assert_eq!(reset(t1).await, StatusCode::BAD_REQUEST);
    assert_eq!(reset(t2).await, StatusCode::NO_CONTENT);
    assert_eq!(
        count().await,
        0,
        "a successful reset clears all reset tokens"
    );

    // a password change also clears any outstanding reset link
    let _t3 = forgot().await;
    assert_eq!(count().await, 1);
    let (st, _, ck) = call(
        &c.app,
        Method::POST,
        "/api/auth/login",
        None,
        Some(json!({"login": email, "password": "another-pass-1"})),
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    let cookie = ck
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .trim_start_matches("wt_session=")
        .to_string();
    let (st, _, _) = call(
        &c.app,
        Method::POST,
        "/api/me/password",
        Some(&cookie),
        Some(json!({"current_password": "another-pass-1", "new_password": "yet-another-1"})),
    )
    .await;
    assert_eq!(st, StatusCode::NO_CONTENT);
    assert_eq!(count().await, 0);

    // expired sessions / tokens are purged
    sqlx::query("INSERT INTO sessions (token_hash, user_id, expires_at) VALUES ('stale-session', $1, now() - interval '1 day')")
        .bind(u.id)
        .execute(&c.state.db)
        .await
        .unwrap();
    sqlx::query("INSERT INTO email_tokens (token_hash, user_id, purpose, expires_at) VALUES ('stale-token', $1, 'verify', now() - interval '1 day')")
        .bind(u.id)
        .execute(&c.state.db)
        .await
        .unwrap();
    let n = wartide_server::services::timeouts::purge_expired(&c.state)
        .await
        .unwrap();
    assert!(n >= 2);
    let (left,): (i64,) = sqlx::query_as(
        "SELECT (SELECT count(*) FROM sessions WHERE token_hash='stale-session') + (SELECT count(*) FROM email_tokens WHERE token_hash='stale-token')",
    )
    .fetch_one(&c.state.db)
    .await
    .unwrap();
    assert_eq!(left, 0);
    // live rows survive
    assert_eq!(
        call(&c.app, Method::GET, "/api/me", Some(&cookie), None)
            .await
            .0,
        StatusCode::OK
    );
}

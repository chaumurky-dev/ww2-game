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
    let _ = dotenvy::dotenv();
    let Ok(url) = std::env::var("DATABASE_URL") else {
        eprintln!("NOTICE: DATABASE_URL not set; skipping DB integration test");
        return None;
    };
    let db = connect_db(&url).await.expect("connect");
    MIGRATOR.run(&db).await.expect("migrate");
    let (mailer, outbox) = Mailer::capture();
    let state = build_state(db, Config::for_test(url), mailer);
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
    let (st, err, _) = call(
        &c.app,
        Method::POST,
        &actions,
        Some(&a.cookie),
        Some(json!({"action": {"type": "noop"}})),
    )
    .await;
    assert_eq!(st, StatusCode::CONFLICT);
    assert_eq!(err["code"], "not_your_turn");
    let (st, env, _) = call(
        &c.app,
        Method::POST,
        &actions,
        Some(&b.cookie),
        Some(json!({"action": {"type": "noop"}})),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{env}");
    assert_eq!(env["seq"], 1);
    let (st, env, _) = call(
        &c.app,
        Method::POST,
        &actions,
        Some(&b.cookie),
        Some(json!({"action": {"type": "end_turn"}})),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{env}");
    let (_, log, _) = call(
        &c.app,
        Method::GET,
        &format!("{actions}?since=1"),
        Some(&a.cookie),
        None,
    )
    .await;
    assert_eq!(log.as_array().unwrap().len(), 1);
    let (_, sum, _) = call(
        &c.app,
        Method::GET,
        &format!("/api/games/{gameid}"),
        Some(&a.cookie),
        None,
    )
    .await;
    assert_eq!(sum["current_power"], "germany");
    let (st, _, _) = call(
        &c.app,
        Method::POST,
        &actions,
        Some(&a.cookie),
        Some(json!({"action": {"type": "end_turn"}})),
    )
    .await;
    assert_eq!(st, StatusCode::OK);

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
    assert_eq!(done["status"], "finished");
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
    let (_, sum, _) = call(
        &c.app,
        Method::GET,
        &format!("/api/games/{id}"),
        Some(&a.cookie),
        None,
    )
    .await;
    assert_eq!(sum["current_power"], "germany");
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
}

# wartide-server

axum 0.8 + sqlx 0.8 (Postgres) backend. Wire types live in `crates/protocol` (`wartide_protocol::…`).

## Run

```sh
cp .env.example .env          # from repo root
docker compose up -d          # Postgres :5432, Mailpit SMTP :1025 / UI :8025
cargo run -p wartide-server   # migrations run on startup; serves SPA from WEB_DIST_DIR
cargo test -p wartide-server  # DB tests run only when DATABASE_URL is set (otherwise they print a notice and skip)
```

Config (env / `.env`): `PORT` (8080), `DATABASE_URL`, `BASE_URL`, `SMTP_HOST/PORT/USER/PASS/FROM`
(no `SMTP_HOST` = emails are only logged), `SESSION_SECRET`, `WEB_DIST_DIR` (`crates/web/dist`),
`DEFAULT_TURN_TIMEOUT_SECS`, `COOKIE_SECURE` (default true), `RATE_LIMIT` (default true).

## Conventions
- Errors are `ApiError {code, message}` JSON with a matching status. Codes: `unauthorized` (401),
  `email_unverified` (403), `forbidden` (403), `not_found` (404), `bad_request`/`invalid_*`/`illegal_action` (400),
  `already_exists`/`not_your_turn`/`game_full`/... (409), `internal` (500). Rate-limited auth routes return 429.
- Auth is the HttpOnly `wt_session` cookie (opaque token, only its salted SHA-256 is stored).
- **V** = requires a verified email; **A** = requires login; **-** = public.
- Layering: `routes/*` (thin) -> `services/*` (logic) -> sqlx runtime-checked queries. Migrations: `migrations/`.

## HTTP endpoints (`/api`)

| Method & path | Auth | Request | Response |
|---|---|---|---|
| POST `/auth/register` | - (rate limited) | `RegisterRequest` | 201 `SessionUser`; emails `BASE_URL/verify?token=` |
| POST `/auth/login` | - (rate limited) | `LoginRequest {login (email or username), password}` | `SessionUser` + `Set-Cookie: wt_session` |
| POST `/auth/logout` | any | - | 204, cookie cleared |
| POST `/auth/verify-email` | - | `VerifyEmailRequest` | 204 |
| POST `/auth/resend-verification` | A | - | 204 |
| POST `/auth/forgot-password` | - | `ForgotPasswordRequest` | 204 (always) |
| POST `/auth/reset-password` | - | `ResetPasswordRequest` | 204 (all sessions revoked) |
| GET `/me` | A | - | `SessionUser` |
| GET `/users/{username}` | - | - | `PublicProfile` (public ladders only) |
| PATCH `/me/profile` | A | `UpdateProfileRequest` | `PublicProfile` |
| GET / PATCH `/me/settings` | A | `UpdateSettingsRequest` | `UserSettings` |
| GET `/leaderboard?mode=two_player\|five_player` | - | - | `[LeaderboardEntry]` (public ladder) |
| POST `/groups` | A | `CreateGroupRequest` | 201 `Group` |
| GET `/groups` | A | - | `[Group]` (mine) |
| GET `/groups/{id}`, `/groups/by-slug/{slug}` | A | - | `Group` (`invite_code` only for owner/admin) |
| POST `/groups/join` | A | `JoinGroupRequest` | `Group` |
| POST `/groups/{id}/leave` | A | - | 204 (owner cannot leave) |
| POST `/groups/{id}/invite/rotate` | A owner/admin | - | `Group` |
| GET `/groups/{id}/members` | A member | - | `[GroupMember]` |
| PUT `/groups/{id}/members/{user_id}/role` | A owner | `SetMemberRoleRequest` | 204 |
| DELETE `/groups/{id}/members/{user_id}` | A owner/admin | - | 204 |
| GET `/groups/{id}/leaderboard?mode=` | A member | - | `[LeaderboardEntry]` (ladder `group:{id}:2p/5p`) |
| POST `/games` | V | `CreatePrivateGameRequest` | `GameSummary` (auto-starts if no other human seats) |
| POST `/games/join` | V | `JoinByInviteRequest` | `GameSummary` (starts when full) |
| GET `/games?status=active\|finished` | A | - | `[GameSummary]` (mine) |
| GET `/games/{id}` | A participant (or public) | - | `GameSummary` |
| GET `/games/{id}/state` | A participant | - | `GameStateResponse` |
| GET `/games/{id}/actions?since=N` | A participant | - | `[GameActionEnvelope]` |
| POST `/games/{id}/actions` | V seat owning current power | `SubmitActionRequest {action}` | `GameActionEnvelope` |
| POST `/games/{id}/start` | V host | - | `GameSummary` (empty seats become bots) |
| POST `/games/{id}/resign` | V participant | - | `GameSummary` (own team loses; rated if ladder) |
| POST `/games/{id}/abandon` | V host | - | `GameSummary` (never rated) |
| GET `/games/{id}/chat?channel=` | A participant | - | `[ChatMessage]` (only channels you may see) |
| POST `/games/{id}/chat` | A participant | `SendChatRequest` | 201 `ChatMessage` |
| POST `/matchmaking/queue` | V | `MatchmakingRequest` | 202 |
| DELETE `/matchmaking/queue` | A | - | 204 |
| GET `/matchmaking/queue` | A | - | `{queued, mode}` |

Other: `GET /healthz` (checks DB), `GET /ws` (WebSocket), everything else serves `WEB_DIST_DIR` with an
`index.html` fallback for client-side routes.

## WebSocket `/ws`
Cookie-authenticated. Send `ClientMessage` (`{"type":"subscribe","game_id":…}`, `unsubscribe`, `ping`);
receive `ServerMessage`: `subscribed`, `game_updated {game_id, seq}` (seq `-1` = status/turn change without a new
log entry; refetch the summary), `chat_received {message}` (filtered per connection by team/power),
`your_turn`, `match_found`, `pong`, `error`. Only seated players may subscribe (spectating is not implemented).

## Ratings (Glicko-2, `services/ratings.rs`)
Ladders: `public:2p`, `public:5p`, `group:{id}:2p`, `group:{id}:5p`. Private games without a group are unrated;
group games affect only that group's ladder; public (matchmade) games affect the public ladder. Team games
(always Axis vs Allies): each player is rated against every opposing human one at a time (win/loss by team result,
pre-game ratings), and the resulting rating/deviation/volatility are averaged. Teammates do not influence each
other; bots are never rated. Resign = your team loses. Abandon = no result.

## Games, turns, timeouts
- Seats: 2p = seat 0 Axis (Germany, Japan), seat 1 Allies (USSR, UK, USA); 5p = one power per seat.
  Turn order USSR, Germany, UK, Japan, USA.
- `games.turn_deadline` = turn start + `settings.turn_timeout_secs`; bot turns are due immediately.
  A 60 s task calls `TurnTimeoutHandler` (default: system chat message `timeout: ...` then skip turn).
- Matchmaking: 10 s task; window +-100 rating, +50 per minute waited (the narrowest window in a group governs).

## Engine / AI integration points (TODO(engine))
- `services/rules.rs`: `trait Rules { initial_state, apply }` with `PassthroughRules`. Until the engine lands the
  server understands `{"type":"end_turn"}` actions and `game_over` events (`{"type":"game_over","winner":"axis"|"allies"}`).
- `services/timeouts.rs`: auto-play via `wartide-ai` for timed-out players and bot seats.
- `services/chat.rs::post_bot_message`: AI commentary.
- `services/games.rs::state_view`: per-viewer projection of hidden info.

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
(no `SMTP_HOST` = emails are only logged), `SMTP_TLS` (`starttls` default, `tls`, or `none` for Mailpit), `SESSION_SECRET`, `WEB_DIST_DIR` (`crates/web/dist`),
`DEFAULT_TURN_TIMEOUT_SECS`, `COOKIE_SECURE` (default true), `RATE_LIMIT` (default true),
`BOT_ACTION_DELAY_MS` (default 0; pause after every computer-player action so bot turns are watchable).

## Conventions
- Errors are `ApiError {code, message}` JSON with a matching status. Codes: `unauthorized` (401),
  `email_unverified` (403), `forbidden` (403, also non-admins on `/admin`), `not_found` (404), `bad_request`/`invalid_*`/`illegal_action` (400),
  `already_exists`/`not_your_turn`/`game_full`/... (409), `internal` (500). Rate limits return 429 `rate_limited` as `ApiError` JSON with a `Retry-After` header: per-IP on the credential routes, per-user on chat (20/min), game creation (10/h), password change (5/15 min), account deletion (3/h). `RATE_LIMIT=false` disables all of them.
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
| POST `/me/password` | A (rate limited) | `ChangePasswordRequest {current_password, new_password}` | 204; revokes all other sessions, keeps the current one |
| GET `/me` | A | - | `SessionUser` (has `is_admin`) |
| DELETE `/me` | A (rate limited) | - | 204, cookie cleared. Soft delete: email/username become `deleted-<id>`, display name "Deleted user", profile/sessions/queue scrubbed; games, seats and chat stay intact; the user disappears from profiles and leaderboards |
| GET `/bots/personas` | - | - | `[BotPersona {id, name, style, verbosity, blurb}]` (fixed catalog of 6) |
| GET `/users/{username}` | - | - | `PublicProfile` (public ladders only; `stats[].current_win_streak/best_win_streak`; `recent_games`: last 10 active/finished public or group games, group games redacted to name/mode/status/result) |
| PATCH `/me/profile` | A | `UpdateProfileRequest` | `PublicProfile`; `timezone` must match `^[A-Za-z_]+(/[A-Za-z0-9_+-]+)*$` (IANA name, max 64) else 400; `""` clears it. `SessionUser.timezone` (from `/me`) is `null` until set, which makes the web client ask once |
| GET / PATCH `/me/settings` | A | `UpdateSettingsRequest` | `UserSettings {email_turn_notifications, email_match_found}` |
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
| POST `/games` | V (rate limited) | `CreatePrivateGameRequest {seats?, mode, name?, bot_seats, bot_personas?, human_seats?, settings, group_id?}` (see Seat model below) | `GameSummary` (auto-starts if no other human seats). `name` 1-60 chars, default `Private game #A1B2`. `bot_personas` is indexed by seat; `null`/missing = default for that seat; only allowed for seats in `bot_seats`; ids from `/bots/personas` (else 400 `invalid_persona`) |
| POST `/games/join` | V | `JoinByInviteRequest` | `GameSummary` (starts when full) |
| GET `/games?status=active\|finished` | A | - | `[GameSummary]` (mine) |
| GET `/games/public?status=active\|finished&page=` | A | - | `Paginated<GameSummary>` (matchmade games, 20 per page, newest first) for the watch list |
| GET `/games/{id}` | A participant, or any user if the game is public or finished | - | `GameSummary` (`name`, `winner`) |
| GET `/games/{id}/state` | A participant or spectator | - | `GameStateResponse {seq, state}`; `state` is the engine `GameState` JSON (everything is public, no fog) |
| GET `/games/{id}/actions?since=N` | A participant or spectator | - | `[GameActionEnvelope {seq, actor (seat), action, events, created_at}]`; `action` is an engine `Action`, `events` the engine `Vec<Event>` (max 1000 per call) |
| POST `/games/{id}/actions` | V seat controlling the pending power | `SubmitActionRequest {action}` (engine `Action`, e.g. `{"type":"end_phase"}`) | `GameActionEnvelope`; 409 `not_your_turn`, 400 `illegal_action` with the engine's message |
| POST `/games/{id}/start` | V host | - | `GameSummary` (empty seats become bots; bots start playing immediately) |
| POST `/games/{id}/resign` | V participant | - | `GameSummary` (a bot takes over your seat, the game continues; you count as a loss, see below) |
| POST `/games/{id}/abandon` | V host | - | `GameSummary` (never rated) |
| GET `/games/{id}/chat?channel=` | A participant or spectator | - | `[ChatMessage]` (only channels you may see; spectators get `all` only, other channels 403) |
| POST `/games/{id}/chat` | A participant (rate limited) | `SendChatRequest` | 201 `ChatMessage` |
| POST `/matchmaking/queue` | V | `MatchmakingRequest` | 202 |
| DELETE `/matchmaking/queue` | A | - | 204 |
| GET `/matchmaking/queue` | A | - | `{queued, mode}` |
| GET `/admin/stats` | admin | - | `AdminStats {users, verified_users, games_lobby, games_in_progress, games_finished, games_abandoned, queue_two_player, queue_five_player}` |
| POST `/admin/games/{id}/abandon` | admin | - | `GameSummary` (lobby/in-progress games only, else 409; never rated) |

Admins: `UPDATE users SET is_admin = TRUE WHERE username = '...';` (no API to grant it).

## Hardening
- Request bodies are limited to 1 MB (413). All `/api` responses carry `Cache-Control: no-store` and `X-Content-Type-Options: nosniff`.
- SPA responses carry `X-Frame-Options: DENY`, `Referrer-Policy`, and a CSP: self + Google Fonts (styles/fonts), `data:` images,
  `connect-src 'self' ws: wss:`, `frame-ancestors 'none'`. The inline wasm boot script emitted by trunk is allowed by a
  `sha256-` hash computed from `WEB_DIST_DIR/index.html` (re-read when the file changes), not by `'unsafe-inline'`.

## Email
`email_templates.rs`: HTML + text (multipart) for verification, password reset, "your turn" and "match found"; branded text only, no
images, inline CSS, user data HTML-escaped. `services/notify.rs` sends the turn / match-found mails and honours
`UserSettings.email_turn_notifications` / `email_match_found` (and skips unverified or deleted accounts). Verification and reset mails
are transactional and always sent.

## Bot personas
Catalog in `services/bots.rs` (ids `brass, frost, kestrel, volkov, tide, thistle`). `game_seats.bot_persona` stores the id;
`SeatAssignment.bot_persona` is the id and `bot_name` the display name. Seats without a choice get `default_for_seat(seat)`.

Other: `GET /healthz` (checks DB), `GET /ws` (WebSocket), everything else serves `WEB_DIST_DIR` with an
`index.html` fallback for client-side routes.

## WebSocket `/ws`
Cookie-authenticated. Send `ClientMessage` (`{"type":"subscribe","game_id":…}`, `unsubscribe`, `ping`);
receive `ServerMessage`: `subscribed`, `game_updated {game_id, seq}` (seq `-1` = status/turn change without a new
log entry; refetch the summary), `chat_received {message}` (filtered per connection by team/power),
`your_turn`, `match_found`, `pong`, `error`. Seated players get everything they may see; any logged-in user may subscribe to a public or finished game as a spectator and receives `game_updated` and `all`-channel chat, never team/power chat.

## Ratings (Glicko-2, `services/ratings.rs`)
Ladders: `public:2p`, `public:5p`, `group:{id}:2p`, `group:{id}:5p`. Private games without a group are unrated;
group games affect only that group's ladder; public (matchmade) games affect the public ladder. Team games
(always Axis vs Allies): each player is rated against every opposing human one at a time (win/loss by team result,
pre-game ratings), and the resulting rating/deviation/volatility are averaged. Teammates do not influence each
other; bots are never rated. A resigned player (also one replaced after three missed turns in a row) is always
rated as a loss at game end, whatever the outcome; if every human has resigned the game ends at once and the
other team wins. Abandon = no result.

## Seat model (private games)
`CreatePrivateGameRequest.seats: [SeatSpec {powers, kind: host|open|bot, persona?}]` (1-5 seats, in seat order). The five
powers must be partitioned exactly once (each power in exactly one seat, no empty seat), exactly one seat is `host`
(the creator), `persona` only on `bot` seats (default by seat index). Zero `open` seats = the game starts at once;
otherwise friends join by invite code, each taking the next free `open` seat with all its powers (`seat` order), and the
host may start early (empty seats become bots). When `seats` is present it supersedes `mode`, `human_seats`,
`bot_seats` and `bot_personas`; those legacy fields still work and are mapped onto seats (`two_player`: seat 0 = Axis,
seat 1 = Allies; `five_player`: one power per seat; the first non-bot seat is the host, other human seats are open).
The stored `games.mode` is derived (`GameMode::derive`): `two_player` for exactly two seats split by team,
`five_player` for five single-power seats, otherwise `custom` (migration 0004 widens the CHECK). Presets for the
dialog live in `wartide_protocol::game::preset_layout`. **Rating**: only `two_player`/`five_player` layouts can be
rated (group ladders); `custom` layouts are never rated, also not in a group. Public matchmaking is unchanged
(2p/5p only; `custom` is rejected).
Multi-power seats: `seat_for_power` keys off `game_seats.powers`; the bot driver plays every power of a bot seat (one
`wartide_ai::Bot` per game+power, same persona); a human seat gets a WS `your_turn` for each of its powers, but the
turn e-mail only when the acting seat changes.

## Games, turns, timeouts
- Seats: 2p = seat 0 Axis (Germany, Japan), seat 1 Allies (USSR, UK, USA); 5p = one power per seat.
  A seat may act when it controls the power named by the engine's `pending_decision` (which is the
  defender during casualty selection / sub withdrawal) and is not bot-controlled.
- `GameSummary`: `current_power` (whose turn), `phase` (engine phase string), `round`, `pending_power` (who must act now).
  They are denormalised into `games.{round,current_power,phase,pending_power}` after every action.
- Settings mirror the engine (`GameSettings` in `crates/protocol`): `weapons_development`, `axis_economic_victory`
  (default 84, `null` = off), `allied_economic_victory`, `soviet_first_turn_restriction`, `original_complexes_only`,
  `total_victory`, `germany_jet_power`, `japan_super_subs`, `turn_timeout_secs`. The old `economic_victory`/
  `economic_victory_threshold` pair was migrated to `axis_economic_victory` (migration 0003). The engine seed is
  `games.seed` (random per game).
- Every action goes through `games::apply_locked`: one transaction that locks the game row, parses the stored
  `GameState`, calls `wartide_engine::apply(state, pending_power, action, seed)` and appends action + events + new
  state. The RNG seed of action `seq` is `splitmix64(fnv1a64("wartide/action" ++ le64(games.seed) ++ le64(seq)))`
  (`rules::action_seed`), so a game replays from its log (`wartide_engine::replay`).
- Per-game serialisation: `state.runtime.lock(game_id)` (one `tokio::sync::Mutex` per game) is held while an action is
  applied, by HTTP actions, resign, timeout handling and the bot driver alike.
- **Bot driver** (`services/bots.rs`, `spawn_driver`): started after game start, after every accepted action, after
  resign/timeouts and by the sweep. One task per game (extra requests set a "rerun" flag). While `pending_decision`
  names a power on a bot seat it asks `wartide_ai::Bot` (cached per game+power in memory, rebuilt from
  `(persona, power, hash(game_seed, power))` after a restart) for the next action, applies it and posts the bot's
  `Commentary` lines to the bot's **team** chat channel. At most 5000 actions per pass (the sweep resumes a game
  after that). A rejected action is retried; after 3 rejections for the same decision the engine's `RandomPolicy`
  plays it (20 tries), then `EndPhase` (phase decisions) or `Concede`, and an error is logged. `BOT_ACTION_DELAY_MS`
  slows bots down.
- Deadlines: when the pending power changes the clock is set: humans get `now + turn_timeout_secs` (plus WS
  `your_turn` and the e-mail when the acting seat changes); bot turns are due immediately so the 60 s sweep also
  restarts bots after a server restart.
- **Timeouts** (60 s sweep -> `TurnTimeoutHandler`): system chat line `timeout: ...`, `game_seats.timeouts += 1`, clock
  restarted, and a computer player (the seat's default persona) plays that power until the engine moves on to a
  different power (temporary takeover). Three consecutive misses (`missed_in_row`, reset by any human action) make the
  seat bot-controlled for good (`resigned = true`, rated as a loss).
- **Game end**: when the engine reports `GameOver`, the game becomes `finished` with `winner_team`, ratings are applied
  (team result, resigned seats lose), a system chat line is posted and `game_updated` broadcast. **Resign** hands all
  of the user's seats to bot "brass" (`resigned = true`) and the game continues. **Abandon** (host/admin) is unrated.
- Matchmaking: 10 s task; window +-100 rating, +50 per minute waited (the narrowest window in a group governs).

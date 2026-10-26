# Wartide architecture

## Overview
```
 browser (Leptos/WASM, crates/web) ──HTTP /api, WS /ws──▶ axum server (crates/server)
        │ uses wartide-engine (wasm) for                      │ owns Postgres, auth, lobby, chat,
        │ legality hints / previews                           │ matchmaking, ratings, timeouts
        └─ shares DTOs: crates/protocol ◀────────────────────┘
                                                              │ calls
                                     wartide-engine (crates/engine)  ◀── wartide-ai (crates/ai)
```
One container (server + static SPA) behind an ALB on ECS Fargate; RDS Postgres; SES for mail.

## Engine (`wartide-engine`) — the single source of truth
Pure, deterministic, `std`-only, compiles to wasm. No I/O, no clocks, no global RNG.

Key types (module names in parentheses):
- `Power {Ussr, Germany, Uk, Japan, Usa}`, `Team {Allies, Axis}` (`power`).
- `UnitType {Infantry, Armor, Fighter, Bomber, Battleship, Carrier, Transport, Submarine, AaGun, IndustrialComplex}`
  with `stats()` from `data/units.json` (`units`).
- `MapData` (`map`): territories, sea zones, adjacency, IPC values, original owners, capitals,
  starting units — loaded once from `data/map.json` (`include_str!`, `OnceLock`). `LocationId`
  is a small integer index; string ids are used only in JSON.
- `GameSettings { weapons_development, economic_victory, economic_victory_threshold, … }`.
- `GameState` (`state`): round, `current_power`, `phase`, per-power treasury & techs, owner of each
  territory, every `Unit { id, kind, owner, location, moves_used, cargo, … }`, pending purchases,
  pending battles, the in-progress `Battle` (step machine: AA fire → bombardment → sub strike →
  attacker fire → defender fire → casualties → retreat/press), income cache, `winner`.
  All information in the game is public, so every seat sees the full `GameState`.
- `Phase {DevelopWeapons, Purchase, CombatMove, Combat, NonCombatMove, Mobilize, CollectIncome}`.
- `Action` (`action`): `BuyResearch{dice}`, `Purchase{items}`, `Move{units, path}`,
  `Load{units, transport}`, `Unload{transport, units, to}`, `EndPhase`, `BeginBattle{location}`,
  `DeclareStrategicBombing{location, bombers}`, `SelectCasualties{units}`, `Retreat{to}`,
  `Submerge{subs}`, `PressAttack`, `Place{unit, location, count}`. Plus `Concede` (power leaves;
  server hands the seat to a bot).
- `Event` (`event`): everything that happened, incl. every die roll, for logs/replays/UI animation.
- `apply(&mut GameState, actor: Power, &Action, seed: u64) -> Result<Vec<Event>, RuleError>`:
  validates completely (server trusts nothing from clients), mutates, returns events. RNG is
  `ChaCha8Rng::seed_from_u64(seed)`; the server derives `seed = hash(game_seed, action_seq)`,
  so the whole game replays from the action log. Automatic steps (dice, income, phase/turn
  advance when nothing is left to decide, victory check) happen inside `apply`.
- Query helpers for UI and AI (`query`): `reachable(state, unit) -> Vec<(LocationId, Vec<LocationId>)>`,
  `legal_placements`, `purchase_options`, `pending_decision(state) -> Decision` (whose input is
  needed and what kind), `income(state, power)`, `battle_odds` (optional Monte-Carlo helper).

Seats: in 5-player games a seat is one power; in 2-player games a seat is a team; in private
games the host assigns each of the five power seats to a human (possibly the same human several
times), or to a bot. The server maps `(game, user) -> powers controlled` and asks the engine
whether `actor` may act.

## AI (`wartide-ai`)
`Bot::next_action(&GameState, power) -> (Action, Vec<Commentary>)`. Heuristic planner
(threat/value maps, purchase templates per power, battle odds via `query::battle_odds`, retreat
thresholds). `Commentary` strings are posted by the server into the team chat ("I'm massing
infantry in Karelia; can you cover the Baltic?"). Personalities tune aggression and verbosity.

## Server (`wartide-server`)
axum 0.8 + sqlx/Postgres. Append-only `game_actions` log + latest `game_states` snapshot.
Turn loop: on each accepted action the server broadcasts `GameUpdated` over WS; if the seat to
act is a bot it runs the bot synchronously (bounded) until a human must decide. Turn deadlines
(`turn_timeout_secs`, default 48h) are enforced by a background task: on expiry the bot plays the
remainder of the turn for the absent player. Ratings: Glicko-2 per ladder (`public:2p`,
`public:5p`, `group:{id}:{2p|5p}`); private non-group games are unrated.

Layout: `routes/*` are thin axum handlers; `services/*` hold the logic and all SQL. The games
service is a directory module: `games/rows.rs` (row types, column lists, seat lookups),
`games/read.rs` (summaries, listings, state view), `games/lobby.rs` (create, join, start),
`games/play.rs` (`submit_action` → `apply_locked`, the one action pipeline shared by humans,
bots and automatic defender decisions), `games/ending.rs` (finish, resign, abandon).
`services/transition.rs` is the pure rule that turns a before/after pending decision into
"start a clock / notify / finish" and is unit-tested without a database. `services/rules.rs`
is the engine adapter (settings mapping, applying an action); seed derivation lives in
`wartide_protocol::seed` so the web replay uses the same function. Protocol DTOs carry the
engine's `GameState`, `Action` and `Event` types directly (JSON only at the Postgres boundary via
`sqlx::types::Json`), and the `status`/`kind`/`mode` TEXT columns decode straight into the
protocol enums (`sqlx::Type` derive behind the protocol crate's server-only `sqlx` feature).
`Power` and `Team` exist once, in the engine; protocol re-exports them.

## Web (`wartide-web`)
Leptos 0.8 CSR. SVG map generated from public-domain Natural Earth geometry (see
`scripts/mapgen/`), unit stacks rendered as tinted icon badges per power, phase-driven action
panel, battle board with dice animation, chat drawer (All / Team / Power), lobby, profile, groups,
leaderboards. Uses `wartide-engine` in wasm for instant legality hints; the server re-validates.

## Data flow for a move
client `POST /api/games/{id}/actions {action}` → server loads state, checks seat → `engine::apply`
→ persist action+events+state in one transaction → broadcast → if next actor is a bot, run bot →
repeat → if a human's turn starts, set deadline, send WS `YourTurn` + optional email.

# Wartide — project guide for Claude Code

Wartide is an online, asynchronous (play-by-web) implementation of the *mechanics* of the
1986 "Classic, 2nd edition" global WWII strategy board game (five powers, IPC economy,
dice combat). Mechanics are implemented exactly; all names, artwork, icons and text are
original. **Never** use the trademarked name of the original game, its logo, box art,
sculpt silhouettes, or rulebook prose anywhere in code, assets or UI. Geographic
territory names are fine. Call production points "IPC" (generic acronym) — fine.

## Layout (Cargo workspace)
- `crates/engine`   — `wartide-engine`: pure, deterministic rules engine (no I/O, wasm-safe).
                      Map data lives in `crates/engine/data/*.json` and is `include_str!`'d.
- `crates/ai`       — `wartide-ai`: heuristic computer player + chatty strategy commentary.
- `crates/protocol` — `wartide-protocol`: serde DTOs shared by server and web (REST + WS).
- `crates/server`   — `wartide-server`: axum 0.8 + sqlx 0.8/Postgres; auth, lobby, games, chat,
                      matchmaking, ratings, groups, email, turn timeouts.
- `crates/web`      — `wartide-web`: Leptos 0.8 CSR (wasm) frontend built with trunk; SVG map.
- `infra/terraform` — AWS (ECS Fargate, RDS Postgres, ALB, S3+CloudFront, SES, Secrets Manager).
- `.github/workflows` — CI (fmt, clippy, test, wasm build) and deploy to AWS.
- `docs/`           — RULES.md (authoritative rules reference), ARCHITECTURE.md, DECISIONS.md.
- `docker-compose.yml` + `justfile` — local sandbox (Postgres + Mailpit + server + web).

## Conventions
- Rust 2021, `cargo fmt`, `cargo clippy -D warnings` must pass. Tests live beside code.
- Engine is the single source of truth: server and AI call `engine::apply`; the web client
  uses the same crate (wasm) for client-side legality hints only, server re-validates.
- All randomness in the engine comes from an injected seeded RNG (`rand_chacha`) so games
  are replayable from the action log.
- Commit small and often with imperative messages. Don't commit secrets; use `.env`.
- `main` is branch-protected: work on a topic branch, `gh pr create --fill`, then
  `gh pr merge --squash --auto` (merges once CI passes). Never try to push to `main` directly.
- Local run: `just dev` (see justfile). Local DB: `postgres://wartide:wartide@localhost:5432/wartide`.
- Delegate implementation to cheaper models where possible; the orchestrator reviews.

## Rules source
`docs/RULES.md` is the authority on game mechanics for this project. If engine behaviour and
RULES.md disagree, fix the engine (or fix RULES.md with a cited source and note in DECISIONS.md).

## Local toolchain note (this Mac)
Homebrew's `cargo` (no wasm target) shadows rustup's. Run `source scripts/env.sh` first in every
shell (puts `/usr/local/opt/rustup/bin` on PATH) — or wasm builds fail with "can't find crate for core".

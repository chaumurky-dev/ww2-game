# Decisions log

- 2026-10-24: Project name "Wartide". Rust workspace; axum server, Leptos CSR web client,
  Postgres via sqlx, AWS ECS Fargate deploy via GitHub Actions + Terraform.
- 2026-10-24: Rules edition: Classic, 2nd edition (1986). Optional rules (economic victory,
  weapons development) are exposed as per-game settings with Classic defaults.
- 2026-10-24: Ratings use Glicko-2, maintained separately per ladder (public 2p, public 5p,
  and per-group). Private games never affect the public ladders.
- 2026-10-24: Private games always have five power seats; the host assigns each seat to a human
  (one human may hold several seats) or a bot persona. 2-player public = one human per team;
  5-player public = one human per power.
- 2026-10-24: Server stores an append-only action log plus the latest state snapshot; every action's
  dice come from `seed = hash(game_seed, seq)` so games replay deterministically.

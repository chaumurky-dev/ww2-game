# Decisions log

- 2026-10-24: Project name "Wartide". Rust workspace; axum server, Leptos CSR web client,
  Postgres via sqlx, AWS ECS Fargate deploy via GitHub Actions + Terraform.
- 2026-10-24: Rules edition: Classic, 2nd edition (1986). Optional rules (economic victory,
  weapons development) are exposed as per-game settings with Classic defaults.
- 2026-10-24: Ratings use Glicko-2, maintained separately per ladder (public 2p, public 5p,
  and per-group). Private games never affect the public ladders.

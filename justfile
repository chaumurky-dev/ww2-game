set dotenv-load := true
set shell := ["bash", "-cu"]

# Prefer the rustup toolchain (has the wasm target) over Homebrew's cargo; see scripts/env.sh
export PATH := "/usr/local/opt/rustup/bin:" + env_var("HOME") + "/.cargo/bin:" + env_var("HOME") + "/.local/bin:" + env_var("PATH")
export DYLD_FALLBACK_LIBRARY_PATH := env_var("HOME") + "/.rustup/toolchains/stable-x86_64-apple-darwin/lib"
export COOKIE_SECURE := env_var_or_default("COOKIE_SECURE", "false")

# Local defaults (override in .env)
export DATABASE_URL := env_var_or_default("DATABASE_URL", "postgres://wartide:wartide@localhost:5432/wartide")
export BASE_URL := env_var_or_default("BASE_URL", "http://localhost:8080")
export SMTP_HOST := env_var_or_default("SMTP_HOST", "localhost")
export SMTP_PORT := env_var_or_default("SMTP_PORT", "1025")
export SMTP_FROM := env_var_or_default("SMTP_FROM", "Wartide <noreply@wartide.local>")
export SESSION_SECRET := env_var_or_default("SESSION_SECRET", "local-dev-session-secret-change-me-0123456789abcdef0123456789abcdef")
export WEB_DIST_DIR := env_var_or_default("WEB_DIST_DIR", "crates/web/dist")

default:
    @just --list

db-up:
    docker compose up -d --wait postgres mailpit

db-down:
    docker compose down

migrate:
    sqlx migrate run --source crates/server/migrations

server:
    cargo run -p wartide-server

web:
    # proxies for /api and /ws are configured in crates/web/Trunk.toml
    cd crates/web && trunk serve

dev: db-up
    #!/usr/bin/env bash
    set -euo pipefail
    trap 'kill 0' EXIT INT TERM
    just server &
    just web &
    wait

test:
    cargo test --workspace

lint:
    cargo fmt --all -- --check
    cargo clippy --workspace --exclude wartide-web --all-targets -- -D warnings
    cargo clippy -p wartide-web --target wasm32-unknown-unknown -- -D warnings

build-image:
    docker build -t wartide:local .

psql:
    docker compose exec postgres psql -U wartide wartide

reset-db:
    docker compose down -v
    docker compose up -d --wait postgres

# Wait for the local server and print OK (works for `just dev` and the compose app profile)
smoke:
    scripts/dev-smoke.sh

# Containerised app: build + run (http://localhost:8080)
app-up:
    docker compose --profile app up -d --build --wait

app-down:
    docker compose --profile app stop app

# Full API flow (register, Mailpit verify, bot game); BASE=http://localhost:8080 by default
smoke-full:
    scripts/dev-smoke.sh --full

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
    cd crates/web && trunk serve --proxy-backend=http://localhost:8080/api --proxy-backend=ws://localhost:8080/ws --proxy-ws

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

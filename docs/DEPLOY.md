# Deploying Wartide

## Local sandbox

Prereqs: Docker (colima ok), `just`, Rust + `wasm32-unknown-unknown`, `trunk`, `sqlx-cli` (optional;
the server runs its own migrations at startup).

```
just db-up      # Postgres 16 (localhost:5432, wartide/wartide/wartide) + Mailpit (SMTP 1025, UI http://localhost:8025)
just dev        # server on :8080 and `trunk serve` (proxies /api and /ws to the server)
scripts/dev-smoke.sh   # waits for http://localhost:8080/healthz, prints OK
just lint | just test | just psql | just reset-db
```

Defaults for `DATABASE_URL`, `BASE_URL`, `SMTP_*`, `SESSION_SECRET`, `WEB_DIST_DIR` are baked into the
justfile; override them in a `.env` file. Verification emails land in Mailpit.

### Containerised run (verified)

```
just build-image                  # docker build -t wartide:local .  (needs BuildKit/buildx: cache mounts)
just app-up                       # docker compose --profile app up -d --build --wait
just smoke                        # waits for /healthz, prints OK
open http://localhost:8080        # SPA; verification mail appears in Mailpit http://localhost:8025
just app-down                     # stop the app, leave Postgres/Mailpit running
```

The compose `app` service sets `COOKIE_SECURE=false` (the server defaults to secure cookies, which browsers
drop over plain http), `SMTP_HOST=mailpit` and `DATABASE_URL` pointing at the `postgres` service.
The image is multi-stage: `rust:1-bookworm` builds the server and the SPA (trunk, with apt `binaryen` for
wasm-opt); the runtime stage is `debian:bookworm-slim`, runs as uid 10001 (non-root) and serves the SPA from
`/app/dist`. Image: about 118 MB of layers
(debian-slim 85 MB, ca-certificates 10 MB, server binary 18 MB, SPA 6 MB; `docker images` may show a larger
number on colima because of the containerd image store). Cold build about 12 min on an x86_64 colima VM, about
2 min with a warm cache.

Verified flow (`scripts/dev-smoke.sh --full`, or `just smoke-full`): register, read the verification mail from
Mailpit, verify, log in, create a private game vs 4 bots, then play USSR's turn by ending phases; the bots
(seat 1-4) then made about 20 actions in the first 10 s. Bots stall only when they need a decision from the human
(e.g. choosing casualties when USSR defends). Notes:

- Colima: `docker build` needs BuildKit (cache mounts). If the macOS docker CLI has no buildx plugin, build
  inside the VM: `colima ssh -- sh -c 'cd $PWD && docker buildx build --load -t wartide:local .'`.
- If port 8080 is taken (for example by `just dev`), run the container on another port:
  `APP_PORT=18080 docker compose --profile app up -d --wait app` (BASE_URL follows APP_PORT).
- Bot commentary is posted to the team chat channel, so `GET /chat?channel=all` is normally empty.

## Architecture on AWS

One ECS Fargate task (0.5 vCPU / 1 GB) runs the container, which serves API, WebSocket and SPA, behind an
ALB (HTTP redirects to HTTPS, ACM cert via DNS). RDS Postgres 16 (`db.t4g.micro`) sits in private subnets
and accepts traffic only from the ECS security group. Secrets (DATABASE_URL, SESSION_SECRET, SMTP creds)
live in Secrets Manager and are injected by ECS. Email goes through SES SMTP.

Cost tradeoff: there is no NAT gateway. Tasks run in public subnets with public IPs (inbound locked to the
ALB security group) so they can reach ECR, Secrets Manager, SES and CloudWatch. Move tasks to private subnets
and add a NAT or VPC endpoints if you want stricter isolation.

## One-time bootstrap

1. Create an S3 bucket (and optionally a DynamoDB lock table) for Terraform state.
2. `cd infra/terraform && cp backend.hcl.example backend.hcl && cp terraform.tfvars.example terraform.tfvars`
   and edit both (`domain_name`, `route53_zone_id` or empty, `github_repo = "owner/repo"`). If the account
   already has the GitHub OIDC provider, set `create_github_oidc_provider = false`.
3. `terraform init -backend-config=backend.hcl`
4. Create ECR first so an image exists before the service starts:
   `terraform apply -target=aws_ecr_repository.app`, then build and push an image
   (`docker build` + `docker push <ecr_repository_url>:latest`, linux/amd64), or push by running the Deploy
   workflow after step 6 (the first service start will fail health checks until an image exists).
5. `terraform apply`.
   - With `route53_zone_id` set, ACM validation, SES verification/DKIM and the A record are automatic.
   - Without it: apply will wait/fail at the HTTPS listener; add the records from the `acm_validation_records`
     and `ses_records` outputs at your DNS provider, then re-run apply. Point your domain at `alb_dns_name`.
6. Set GitHub repository configuration (Settings > Secrets and variables > Actions):
   - Secret `AWS_DEPLOY_ROLE_ARN` = output `deploy_role_arn`
   - Variables: `AWS_REGION`, `ECR_REPOSITORY` (last path segment of `ecr_repository_url`, e.g. `wartide-prod`),
     `ECS_CLUSTER`, `ECS_SERVICE`, `ECS_TASK_FAMILY` (= `ecs_task_family` output)
7. SES starts in sandbox mode: request production access in the SES console, otherwise you can only send to
   verified addresses.

The deploy role trusts only `repo:<github_repo>:ref:refs/heads/main`.

## CI/CD flow

- `ci.yml` (push/PR): rustfmt, clippy (native + wasm32 for web), `cargo test --workspace` against a Postgres
  service container, trunk release build.
- `deploy.yml`: triggered by `workflow_run` when CI succeeds on `main` (or manually via workflow_dispatch). It
  checks out the CI-validated SHA, builds and pushes `:<sha>` and `:latest` with buildx (GHA cache), fetches the
  current task definition, swaps the image, and deploys waiting for service stability. The ECS circuit breaker
  rolls back failed deploys. Terraform ignores task definition and desired-count drift so it does not fight CI.
- Database migrations run at server startup.

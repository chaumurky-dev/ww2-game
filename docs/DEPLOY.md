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

One ECS Fargate Spot task (0.25 vCPU / 0.5 GB) runs the container, which serves API, WebSocket and SPA, behind an
ALB (HTTP redirects to HTTPS, ACM cert via DNS). RDS Postgres 16 (`db.t4g.micro`) sits in private subnets
and accepts traffic only from the ECS security group. Secrets (DATABASE_URL, SESSION_SECRET, SMTP creds)
live in Secrets Manager and are injected by ECS. Email goes through SES SMTP.

Cost tradeoff: there is no NAT gateway. Tasks run in public subnets with public IPs (inbound locked to the
ALB security group) so they can reach ECR, Secrets Manager, SES and CloudWatch. Move tasks to private subnets
and add a NAT or VPC endpoints if you want stricter isolation.

## One-time bootstrap

Target: `ww2.chaumurky.net` (parent zone at GoDaddy), us-east-1, cheapest configuration. Needs Terraform >= 1.10
(S3 native state locking, no DynamoDB), the AWS CLI, `gh`, and Docker.

```
export AWS_PROFILE=<your-profile>

# 1. State bucket (idempotent: private, versioned, encrypted, public access blocked)
scripts/aws-bootstrap.sh

# 2. Configure and init
cd infra/terraform
cp backend.hcl.example backend.hcl
cp terraform.tfvars.example terraform.tfvars      # already holds the intended prod values
terraform init -backend-config=backend.hcl

# 3. Create the hosted zone first, then delegate it
terraform apply -target=aws_route53_zone.main
terraform output route53_name_servers             # four ns-*.awsdns-* names

# 4. At GoDaddy (DNS for chaumurky.net) add four NS records: Name/Host `ww2`, one per name server above.
#    Wait until delegation answers:
dig NS ww2.chaumurky.net +short

# 5. ECR first so an image can exist before the service starts
terraform apply -target=aws_ecr_repository.app
aws ecr get-login-password --region us-east-1 | docker login --username AWS --password-stdin "$(terraform output -raw ecr_repository_url | cut -d/ -f1)"
docker buildx build --platform linux/amd64 -t "$(terraform output -raw ecr_repository_url):latest" --push ../..

# 6. Everything else (ACM validation, SES records and the ALB alias are created in the new zone)
terraform apply

# 7. GitHub Actions configuration
gh secret set AWS_DEPLOY_ROLE_ARN --repo chaumurky-dev/ww2-game --body "$(terraform output -raw deploy_role_arn)"
gh variable set AWS_REGION        --repo chaumurky-dev/ww2-game --body "$(terraform output -raw aws_region)"
gh variable set ECR_REPOSITORY    --repo chaumurky-dev/ww2-game --body "$(terraform output -raw ecr_repository_name)"
gh variable set ECS_CLUSTER       --repo chaumurky-dev/ww2-game --body "$(terraform output -raw ecs_cluster)"
gh variable set ECS_SERVICE       --repo chaumurky-dev/ww2-game --body "$(terraform output -raw ecs_service)"
gh variable set ECS_TASK_FAMILY   --repo chaumurky-dev/ww2-game --body "$(terraform output -raw ecs_task_family)"
```

Notes:

- The hosted zone apex is the app hostname itself (`create_route53_zone = true`), so the A alias, ACM
  validation CNAME and SES TXT/DKIM records live in that zone. Do not apply the full stack before the NS
  delegation answers: ACM validation would hang until it times out. If you already have a zone, set
  `create_route53_zone = false` and `route53_zone_id`; with neither, add the records from the
  `acm_validation_records` and `ses_records` outputs manually and point your domain at `alb_dns_name`.
- If the account lacks the GitHub OIDC provider, set `create_github_oidc_provider = true`.
- SES starts in sandbox mode: request production access in the SES console, otherwise you can only send to
  verified addresses.
- The deploy role trusts only `repo:<github_repo>:ref:refs/heads/main`.

### Cost-minimised posture

- ECS service runs on `FARGATE_SPOT` (`use_fargate_spot = true`). Spot is about 70% cheaper but AWS can reclaim
  the task with a 2 minute warning; with a single task that means a brief outage and dropped WebSockets until
  ECS starts a replacement. Set `use_fargate_spot = false` for on-demand. The cluster has both capacity providers.
- Task 0.25 vCPU / 512 MB, `desired_count = 1` (raise `task_cpu`, `task_memory`, `desired_count`).
- No NAT gateway, no ALB access logs, Container Insights off, log retention 7 days, ECR keeps the last 5 images.
- RDS: `db.t4g.micro`, gp3 20 GB, single AZ, 1 day of backups, no Performance Insights or enhanced monitoring.
  This is a cheap/dev posture: `deletion_protection = false` and `skip_final_snapshot = true`, so
  `terraform destroy` deletes the data. To harden, set in `terraform.tfvars`:
  `db_deletion_protection = true`, `db_skip_final_snapshot = false`, `db_backup_retention_days = 7`,
  `db_multi_az = true` (doubles RDS cost), and optionally `use_fargate_spot = false`.

| Item (us-east-1, approx.) | Monthly |
|---|---|
| Application Load Balancer (hourly + minimal LCU) | $17 |
| Fargate Spot, 0.25 vCPU / 0.5 GB, 1 task | $3 (on-demand about $9) |
| RDS `db.t4g.micro` single AZ | $12 |
| RDS gp3 20 GB + backups | $2.5 |
| Route53 hosted zone | $0.50 |
| Secrets Manager, 4 secrets ($0.40 each) | $1.60 |
| Public IPv4 addresses (ALB x2, task x1, $0.005/h each) | $11 |
| Data transfer out, ECR, CloudWatch, SES | $1-3 |
| Total | about $45-50 |

Numbers are rough list prices and vary with usage.

## CI/CD flow

- `ci.yml` (push/PR): rustfmt, clippy (native + wasm32 for web), `cargo test --workspace` against a Postgres
  service container, trunk release build.
- `deploy.yml`: triggered by `workflow_run` when CI succeeds on `main` (or manually via workflow_dispatch). It
  checks out the CI-validated SHA, builds and pushes `:<sha>` and `:latest` with buildx (GHA cache), fetches the
  current task definition, swaps the image, and deploys waiting for service stability. The ECS circuit breaker
  rolls back failed deploys. Terraform ignores task definition and desired-count drift so it does not fight CI.
- Database migrations run at server startup.

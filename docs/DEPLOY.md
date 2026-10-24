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

Full containerised run: `docker compose --profile app up --build` then open http://localhost:8080.

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

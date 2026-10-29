#!/usr/bin/env bash
# Wartide hibernate: stop everything AWS bills by the hour (ECS service, ALB, RDS) and bring it back later.
#
# Subcommands (see docs/DEPLOY.md section 7):
#   down              snapshot + destroy ECS service, ALB, RDS (two terraform applies, each asks to confirm)
#   up [SNAPSHOT]     restore RDS from SNAPSHOT (default: newest wartide-prod-final-*), recreate ALB + service,
#                     redeploy, wait for /healthz
#   status            show what currently exists
#
# State is kept in the local, gitignored infra/terraform/hibernate.auto.tfvars (matches the *.tfvars ignore
# rule) so a plain `terraform apply` later keeps the chosen state.
# Run with the same environment as terraform: AWS_PROFILE=wartide (default here), gh authenticated.
# Environment: AWS_PROFILE (default wartide), AWS_REGION (default us-east-1), ENV_NAME (default prod),
#   GITHUB_REPO (default chaumurky-dev/ww2-game), HEALTH_URL (default https://ww2.chaumurky.net/healthz).
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TF_DIR="$REPO_ROOT/infra/terraform"
AUTO_VARS="$TF_DIR/hibernate.auto.tfvars"
export AWS_PROFILE="${AWS_PROFILE:-wartide}"
export AWS_REGION="${AWS_REGION:-us-east-1}"
export AWS_DEFAULT_REGION="$AWS_REGION"
ENV_NAME="${ENV_NAME:-prod}"
GITHUB_REPO="${GITHUB_REPO:-chaumurky-dev/ww2-game}"
HEALTH_URL="${HEALTH_URL:-https://ww2.chaumurky.net/healthz}"
NAME="wartide-$ENV_NAME"
SNAP_PREFIX="$NAME-final-"
export PATH="$HOME/.local/bin:$PATH"

usage() {
  cat <<USAGE
Usage: scripts/aws-hibernate.sh <down|up [SNAPSHOT]|status>
  down            tear down ALB, ECS service and RDS (final snapshot ${SNAP_PREFIX}<date>), keep everything else
  up [SNAPSHOT]   wake: restore from SNAPSHOT (default newest ${SNAP_PREFIX}*), redeploy, wait for $HEALTH_URL
  status          what exists right now
USAGE
}

die() { printf 'error: %s\n' "$*" >&2; exit 1; }
run() { printf '+ %s\n' "$*" >&2; "$@"; }
tf() { run terraform -chdir="$TF_DIR" "$@"; }

need() { command -v "$1" >/dev/null 2>&1 || die "$1 not found on PATH"; }

hibernated_in_file() {
  [ -f "$AUTO_VARS" ] && grep -Eq '^[[:space:]]*hibernate[[:space:]]*=[[:space:]]*true' "$AUTO_VARS"
}

db_status() { # prints the RDS status or "absent"
  aws rds describe-db-instances --db-instance-identifier "$NAME" \
    --query 'DBInstances[0].DBInstanceStatus' --output text 2>/dev/null || echo absent
}

alb_state() {
  local arn
  arn="$(aws elbv2 describe-load-balancers --names "$NAME" --query 'LoadBalancers[0].LoadBalancerArn' --output text 2>/dev/null || true)"
  if [ -n "$arn" ] && [ "$arn" != "None" ]; then echo present; else echo absent; fi
}

ecs_running() {
  aws ecs describe-services --cluster "$NAME" --services app \
    --query 'services[?status!=`INACTIVE`].runningCount | [0]' --output text 2>/dev/null | sed 's/^None$/0/;s/^$/0/'
}

manual_snapshots() {
  aws rds describe-db-snapshots --snapshot-type manual \
    --query "reverse(sort_by(DBSnapshots[?starts_with(DBSnapshotIdentifier, \`$SNAP_PREFIX\`)], &SnapshotCreateTime))[].[DBSnapshotIdentifier,Status,SnapshotCreateTime,AllocatedStorage]" \
    --output text
}

gh_var() { gh variable get WARTIDE_HIBERNATED --repo "$GITHUB_REPO" 2>/dev/null || echo "(unset)"; }

guard_no_inflight_merges() {
  # Same guard as the deploy race: no auto-merging PRs, no workflow running on main.
  local auto running
  auto="$(gh pr list --repo "$GITHUB_REPO" --state open --json number,autoMergeRequest \
    --jq '[.[] | select(.autoMergeRequest != null) | .number] | join(",")')"
  [ -z "$auto" ] || die "open PR(s) with auto-merge enabled: #${auto//,/, #}. Disable auto-merge or wait for them to merge, then retry."
  running="$(gh run list --repo "$GITHUB_REPO" --branch main --json status,databaseId,name \
    --jq '[.[] | select(.status == "in_progress" or .status == "queued" or .status == "waiting" or .status == "pending") | .name] | join(", ")')"
  [ -z "$running" ] || die "workflow run(s) in progress on main: $running. Wait for them to finish, then retry."
}

cmd_status() {
  echo "hibernate.auto.tfvars : $(if [ -f "$AUTO_VARS" ]; then if hibernated_in_file; then echo 'hibernate = true (hibernated)'; else echo 'present, hibernate = false'; fi; else echo 'absent (not hibernated)'; fi)"
  echo "RDS instance $NAME    : $(db_status)"
  echo "ALB $NAME             : $(alb_state)"
  echo "ECS running count     : $(ecs_running)"
  echo "WARTIDE_HIBERNATED    : $(gh_var)"
  echo "Manual snapshots (${SNAP_PREFIX}*), newest first:"
  manual_snapshots | sed 's/^/  /'
}

cmd_down() {
  need terraform; need aws; need gh
  hibernated_in_file && die "already hibernated per $AUTO_VARS; use 'status' or 'up'."
  [ "$(db_status)" != absent ] || die "RDS instance $NAME not found; nothing to hibernate (try 'status')."

  guard_no_inflight_merges
  run gh variable set WARTIDE_HIBERNATED --repo "$GITHUB_REPO" --body true

  local tag
  tag="$(date -u +%Y%m%d-%H%M)"
  echo ">>> Step 1/2: turn off deletion protection and record the final snapshot name ($SNAP_PREFIX$tag)."
  cat >"$AUTO_VARS" <<EOF
# Written by scripts/aws-hibernate.sh; gitignored. Do not edit by hand.
hibernate               = false
db_deletion_protection  = false
alb_deletion_protection = false
db_final_snapshot_tag   = "$tag"
EOF
  tf apply
  echo ">>> Step 2/2: destroy ECS service, autoscaling, ALB, Route53 app record, alarm, DATABASE_URL version and RDS."
  cat >"$AUTO_VARS" <<EOF
# Written by scripts/aws-hibernate.sh; gitignored. Do not edit by hand.
hibernate               = true
db_deletion_protection  = false
alb_deletion_protection = false
db_final_snapshot_tag   = "$tag"
EOF
  tf apply

  local snap="$SNAP_PREFIX$tag" st="" i
  echo ">>> Waiting for snapshot $snap to become available..."
  for i in $(seq 1 90); do
    st="$(aws rds describe-db-snapshots --db-snapshot-identifier "$snap" \
      --query 'DBSnapshots[0].Status' --output text 2>/dev/null || echo missing)"
    [ "$st" = available ] && break
    echo "  snapshot status: $st (attempt $i/90)"
    sleep 20
  done
  [ "$st" = available ] || die "snapshot $snap is '$st', not available. Do NOT assume the data is safe; check the RDS console."

  cat <<EOF

=== Hibernated ===
Final snapshot : $snap (available)
Still billed   : Route53 zone, Secrets Manager, the snapshot, ECR/S3/CloudWatch storage (about \$4/month)
GitHub         : WARTIDE_HIBERNATED=true (Deploy workflow skips)
Wake up with   : scripts/aws-hibernate.sh up $snap
EOF
}

cmd_up() {
  need terraform; need aws; need gh; need curl
  local snap="${1:-}"
  if [ -z "$snap" ]; then
    snap="$(manual_snapshots | awk 'NR==1 && $2=="available" {print $1}')"
    [ -n "$snap" ] || die "no available ${SNAP_PREFIX}* manual snapshot found; pass one explicitly."
  fi
  aws rds describe-db-snapshots --db-snapshot-identifier "$snap" --query 'DBSnapshots[0].Status' --output text >/dev/null \
    || die "snapshot $snap not found"
  [ "$(db_status)" = absent ] || die "RDS instance $NAME already exists (status $(db_status)); not hibernated?"

  echo ">>> Waking from snapshot $snap"
  cat >"$AUTO_VARS" <<EOF
# Written by scripts/aws-hibernate.sh; gitignored. Do not edit by hand.
hibernate           = false
db_restore_snapshot = "$snap"
EOF
  tf apply
  run gh variable delete WARTIDE_HIBERNATED --repo "$GITHUB_REPO" || true
  run gh workflow run deploy.yml --repo "$GITHUB_REPO" --ref main || echo "warning: could not dispatch deploy.yml; the service runs the Terraform task definition (:latest image)." >&2

  echo ">>> Polling $HEALTH_URL (up to 10 minutes)"
  local i code=""
  for i in $(seq 1 60); do
    code="$(curl -s -o /dev/null -w '%{http_code}' --max-time 8 "$HEALTH_URL" || true)"
    if [ "$code" = 200 ]; then
      echo "healthy: $HEALTH_URL returned 200"
      return 0
    fi
    echo "  attempt $i/60: HTTP ${code:-none}"
    sleep 10
  done
  cat >&2 <<EOF
error: $HEALTH_URL did not return 200 within 10 minutes (last: ${code:-none}).
Check:
  aws ecs describe-services --cluster $NAME --services app --query 'services[0].events[:10]'
  aws logs tail /ecs/$NAME --since 30m
  aws elbv2 describe-target-health --target-group-arn \$(aws elbv2 describe-target-groups --names $NAME --query 'TargetGroups[0].TargetGroupArn' --output text)
  DNS can lag a few minutes after the ALB is recreated.
EOF
  exit 1
}

case "${1:-}" in
  down) cmd_down ;;
  up) shift; cmd_up "$@" ;;
  status) cmd_status ;;
  -h | --help | help | "") usage ;;
  *) usage >&2; exit 2 ;;
esac

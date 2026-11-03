#!/usr/bin/env bash
# Wartide deep hibernate: remove everything AWS bills for (by the hour or by the month) and bring it back later.
# While hibernated the account costs cents per month; the database is kept as a verified pg_dump in the
# Terraform state bucket (db-backups/), not as an RDS snapshot.
#
# Subcommands (see docs/DEPLOY.md section 7):
#   down                    dump + trial-restore + verify the database to S3, then destroy ECS, ALB, RDS, Route53 zone,
#                           SES identity, secrets, alarms, GuardDuty (two terraform applies, each asks to confirm)
#   backup                  ad-hoc dump of the running database to S3 (same verified dump as `down`)
#   up [DUMP_KEY|--empty]   recreate everything, restore DUMP_KEY (default: newest) into the fresh database,
#                           redeploy, wait for /healthz. --empty skips the restore.
#   status                  show what currently exists (no Cost Explorer calls; each costs $0.01)
#
# State is kept in the local, gitignored infra/terraform/hibernate.auto.tfvars (matches the *.tfvars ignore
# rule) so a plain `terraform apply` later keeps the chosen state.
# Run with the same environment as terraform: AWS_PROFILE=wartide (default here), gh authenticated.
# Environment: AWS_PROFILE (default wartide), AWS_REGION (default us-east-1), ENV_NAME (default prod),
#   GITHUB_REPO (default chaumurky-dev/ww2-game), HEALTH_URL (default https://<domain_name>/healthz).
# Needs bash 3.2+ (macOS), python3, aws-cli, terraform, gh, curl, dig. The presigned URLs come from scripts/s3-presign.py.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TF_DIR="$REPO_ROOT/infra/terraform"
AUTO_VARS="$TF_DIR/hibernate.auto.tfvars"
TFVARS="$TF_DIR/terraform.tfvars"
PRESIGN="$REPO_ROOT/scripts/s3-presign.py"
export AWS_PROFILE="${AWS_PROFILE:-wartide}"
export AWS_REGION="${AWS_REGION:-us-east-1}"
export AWS_DEFAULT_REGION="$AWS_REGION"
ENV_NAME="${ENV_NAME:-prod}"
GITHUB_REPO="${GITHUB_REPO:-chaumurky-dev/ww2-game}"
NAME="wartide-$ENV_NAME"
export PATH="$HOME/.local/bin:$PATH"
BACKUP_PREFIX="db-backups/"
DBTOOL_LOG=""

die() { printf 'error: %s\n' "$*" >&2; exit 1; }
run() { printf '+ %s\n' "$*" >&2; "$@"; }
tf() { run terraform -chdir="$TF_DIR" "$@"; }
need() { local c; for c in "$@"; do command -v "$c" >/dev/null 2>&1 || die "$c not found on PATH"; done; }

ACCOUNT="$(aws sts get-caller-identity --query Account --output text 2>/dev/null)" \
  || die "cannot call AWS (profile $AWS_PROFILE); log in / check credentials first"
STATE_BUCKET="wartide-terraform-state-$ACCOUNT"
tf_out() { terraform -chdir="$TF_DIR" output -raw "$1" 2>/dev/null; }
tf_out_json() { terraform -chdir="$TF_DIR" output -json "$1" 2>/dev/null; }
DOMAIN="$(tf_out domain_name || true)"
DOMAIN="${DOMAIN:-ww2.chaumurky.net}"
HEALTH_URL="${HEALTH_URL:-https://$DOMAIN/healthz}"

usage() {
  cat <<USAGE
Usage: scripts/aws-hibernate.sh <down|backup|up [DUMP_KEY|--empty]|status>
  down                 verified pg_dump to s3://$STATE_BUCKET/$BACKUP_PREFIX, then tear down everything with a price
  backup               ad-hoc verified pg_dump of the running database
  up [DUMP_KEY|--empty]  wake: recreate, restore DUMP_KEY (default newest), redeploy, wait for $HEALTH_URL
  status               what exists right now
USAGE
}

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
    --query "reverse(sort_by(DBSnapshots[?starts_with(DBSnapshotIdentifier, \`$NAME-\`)], &SnapshotCreateTime))[].[DBSnapshotIdentifier,Status,SnapshotCreateTime,AllocatedStorage]" \
    --output text
}

gh_var() { gh variable get WARTIDE_HIBERNATED --repo "$GITHUB_REPO" 2>/dev/null || echo "(unset)"; }

# "LastModified<TAB>Key<TAB>Size" per dump, newest first.
list_dumps() {
  aws s3api list-objects-v2 --bucket "$STATE_BUCKET" --prefix "$BACKUP_PREFIX" \
    --query 'Contents[].[LastModified,Key,Size]' --output text 2>/dev/null \
    | grep -v '^None$' | grep -v "[[:space:]]${BACKUP_PREFIX}[[:space:]]" | sort -r || true
}
newest_dump() { list_dumps | head -1 | cut -f2; }

delegation_set_in_tfvars() {
  [ -f "$TFVARS" ] && grep -Eq '^[[:space:]]*route53_delegation_set_id[[:space:]]*=[[:space:]]*"[^"]+"' "$TFVARS"
}

# Name servers as sorted, lowercase, dot-less lines.
norm_ns() { tr 'A-Z' 'a-z' | sed 's/\.$//' | grep -v '^$' | sort; }
zone_ns() { tf_out_json route53_name_servers | python3 -c 'import json,sys; print("\n".join(json.load(sys.stdin)))' | norm_ns; }
dns_delegated_ns() {
  local parent srv
  parent="${DOMAIN#*.}"
  srv="$(dig +short NS "$parent" | head -1)"
  [ -n "$srv" ] || return 0
  dig +short NS "$DOMAIN" "@$srv" | norm_ns
}

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

# run_dbtool MODE URL [KEY=VALUE ...]: run the one-off dbtool Fargate task, wait for it, print its "dbtool:"
# log lines (also left in $DBTOOL_LOG). Returns 1 if the container did not exit 0.
run_dbtool() {
  local mode="$1" url="$2" family subnets sg overrides arn tid exitc reason code i
  shift 2
  family="$(tf_out dbtool_task_family || true)"
  [ -n "$family" ] && [ "$family" != null ] \
    || die "the dbtool task definition is not in Terraform state; run 'terraform apply' (hibernate = false) first."
  subnets="$(tf_out_json public_subnet_ids | python3 -c 'import json,sys; print(",".join(json.load(sys.stdin)))')"
  sg="$(tf_out ecs_security_group_id)"
  [ -n "$subnets" ] && [ -n "$sg" ] || die "could not read public_subnet_ids / ecs_security_group_id from Terraform outputs"
  overrides="$(python3 - "$mode" "$url" "$@" <<'PY'
import json, sys
env = [{"name": "MODE", "value": sys.argv[1]}, {"name": "URL", "value": sys.argv[2]}]
for kv in sys.argv[3:]:
    k, v = kv.split("=", 1)
    env.append({"name": k, "value": v})
print(json.dumps({"containerOverrides": [{"name": "dbtool", "environment": env}]}))
PY
)"
  # Not echoed through `run`: the overrides carry a presigned URL (a bearer credential for one object, for 1 hour).
  echo "+ aws ecs run-task --cluster $NAME --task-definition $family (MODE=$mode, presigned URL hidden)" >&2
  arn="$(aws ecs run-task --cluster "$NAME" --launch-type FARGATE --task-definition "$family" \
    --network-configuration "awsvpcConfiguration={subnets=[$subnets],securityGroups=[$sg],assignPublicIp=ENABLED}" \
    --overrides "$overrides" --query 'tasks[0].taskArn' --output text)" || die "ecs run-task failed"
  if [ -z "$arn" ] || [ "$arn" = None ]; then
    die "ecs run-task returned no task (capacity or network configuration problem?)"
  fi
  tid="${arn##*/}"
  echo "dbtool task $tid started; waiting (up to 10 minutes)..." >&2
  aws ecs wait tasks-stopped --cluster "$NAME" --tasks "$arn" \
    || die "dbtool task did not stop within 10 minutes: $arn. Inspect: aws ecs describe-tasks --cluster $NAME --tasks $arn; aws logs tail /ecs/$NAME --log-stream-name-prefix dbtool/dbtool/$tid"
  exitc="$(aws ecs describe-tasks --cluster "$NAME" --tasks "$arn" --query 'tasks[0].containers[0].exitCode' --output text)"
  reason="$(aws ecs describe-tasks --cluster "$NAME" --tasks "$arn" --query 'tasks[0].stoppedReason' --output text)"
  code="$(aws ecs describe-tasks --cluster "$NAME" --tasks "$arn" --query 'tasks[0].stopCode' --output text)"
  DBTOOL_LOG=""
  for i in 1 2 3 4 5 6; do
    DBTOOL_LOG="$(aws logs get-log-events --log-group-name "/ecs/$NAME" --log-stream-name "dbtool/dbtool/$tid" \
      --start-from-head --query 'events[].message' --output text 2>/dev/null | tr '\t' '\n' | grep '^dbtool:' || true)"
    [ -n "$DBTOOL_LOG" ] && break
    sleep 5
  done
  if [ -n "$DBTOOL_LOG" ]; then printf '%s\n' "$DBTOOL_LOG" | sed 's/^/  /'; else echo "  (no dbtool: log lines found in /ecs/$NAME dbtool/dbtool/$tid)"; fi
  if [ "$exitc" != 0 ]; then
    {
      echo "dbtool task failed: exitCode=$exitc stopCode=$code"
      echo "stoppedReason: $reason"
      echo "task: $arn"
      echo "logs: aws logs tail /ecs/$NAME --log-stream-name-prefix dbtool/dbtool/$tid"
    } >&2
    return 1
  fi
  return 0
}

# dbtool_field KEY: value of "KEY=..." from the last dbtool log (first match, up to whitespace).
dbtool_field() { printf '%s\n' "$DBTOOL_LOG" | grep -o "$1=[^ ]*" | tail -1 | cut -d= -f2; }

# Take, trial-restore, upload and verify one dump. Sets DUMP_KEY, DUMP_BYTES, DUMP_MD5, DUMP_USERS.
do_backup() {
  local tag="$1" url head_len head_etag
  DUMP_KEY="${BACKUP_PREFIX}${NAME}-${tag}.dump"
  url="$(python3 "$PRESIGN" PUT "s3://$STATE_BUCKET/$DUMP_KEY" 3600)" || die "could not presign the upload URL"
  echo ">>> Dumping the database to s3://$STATE_BUCKET/$DUMP_KEY (with a trial restore)"
  run_dbtool backup "$url" || die "backup failed; NOTHING was destroyed."
  DUMP_BYTES="$(dbtool_field bytes)"
  DUMP_MD5="$(dbtool_field md5)"
  DUMP_USERS="$(printf '%s\n' "$DBTOOL_LOG" | grep 'restore-test ok' | grep -o 'users=[^ ]*' | cut -d= -f2)"
  [ -n "$DUMP_BYTES" ] && [ -n "$DUMP_MD5" ] || die "backup log lacks bytes/md5; NOTHING was destroyed."
  [ -n "$DUMP_USERS" ] || die "backup log lacks 'restore-test ok'; NOTHING was destroyed."
  printf '%s\n' "$DBTOOL_LOG" | grep -q '^dbtool: uploaded' || die "backup log lacks 'uploaded'; NOTHING was destroyed."
  head_len="$(aws s3api head-object --bucket "$STATE_BUCKET" --key "$DUMP_KEY" --query ContentLength --output text)" \
    || die "dump object not found in S3; NOTHING was destroyed."
  head_etag="$(aws s3api head-object --bucket "$STATE_BUCKET" --key "$DUMP_KEY" --query ETag --output text)"
  [ "$head_len" = "$DUMP_BYTES" ] || die "S3 size $head_len != dumped $DUMP_BYTES; NOTHING was destroyed."
  [ "$head_etag" = "\"$DUMP_MD5\"" ] || die "S3 ETag $head_etag != md5 \"$DUMP_MD5\"; NOTHING was destroyed."
  echo "verified: s3://$STATE_BUCKET/$DUMP_KEY bytes=$DUMP_BYTES md5=$DUMP_MD5 users=$DUMP_USERS (trial restore ok, S3 size and ETag match)"
}

write_auto_vars() { # write_auto_vars LINE...  (one tfvars line per argument)
  { echo "# Written by scripts/aws-hibernate.sh; gitignored. Do not edit by hand."; printf '%s\n' "$@"; } >"$AUTO_VARS"
}

cmd_status() {
  echo "hibernate.auto.tfvars : $(if [ -f "$AUTO_VARS" ]; then if hibernated_in_file; then echo 'hibernate = true (hibernated)'; else echo 'present, hibernate = false'; fi; else echo 'absent (not hibernated)'; fi)"
  echo "RDS instance $NAME    : $(db_status)"
  echo "ALB $NAME             : $(alb_state)"
  echo "ECS running count     : $(ecs_running)"
  echo "Route53 zone $DOMAIN  : $(if [ "$(aws route53 list-hosted-zones-by-name --dns-name "$DOMAIN" --query "length(HostedZones[?Name=='$DOMAIN.'])" --output text 2>/dev/null)" -gt 0 ] 2>/dev/null; then echo present; else echo absent; fi)"
  echo "Secrets $NAME/*       : $(aws secretsmanager list-secrets --filters "Key=name,Values=$NAME/" --query 'length(SecretList)' --output text 2>/dev/null || echo '?')"
  echo "GuardDuty detectors   : $(aws guardduty list-detectors --query 'length(DetectorIds)' --output text 2>/dev/null || echo '?')"
  echo "ECR images in $NAME   : $(aws ecr list-images --repository-name "$NAME" --query 'length(imageIds)' --output text 2>/dev/null || echo '?')"
  echo "Elastic IPs           : $(aws ec2 describe-addresses --query 'length(Addresses)' --output text 2>/dev/null || echo '?')"
  echo "WARTIDE_HIBERNATED    : $(gh_var)"
  echo "Delegation set in tfvars : $(if delegation_set_in_tfvars; then echo yes; else echo 'no (zone gets new name servers on wake)'; fi)"
  echo "Dumps in s3://$STATE_BUCKET/$BACKUP_PREFIX (newest first):"
  list_dumps | awk -F'\t' '{printf "  %s  %s  %s bytes\n", $1, $2, $3}'
  local snaps
  snaps="$(manual_snapshots || true)"
  if [ -n "$snaps" ]; then
    echo "Manual RDS snapshots ($NAME-*; these cost money, ~\$0.095/GB-month):"
    printf '%s\n' "$snaps" | sed 's/^/  /'
  fi
}

cmd_backup() {
  need terraform aws python3
  [ "$(db_status)" != absent ] || die "RDS instance $NAME not found; nothing to back up (try 'status')."
  do_backup "$(date -u +%Y%m%d-%H%M)"
}

cmd_down() {
  need terraform aws gh python3
  local db
  db="$(db_status)"
  if hibernated_in_file && [ "$db" = absent ]; then die "already hibernated per $AUTO_VARS; use 'status' or 'up'."; fi
  [ "$db" != absent ] || die "RDS instance $NAME not found; nothing to hibernate (try 'status')."

  guard_no_inflight_merges
  run gh variable set WARTIDE_HIBERNATED --repo "$GITHUB_REPO" --body true

  local tag
  tag="$(date -u +%Y%m%d-%H%M)"
  echo ">>> Step 1/3: turn off deletion protection, set the secret recovery window to 0 and create the dbtool task"
  echo "    definition. Nothing is destroyed by this apply."
  write_auto_vars 'hibernate                   = false' 'db_deletion_protection      = false' \
    'alb_deletion_protection     = false' 'secret_recovery_window_days = 0' "db_final_snapshot_tag       = \"$tag\""
  # Nothing is destroyed yet: if this apply fails or is declined, put everything back (protection on,
  # deploys unpaused) so the stack is exactly as before.
  if ! tf apply; then
    rm -f "$AUTO_VARS"
    run gh variable delete WARTIDE_HIBERNATED --repo "$GITHUB_REPO" || true
    die "step 1 apply failed or was declined; nothing was torn down. Run 'terraform apply' to confirm protection is back on, then retry."
  fi

  echo ">>> Step 2/3: verified database dump (nothing destroyed yet)"
  do_backup "$tag" || exit 1

  echo ">>> Step 3/3: destroy RDS (no final snapshot: the trial-restored dump above is verified in S3), ALB stack,"
  echo "    ECS service and autoscaling, both task definitions, Route53 zone and records, SES identity and DKIM,"
  echo "    the four secrets (no recovery window), both alarms and the GuardDuty detector."
  write_auto_vars 'hibernate                   = true' 'db_deletion_protection      = false' \
    'alb_deletion_protection     = false' 'secret_recovery_window_days = 0' 'db_skip_final_snapshot       = true' \
    "db_final_snapshot_tag       = \"$tag\""
  tf apply || die "step 3 apply failed or was declined. The dump $DUMP_KEY is safe; re-run 'down' to resume (protection is currently OFF)."

  echo ">>> Cleaning up what Terraform does not own"
  local ids n
  ids="$(aws ecr list-images --repository-name "$NAME" --query 'imageIds[*]' --output json)"
  n="$(printf '%s' "$ids" | python3 -c 'import json,sys; print(len(json.load(sys.stdin)))')"
  if [ "$n" != 0 ]; then
    run aws ecr batch-delete-image --repository-name "$NAME" --image-ids "$ids" >/dev/null
  fi
  echo "ECR: deleted $n image(s) from $NAME"
  n="$(aws ec2 describe-addresses --query 'length(Addresses)' --output text)"
  if [ "$n" != 0 ]; then
    echo "WARNING: $n Elastic IP(s) exist in the account; each costs about \$3.60/month. Release unused ones (aws ec2 describe-addresses)." >&2
  fi
  local snaps
  snaps="$(manual_snapshots || true)"
  if [ -n "$snaps" ]; then
    echo "WARNING: manual RDS snapshots exist and cost money (~\$0.095/GB-month):" >&2
    printf '%s\n' "$snaps" | sed 's/^/  /' >&2
    echo "  Delete one with: aws rds delete-db-snapshot --db-snapshot-identifier <id>" >&2
  fi

  cat <<EOF

=== Hibernated ===
Dump         : s3://$STATE_BUCKET/$DUMP_KEY
               bytes=$DUMP_BYTES md5=$DUMP_MD5 users=$DUMP_USERS (trial restore ok, size and ETag verified)
Still exists : VPC and security groups, ECS cluster, ECR repository (empty), IAM/OIDC, ACM certificate, S3 buckets,
               SNS, budget, anomaly subscription, log group, account settings (all free)
Expected cost: a few cents per month (S3 storage of state, dump and logs; CloudWatch log storage); under \$0.10
GitHub       : WARTIDE_HIBERNATED=true (Deploy workflow skips)
Wake up with : scripts/aws-hibernate.sh up
EOF
  if ! delegation_set_in_tfvars; then
    cat <<EOF

Note: route53_delegation_set_id is not set in terraform.tfvars, so the recreated zone will get NEW name
servers and the NS records at DreamHost will need updating on wake. To avoid that, run
'scripts/aws-bootstrap.sh delegation-set' (as the admin profile) and follow its instructions before waking.
EOF
  fi
}

cmd_up() {
  need terraform aws gh python3 curl dig
  local arg="${1:-}" key="" empty=0
  if [ "$arg" = "--empty" ]; then
    empty=1
  else
    key="$arg"
    [ -n "$key" ] || key="$(newest_dump)"
    [ -n "$key" ] || die "no dump found under s3://$STATE_BUCKET/$BACKUP_PREFIX; pass --empty for a fresh database."
    case "$key" in "$BACKUP_PREFIX"*) ;; *) key="$BACKUP_PREFIX$key" ;; esac
    aws s3api head-object --bucket "$STATE_BUCKET" --key "$key" >/dev/null 2>&1 \
      || die "s3://$STATE_BUCKET/$key not found (see 'status' for the list)"
    echo ">>> Waking with dump s3://$STATE_BUCKET/$key"
  fi

  write_auto_vars 'hibernate = false'
  echo ">>> Step 1: terraform apply. A fresh RDS instance takes about 10 minutes. The zone, SES identity, secrets,"
  echo "    task definitions, ALB and service are recreated; the service cannot pull :latest from the (empty) ECR"
  echo "    repository until the deploy below pushes an image, which is expected."
  tf apply

  if [ "$empty" = 1 ]; then
    echo ">>> --empty: skipping the restore; the app creates the schema at startup."
  else
    local url
    url="$(python3 "$PRESIGN" GET "s3://$STATE_BUCKET/$key" 3600)" || die "could not presign the download URL"
    echo ">>> Step 2: restoring the dump into the new database"
    run_dbtool restore "$url" || die "restore failed. 'up' is idempotent: fix the cause and re-run 'scripts/aws-hibernate.sh up $key' (an already-populated database is skipped; use FORCE=1 via the task to overwrite)."
  fi

  run gh variable delete WARTIDE_HIBERNATED --repo "$GITHUB_REPO" || true
  run gh workflow run deploy.yml --repo "$GITHUB_REPO" --ref main \
    || echo "warning: could not dispatch deploy.yml; run the Deploy workflow by hand to push an image (the service runs :latest)." >&2

  local want got wait_attempts=60
  want="$(zone_ns || true)"
  got="$(dns_delegated_ns || true)"
  if [ -n "$want" ] && [ "$want" = "$got" ]; then
    echo "DNS: $DOMAIN is delegated to this zone's name servers."
  else
    wait_attempts=12
    cat >&2 <<EOF

!!! DNS delegation does not match. $DOMAIN must be delegated to this zone's name servers:
$(printf '%s\n' "$want" | sed 's/^/      /')
    At the DNS host of ${DOMAIN#*.} (DreamHost for chaumurky.net) set the NS records for host '${DOMAIN%%.*}' to
    exactly those servers. Until then the site is unreachable, SES verification cannot complete and ACM
    renewals fail. Re-run 'scripts/aws-hibernate.sh up' later to re-check and poll (it is idempotent).
    Polling $HEALTH_URL for only 2 minutes this time.
EOF
  fi

  echo ">>> Polling $HEALTH_URL (up to $((wait_attempts / 6)) minutes)"
  local i code=""
  for i in $(seq 1 "$wait_attempts"); do
    code="$(curl -s -o /dev/null -w '%{http_code}' --max-time 8 "$HEALTH_URL" || true)"
    if [ "$code" = 200 ]; then
      echo "healthy: $HEALTH_URL returned 200"
      return 0
    fi
    echo "  attempt $i/$wait_attempts: HTTP ${code:-none}"
    sleep 10
  done
  cat >&2 <<EOF
error: $HEALTH_URL did not return 200 in time (last: ${code:-none}).
Check:
  aws ecs describe-services --cluster $NAME --services app --query 'services[0].events[:10]'
  aws logs tail /ecs/$NAME --since 30m
  aws elbv2 describe-target-health --target-group-arn \$(aws elbv2 describe-target-groups --names $NAME --query 'TargetGroups[0].TargetGroupArn' --output text)
  DNS can lag a few minutes after the zone and ALB are recreated; the Deploy workflow must have pushed an image.
EOF
  exit 1
}

case "${1:-}" in
  down) cmd_down ;;
  backup) cmd_backup ;;
  up) shift; cmd_up "$@" ;;
  status) cmd_status ;;
  -h | --help | help | "") usage ;;
  *) usage >&2; exit 2 ;;
esac

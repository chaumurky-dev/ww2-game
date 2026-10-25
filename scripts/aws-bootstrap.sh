#!/usr/bin/env bash
# Wartide AWS bootstrap: everything needed before the first `terraform apply`, scripted and idempotent.
#
# Subcommands (see docs/DEPLOY.md):
#   operator-role  [admin profile] create/update the least-privilege `wartide-terraform` role + policy
#   profile        write the `wartide` CLI profile that assumes that role
#   state-bucket   [wartide profile] create the private, versioned, encrypted Terraform state bucket
#   github         set GitHub secret/variables from `terraform output`
#   all            operator-role, profile, state-bucket, then guided Terraform/DNS-delegation/github steps
#
# Every mutating command is printed (prefixed with "+") before it runs.
# Environment:
#   AWS_PROFILE         profile for the subcommand (operator-role: an admin profile; state-bucket: wartide)
#   AWS_REGION          default us-east-1
#   ADMIN_PRINCIPAL_ARN principal allowed to assume the role (default: caller of operator-role)
#   ROLE_NAME           default wartide-terraform      POLICY_NAME  default wartide-terraform
#   STATE_BUCKET        default wartide-terraform-state-<account id>
#   GITHUB_REPO         default chaumurky-dev/ww2-game
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
IAM_DIR="$REPO_ROOT/infra/iam"
REGION="${AWS_REGION:-us-east-1}"
ROLE_NAME="${ROLE_NAME:-wartide-terraform}"
POLICY_NAME="${POLICY_NAME:-wartide-terraform}"
GITHUB_REPO="${GITHUB_REPO:-chaumurky-dev/ww2-game}"
ASSUME_YES=0
TF_DIR="$REPO_ROOT/infra/terraform"

usage() {
  cat <<USAGE
Usage: scripts/aws-bootstrap.sh [--yes] <subcommand> [args]
  operator-role              run as admin: AWS_PROFILE=default ADMIN_PRINCIPAL_ARN=arn:aws:iam::<acct>:user/<you> $0 operator-role
  profile [SRC_PROFILE] [REGION]   write CLI profile 'wartide' (defaults: default, us-east-1)
  state-bucket               run as the role: AWS_PROFILE=wartide $0 state-bucket
  github [TERRAFORM_DIR]     set GitHub secret/variables (default: infra/terraform); needs gh auth
  all [SRC_PROFILE] [REGION] operator-role + profile + state-bucket + guided remainder
  --yes                      skip interactive confirmations
USAGE
}

# Print a command, then run it.
run() { printf '+ %s\n' "$*" >&2; "$@"; }

confirm() { # confirm "message"
  printf '\n>>> %s\n' "$1" >&2
  [ "$ASSUME_YES" = 1 ] && return 0
  read -r -p "Press Enter when done (Ctrl-C to abort)... " _
}

account_id() { aws sts get-caller-identity --query Account --output text; }

# ---------------------------------------------------------------- operator-role
cmd_operator_role() {
  : "${AWS_PROFILE:?set AWS_PROFILE to an admin profile (not wartide)}"
  export AWS_PROFILE
  local acct caller principal trust policy_arn
  acct="$(account_id)"
  caller="$(aws sts get-caller-identity --query Arn --output text)"
  principal="${ADMIN_PRINCIPAL_ARN:-$caller}"
  case "$principal" in
    arn:aws:iam::"$acct":user/* | arn:aws:iam::"$acct":role/*) ;;
    *) echo "ADMIN_PRINCIPAL_ARN must be a user/role ARN in account $acct (got: $principal)" >&2; exit 1 ;;
  esac
  policy_arn="arn:aws:iam::${acct}:policy/${POLICY_NAME}"
  echo "Account ${acct}; admin principal ${principal}; role ${ROLE_NAME}"

  trust="$(mktemp)"; trap 'rm -f "$trust"' RETURN
  sed "s|__ADMIN_PRINCIPAL_ARN__|${principal}|g" "$IAM_DIR/wartide-terraform-trust.json" > "$trust"

  # 1. Role with trust policy (only the admin principal may assume it), 2h sessions.
  if aws iam get-role --role-name "$ROLE_NAME" >/dev/null 2>&1; then
    run aws iam update-assume-role-policy --role-name "$ROLE_NAME" --policy-document "file://$trust"
    run aws iam update-role --role-name "$ROLE_NAME" --max-session-duration 7200
  else
    run aws iam create-role --role-name "$ROLE_NAME" --max-session-duration 7200 \
      --description "Wartide Terraform operator (least privilege)" \
      --assume-role-policy-document "file://$trust"
  fi

  # 2. Managed policy: create, or publish a new default version only if the document changed.
  local doc="$IAM_DIR/wartide-terraform-policy.json"
  if aws iam get-policy --policy-arn "$policy_arn" >/dev/null 2>&1; then
    local ver current want
    ver="$(aws iam get-policy --policy-arn "$policy_arn" --query Policy.DefaultVersionId --output text)"
    current="$(aws iam get-policy-version --policy-arn "$policy_arn" --version-id "$ver" \
      --query PolicyVersion.Document --output json | python3 -c 'import json,sys;print(json.dumps(json.load(sys.stdin),sort_keys=True))')"
    want="$(python3 -c 'import json,sys;print(json.dumps(json.load(open(sys.argv[1])),sort_keys=True))' "$doc")"
    if [ "$current" = "$want" ]; then
      echo "Policy ${POLICY_NAME} is up to date (${ver})"
    else
      # IAM allows 5 versions; prune oldest non-default ones until at most 4 remain.
      local non_default n oldest
      non_default="$(aws iam list-policy-versions --policy-arn "$policy_arn" \
        --query 'sort_by(Versions,&CreateDate)[?IsDefaultVersion==`false`].VersionId' --output text)"
      n="$(wc -w <<<"$non_default")"
      for oldest in $non_default; do
        [ "$n" -le 3 ] && break
        run aws iam delete-policy-version --policy-arn "$policy_arn" --version-id "$oldest"
        n=$((n - 1))
      done
      run aws iam create-policy-version --policy-arn "$policy_arn" \
        --policy-document "file://$doc" --set-as-default
    fi
  else
    run aws iam create-policy --policy-name "$POLICY_NAME" --policy-document "file://$doc"
  fi

  # 3. Attach our policy; make sure AdministratorAccess is not attached.
  run aws iam attach-role-policy --role-name "$ROLE_NAME" --policy-arn "$policy_arn"
  local admin_arn="arn:aws:iam::aws:policy/AdministratorAccess"
  if aws iam list-attached-role-policies --role-name "$ROLE_NAME" \
    --query "AttachedPolicies[?PolicyArn=='${admin_arn}'].PolicyArn" --output text | grep -q .; then
    run aws iam detach-role-policy --role-name "$ROLE_NAME" --policy-arn "$admin_arn"
  fi
  echo "Role ready: arn:aws:iam::${acct}:role/${ROLE_NAME}"
}

# ---------------------------------------------------------------- profile
cmd_profile() {
  local src="${1:-default}" region="${2:-$REGION}" acct
  acct="$(AWS_PROFILE="$src" account_id)"
  local role_arn="arn:aws:iam::${acct}:role/${ROLE_NAME}"
  run aws configure set profile.wartide.role_arn "$role_arn"
  run aws configure set profile.wartide.source_profile "$src"
  run aws configure set profile.wartide.region "$region"
  run aws configure set profile.wartide.output json
  echo "Profile 'wartide' -> ${role_arn} (via ${src}). Test: AWS_PROFILE=wartide aws sts get-caller-identity"
}

# ---------------------------------------------------------------- state-bucket
cmd_state_bucket() {
  : "${AWS_PROFILE:?set AWS_PROFILE (normally wartide)}"
  export AWS_PROFILE
  local acct bucket
  acct="$(account_id)"
  bucket="${STATE_BUCKET:-wartide-terraform-state-${acct}}"
  echo "Account ${acct}, region ${REGION}, bucket ${bucket}"
  if aws s3api head-bucket --bucket "$bucket" 2>/dev/null; then
    echo "Bucket exists; (re)applying settings"
  elif [ "$REGION" = "us-east-1" ]; then
    run aws s3api create-bucket --bucket "$bucket" --region "$REGION"
  else
    run aws s3api create-bucket --bucket "$bucket" --region "$REGION" \
      --create-bucket-configuration "LocationConstraint=${REGION}"
  fi
  run aws s3api put-public-access-block --bucket "$bucket" --public-access-block-configuration \
    BlockPublicAcls=true,IgnorePublicAcls=true,BlockPublicPolicy=true,RestrictPublicBuckets=true
  run aws s3api put-bucket-versioning --bucket "$bucket" --versioning-configuration Status=Enabled
  run aws s3api put-bucket-encryption --bucket "$bucket" --server-side-encryption-configuration \
    '{"Rules":[{"ApplyServerSideEncryptionByDefault":{"SSEAlgorithm":"AES256"},"BucketKeyEnabled":true}]}'
  echo "State bucket ready: ${bucket}"
}

# ---------------------------------------------------------------- github
cmd_github() {
  local dir="${1:-$TF_DIR}"
  [ -d "$dir" ] || { echo "no such directory: $dir" >&2; exit 1; }
  local tf="terraform -chdir=$dir output -raw"
  run gh secret set AWS_DEPLOY_ROLE_ARN --repo "$GITHUB_REPO" --body "$($tf deploy_role_arn)"
  local pair var out
  for pair in AWS_REGION:aws_region ECR_REPOSITORY:ecr_repository_name ECS_CLUSTER:ecs_cluster \
    ECS_SERVICE:ecs_service ECS_TASK_FAMILY:ecs_task_family; do
    var="${pair%%:*}"; out="${pair##*:}"
    run gh variable set "$var" --repo "$GITHUB_REPO" --body "$($tf "$out")"
  done
}

# ---------------------------------------------------------------- all
cmd_all() {
  local src="${1:-default}" region="${2:-$REGION}"
  echo "== 1/6 operator role (as profile '$src')"
  AWS_PROFILE="$src" cmd_operator_role
  echo "== 2/6 CLI profile"
  cmd_profile "$src" "$region"
  echo "== 3/6 state bucket (as profile 'wartide')"
  AWS_PROFILE=wartide cmd_state_bucket

  echo "== 4/6 Terraform init + hosted zone + ECR"
  [ -f "$TF_DIR/backend.hcl" ] || run cp "$TF_DIR/backend.hcl.example" "$TF_DIR/backend.hcl"
  [ -f "$TF_DIR/terraform.tfvars" ] || run cp "$TF_DIR/terraform.tfvars.example" "$TF_DIR/terraform.tfvars"
  export AWS_PROFILE=wartide
  run terraform -chdir="$TF_DIR" init -backend-config=backend.hcl
  run terraform -chdir="$TF_DIR" apply -target=aws_route53_zone.main -target=aws_ecr_repository.app
  terraform -chdir="$TF_DIR" output route53_name_servers
  confirm "MANUAL: in the DNS host of chaumurky.net (DreamHost; GoDaddy is only the registrar) add NS records for host 'ww2', one per name server above. Verify: dig NS ww2.chaumurky.net +short"
  confirm "MANUAL: push a first image to ECR (docs/DEPLOY.md, 'First image'), so the ECS service has something to run."

  echo "== 5/6 full apply"
  run terraform -chdir="$TF_DIR" apply
  echo "== 6/6 GitHub configuration"
  cmd_github "$TF_DIR"
  echo "Done. Remaining manual step: request SES production access in the console. Then run the Deploy workflow."
}

# ---------------------------------------------------------------- dispatch
args=()
for a in "$@"; do
  if [ "$a" = "--yes" ]; then ASSUME_YES=1; else args+=("$a"); fi
done
[ "${#args[@]}" -ge 1 ] || { usage; exit 1; }
sub="${args[0]}"; rest=("${args[@]:1}")
case "$sub" in
  operator-role) cmd_operator_role ;;
  profile) cmd_profile "${rest[@]+"${rest[@]}"}" ;;
  state-bucket) cmd_state_bucket ;;
  github) cmd_github "${rest[@]+"${rest[@]}"}" ;;
  all) cmd_all "${rest[@]+"${rest[@]}"}" ;;
  -h | --help | help) usage ;;
  *) usage; exit 1 ;;
esac

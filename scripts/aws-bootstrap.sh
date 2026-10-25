#!/usr/bin/env bash
# Wartide AWS bootstrap: everything needed before the first `terraform apply`, scripted and idempotent.
#
# Subcommands (see docs/DEPLOY.md):
#   operator-role  [admin profile] create/update the `wartide-terraform` role + policy and the `wartide-boundary` permissions boundary
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
#   BOUNDARY_NAME       default wartide-boundary       ENV_NAME     default prod (Terraform env suffix)
#   STATE_BUCKET        default wartide-terraform-state-<account id>
#   GITHUB_REPO         default chaumurky-dev/ww2-game
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
IAM_DIR="$REPO_ROOT/infra/iam"
REGION="${AWS_REGION:-us-east-1}"
ROLE_NAME="${ROLE_NAME:-wartide-terraform}"
POLICY_NAME="${POLICY_NAME:-wartide-terraform}"
BOUNDARY_NAME="${BOUNDARY_NAME:-wartide-boundary}"
ENV_NAME="${ENV_NAME:-prod}"
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

# upsert_policy NAME DOCUMENT_FILE POLICY_ARN: create the managed policy, or publish a new default version
# only when the document changed. IAM keeps at most 5 versions, so the oldest non-default one is deleted
# first when 4 non-default versions already exist.
upsert_policy() {
  local name="$1" doc="$2" arn="$3"
  python3 -m json.tool "$doc" >/dev/null || { echo "invalid JSON: $doc" >&2; exit 1; }
  if ! aws iam get-policy --policy-arn "$arn" >/dev/null 2>&1; then
    run aws iam create-policy --policy-name "$name" --policy-document "file://$doc"
    echo "Policy ${name}: created"
    return 0
  fi
  local ver current want
  ver="$(aws iam get-policy --policy-arn "$arn" --query Policy.DefaultVersionId --output text)"
  current="$(aws iam get-policy-version --policy-arn "$arn" --version-id "$ver" \
    --query PolicyVersion.Document --output json | python3 -c 'import json,sys;print(json.dumps(json.load(sys.stdin),sort_keys=True))')"
  want="$(python3 -c 'import json,sys;print(json.dumps(json.load(open(sys.argv[1])),sort_keys=True))' "$doc")"
  if [ "$current" = "$want" ]; then
    echo "Policy ${name}: up to date (${ver})"
    return 0
  fi
  local non_default n oldest
  non_default="$(aws iam list-policy-versions --policy-arn "$arn" \
    --query 'sort_by(Versions,&CreateDate)[?IsDefaultVersion==`false`].VersionId' --output text)"
  n="$(wc -w <<<"$non_default")"
  for oldest in $non_default; do
    [ "$n" -le 3 ] && break
    run aws iam delete-policy-version --policy-arn "$arn" --version-id "$oldest"
    n=$((n - 1))
  done
  run aws iam create-policy-version --policy-arn "$arn" --policy-document "file://$doc" --set-as-default
  echo "Policy ${name}: new default version published (was ${ver})"
}

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

  # 2. Permissions boundary first (the operator policy only allows creating principals that carry it), then the
  #    operator policy itself. Each is created, or gets a new default version only if the document changed.
  upsert_policy "$BOUNDARY_NAME" "$IAM_DIR/wartide-boundary-policy.json" "arn:aws:iam::${acct}:policy/${BOUNDARY_NAME}"
  upsert_policy "$POLICY_NAME" "$IAM_DIR/wartide-terraform-policy.json" "$policy_arn"

  # 3. Attach our policy; make sure AdministratorAccess is not attached.
  run aws iam attach-role-policy --role-name "$ROLE_NAME" --policy-arn "$policy_arn"
  local admin_arn="arn:aws:iam::aws:policy/AdministratorAccess"
  if aws iam list-attached-role-policies --role-name "$ROLE_NAME" \
    --query "AttachedPolicies[?PolicyArn=='${admin_arn}'].PolicyArn" --output text | grep -q .; then
    run aws iam detach-role-policy --role-name "$ROLE_NAME" --policy-arn "$admin_arn"
  fi
  # 4. Principals Terraform created before the boundary existed need it attached once, by the admin: the operator
  #    policy denies iam:Put*PermissionsBoundary, so Terraform itself cannot. New ones get it from Terraform.
  local boundary_arn="arn:aws:iam::${acct}:policy/${BOUNDARY_NAME}" r u cur
  for r in $(aws iam list-roles --query "Roles[?starts_with(RoleName, 'wartide-${ENV_NAME}-')].RoleName" --output text); do
    cur="$(aws iam get-role --role-name "$r" --query 'Role.PermissionsBoundary.PermissionsBoundaryArn' --output text)"
    if [ "$cur" = "$boundary_arn" ]; then
      echo "Role ${r}: boundary already attached"
    else
      run aws iam put-role-permissions-boundary --role-name "$r" --permissions-boundary "$boundary_arn"
    fi
  done
  for u in $(aws iam list-users --query "Users[?starts_with(UserName, 'wartide-${ENV_NAME}-')].UserName" --output text); do
    cur="$(aws iam get-user --user-name "$u" --query 'User.PermissionsBoundary.PermissionsBoundaryArn' --output text)"
    if [ "$cur" = "$boundary_arn" ]; then
      echo "User ${u}: boundary already attached"
    else
      run aws iam put-user-permissions-boundary --user-name "$u" --permissions-boundary "$boundary_arn"
    fi
  done
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
  run aws s3api put-bucket-ownership-controls --bucket "$bucket" \
    --ownership-controls 'Rules=[{ObjectOwnership=BucketOwnerEnforced}]'
  run aws s3api put-bucket-versioning --bucket "$bucket" --versioning-configuration Status=Enabled
  run aws s3api put-bucket-encryption --bucket "$bucket" --server-side-encryption-configuration \
    '{"Rules":[{"ApplyServerSideEncryptionByDefault":{"SSEAlgorithm":"AES256"},"BucketKeyEnabled":true}]}'
  run aws s3api put-bucket-lifecycle-configuration --bucket "$bucket" --lifecycle-configuration \
    '{"Rules":[{"ID":"expire-noncurrent-90d","Status":"Enabled","Filter":{},"NoncurrentVersionExpiration":{"NoncurrentDays":90},"AbortIncompleteMultipartUpload":{"DaysAfterInitiation":7}}]}'
  local policy
  policy="$(printf '{"Version":"2012-10-17","Statement":[{"Sid":"DenyInsecureTransport","Effect":"Deny","Principal":"*","Action":"s3:*","Resource":["arn:aws:s3:::%s","arn:aws:s3:::%s/*"],"Condition":{"Bool":{"aws:SecureTransport":"false"}}}]}' "$bucket" "$bucket")"
  run aws s3api put-bucket-policy --bucket "$bucket" --policy "$policy"
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

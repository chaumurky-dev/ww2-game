#!/usr/bin/env bash
# Idempotently create the private, versioned, encrypted S3 bucket for Terraform state.
# Usage: AWS_PROFILE=<profile> scripts/aws-bootstrap.sh
set -euo pipefail

REGION="${AWS_REGION:-us-east-1}"
: "${AWS_PROFILE:?set AWS_PROFILE to the profile for the target account}"
export AWS_PROFILE

ACCOUNT_ID="$(aws sts get-caller-identity --query Account --output text)"
BUCKET="${STATE_BUCKET:-wartide-terraform-state-${ACCOUNT_ID}}"
echo "Account ${ACCOUNT_ID}, region ${REGION}, bucket ${BUCKET}"

if aws s3api head-bucket --bucket "$BUCKET" 2>/dev/null; then
  echo "Bucket exists; (re)applying settings"
else
  if [ "$REGION" = "us-east-1" ]; then
    aws s3api create-bucket --bucket "$BUCKET" --region "$REGION"
  else
    aws s3api create-bucket --bucket "$BUCKET" --region "$REGION" \
      --create-bucket-configuration "LocationConstraint=${REGION}"
  fi
fi

aws s3api put-public-access-block --bucket "$BUCKET" --public-access-block-configuration \
  BlockPublicAcls=true,IgnorePublicAcls=true,BlockPublicPolicy=true,RestrictPublicBuckets=true
aws s3api put-bucket-versioning --bucket "$BUCKET" --versioning-configuration Status=Enabled
aws s3api put-bucket-encryption --bucket "$BUCKET" --server-side-encryption-configuration \
  '{"Rules":[{"ApplyServerSideEncryptionByDefault":{"SSEAlgorithm":"AES256"},"BucketKeyEnabled":true}]}'

cat <<NEXT

State bucket ready: ${BUCKET}
Next steps (see docs/DEPLOY.md):
  cd infra/terraform
  cp backend.hcl.example backend.hcl && cp terraform.tfvars.example terraform.tfvars   # check the bucket name
  terraform init -backend-config=backend.hcl
  terraform apply -target=aws_route53_zone.main
  terraform output route53_name_servers      # add 4 NS records for host 'ww2' at GoDaddy
  dig NS ww2.chaumurky.net +short            # wait until it answers
  terraform apply -target=aws_ecr_repository.app   # then push an image
  terraform apply
NEXT

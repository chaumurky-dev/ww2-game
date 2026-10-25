#!/usr/bin/env sh
# Build the linux/amd64 image and push it to ECR. Normally ONLY the Deploy workflow does this;
# this script exists for the very first image before CI is wired, and for emergencies.
# Usage (host, from repo root): AWS_PROFILE=wartide scripts/push-image.sh
# On macOS with colima (no buildx plugin in the host CLI) it runs the build inside the VM.
set -eu
REGION="${AWS_REGION:-us-east-1}"
ACCOUNT="$(aws sts get-caller-identity --query Account --output text)"
REGISTRY="${ACCOUNT}.dkr.ecr.${REGION}.amazonaws.com"
REPO="${REGISTRY}/${ECR_REPOSITORY:-wartide-prod}"
SHA="$(git rev-parse --short HEAD)"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
echo "Pushing ${REPO}:${SHA} and :latest"
if command -v colima >/dev/null 2>&1 && ! docker buildx version >/dev/null 2>&1; then
  aws ecr get-login-password --region "$REGION" | colima ssh -- docker login --username AWS --password-stdin "$REGISTRY"
  colima ssh -- docker buildx build --platform linux/amd64 -t "${REPO}:latest" -t "${REPO}:${SHA}" --push "$ROOT"
else
  aws ecr get-login-password --region "$REGION" | docker login --username AWS --password-stdin "$REGISTRY"
  docker buildx build --platform linux/amd64 -t "${REPO}:latest" -t "${REPO}:${SHA}" --push "$ROOT"
fi

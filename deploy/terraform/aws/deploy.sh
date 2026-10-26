#!/usr/bin/env bash
# Deploy (or update) Feast for one country on AWS.
#   ./deploy.sh au            # uses au.tfvars; prints the URL at the end
# Needs: terraform, aws CLI (logged in: `aws configure` or SSO), docker with buildx.
set -euo pipefail
CC=${1:?usage: ./deploy.sh <country>   e.g. ./deploy.sh au}
cd "$(dirname "$0")"
ROOT=../../..
TAG=$(date -u +%Y%m%d%H%M%S)

terraform init -upgrade -input=false >/dev/null
terraform workspace select -or-create "$CC"

echo "==> 1/3 Container registries"
terraform apply -input=false -auto-approve -var-file="$CC.tfvars" -target=aws_ecr_repository.api -target=aws_ecr_repository.web
REGION=$(grep -E '^\s*region' "$CC.tfvars" | sed -E 's/.*"(.*)".*/\1/')
API_REPO=$(terraform output -raw api_repository)
WEB_REPO=$(terraform output -raw web_repository)
PLATFORM=$(grep -qE 'cpu_architecture\s*=\s*"X86_64"' "$CC.tfvars" && echo linux/amd64 || echo linux/arm64)

echo "==> 2/3 Build and push images ($PLATFORM, tag $TAG)"
aws ecr get-login-password --region "$REGION" | docker login --username AWS --password-stdin "${API_REPO%%/*}"
docker buildx build --platform "$PLATFORM" -t "$API_REPO:$TAG" --push "$ROOT/backend"
docker buildx build --platform "$PLATFORM" -t "$WEB_REPO:$TAG" --push "$ROOT/webapp"

echo "==> 3/3 Infrastructure + app"
terraform apply -input=false -auto-approve -var-file="$CC.tfvars" -var "image_tag=$TAG"

echo
echo "Feast ($CC) is deploying: $(terraform output -raw url)"
echo "Admin/owner password:     terraform output -raw seed_password"
echo "Logs:                     $(terraform output -raw logs)"
echo "(First start takes a few minutes while the database is prepared.)"

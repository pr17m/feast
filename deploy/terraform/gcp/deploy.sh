#!/usr/bin/env bash
# Deploy (or update) Feast for one country on Google Cloud.
#   ./deploy.sh au      (set project_id in au.tfvars first)
# Needs: terraform, gcloud logged in (`gcloud auth login && gcloud auth application-default login`).
# Images are built by Cloud Build — no local Docker needed.
set -euo pipefail
CC=${1:?usage: ./deploy.sh <country>   e.g. ./deploy.sh au}
cd "$(dirname "$0")"
ROOT=../../..
TAG=$(date -u +%Y%m%d%H%M%S)
PROJECT=$(grep -E '^\s*project_id' "$CC.tfvars" | sed -E 's/.*"(.*)".*/\1/')

terraform init -upgrade -input=false >/dev/null
terraform workspace select -or-create "$CC"

echo "==> 1/3 APIs + image repository"
terraform apply -input=false -auto-approve -var-file="$CC.tfvars" -target=google_artifact_registry_repository.images
REPO=$(terraform output -raw image_repository)

echo "==> 2/3 Build images with Cloud Build (tag $TAG)"
gcloud builds submit "$ROOT/backend" --project "$PROJECT" --tag "$REPO/feast-api:$TAG" --timeout=3600s
gcloud builds submit "$ROOT/webapp" --project "$PROJECT" --tag "$REPO/feast-web:$TAG"

echo "==> 3/3 Infrastructure + app (Cloud SQL and Memorystore take ~10–15 minutes the first time)"
terraform apply -input=false -auto-approve -var-file="$CC.tfvars" -var "image_tag=$TAG"

echo
echo "Feast ($CC): $(terraform output -raw url)"
echo "Admin/owner password: terraform output -raw seed_password"
echo "Logs: $(terraform output -raw logs)"

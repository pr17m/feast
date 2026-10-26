#!/usr/bin/env bash
# Deploy (or update) Feast for one country on Azure.
#   ./deploy.sh au
# Needs: terraform, Azure CLI logged in (`az login`; `az account set -s <subscription>`).
# Images are built in Azure (az acr build) — no local Docker needed.
set -euo pipefail
CC=${1:?usage: ./deploy.sh <country>   e.g. ./deploy.sh au}
cd "$(dirname "$0")"
ROOT=../../..
TAG=$(date -u +%Y%m%d%H%M%S)
export ARM_SUBSCRIPTION_ID=${ARM_SUBSCRIPTION_ID:-$(az account show --query id -o tsv)}

terraform init -upgrade -input=false >/dev/null
terraform workspace select -or-create "$CC"

echo "==> 1/3 Container registry"
terraform apply -input=false -auto-approve -var-file="$CC.tfvars" -target=azurerm_container_registry.main
ACR=$(terraform output -raw registry_name)

echo "==> 2/3 Build images in Azure (tag $TAG)"
az acr build --registry "$ACR" --image "feast-api:$TAG" --platform linux/amd64 "$ROOT/backend"
az acr build --registry "$ACR" --image "feast-web:$TAG" --platform linux/amd64 "$ROOT/webapp"

echo "==> 3/3 Infrastructure + app (Redis takes ~20 minutes the first time)"
terraform apply -input=false -auto-approve -var-file="$CC.tfvars" -var "image_tag=$TAG"

echo
echo "Feast ($CC): $(terraform output -raw url)"
echo "Admin/owner password: terraform output -raw seed_password"
echo "Logs: $(terraform output -raw logs)"

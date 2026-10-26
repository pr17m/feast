#!/usr/bin/env bash
# Rebuild the Feast Docker images from the current code and restart the stack.
#
#   ./redeploy.sh                 # Australia (au.env)
#   ./redeploy.sh in              # India (in.env)
#   ./redeploy.sh au --photos     # also refresh the demo restaurants' photos (keeps all other data)
#   ./redeploy.sh au --no-cache   # full rebuild, ignoring Docker's build cache
#
# Data is kept: the database and uploaded files live in Docker volumes (or your
# own Postgres). Migrations run automatically before the API starts.
set -euo pipefail

cd "$(dirname "$0")"

country="au"
photos=false
nocache=""
for arg in "$@"; do
  case "$arg" in
    au|in|nz|ca|uk) country="$arg" ;;
    --photos) photos=true ;;
    --no-cache) nocache="--no-cache" ;;
    -h|--help) sed -n '2,10p' "$0"; exit 0 ;;
    *) echo "Unknown option: $arg (try --help)" >&2; exit 2 ;;
  esac
done

env_file="${country}.env"
if [[ ! -f "$env_file" ]]; then
  echo "Missing $env_file. Copy ${country}.env.example to $env_file and fill it in first." >&2
  exit 1
fi
if ! docker info >/dev/null 2>&1; then
  echo "Docker isn't running. Start Docker Desktop and try again." >&2
  exit 1
fi

port="$(grep -E '^PORT=' "$env_file" | tail -1 | cut -d= -f2 || true)"
port="${port:-8080}"
dc() { docker compose --env-file "$env_file" "$@"; }

echo "==> Building images for $(echo "$country" | tr a-z A-Z) (the Rust API takes a few minutes after code changes)"
dc build $nocache

echo "==> Restarting the stack (migrations run first)"
dc up -d --remove-orphans

echo "==> Waiting for Feast to be ready on http://localhost:${port}"
ready=false
for _ in $(seq 1 60); do
  if curl -fsS "http://localhost:${port}/v1/markets" >/dev/null 2>&1; then ready=true; break; fi
  sleep 3
done
if [[ "$ready" != true ]]; then
  echo "Feast didn't become ready within 3 minutes. Recent API logs:" >&2
  dc logs --tail 60 migrate api >&2 || true
  exit 1
fi

if [[ "$photos" == true ]]; then
  echo "==> Refreshing demo photos"
  dc run --rm api seed --photos
fi

echo "==> Removing old, unused images"
docker image prune -f >/dev/null

dc ps
echo
echo "Feast $(echo "$country" | tr a-z A-Z) is up: http://localhost:${port}"

#!/bin/sh
# Print the public URL of a shared stack: ./share-url.sh au.env
ENV_FILE=${1:-au.env}
cd "$(dirname "$0")"
for i in $(seq 1 30); do
  url=$(docker compose --env-file "$ENV_FILE" --profile share logs tunnel 2>/dev/null | grep -o 'https://[a-z0-9-]*\.trycloudflare\.com' | tail -1)
  [ -n "$url" ] && echo "$url" && exit 0
  sleep 2
done
echo "No tunnel URL yet. Is it running?  docker compose --env-file $ENV_FILE --profile share up -d" >&2
exit 1

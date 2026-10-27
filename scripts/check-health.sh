#!/bin/sh
# Waits up to a minute for a deployed bot to answer /health/live and /health/ready.
# Usage: scripts/check-health.sh https://just-ingredients.fly.dev
set -u
url="$1"
for attempt in 1 2 3 4 5 6; do
  if curl -fsS --max-time 10 "$url/health/live" > /dev/null \
    && curl -fsS --max-time 10 "$url/health/ready" > /dev/null; then
    echo "$url is healthy"
    exit 0
  fi
  echo "health check attempt $attempt failed, retrying in 10 s"
  sleep 10
done
echo "$url did not become healthy"
exit 1

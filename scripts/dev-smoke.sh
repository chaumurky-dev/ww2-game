#!/usr/bin/env bash
# Wait for the local server health endpoint and print OK.
set -euo pipefail
URL="${1:-http://localhost:8080/healthz}"
for _ in $(seq 1 60); do
  if curl -fsS "$URL" >/dev/null 2>&1; then echo OK; exit 0; fi
  sleep 1
done
echo "timed out waiting for $URL" >&2
exit 1

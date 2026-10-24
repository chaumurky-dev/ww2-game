#!/usr/bin/env bash
# Smoke test for a running Wartide server (just dev, or `docker compose --profile app up`).
#   scripts/dev-smoke.sh                 wait for /healthz and print OK
#   scripts/dev-smoke.sh --full [BASE]   also: register, verify via Mailpit, log in, create a
#                                        game vs 4 bots, wait for bot actions, read chat
# Env: BASE (default http://localhost:8080), MAILPIT (default http://localhost:8025), WAIT (secs of bot play, 60)
set -euo pipefail
FULL=0; [ "${1:-}" = "--full" ] && { FULL=1; shift; }
BASE="${1:-${BASE:-http://localhost:8080}}"; BASE="${BASE%/healthz}"
MAILPIT="${MAILPIT:-http://localhost:8025}"
for _ in $(seq 1 60); do
  if curl -fsS "$BASE/healthz" >/dev/null 2>&1; then break; fi
  sleep 1
done
curl -fsS "$BASE/healthz" >/dev/null 2>&1 || { echo "timed out waiting for $BASE/healthz" >&2; exit 1; }
echo OK
[ "$FULL" = 1 ] || exit 0

command -v python3 >/dev/null || { echo "python3 required for --full" >&2; exit 1; }
jq_() { python3 -c "import sys,json;d=json.load(sys.stdin);print($1)"; }
JAR="$(mktemp)"; trap 'rm -f "$JAR"' EXIT
U="smoke$RANDOM$RANDOM"; PW="correct-horse-battery"
J='content-type: application/json'
curl -fsS -X POST "$BASE/api/auth/register" -H "$J" \
  -d "{\"email\":\"$U@example.com\",\"username\":\"$U\",\"password\":\"$PW\"}" >/dev/null
echo "registered $U"
TOKEN=""
for _ in $(seq 1 20); do
  TOKEN="$(curl -fsS "$MAILPIT/api/v1/search?query=to:$U@example.com" | python3 -c "
import sys,json,re,urllib.request
d=json.load(sys.stdin)
if d['messages']:
    t=json.load(urllib.request.urlopen('$MAILPIT/api/v1/message/'+d['messages'][0]['ID']))['Text']
    m=re.search(r'token=([A-Za-z0-9_-]+)',t); print(m.group(1) if m else '')")"
  [ -n "$TOKEN" ] && break; sleep 1
done
[ -n "$TOKEN" ] || { echo "no verification email in Mailpit" >&2; exit 1; }
curl -fsS -X POST "$BASE/api/auth/verify-email" -H "$J" -d "{\"token\":\"$TOKEN\"}" >/dev/null
echo "verified"
curl -fsS -c "$JAR" -X POST "$BASE/api/auth/login" -H "$J" -d "{\"login\":\"$U\",\"password\":\"$PW\"}" >/dev/null
echo "logged in"
GID="$(curl -fsS -b "$JAR" -X POST "$BASE/api/games" -H "$J" \
  -d '{"mode":"five_player","bot_seats":[1,2,3,4]}' | jq_ "d['id']")"
echo "game $GID"
# Seat 0 (USSR) moves first, so bots only act once the human ends its turn: end phases until it passes.
for _ in $(seq 1 12); do
  CUR="$(curl -fsS -b "$JAR" "$BASE/api/games/$GID/state" | jq_ "d['state']['current_power']")"
  [ "$CUR" = ussr ] || break
  curl -sS -b "$JAR" -X POST "$BASE/api/games/$GID/actions" -H "$J" -d '{"action":{"type":"end_phase"}}' -o /dev/null
done
echo "human turn passed; bots playing"
N=0
for _ in $(seq 1 "${WAIT:-60}"); do
  sleep 1
  N="$(curl -fsS -b "$JAR" "$BASE/api/games/$GID/actions?since=0" | python3 -c "
import sys,json;d=json.load(sys.stdin);print(len(d if isinstance(d,list) else d.get('actions',[])))")"
done
ROUND="$(curl -fsS -b "$JAR" "$BASE/api/games/$GID" | jq_ "d.get('round', d.get('game',{}).get('round'))")"
echo "round=$ROUND bot_actions=$N after ${WAIT:-60}s"
CHAT="$(curl -fsS -b "$JAR" "$BASE/api/games/$GID/chat?channel=all" | python3 -c "
import sys,json;d=json.load(sys.stdin);print(len(d if isinstance(d,list) else d.get('messages',[])))")"
echo "chat messages=$CHAT"
[ "$N" -gt 0 ] || { echo "FAIL: bots made no actions" >&2; exit 1; }
echo FULL OK

#!/usr/bin/env bash
# A1 self-test: /ctl state must survive kill -9 + relaunch (the app auto-restart path).
# Also covers the ctl ready/notready gate. Cleans up after itself.
# usage: ./selftest_mock_state.sh [port]
set -euo pipefail
PORT="${1:-18796}"
DIR="$(cd "$(dirname "$(readlink -f "$0")")" && pwd)"
MOCK="$DIR/../scripts/mock_llama_server.py"
STATE="$DIR/../scripts/mock_state.json"
PIDS=""
cleanup() {
  # shellcheck disable=SC2086
  for p in $PIDS; do kill -9 "$p" 2>/dev/null || true; done
  rm -f "$STATE" mock_requests.jsonl
}
trap cleanup EXIT
rm -f "$STATE" mock_requests.jsonl

python3 "$MOCK" --port "$PORT" --mode normal >/tmp/mock_a1.log 2>&1 &
PIDS="$PIDS $!"
sleep 1.5
python3 "$DIR/mock_ctl.py" "$PORT" ctx400 | grep -q ctx400 || { echo "FAIL: ctl set"; exit 1; }
echo "ok: mode set to ctx400"

kill -9 $! 2>/dev/null; sleep 1.0   # hard kill, like an app worker death
python3 "$MOCK" --port "$PORT" --mode normal >/tmp/mock_a1b.log 2>&1 &
PIDS="$PIDS $!"
sleep 1.5
MODE=$(python3 "$DIR/mock_ctl.py" "$PORT" show)
echo "after relaunch: $MODE"
echo "$MODE" | grep -q '"mode": "ctx400"' || { echo "FAIL: mode did not persist across respawn"; exit 1; }
echo "ok: mode persisted across kill -9 + relaunch"

python3 "$DIR/mock_ctl.py" "$PORT" notready >/dev/null
[ "$(curl -s -o /dev/null -w "%{http_code}" http://127.0.0.1:$PORT/v1/models)" = "503" ] \
  || { echo "FAIL: notready gate"; exit 1; }
python3 "$DIR/mock_ctl.py" "$PORT" ready >/dev/null
curl -s http://127.0.0.1:$PORT/v1/models | grep -q mock-model || { echo "FAIL: ready gate"; exit 1; }
echo "ok: ctl ready/notready gate works"

python3 "$DIR/mock_ctl.py" "$PORT" redirect >/dev/null
CODE=$(curl -s -o /dev/null -w "%{http_code}" -X POST http://127.0.0.1:$PORT/v1/chat/completions \
  -H 'Content-Type: application/json' -d '{"messages":[]}')
[ "$CODE" = "302" ] || { echo "FAIL: redirect status $CODE"; exit 1; }
LOC=$(curl -s -D - -o /dev/null -X POST http://127.0.0.1:$PORT/v1/chat/completions \
  -H 'Content-Type: application/json' -d '{"messages":[]}' | grep -i "^location:" | tr -d '\r')
echo "redirect location: $LOC"
echo "$LOC" | grep -q "/REDIRECT_CANARY" || { echo "FAIL: not a canary redirect"; exit 1; }
echo "ok: redirect serves relative canary"

echo "A1 SELF-TEST PASS"

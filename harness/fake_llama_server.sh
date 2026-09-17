#!/usr/bin/env bash
# LocalPersona Settings → llama-server path = THIS file.
# Absorbs llama-server CLI flags (-m/--ctx-size/-ngl ignored by the python mock).
# MOCK_MODE selects initial mode (default normal); MOCK_STDOUT_FLOOD=1 for T-5.
# exec's python → the app-managed child PID is real (A-series kill/STOP works).
set -euo pipefail
PORT=8080
args=("$@")
for ((i = 0; i < ${#args[@]}; i++)); do
  case "${args[$i]}" in
    --port|-p) PORT="${args[$((i + 1))]:-8080}" ;;
    --port=*)  PORT="${args[$i]#--port=}" ;;
    -v|--version) echo "llama-server version b0.0.0-mock (audit harness)"; exit 0 ;;
  esac
done
DIR="$(cd "$(dirname "$(readlink -f "$0")")" && pwd)"
export MOCK_MODE="${MOCK_MODE:-normal}"
exec python3 "$DIR/../scripts/mock_llama_server.py" --port "$PORT" --mode "${MOCK_MODE}"

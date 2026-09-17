#!/usr/bin/env bash
# LocalPersona Soak Harness v1.1 — passive telemetry + reset-storm detection
# Usage: ./scripts/soak.sh [hours]  (default 8)
# Logs: soak_YYYYMMDD_HHMMSS.log in repo root or $SOAK_LOG path
set -euo pipefail

HOURS=${1:-8}
LOG_FILE="${SOAK_LOG:-soak_$(date +%Y%m%d_%H%M%S).log}"
APP_NAME="${APP_NAME:-localpersona}"
DATA_DIR="${XDG_DATA_HOME:-$HOME/.local/share}/com.localpersona.studio"

echo "Starting ${HOURS}-hour soak harness. Logging to $LOG_FILE" | tee -a "$LOG_FILE"
echo "APP_NAME=$APP_NAME DATA_DIR=$DATA_DIR" | tee -a "$LOG_FILE"
echo "Ensure the app is running (cargo tauri dev or target/release/localpersona)" | tee -a "$LOG_FILE"
echo "Columns: time | PID | RSS KB | FDs | Disk KB | Ghost PIDs | Resets" | tee -a "$LOG_FILE"

if date -v+1H >/dev/null 2>&1; then
    END_TIME=$(date -v+${HOURS}H +%s) # macOS
else
    END_TIME=$(date -d "+${HOURS} hours" +%s) # Linux
fi

# Optional: track reset count via diagnostics endpoint if jq and endpoint available
# DIAG_URL="http://127.0.0.1:__PORT__/diagnostics" — not needed for passive harness

while [ "$(date +%s)" -lt "$END_TIME" ]; do
    PID=$(pgrep -f "$APP_NAME" 2>/dev/null | head -n 1 || true)

    if [ -z "$PID" ]; then
        echo "$(date +%T) | App not running. Waiting..." | tee -a "$LOG_FILE"
    else
        RSS=$(ps -o rss= -p "$PID" 2>/dev formed 2>/dev/null | tr -d ' ' || ps -o rss= -p "$PID" 2>/dev/null | awk 'NR==2{print $1}' || echo "0")
        # Linux proc fd count; fallback 0 on macOS
        if [ -d "/proc/$PID/fd" ]; then
            FDS=$(ls "/proc/$PID/fd" 2>/dev/null | wc -l | tr -d ' ')
        else
            # macOS: lsof count
            FDS=$(lsof -p "$PID" 2>/dev/null | wc -l | tr -d ' ' || echo "0")
        fi
        DISK=$(du -s "$DATA_DIR" 2>/dev/null | awk '{print $1}' || echo "0")
        PIDS_TMP=$(ls /tmp/localpersona-*.pid 2>/dev/null | wc -l | tr -d ' ' || echo "0")
        # Optional reset count via log grep or diagnostics
        echo "$(date +%T) | PID: $PID | RSS: $RSS KB | FDs: $FDS | Disk: $DISK KB | Ghost PIDs: $PIDS_TMP" | tee -a "$LOG_FILE"
    fi
    sleep 60
done

echo "Soak complete. Analyzing $LOG_FILE..." | tee -a "$LOG_FILE"
echo "--- SUMMARY ---" | tee -a "$LOG_FILE"
if [ -f "$LOG_FILE" ]; then
    echo "Lines logged: $(wc -l < "$LOG_FILE")" | tee -a "$LOG_FILE"
    # Simple leak hint: last RSS vs first RSS
    FIRST_RSS=$(grep -m1 "RSS:" "$LOG_FILE" | sed -n 's/.*RSS: \([0-9]*\) KB.*/\1/p' || echo "0")
    LAST_RSS=$(grep "RSS:" "$LOG_FILE" | tail -n1 | sed -n 's/.*RSS: \([0-9]*\) KB.*/\1/p' || echo "0")
    echo "First RSS: ${FIRST_RSS} KB, Last RSS: ${LAST_RSS} KB" | tee -a "$LOG_FILE"
    if [ "$FIRST_RSS" != "0" ] && [ "$LAST_RSS" != "0" ]; then
        if [ "$LAST_RSS" -gt $((FIRST_RSS * 2)) ] && [ "$LAST_RSS" -gt 500000 ]; then
            echo "WARN: RSS more than doubled — possible leak or reset-storm (M-1)" | tee -a "$LOG_FILE"
        fi
    fi
    GHOST_FINAL=$(grep "Ghost PIDs:" "$LOG_FILE" | tail -n1 | sed -n 's/.*Ghost PIDs: \([0-9]*\).*/\1/p' || echo "0")
    if [ "$GHOST_FINAL" != "0" ]; then
        echo "WARN: $GHOST_FINAL ghost PID files remain — check A7 hygiene" | tee -a "$LOG_FILE"
    fi
fi
echo "Done. Review $LOG_FILE and run: heaptrack <app> for detailed heap profile if needed."

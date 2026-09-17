#!/bin/bash
# Phase 12.0: cgroups v2 Memory Monitor for Preemptive Reset
#
# This script monitors memory.current for the app's cgroup and triggers
# a Preemptive Reset when the rate of consumption (first derivative)
# exceeds a configurable threshold.
#
# Usage:
#   ./cgroup_monitor.sh [threshold_kb_per_sec] [check_interval_secs]
#
# Defaults:
#   threshold_kb_per_sec = 10240 (10 MB/s)
#   check_interval_secs = 5
#
# Requirements:
#   - Linux with cgroups v2 enabled
#   - The app must be running in a cgroup with memory.current available
#   - On systemd systems: systemd-run --scope -p MemoryMax=4G ./localpersona
#
# How it works:
#   1. Reads memory.current every N seconds
#   2. Calculates slope = (current - previous) / delta_t
#   3. If slope > threshold for 3 consecutive samples, signals the app
#   4. The app responds by resetting the LLM server (Arena Reset)

set -euo pipefail

# Configuration
THRESHOLD_KB_PER_SEC=${1:-10240}
CHECK_INTERVAL=${2:-5}
CONSECUTIVE_REQUIRED=3
MEMORY_FILE="/sys/fs/cgroup/user.slice/memory.current"
SIGNAL_FILE="/tmp/localpersona-preemptive-reset"

# Colors for output
RED='\033[0;31m'
YELLOW='\033[1;33m'
GREEN='\033[0;32m'
NC='\033[0m' # No Color

log_info() { echo -e "${GREEN}[INFO]${NC} $(date '+%Y-%m-%d %H:%M:%S') $*"; }
log_warn() { echo -e "${YELLOW}[WARN]${NC} $(date '+%Y-%m-%d %H:%M:%S') $*"; }
log_crit() { echo -e "${RED}[CRIT]${NC} $(date '+%Y-%m-%d %H:%M:%S') $*"; }

# Check if cgroups v2 memory controller is available
if [ ! -f "$MEMORY_FILE" ]; then
    log_warn "memory.current not found at $MEMORY_FILE"
    log_info "Trying alternative cgroup paths..."

    # Try common alternative paths
    for path in \
        "/sys/fs/cgroup/memory.current" \
        "/sys/fs/cgroup/$(cat /proc/self/cgroup 2>/dev/null | cut -d: -f3)/memory.current" \
        "/proc/self/cgroup"; do
        if [ -f "$path" ]; then
            MEMORY_FILE="$path"
            log_info "Using: $MEMORY_FILE"
            break
        fi
    done

    if [ ! -f "$MEMORY_FILE" ]; then
        log_crit "No cgroups v2 memory controller found."
        log_info "To enable cgroups v2:"
        log_info "  1. Boot with: systemd.unified_cgroup_hierarchy=1"
        log_info "  2. Or run in a container with cgroups v2"
        log_info "  3. Or use: systemd-run --scope -p MemoryMax=4G $0"
        exit 1
    fi
fi

log_info "Starting cgroups v2 Memory Monitor"
log_info "  Memory file: $MEMORY_FILE"
log_info "  Threshold: ${THRESHOLD_KB_PER_SEC} KB/s"
log_info "  Check interval: ${CHECK_INTERVAL}s"
log_info "  Consecutive samples required: ${CONSECUTIVE_REQUIRED}"
echo ""

# Read initial memory
PREV_MEM=$(cat "$MEMORY_FILE" 2>/dev/null || echo 0)
PREV_TIME=$(date +%s)
CONSECUTIVE_HIGH=0

while true; do
    sleep "$CHECK_INTERVAL"

    # Read current memory
    NOW_MEM=$(cat "$MEMORY_FILE" 2>/dev/null || echo 0)
    NOW_TIME=$(date +%s)

    # Calculate slope (first derivative)
    DELTA_T=$((NOW_TIME - PREV_TIME))
    if [ "$DELTA_T" -gt 0 ]; then
        DELTA_MEM=$((NOW_MEM - PREV_MEM))
        # Integer division for slope (KB/s)
        SLOPE=$((DELTA_MEM / DELTA_T))
    else
        SLOPE=0
    fi

    # Convert to MB for display
    NOW_MB=$((NOW_MEM / 1024))
    SLOPE_MB=$((SLOPE / 1024))

    # Check threshold
    if [ "$SLOPE" -gt "$THRESHOLD_KB_PER_SEC" ]; then
        CONSECUTIVE_HIGH=$((CONSECUTIVE_HIGH + 1))
        log_warn "High memory slope: ${SLOPE_MB} MB/s (threshold: $((THRESHOLD_KB_PER_SEC / 1024)) MB/s) [${CONSECUTIVE_HIGH}/${CONSECUTIVE_REQUIRED}]"
    else
        if [ "$CONSECUTIVE_HIGH" -gt 0 ]; then
            log_info "Slope normalized: ${SLOPE_MB} MB/s — counter reset"
        fi
        CONSECUTIVE_HIGH=0
    fi

    # Trigger preemptive reset if threshold exceeded
    if [ "$CONSECUTIVE_HIGH" -ge "$CONSECUTIVE_REQUIRED" ]; then
        log_crit "PREEMPTIVE RESET TRIGGERED!"
        log_crit "  Memory: ${NOW_MB} MB"
        log_crit "  Slope: ${SLOPE_MB} MB/s for ${CONSECUTIVE_HIGH} consecutive samples"

        # Write signal file for the app to pick up
        echo "$(date +%s) preemptive_reset" > "$SIGNAL_FILE"

        # Also try to signal the app via Tauri (if running)
        # This is a fallback — the app should be monitoring this file
        CONSECUTIVE_HIGH=0
    fi

    # Status line (overwrite)
    printf "\r  RSS: %6d MB | Slope: %+6d MB/s | High: %d/%d " \
        "$NOW_MB" "$SLOPE_MB" "$CONSECUTIVE_HIGH" "$CONSECUTIVE_REQUIRED"

    # Update previous values
    PREV_MEM="$NOW_MEM"
    PREV_TIME="$NOW_TIME"
done

#!/usr/bin/env bash
# usage: ./soak_monitor.sh [proc_pattern] [appdata_dir] [interval_s] [out.csv]
# Default pattern matches the installed binary name. CSV columns:
#   ts, rss_kb, fds, disk_kb, tmp_pid_files, llama_procs
# PASS: rss bounded growth (after model-load warmup), fds plateau,
#       tmp_pid_files stable (<=2), llama_procs stable (<=2 with voice).
set -u
PAT="${1:-localpersona-studio}"
DATA="${2:-$HOME/.local/share/com.localpersona.studio}"
INT="${3:-60}"; OUT="${4:-soak.csv}"
echo "ts,rss_kb,fds,disk_kb,tmp_pid_files,llama_procs" >> "$OUT"
misses=0
while true; do
  P=$(pgrep -x "$PAT" | head -n1)
  [ -z "$P" ] && P=$(pgrep -f "$PAT" | grep -vw $$ | head -n1)
  if [ -z "$P" ]; then
    misses=$((misses + 1))
    [ "$misses" -ge 5 ] && { echo "$(date +%s),app-exited,0,0,0,$(pgrep -c llama-server || echo 0)" >> "$OUT"; break; }
    sleep "$INT"; continue
  fi
  misses=0
  RSS=$(ps -o rss= -p "$P" 2>/dev/null | tr -d ' ')
  FDS=$(ls "/proc/$P/fd" 2>/dev/null | wc -l)
  DISK=$(du -sk "$DATA" 2>/dev/null | cut -f1)
  TMPF=$(ls /tmp/localpersona-*.pid 2>/dev/null | wc -l)
  LLP=$(pgrep -c llama-server 2>/dev/null || echo 0)
  echo "$(date +%s),${RSS:-0},${FDS:-0},${DISK:-0},${TMPF:-0},${LLP}" >> "$OUT"
  sleep "$INT"
done
echo "monitor done — check last row: llama_procs must be 0 after app exit (ghost check A7)"

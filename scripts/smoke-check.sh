#!/usr/bin/env bash
# LocalPersona RC smoke gate (non-UI). Run after overnight fixes / before human sunrise review.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

echo "=== LocalPersona smoke-check ==="
echo "Root: $ROOT"
echo

echo "[1/5] JS syntax…"
node --check frontend/script.js
echo "  OK"

echo "[2/5] cargo check…"
cargo check -q 2>/dev/null || cargo check
echo "  OK"

echo "[3/5] lib tests…"
cargo test --lib -q
echo "  OK"

echo "[4/5] property tests…"
cargo test --test property_tests -q
echo "  OK"

echo "[5/5] destructive tests…"
cargo test --test destructive_tests -q
echo "  OK"

echo
echo "=== Smoke checks passed ==="
echo "Human still required: cargo tauri dev → Settings, send, regen, server start."
echo "See docs/handbook/SUNRISE_REVIEW.md if present."

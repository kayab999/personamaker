#!/usr/bin/env bash
# 🛡️ El Tribunal Implacable — LocalPersona v12.0 (mirrors .github/workflows/tribunal.yml)
#
# This is the enforceable root gate for the audit-fix-audit loop.
# Every commit (human or agent) must pass this or be rejected.
# Version is kept in lockstep with the CI workflow name (Forensic Tribunal — vX.Y).
# Bump BOTH files together.
#
# Invariants enforced today:
#   - cargo check (no compilation debt)
#   - All destructive tests (the current Tribunal test battery)
#
# Future expansions (C01/C03 roadmap):
#   - Schema drift detection (generate_type_schemas + TS compile gate)
#   - Additional chaos mocks (partial HTTP, SIGKILL workers, disk full, Arena Reset at exact N)
#   - Run inside an ephemeral container for stronger isolation
#
# Usage:
#   ./scripts/tribunal.sh
#
# The pre-commit hook calls this automatically.
#
# Exit codes:
#   0 = Tribunal approves (Blast Radius = 0 for this change)
#   1 = Tribunal rejects — fix the failure and retry

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

echo "🛡️  Ejecutando el Tribunal Implacable (v12.0)..."
echo "    Scope: cargo check + destructive tests (Arena Reset, ndjson integrity, locks, ID validation, etc.)"
echo ""

# 1. Compilation gate
echo "→ [1/3] cargo check (compilation + type safety)..."
if ! cargo check 2>&1; then
    echo ""
    echo "❌ El Tribunal rechaza este commit."
    echo "   Razón: cargo check falló. Blast Radius potencial > 0 (código que ni compila)."
    echo "   Corrige los errores de compilación y reintenta."
    exit 1
fi
echo "   ✓ cargo check limpio."

# 2. Destructive / Invariant tests (the real Tribunal battery)
echo ""
echo "→ [2/3] Destructive tests (invariantes de integridad, Arena Reset, corrupción, concurrencia, identity)..."
if ! cargo test --test destructive_tests -- --quiet 2>&1; then
    echo ""
    echo "❌ El Tribunal rechaza este commit."
    echo "   Razón: Uno o más tests destructivos fallaron."
    echo "   Esto viola invariantes críticas (partial reads, locks, Arena Reset, path traversal, etc.)."
    echo "   Blast Radius > 0 detectado. No se permite el commit."
    echo ""
    echo "   Ejecuta manualmente para detalles:"
    echo "     cargo test --test destructive_tests -- --nocapture"
    exit 1
fi
echo "   ✓ Destructive / invariant tests passed."

# 3. Property tests (aligned with CI proptest job — R0)
echo ""
echo "→ [3/3] Property-based tests (ID validation, truncate, circuit breaker)..."
if ! cargo test --test property_tests -- --quiet 2>&1; then
    echo ""
    echo "❌ El Tribunal rechaza este commit."
    echo "   Razón: Property tests failed (fuzzed invariants)."
    echo "   Ejecuta: cargo test --test property_tests -- --nocapture"
    exit 1
fi
echo "   ✓ Property tests passed."

echo ""
echo "✅ El Tribunal aprueba este commit. Blast Radius = 0 verificado."
echo "   Gates: cargo check + destructive_tests + property_tests."
echo "   Principios: #1 (nada silencioso), #5 (test precede solución),"
echo "   #8 (Blast Radius control), #11 (mensajes no envenenan), #12 (Arena Reset)."
echo ""
exit 0
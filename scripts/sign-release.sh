#!/usr/bin/env bash
# LocalPersona — release signing helper (sweep W3, F-09 remainder).
#
# Signs dist/<version>/CHECKSUMS.txt so testers can verify provenance.
# Optional and non-fatal: without a key it prints the manual checklist.
#
# Usage:
#   LOCALPERSONA_SIGN_KEY=<gpg-key-id> ./scripts/sign-release.sh [dist-dir]
#   MINISIGN_KEY=~/.minisign/key.pub ./scripts/sign-release.sh [dist-dir]
#
# With neither key configured, exits 0 after printing setup instructions
# (packaging must not fail for lack of signing keys on a dev machine).
set -euo pipefail

OUT_DIR="${1:-$(ls -dt dist/*/ 2>/dev/null | head -n 1)}"
if [[ -z "${OUT_DIR:-}" || ! -d "$OUT_DIR" ]]; then
  echo "sign-release: no dist dir found; build first (scripts/package-release.sh)"
  exit 0
fi

SUMS="$OUT_DIR/CHECKSUMS.txt"
if [[ ! -f "$SUMS" ]]; then
  echo "sign-release: $SUMS missing; nothing to sign"
  exit 0
fi

signed=0

if [[ -n "${LOCALPERSONA_SIGN_KEY:-}" ]]; then
  if command -v gpg >/dev/null; then
    echo "→ gpg detached signature with key $LOCALPERSONA_SIGN_KEY"
    gpg --batch --yes --local-user "$LOCALPERSONA_SIGN_KEY" \
      --armor --detach-sign --output "$SUMS.asc" "$SUMS"
    gpg --verify "$SUMS.asc" "$SUMS" && echo "✅ gpg signature verified"
    signed=1
  else
    echo "⚠️  LOCALPERSONA_SIGN_KEY set but gpg not installed"
  fi
fi

if [[ -n "${MINISIGN_KEY:-}" && "$signed" -eq 0 ]]; then
  if command -v minisign >/dev/null; then
    echo "→ minisign signature"
    minisign -S -m "$SUMS" -s "${MINISIGN_SECRET:-$HOME/.minisign/minisign.key}"
    echo "✅ minisign signature created ($SUMS.minisig)"
    signed=1
  else
    echo "⚠️  MINISIGN_KEY set but minisign not installed"
  fi
fi

if [[ "$signed" -eq 0 ]]; then
  cat <<'EOF'
sign-release: no signing key configured — CHECKSUMS.txt left unsigned.
To enable provenance for the next release, EITHER:
  1. gpg:      gpg --quick-generate-key "LocalPersona Releases" \
               && LOCALPERSONA_SIGN_KEY=<key-id> ./scripts/sign-release.sh
  2. minisign: minisign -G -p ~/.minisign/key.pub \
               && MINISIGN_KEY=~/.minisign/key.pub ./scripts/sign-release.sh
Testers verify with: gpg --verify CHECKSUMS.txt.asc CHECKSUMS.txt
                  or: minisign -V -p <key.pub> -m CHECKSUMS.txt
EOF
fi

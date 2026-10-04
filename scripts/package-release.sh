#!/usr/bin/env bash
# LocalPersona — commercial release packaging (Linux AppImage + deb)
#
# Usage:
#   ./scripts/package-release.sh           # build only
#   ./scripts/package-release.sh --tribunal  # run Tribunal first
#
# Output: dist/<version>/ with installers + CHECKSUMS.txt

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

VERSION="$(grep -m1 '^version' Cargo.toml | sed 's/.*"\(.*\)"/\1/')"
OUT_DIR="$REPO_ROOT/dist/${VERSION}"
RUN_TRIBUNAL=0

for arg in "$@"; do
  case "$arg" in
    --tribunal) RUN_TRIBUNAL=1 ;;
    -h|--help)
      echo "Usage: $0 [--tribunal]"
      exit 0
      ;;
  esac
done

echo "══════════════════════════════════════════════"
echo " LocalPersona package-release  v${VERSION}"
echo "══════════════════════════════════════════════"

if [[ "$RUN_TRIBUNAL" -eq 1 ]]; then
  echo "→ Tribunal gate..."
  ./scripts/tribunal.sh
fi

# Guard: never allow test-models in tauri resources
if grep -q 'test-models' tauri.conf.json; then
  echo "❌ Refusing to package: tauri.conf.json still references test-models"
  exit 1
fi

echo "→ cargo tauri build --bundles deb (primary commercial artifact)..."
# AppImage/linuxdeploy is fragile on paths with spaces; deb is the reliable ship target.
cargo tauri build --bundles deb || {
  echo "deb bundle failed; still packing bare release binary if present"
}

# Optional AppImage attempt (non-fatal)
echo "→ optional AppImage attempt..."
cargo tauri build --bundles appimage || echo "⚠️  AppImage skipped/failed (common with spaces in path or missing linuxdeploy)"

mkdir -p "$OUT_DIR"

shopt -s nullglob
COPIED=0
for f in \
  target/release/bundle/appimage/*.AppImage \
  target/release/bundle/deb/*.deb \
  target/release/bundle/rpm/*.rpm
do
  if [[ -f "$f" ]]; then
    cp -v "$f" "$OUT_DIR/"
    COPIED=$((COPIED + 1))
  fi
done
shopt -u nullglob

if [[ -f target/release/localpersona ]]; then
  cp -v target/release/localpersona "$OUT_DIR/localpersona-linux-x86_64"
  COPIED=$((COPIED + 1))
fi

if [[ -d target/release/bundle/appimage/LocalPersona.AppDir ]]; then
  tar -C target/release/bundle/appimage -czf "$OUT_DIR/LocalPersona_${VERSION}_amd64-AppDir.tar.gz" LocalPersona.AppDir
  echo "Packed AppDir tarball (portable)"
  COPIED=$((COPIED + 1))
fi

if [[ "$COPIED" -eq 0 ]]; then
  echo "❌ No artifacts produced"
  exit 1
fi

# Installer docs
cp -v docs/packaging/INSTALL.md "$OUT_DIR/" 2>/dev/null || true
cp -v docs/handbook/RELEASE_NOTES_0.9.0-rc.1.md "$OUT_DIR/RELEASE_NOTES.md" 2>/dev/null || true
cp -v LICENSE "$OUT_DIR/" 2>/dev/null || true

(
  cd "$OUT_DIR"
  sha256sum * > CHECKSUMS.txt 2>/dev/null || true
)

# Sweep W3: optional provenance signature (non-fatal without keys).
"$REPO_ROOT/scripts/sign-release.sh" "$OUT_DIR" || true

echo ""
echo "✅ Packaging complete → $OUT_DIR"
ls -lah "$OUT_DIR" || true
echo ""
echo "Next: smoke-test AppImage, then share dist/${VERSION}/ with testers."

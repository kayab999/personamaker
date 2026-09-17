#!/usr/bin/env bash
# Install LocalPersona so it appears in the app menu / can be pinned to the dock.
#
# Usage:
#   ./scripts/install-desktop-launcher.sh
#   ./scripts/install-desktop-launcher.sh /path/to/localpersona
#
# Prefer system package when available:
#   sudo dpkg -i dist/0.9.0-rc.1/LocalPersona_*.deb
#   then re-run this script if icons need a cache refresh.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

APP_BIN="${1:-}"
if [[ -z "$APP_BIN" ]]; then
  if command -v localpersona >/dev/null 2>&1; then
    APP_BIN="$(command -v localpersona)"
  elif [[ -x "$REPO_ROOT/target/release/localpersona" ]]; then
    APP_BIN="$REPO_ROOT/target/release/localpersona"
  elif [[ -x "$REPO_ROOT/dist/0.9.0-rc.1/localpersona-linux-x86_64" ]]; then
    APP_BIN="$REPO_ROOT/dist/0.9.0-rc.1/localpersona-linux-x86_64"
  else
    echo "❌ No LocalPersona binary found."
    echo "   Build first: cargo tauri build --bundles deb"
    echo "   Or install:  sudo dpkg -i dist/0.9.0-rc.1/LocalPersona_*.deb"
    exit 1
  fi
fi

if [[ ! -x "$APP_BIN" ]]; then
  echo "❌ Not executable: $APP_BIN"
  exit 1
fi

ICON_SRC="$REPO_ROOT/icons/icon.png"
[[ -f "$ICON_SRC" ]] || ICON_SRC="$REPO_ROOT/icons/512x512.png"

APP_DIR="$HOME/.local/share/applications"
ICON_ROOT="$HOME/.local/share/icons/hicolor"
DESKTOP="$APP_DIR/com.localpersona.studio.desktop"

mkdir -p "$APP_DIR"
for size in 16 32 48 64 128 256 512; do
  mkdir -p "$ICON_ROOT/${size}x${size}/apps"
  dest="$ICON_ROOT/${size}x${size}/apps/localpersona.png"
  src="$REPO_ROOT/icons/${size}x${size}.png"
  if [[ -f "$src" ]]; then
    cp -f "$src" "$dest"
  elif command -v convert >/dev/null 2>&1; then
    convert "$ICON_SRC" -resize "${size}x${size}" "$dest" || cp -f "$ICON_SRC" "$dest"
  elif command -v magick >/dev/null 2>&1; then
    magick "$ICON_SRC" -resize "${size}x${size}" "$dest" || cp -f "$ICON_SRC" "$dest"
  else
    cp -f "$ICON_SRC" "$dest"
  fi
done

# Pixmap fallback (some docks)
mkdir -p "$HOME/.local/share/pixmaps"
cp -f "$ICON_SRC" "$HOME/.local/share/pixmaps/localpersona.png"

# Quote Exec path so spaces in the project directory work (e.g. "persona maker")
cat > "$DESKTOP" << EOF
[Desktop Entry]
Type=Application
Version=1.0
Name=LocalPersona
GenericName=AI Persona Studio
Comment=Local GGUF persona studio — own your models and characters
Exec="${APP_BIN}" %U
Icon=localpersona
Terminal=false
Categories=Utility;
Keywords=AI;LLM;GGUF;persona;chat;local;
StartupNotify=true
StartupWMClass=localpersona
EOF
chmod +x "$DESKTOP"

update-desktop-database "$APP_DIR" 2>/dev/null || true
gtk-update-icon-cache -f -t "$ICON_ROOT" 2>/dev/null || true

echo "✅ Dock / menu launcher installed"
echo "   Desktop: $DESKTOP"
echo "   Binary:  $APP_BIN"
echo "   Icon:    localpersona (hicolor theme)"
echo ""
echo "Next:"
echo "  • Open Activities / app grid and search “LocalPersona”"
echo "  • Right-click → Add to Favorites / Pin to dock"
echo "  • If icon is blank, log out/in or: gtk-update-icon-cache -f -t ~/.local/share/icons/hicolor"

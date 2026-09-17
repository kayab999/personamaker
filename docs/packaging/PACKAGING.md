# LocalPersona — Packaging Guide

## Brand

- **Product mark:** purple neural-profile icon set under `icons/`
- **Master archive:** `docs/branding/source-icon-1024.jpeg` (1024×1024 source)
- Runtime packaging uses only `icons/**` as declared in `tauri.conf.json`

## What ships in the installer

| Included | Not included |
|----------|----------------|
| LocalPersona binary + UI | GGUF models |
| Default personas + avatars | `llama-server` binary |
| USER_MANUAL | CUDA/Vulkan runtimes |

Users configure **llama-server** and model paths in Settings.

## Build installers (Linux)

```bash
# Optional quality gate
./scripts/tribunal.sh

# Preferred: deb (reliable)
cargo tauri build --bundles deb

# Full script (deb + copy to dist/)
./scripts/package-release.sh
```

**Note:** Project path must not break tools. If the repo lives under a path with **spaces** (e.g. `persona maker`), AppImage/`linuxdeploy` may fail. Prefer:

```bash
ln -sfn "/path/to/persona maker" /tmp/localpersona-build
cd /tmp/localpersona-build && cargo tauri build --bundles deb
```

If AppImage still fails, ship the **`.deb`** (primary) plus **AppDir tarball** / bare binary from `dist/`.

Targets in `tauri.conf.json`: **AppImage + deb** (AppImage optional if tooling allows).

## Dock / menu launcher

- Deb packages include `/usr/share/applications/LocalPersona.desktop` + hicolor icons.
- Desktop template: `packaging/linux/LocalPersona.desktop` (wired via `bundle.linux.deb.desktopTemplate`).
- For local/dev binaries: `./scripts/install-desktop-launcher.sh` installs a user-level `.desktop` entry + icons so the app can be pinned to the dock.

`StartupWMClass=localpersona` matches the binary name so the dock keeps the correct icon while the window is open.

## Critical rule

**Never** add `test-models/*` (or any `*.gguf`) to `bundle.resources`.  
`scripts/package-release.sh` refuses to build if `test-models` appears in `tauri.conf.json`.

## Windows / macOS

Change `bundle.targets` temporarily to include `nsis` / `dmg` / `app` on the appropriate host. Cross-compiling is not the default workflow.

## Version

Bump together:

- `Cargo.toml` `version`
- `tauri.conf.json` `version`
- `CHANGELOG.md`

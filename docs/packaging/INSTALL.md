# Install LocalPersona (Closed Beta)

## What you need

1. This package (AppImage or `.deb`)
2. A **llama-server** binary from [llama.cpp](https://github.com/ggerganov/llama.cpp)
3. One or more **`.gguf`** model files on disk

Models are **not** included in the installer.

---

## Linux — AppImage (recommended for testers)

```bash
chmod +x LocalPersona_*.AppImage
./LocalPersona_*.AppImage
```

If your system blocks unprivileged user namespaces, install the `.deb` instead or run with your distro’s AppImage runtime docs.

## Linux — Debian / Ubuntu (`.deb`)

```bash
sudo dpkg -i LocalPersona_*.deb
# if dependencies complain:
sudo apt-get install -f
```

Launch from the application menu / dock (**LocalPersona**) or:

```bash
localpersona
```

After install, pin to the dock: open the app once → right-click the dock icon → **Add to Favorites** / **Pin**.

## Linux — dock icon without system install

If you built from source or use the bare binary:

```bash
./scripts/install-desktop-launcher.sh
# or point at a specific binary:
./scripts/install-desktop-launcher.sh /path/to/localpersona
```

This installs:

- `~/.local/share/applications/com.localpersona.studio.desktop`
- Icons under `~/.local/share/icons/hicolor/*/apps/localpersona.png`

Then search **LocalPersona** in the app grid and pin it.

## First run

1. Open **Settings → Local Inference**
2. Point the app at your `llama-server` binary (or use auto-detect)
3. Scan for GGUF models / pick a model path
4. Start the server, select a contact, chat

Default example personas load automatically on first launch.

## Verify download integrity

```bash
sha256sum -c CHECKSUMS.txt
```

## Known RC limitations

- Token streaming is disabled (full responses only)
- Message edit is not available yet
- Voice / TTS is experimental
- You must supply llama-server and GGUF models yourself

## Data location

Characters and conversations are stored under your OS app data directory (file-based; easy to back up).

## Support

See the in-app **User Manual** (Help menu) and `RELEASE_NOTES.md` in this folder.

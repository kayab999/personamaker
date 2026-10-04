# LocalPersona

A sovereign, local-first persona generation engine.

## Documentation

| Doc | Purpose |
|-----|---------|
| **[docs/handbook/ARCHITECTURE_AND_STATUS.md](docs/handbook/ARCHITECTURE_AND_STATUS.md)** | **Full architecture blueprint + current status dump** |
| [docs/README.md](docs/README.md) | Documentation index |
| [AGENTS.md](AGENTS.md) | Developer / AI agent briefing |
| [assets/USER_MANUAL.md](assets/USER_MANUAL.md) | End-user manual (shipped in app) |
| [docs/packaging/INSTALL.md](docs/packaging/INSTALL.md) | Install closed-beta packages |
| [CHANGELOG.md](CHANGELOG.md) · [ROADMAP.md](ROADMAP.md) | History and phases |

**One-click local GGUF persona & character studio.**

A desktop application that lets you run powerful local AI characters (personas) using your own GGUF models — no cloud, no complex setup.

Offline by design: chat, characters, and conversations never leave your machine. No accounts, no telemetry. One noted exception: the first time you use the Knowledge Base (RAG), the app downloads a small embedding model (~90 MB, AllMiniLML6V2) — a one-time fetch; everything after that is fully offline.

## Features

- Rich character editor (personality, scenario, system prompts, writing instructions, avatars, etc.)
- Multiple writing modes (Advanced Chat, Adventure, Story, Character Generation)
- Real file-based persistence (characters + full chat histories)
- Speaker selection (You / Character / Narrator + future multi-persona support)
- Vision support for compatible models (upload images, character references, scene photos)
- Export / Import of characters
- Default personas included on first launch
- Native desktop experience (Tauri 2)

## Current Status (2026)

LocalPersona is on an **RC-stable track** after a ground-truth re-audit (2026-07). Reliability infrastructure is strong; product Q&A identity and settings contracts were fixed in the RC workplan (Phases R0–R3).

**Current strengths:**
- Strong file-based persistence with atomic writes, exclusive `fs2` locking, and append-only NDJSON conversations
- Correct **conversation UUID + character_id** inference contract (rich editor fields drive the model)
- Professional server lifecycle with explicit `ServerState` machine and clean shutdown handling
- Dual-policy **Arena Reset** (300 requests + 45-minute uptime) on both LLM and Voice servers
- Active **Tribunal** gate: `cargo check` + destructive tests + property tests via `scripts/tribunal.sh`
- GGUF metadata parsing, token-budget context loading, diagnostics panel
- Sampling settings (temperature / max_tokens / top_p) wired into inference
- Non-stream responses are the RC-stable default (token streaming deferred post-RC)

**Version:** `0.9.0-rc.1`  
**Installable:** Linux `.deb` (~14 MB) under `dist/0.9.0-rc.1/` — no models included.  
**RC maturity:** ~8.7–9.0; closed beta after human soak; not GA.

**Known RC limitations:** message edit deferred; streaming UI frozen off; voice/TTS experimental; AR/social out of scope; `.deb` is the ship vehicle (AppImage dropped from default targets — see packaging docs).

**Release notes:** [RELEASE_NOTES](docs/handbook/RELEASE_NOTES_0.9.0-rc.1.md) · [CHANGELOG](CHANGELOG.md) · [RC checklist](docs/handbook/RC_ACCEPTANCE_CHECKLIST.md) · [Blueprint](docs/handbook/ARCHITECTURE_AND_STATUS.md)

## Tech Stack

- **Frontend**: Vanilla HTML + Tailwind + TypeScript-free JavaScript
- **Backend**: Tauri 2 (Rust)
- **Inference**: llama.cpp `llama-server` (OpenAI-compatible)
- **Storage**: File-based (JSON) — easy to back up or edit manually

## Development

```bash
# Development
cargo tauri dev

# Quality gate
./scripts/tribunal.sh

# Production installer (Linux .deb) → dist/<version>/
./scripts/package-release.sh
```

See [docs/packaging/PACKAGING.md](docs/packaging/PACKAGING.md) and [docs/packaging/INSTALL.md](docs/packaging/INSTALL.md).

**Installer does not include GGUF models or llama-server.** Point Settings at your local binaries and models.

**Brand icon:** `icons/` (purple neural-profile mark). Master archive: `docs/branding/source-icon-1024.jpeg`.

### Current focus

- Human RC acceptance matrix + soak ([checklist](docs/handbook/RC_ACCEPTANCE_CHECKLIST.md))
- Post-RC: real streaming, message edit

See [ARCHITECTURE_AND_STATUS.md](docs/handbook/ARCHITECTURE_AND_STATUS.md) and [AGENTS.md](AGENTS.md).

## Default Personas

On first launch (or via the "Force Defaults" button in Settings), the app loads several high-quality example characters demonstrating different voices and styles:

- Support Unit (tactical deadpan AI)
- Cosmic Guide (wonder-struck science narrator)
- Eccentric Composer (avant-garde bandleader)
- Loyal Field Unit (expeditionary robot companion)
- Bitter Satirist (cynical aphorist)
- Neighborly Guide (patient neighborhood mentor)
- Absurdist Performer (lounge-act provocateur)

All seven are original archetypes (no real-person likenesses). Character IDs
are stable across updates; existing user characters are never migrated or
overwritten — these seeds only affect first launch / Force Defaults.

These serve both as useful starting characters and as living documentation of how to build good personas.

## Philosophy & Governance

- Personas/characters are first-class citizens.
- Total user freedom — nothing should be locked or hidden.
- You own your models, your data, and your creative work.
- The app should feel like a creative tool, not just another chatbot frontend.

**Development Governance**: All changes must pass the Tribunal (`./scripts/tribunal.sh`): `cargo check` + destructive/invariant tests + property tests. This protects both human and agent-driven development.

## License

MIT

---

For end-user documentation, see `assets/USER_MANUAL.md` (shipped with the application).

For AI coding agents and contributors, see `AGENTS.md` — it contains the current high-signal project context, architecture notes, philosophy guardrails, and the active remediation status.

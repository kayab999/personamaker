# LocalPersona — Full Architectural Blueprint & Status Dump

| Field | Value |
|-------|-------|
| **Document type** | Canonical architecture + living project status |
| **Version covered** | `0.9.0-rc.1` |
| **Last updated** | 2026-07-25 |
| **Audience** | Maintainers, AI agents, closed-beta partners |
| **Supersedes for “what is true now”** | Partial claims in older audits; use this file as the **start-here** technical dump |

---

## 1. Product definition

**LocalPersona** is a **local-first desktop studio** for creating and chatting with rich AI personas powered by **user-owned GGUF models** via **llama.cpp `llama-server`**.

### Non-negotiable principles

1. **Local ownership** — characters, chats, knowledge, and voice samples are real files under the app data directory.  
2. **No cloud lock-in** — offline by design; no accounts, no telemetry.  
3. **Depth over virality** — creative tool / simulated contacts, not “AI Tinder.”  
4. **External inference** — the app manages a child `llama-server`; it does not embed model weights.  
5. **Blast radius control** — failures should fail closed on data (no silent corruption, no empty assistant rows).

### What is *not* in this product (RC)

| Not included | Why |
|--------------|-----|
| Bundled GGUF models | Size + licensing; BYO models |
| Bundled llama-server | Platform/GPU matrix; BYO binary |
| Live token streaming UI | Frozen for stability (post-RC) |
| Message edit / branch | Deferred |
| AR / social / geolocation | Vision only (`FUTURE_VISION.md`) |
| Production AppImage on this host | `linuxdeploy` failed; **`.deb` ships** |

---

## 2. Current status snapshot (2026-07-25)

### 2.1 Release posture

| Item | Status |
|------|--------|
| Version | `0.9.0-rc.1` |
| Identifier | `com.localpersona.studio` |
| License | MIT (`LICENSE`) |
| Tribunal | `cargo check` + destructive (21) + property (18) |
| Installable package | **Yes** — `dist/0.9.0-rc.1/LocalPersona_0.9.0-rc.1_amd64.deb` (~14 MB) |
| Standalone binary | `dist/0.9.0-rc.1/localpersona-linux-x86_64` (~34 MB) |
| AppDir portable | `LocalPersona_*_AppDir.tar.gz` (~94 MB) |
| AppImage | Not produced on this host (`linuxdeploy` failure; often path/tooling) |
| Models in package | **None** (correct) |
| Manual soak / 8-scenario matrix | Still **human-owned** (`RC_ACCEPTANCE_CHECKLIST.md`) |

### 2.2 Maturity (honest scores)

| Domain | Score | Notes |
|--------|------:|-------|
| Architecture / sovereignty | 8.5–9 | File SoT, dual servers, atomic IO |
| Reliability | 8–8.5 | Arena, health, Tribunal; soak unproven |
| Q&A / persona fidelity | ~8 | Identity + compose + sampling fixed |
| UX polish | 7–7.5 | Messenger direction; single JS file |
| Packaging / commercial | ~8 | Deb + docs; AppImage gap |
| Release engineering | 7–7.5 | Scripts + dist; git history thin in some workspaces |
| **Overall RC readiness** | **~8.7–9.0** | Closed beta after human matrix |

Earlier “9.2–9.3” scores over-weighted infrastructure and under-weighted product contract bugs (since fixed).

### 2.3 Major workstreams completed

| Wave | Outcome |
|------|---------|
| Phase 0 reliability | Atomic writes, locks, PID hygiene, readiness polling |
| v7.2 C01–C03 | Tribunal, Arena 300+45m, golden IPC schema |
| Q&A remediation | Rich `compose_system_prompt`, modes, RAG header |
| R0 identity | `character_id` + conversation UUID; never `"user"` as character file |
| R1–R3 product | Sampling wired, stream freeze, context pill, edit hidden |
| Architecture freeze A–C | NDJSON-only interactive chat; shared inference helpers; diagnostics |
| Commercial packaging P0–P5 | No GGUF in bundle; deb kit; brand icons; LICENSE; packaging docs |

---

## 3. System architecture (blueprint)

### 3.1 High-level diagram

```
┌─────────────────────────────────────────────────────────────────────────┐
│                     LocalPersona Desktop (Tauri 2)                        │
│                                                                           │
│  ┌──────────────────────────┐     invoke / events      ┌──────────────┐ │
│  │ frontend/                  │◄──────────────────────►│ commands.rs  │ │
│  │  index.html · script.js    │   JSON (serde)         │  IPC hub     │ │
│  │  style.css  (~3.3k LOC JS) │                        └──────┬───────┘ │
│  └──────────────────────────┘                               │         │
│         │                                                    │         │
│         │                          ┌─────────────────────────┼───────┐ │
│         │                          ▼                         ▼       │ │
│         │                   storage.rs              conversation.rs  │ │
│         │                   characters, avatars,    NDJSON append,   │ │
│         │                   knowledge, atomic IO    token budget     │ │
│         │                          │                         │       │ │
│         │                          ▼                         │       │ │
│         │                        rag.rs  ◄── embeddings      │       │ │
│         │                        gguf.rs ◄── model metadata  │       │ │
│         │                                                    │       │ │
│         │                          inference.rs              │       │ │
│         │                     ┌────┴────┐                    │       │ │
│         │                     ▼         ▼                    │       │ │
│         │              LlamaServer   VoiceServer             │       │ │
│         │              Manager       Manager (exp.)          │       │ │
│         └─────────────────────┬─────────┬────────────────────┘       │ │
└───────────────────────────────┼─────────┼────────────────────────────┘ │
                                │ HTTP    │ HTTP (localhost only)          │
                                ▼         ▼                                │
                         llama-server  llama-server (TTS)                  │
                         + user GGUF   + TTS GGUF (opt.)                   │
                                                                           │
                         App data dir (files user owns)                    │
                         characters/ · conversations/ · images/ · …        │
```

### 3.2 Technology stack

| Layer | Choice |
|-------|--------|
| Shell | Tauri 2 |
| Backend | Rust 2021, tokio |
| Frontend | Vanilla HTML/JS/CSS (no bundler) |
| Inference API | OpenAI-compatible HTTP to child process |
| Storage | Files only (JSON + NDJSON); no DB |
| Embeddings | fastembed (AllMiniLML6V2) |
| Tests | destructive_tests, property_tests, unit tests in modules |
| CI / local gate | `scripts/tribunal.sh`, `.github/workflows/tribunal.yml` |

### 3.3 Repository layout (post-cleanup)

```
localpersona/
  README.md                 # Product front door
  AGENTS.md                 # Agent/dev briefing (high signal)
  CHANGELOG.md
  LICENSE
  ROADMAP.md
  Cargo.toml / Cargo.lock
  tauri.conf.json
  build.rs
  capabilities/
  icons/                    # Product brand (purple neural profile)
  assets/                   # Shipped resources only
    default_personas.json
    USER_MANUAL.md
    avatars/default/
  frontend/                 # index.html, script.js, style.css
  src/                      # Rust modules (see §4)
  tests/                    # destructive + property
  scripts/
    tribunal.sh
    package-release.sh
  docs/
    handbook/               # Architecture, audits, RC, this blueprint
    packaging/              # INSTALL, PACKAGING
    branding/               # source-icon-1024.jpeg archive
    VOICE_SETUP.md
  dist/<version>/           # Built installers (gitignored)
  test-models/              # Local GGUF only (gitignored, never bundled)
  gen/schemas/              # Golden IPC + Tauri schemas
```

---

## 4. Module map (Rust)

| Module | LOC (approx.) | Responsibility |
|--------|---------------|----------------|
| `main.rs` | ~300 | App setup, menus, managed state, memory-monitor Arena, window close stop |
| `lib.rs` | ~15 | Crate modules for tests |
| `commands.rs` | ~2200 | **IPC hub**: send/regenerate, storage cmds, diagnostics, schemas |
| `inference.rs` | ~1100 | Dual managers, ServerState, Arena, PID, GGUF ctx on start |
| `conversation.rs` | ~820 | NDJSON append, budget load, repair, validate_conv_id |
| `storage.rs` | ~890 | Characters, atomic write, avatars, knowledge chunking, path safety |
| `rag.rs` | ~95 | Embed + cosine top-k |
| `gguf.rs` | ~230 | Metadata parse |
| `memory_monitor.rs` | ~240 | RSS/VMS → reset channel |
| `circuit_breaker.rs` | ~250 | Failure dedup scaffolding |
| `autopsy.rs` | ~215 | Panic dumps scaffolding |
| `models.rs` | stub | Placeholder |

### Shared inference pipeline (single path)

Used by both `send_message_with_images` and `regenerate_last_message`:

1. `resolve_character_id_for_inference` — never treats role `"user"` as character file id  
2. `inference_token_budget` — GGUF context × 0.82 or 8192  
3. `load_history_for_inference` — token budget walk, fallback page  
4. `compose_system_with_rag` — rich prompt + optional knowledge  
5. `post_chat_completion` — health check, drop lock, HTTP, parse, reject empty  
6. `append_message` (+ user on send)  
7. `check_arena_reset`  

### Compose contract

```
if system_prompt non-empty → use only that (power-user override)
else:
  [Interaction Mode] from mode (adventure/story/…)
  You are {name}
  [Personality] [Scenario] [You/user] [Writing] [Greeting reference]
  Stay in character…
then optional [Relevant Knowledge…] from RAG
```

### Identity contract

| Field | Meaning |
|-------|---------|
| `conversation_id` | UUID directory under `conversations/` |
| `character_id` | File key for `characters/{id}.json` + RAG |
| `speaker_id` / `speaker_name` | Attribution on the **user** message only (`user` / nickname) |

Frontend **must not** pass character id as conversation id (fixed; orphan detector exists).

---

## 5. Data model & persistence

### 5.1 Canonical interactive chat (only)

```
{app_data}/conversations/{uuid}/
  metadata.json      # id, name, participant_ids[], message_count, last_* preview, version
  messages.ndjson    # one ChatMessage JSON object per line
```

- **Write:** exclusive lock + append + fsync; metadata atomic update  
- **Read:** paginated or `load_messages_within_token_budget` (reverse accumulate)  
- **Repair:** `repair_conversation` rebuilds valid lines + metadata  

### 5.2 Characters

```
{app_data}/characters/{id}.json
{app_data}/characters/{id}/knowledge/   # docs + *.chunks.json
```

`StoredCharacter`: personality, scenario, writing_instructions, system_prompt, greeting, voice_*, has_knowledge_base, version, …

### 5.3 Legacy (deprecated for interactive UI)

```
{app_data}/chat_histories/{ownerId}.json
```

Still deletable on character remove; FE no longer reads/writes for chat.  
`list_orphan_conversation_hints` finds conversation ids that equal character ids (pre-R0 bug survivors).

### 5.4 Atomicity

- `atomic_write` / `atomic_write_bytes` + optional exclusive lock  
- Applied to characters, metadata, config, binaries (avatars, docs, …)

---

## 6. Runtime: dual servers

### 6.1 `ServerState`

`Idle | Starting | Running | Stopping | Error | Restarting`

### 6.2 Arena Reset (both LLM and Voice)

| Policy | Default |
|--------|---------|
| Request count | 300 |
| Continuous uptime | 45 minutes |
| Memory pressure | Channel → reset with reason `memory_pressure` |

Reasons recorded: `request_threshold`, `uptime_threshold`, `memory_pressure`. Exposed in Diagnostics.

### 6.3 Process hygiene

- PID files in temp  
- Drop → `start_kill`  
- Window close handler stops both managers  
- Health `try_wait` before expensive HTTP  

### 6.4 Security (inference)

- `validate_localhost_endpoint` — only 127.0.0.1 / localhost / ::1  
- Image/doc/TTS size caps  
- Path IDs: ASCII + no `..` / slashes; absolute paths under app base  

---

## 7. Frontend architecture

| Concern | Implementation |
|---------|----------------|
| State | Global `APP_STATE` + `currentConversationId` |
| IPC | `callTauri` → throw + toast |
| Chat load | Bounded `load_messages_for_display` + “Load earlier” |
| Send | Unified path → `send_message_with_images` with sampling + character_id |
| Stream | **Frozen off** (R2-A); toggle disabled in Settings |
| Context | Pill shows estimated tokens / budget |
| Edit | UI removed for RC |
| Diagnostics | ServerState, Arena (+ last reset), memory, RAG status, orphans |

---

## 8. Packaging & distribution

### 8.1 Bundle configuration (`tauri.conf.json`)

| Key | Value |
|-----|--------|
| targets | `appimage`, `deb` (deb reliable) |
| resources | personas, USER_MANUAL, default avatars **only** |
| icons | PNG set + ico under `icons/` |
| licenseFile | `LICENSE` |

### 8.2 Release kit location

```
dist/0.9.0-rc.1/
  LocalPersona_0.9.0-rc.1_amd64.deb     # ~14 MB — primary
  localpersona-linux-x86_64             # bare binary
  LocalPersona_*_AppDir.tar.gz          # portable
  INSTALL.md · LICENSE · CHECKSUMS.txt · RELEASE_NOTES.md
```

### 8.3 Build commands

```bash
./scripts/tribunal.sh
./scripts/package-release.sh          # prefers deb; AppImage optional
# or:
cargo tauri build --bundles deb
```

**Install:** `sudo dpkg -i LocalPersona_0.9.0-rc.1_amd64.deb`  
Depends: `libwebkit2gtk-4.1-0`, `libgtk-3-0`.

### 8.4 Brand

- Runtime: `icons/**` (purple neon profile + neural mesh)  
- Archive master: `docs/branding/source-icon-1024.jpeg`  

---

## 9. Governance & quality gates

### 9.1 Tribunal (`scripts/tribunal.sh`)

1. `cargo check`  
2. `cargo test --test destructive_tests`  
3. `cargo test --test property_tests`  

Pre-commit hook invokes Tribunal when present.

### 9.2 Test inventory (order of magnitude)

| Suite | Count | Focus |
|-------|------:|-------|
| Destructive | 21 | NDJSON, locks, Arena, identity, sampling, schema, chaos parse |
| Property | 18 | ID validation, truncate, circuit breaker |
| Lib unit | 31 | Compose, identity, sampling, history helpers |

### 9.3 Golden IPC

`gen/schemas/localpersona-ipc-schemas.json` + `test_ipc_schema_has_no_drift`.

---

## 10. Documentation map (consolidated)

Use this as the **index of truth**:

| Document | Role | Freshness |
|----------|------|-----------|
| **This file** (`ARCHITECTURE_AND_STATUS.md`) | **Canonical blueprint + status dump** | 2026-07-25 |
| [README.md](../../README.md) | Product front door | Updated packaging |
| [AGENTS.md](../../AGENTS.md) | Agent briefing | Sync partial — prefer this dump for RC state |
| [CHANGELOG.md](../../CHANGELOG.md) | Release history | Current |
| [USER_MANUAL.md](../../assets/USER_MANUAL.md) | End-user (shipped) | RC limitations section |
| [docs/packaging/PACKAGING.md](../packaging/PACKAGING.md) | How to build packages | Current |
| [docs/packaging/INSTALL.md](../packaging/INSTALL.md) | How to install | Current |
| [RC_ACCEPTANCE_CHECKLIST.md](./RC_ACCEPTANCE_CHECKLIST.md) | Human gate for beta | Code boxes checked; soak open |
| [RELEASE_NOTES_0.9.0-rc.1.md](./RELEASE_NOTES_0.9.0-rc.1.md) | RC marketing notes | Current |
| [ARCHITECTURE.md](./ARCHITECTURE.md) | Shorter architecture map | Points here for full dump |
| [AUDIT_v7.2_…](./AUDIT_v7.2_LOCALPERSONA_RC_READINESS.md) | Historical forensic audit | **Historical** + appendix; scores may lag |
| [REMEDIATION_WORKPLAN_2026.md](./REMEDIATION_WORKPLAN_2026.md) | Feature remediation log | Historical / partial |
| [PHASE0_*.md](./PHASE0_STABILIZATION.md) | Stabilization checkpoints | Historical |
| [FUTURE_VISION.md](./FUTURE_VISION.md) | AR/social long-term | Non-blocking |
| [VOICE_SETUP.md](../VOICE_SETUP.md) | Experimental TTS | Dev |
| [ROADMAP.md](../../ROADMAP.md) | Phased product roadmap | High level |

**Rule for agents:** Prefer **this blueprint** and **AGENTS.md** over outdated maturity claims in AUDIT sections written before the identity fix and packaging wave.

---

## 11. Critical data flows

### 11.1 Send message (happy path)

```
selectCharacter → getOrCreateConversation (UUID)
sendMessage → {
  conversation_id: UUID,
  character_id,
  speaker_id: "user",
  temperature, max_tokens, top_p,
  images?
}
→ resolve character → budget history → compose + RAG
→ HTTP stream:false → reject empty
→ append user + assistant (assistant named as character)
→ Arena increment
```

### 11.2 Failure containment

| Failure | Behavior |
|---------|----------|
| Worker dead | Health check → error string → toast; no append |
| Truncated JSON | Parse error → no append |
| Empty content | Explicit error → no append |
| Bad NDJSON line | Skip + warn on load |
| RAG fail | Log + diagnostics; chat continues |
| Arena | Background stop/start; reason stored |

---

## 12. Known limitations (RC)

1. Token streaming disabled (complete responses only).  
2. Message editing not available.  
3. Voice/TTS experimental.  
4. AppImage may not build on paths with spaces / broken linuxdeploy — use **deb**.  
5. Frontend is one large untyped JS file (contract risk mitigated by Tribunal + tests).  
6. RAG cold-start can delay first knowledge query (embedding model init).  
7. Human soak + full Q&A matrix still required before “stable beta declared.”  
8. CDN Tailwind/fonts require network for full UI styling unless offline-cached by WebView.  

---

## 13. Recommended next steps

| Priority | Action |
|----------|--------|
| P0 human | Install `dist/...deb`, run [RC_ACCEPTANCE_CHECKLIST](./RC_ACCEPTANCE_CHECKLIST.md) matrix |
| P0 human | 2–4h soak; confirm no ghost llama-server after quit |
| P1 | Restore full git remote/history if needed; tag `v0.9.0-rc.1` |
| P2 post-RC | Real streaming with abort + Tribunal |
| P2 post-RC | Message edit on NDJSON |
| P3 | AppImage on space-free path or fixed linuxdeploy |
| Later | Optional bundled llama-server variants; AR vision |

### Explicit anti-roadmap until D exits

- AR / geofencing / social  
- Frontend framework rewrite  
- SQLite migration without strong need  
- Shipping multi-GB models inside the installer  

---

## 14. Quick reference commands

```bash
# Dev
cargo tauri dev

# Quality
./scripts/tribunal.sh

# Package (deb + dist kit)
./scripts/package-release.sh

# Install local build
sudo dpkg -i dist/0.9.0-rc.1/LocalPersona_0.9.0-rc.1_amd64.deb
```

---

## 15. One-paragraph summary

LocalPersona `0.9.0-rc.1` is a Tauri 2 + Rust + vanilla JS desktop app that manages dual llama-server children, stores all creative state as user-owned files (append-only NDJSON conversations + character JSON + knowledge chunks), enforces a real identity/Q&A contract for persona fidelity, and ships a **~14 MB Debian package without models**. Reliability and packaging foundations are commercial-grade for closed beta; remaining risk is human validation, AppImage tooling, and post-RC product surfaces (streaming, edit), not missing core architecture.

---

*End of architectural blueprint and status dump. Update this file when version, SoT, or packaging posture changes.*

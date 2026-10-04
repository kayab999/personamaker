# LocalPersona — Agent Context & Development Guide

This file serves as the canonical, high-signal briefing for any AI coding agent or developer working on LocalPersona.

---

## Quick Context (for pasting into new conversations)

```
Project: LocalPersona 0.9.0-rc.1
Type: Tauri 2 desktop app (Rust + vanilla JS) for local GGUF personas via llama-server.

CANONICAL DOCS
• Full blueprint + status: docs/handbook/ARCHITECTURE_AND_STATUS.md
• Doc index: docs/README.md

Philosophy: local file ownership; depth over "AI girlfriend" apps; offline; no telemetry.

Architecture (current truth)
• Persistence: conversations/{uuid}/metadata.json + messages.ndjson (append-only). Legacy chat_histories deprecated for interactive UI.
• Identity: conversation UUID + character_id for compose/RAG — never speaker_id "user" as character file key.
• Dual servers: LlamaServerManager + VoiceServerManager; ServerState; Arena 300 req + 45 min; PID/Drop hygiene.
• Inference path: shared helpers (compose_system_with_rag, post_chat_completion); stream:false for RC.
• RAG: fastembed + cosine; knowledge under characters/{id}/knowledge/.
• Frontend: single script.js messenger UI; bounded load + context pill; edit hidden; stream toggle frozen.

Packaging
• Installer resources: personas + USER_MANUAL + default avatars only — NO GGUF.
• Linux .deb ~14MB in dist/0.9.0-rc.1/; package via scripts/package-release.sh.
• Brand icons in icons/; master in docs/branding/.

Governance: ./scripts/tribunal.sh (check + destructive + property) + CI Tribunal v12 (11 jobs incl. audit/deny/coverage/frontend) required on main.

Status: closed-beta RC. No numeric maturity scores by policy — readiness is
measured, not asserted (see table below). Human soak/matrix still open.
Post-RC: streaming, edit, AR vision stays frozen.

| Criterion (2026-10-04) | State |
|---|---|
| Tribunal CI verdict | GREEN (all jobs incl. coverage floor 30%, audit, deny) |
| Test matrix | 140+ passing, 0 failing (Rust) + 6 frontend smoke |
| Production unwrap() | 0 (ratchet-gated) |
| Open audit findings | High: 0 · Med: E2E-browser deferred · Low: tracked in sweep docs |
| Known unmitigated vulns | 3 accepted-risk ignores in audit.toml (no upstream fix) |
| Human soak / acceptance matrix | OPEN |

Do not: add test-models to bundle.resources; dual-write chat history; reintroduce speaker_id=user as character lookup.
```

---

## Full Project Briefing

### Core Vision
A universal, one-click desktop application for running rich AI personas/characters powered exclusively by local GGUF models. The product must feel like a finished creative tool, not a developer frontend for llama.cpp.

### Non-Negotiable Principles
1. **Local ownership & data sovereignty** — Every character, conversation, voice sample, and knowledge document is a real file the user fully controls.
2. **Depth over virality** — Prioritize consistency, interesting simulated personalities, and creative utility. Actively avoid the app becoming "AI Tinder" or being dominated by low-effort erotic/romance roleplay.
3. **Creative tool aesthetic** — The experience should feel closer to a writing studio or a private simulated social space than a typical chatbot interface.
4. **No cloud lock-in** — Offline by design. No telemetry, no accounts, no forced updates that break local workflows. (One documented exception: first RAG use downloads the fastembed embedding model ~90 MB, one-time.)

### Current Architecture (2026)

**Persistence (High Performance)**
- Conversations use an append-only design:
  - `conversations/{id}/metadata.json`
  - `conversations/{id}/messages.ndjson`
- Writes are O(1). Full rewrites are avoided.
- Frontend uses incremental DOM updates + lazy/paginated loading for long chats.

**Dual Server Model**
- Main `llama-server` (llama.cpp) for LLM inference (OpenAI-compatible API).
- Separate `VoiceServerManager` for TTS (currently Qwen3-TTS experiments).
- Voice and LLM servers are independently managed.

**Knowledge Base / RAG**
- Per-character knowledge lives at: `characters/{id}/knowledge/`
- Documents (PDF/TXT/MD) are uploaded, stored, and automatically chunked.
- Chunking logic lives in `src/storage.rs`.
- Goal: Dramatically higher fidelity for historical, literary, or research-based personas.

**UI Direction (Messenger Simulation)**
- Sidebar = contacts/chat list (not just a character picker).
- Last-message previews, timestamps, and simulated presence.
- Language shifted: "New Character" → "Add Contact", "Edit Contact".
- See `frontend/script.js` around lines 399–464 for current messenger-style rendering.

**Storage Layout (App Data Directory)**
```
characters/
  {id}.json
  {id}/
    avatar.{ext}
    voice/
      samples/
    knowledge/
      {doc}.pdf
      {doc}.txt
      ...
conversations/
  {conv_id}/
    metadata.json
    messages.ndjson
images/
voice_samples/ (legacy location)
```

### Key Source Files

| Area                    | File(s)                              | Notes |
|-------------------------|--------------------------------------|-------|
| Rust entry + commands   | `src/main.rs`, `src/commands.rs`     | Tauri command handlers |
| Append-only conversations | `src/conversation.rs`              | `append_message()`, ndjson reader/writer |
| File storage + RAG      | `src/storage.rs`                     | Characters, avatars, documents, chunking |
| Inference (LLM + Voice) | `src/inference.rs`                   | `VoiceServerManager`, server lifecycle |
| Data models             | `src/models.rs`                      | `StoredCharacter`, etc. |
| Frontend (single file)  | `frontend/script.js`                 | ~3900 LOC vanilla JS UI (no-split policy; tested via frontend/tests/smoke) |
| Styling                 | `frontend/style.css`                 | Tailwind via CDN |

### Development Commands

```bash
# Run in development
cargo tauri dev

# Production build
cargo tauri build

# Just Rust check
cargo check
```

### Related Documentation (Read These)

- **[docs/handbook/ARCHITECTURE_AND_STATUS.md](docs/handbook/ARCHITECTURE_AND_STATUS.md)** — **Canonical architecture blueprint + status dump**
- [docs/README.md](docs/README.md) — Documentation index
- [README.md](README.md) — Product overview
- [docs/handbook/FUTURE_VISION.md](docs/handbook/FUTURE_VISION.md) — Long-term AR/social concept (out of RC scope)
- [assets/USER_MANUAL.md](assets/USER_MANUAL.md) — End-user documentation (shipped)
- [docs/VOICE_SETUP.md](docs/VOICE_SETUP.md) — Experimental voice/TTS notes
- [docs/packaging/PACKAGING.md](docs/packaging/PACKAGING.md) — Building installers

### Current Technical Priorities (as of latest context)

From the latest v7.2-style re-audit + long-context slice, the highest-leverage remaining items are:

1. **Long-context maturity** (biggest remaining usability gap) — Token-budget loading is implemented and wired (`load_messages_within_token_budget`, 8192 default). Next: read real model `context_length` from GGUF metadata, smarter truncation (keep system + recent turns), basic summarization for very long histories, and UI exposure of current usage.
2. **Full Blast Radius validation** — Complete the hardest remaining chaos tests (true partial/truncated HTTP responses during generation + clean recovery + conversation integrity after mid-request worker SIGKILL).
3. **Richer Diagnostics & Observability** — Arena history/reasons per manager, last reset timestamp, per-model GGUF context length when loaded, live context usage.
4. **Tribunal & Governance hardening** — Move closer to the original containerized forensic vision + automatic schema generation gate.
5. **Sovereign Scaling validation** — Automated long-session / high-load tests that exercise Arena Reset and memory behavior under realistic conditions.

Secondary priorities:
- Polish the GGUF experience (auto-suggest context size / ngl / etc. from parsed metadata).
- Continue expanding chaos coverage and error resilience.
- User-facing reliability signals (e.g., "Server restarted due to uptime 14 min ago").

### Philosophy Guardrails (Important)

When suggesting features or UI changes, evaluate against these:
- Does this increase creative depth and consistency, or does it make shallow interaction easier?
- Does this reinforce user ownership (files they can see, edit, back up)?
- Would this feature make the app feel more like "a private list of simulated interesting people" vs "yet another AI girlfriend app"?
- If a feature could be abused for low-effort romance/NSFW spam, is there a way to implement it that still favors thoughtful use?

It is acceptable (and sometimes necessary) to say "this direction risks pulling us toward the shallow end of the pool — here's an alternative that preserves depth."

---

## Working With This Codebase

- The frontend is deliberately a single large `script.js` file. Keep it that way unless a bundler migration is explicitly planned.
- Prefer file-based solutions over adding any database (SQLite, etc.) unless there is a very strong performance argument.
- When modifying conversation logic, always go through the append-only API in `conversation.rs` rather than rewriting history files.
- New knowledge/RAG features should extend the existing chunking + storage primitives in `storage.rs`.
- Voice work should stay isolated in the `VoiceServerManager` — do not couple it tightly to the main LLM server lifecycle.

This document should be updated whenever major architectural shifts occur (new persistence model, major UI paradigm change, new server type, etc.).

---

## Post-Audit Stability Work (May 2026)

In response to a technical audit, the following critical reliability fixes were implemented:

1. **Process hygiene** — Added proper `Drop` impl for `VoiceServerManager` (using `start_kill()`) + hardened the existing `LlamaServerManager` Drop. Prevents orphan `llama-server` TTS processes on abrupt shutdown.

2. **Atomic persistence** — Introduced `atomic_write()` (tempfile + persist/rename) in `storage.rs`. Applied to:
   - Character JSON (`save_character`)
   - Chat history JSON
   - Conversation `metadata.json` (on every message)
   - Inference config + full character export
   This eliminates the main window for data corruption on crash/power loss.

3. **Error visibility** — `callTauri()` in the frontend now throws instead of returning `null` and shows a non-intrusive toast (`showToast`). Previously silent IPC failures are now visible to the user.

4. **Readiness detection** — Replaced fixed `sleep()` in both server `start()` paths with active HTTP polling against `/v1/models` (`wait_for_server_ready`). Dramatically reduces "server started but not accepting requests yet" races.

5. **Logging hygiene** — Replaced silent `.ok()` / `let _ =` patterns in `main.rs` setup and menu handling with proper `log::warn!` so failures during first-run default persona loading are at least observable.

**RAG compilation issues (resolved in later session):**
- All RAG-related compilation errors have been fixed:
  - Added missing `embedding: None` to all `KnowledgeChunk` struct literals.
  - Added `once_cell = "1"` dependency and fixed duplicate import + mutability issue in `rag.rs` (wrapped model in `Mutex` because `embed()` requires `&mut self`).
  - Declared `mod rag;` in `main.rs` (the binary root) to resolve module visibility alongside `lib.rs`.
- The project now passes `cargo check` cleanly (only benign dead-code warnings remain).

**Atomic write status (Post-Fase 3):**
All critical persistence paths now use atomic writes (`atomic_write` for JSON, `atomic_write_bytes` for binaries):
- Characters, conversation metadata, chat histories, chunks.json, inference config, exports
- Avatars, voice samples, knowledge documents
- `mark_character_has_knowledge_base`, empty ndjson initialization

Remaining direct writes are only inside the atomic helpers themselves or low-risk first-run default persona seeding.

**Security Hardening (Fase 1 - Auditoría Integral v3.0):**
- RCE via `open_external_url` eliminated: now uses the safe `open` crate + strict http/https validation only.
- Path Traversal in `get_absolute_path` hardened: strips `..` components + final `starts_with(base)` check.
- Mutex contention: `send_message_with_images` now extracts server info and drops the `SharedLlamaServer` lock before performing HTTP I/O.
- Metadata race condition: added `fs2` exclusive file locking around read-modify of `metadata.json` during append (combined with existing atomic writes).
- Full sweep confirmed no other user-controlled raw `std::process::Command` usage.

**Completion of Full Audit Plan (Fases 1 + 2 + 3 scoped) - May 2026**

---

## Earlier: Phase 0 Stabilization (July 2025, predates the May 2026 work above)

In the most recent stabilization pass, the following high-impact reliability and correctness issues were addressed:

### 1. Mutex Contention / UI Freeze during Server Start
- `LlamaServerManager::start()` (and Voice equivalent) now returns almost immediately after spawning the child process.
- Added `starting: bool` field to `ServerStatus`.
- The long `wait_for_server_ready()` call was moved to a background task.
- Result: `get_inference_status` and other commands no longer block while the server is warming up.

### 2. Orphan Process / Ghosting (PID File Mechanism)
- Implemented PID file tracking (`localpersona-llama-server-{port}.pid` in temp dir).
- PID is written immediately after successful spawn.
- Stale PID detection on next `start()` attempt (best-effort cleanup).
- PID files are cleaned on normal `stop()`.
- Significantly reduces risk of "port already in use" after crashes or forced kills.

### 3. Concurrent Write Corruption (Data Loss Prevention)
- Added `atomic_write_with_lock()` using `fs2::FileExt::lock_exclusive()`.
- Main `atomic_write()` now uses the locked variant.
- Exclusive lock added around the append to `messages.ndjson` in `append_message()`.
- All critical user data paths (characters, conversations, knowledge, voice samples, exports) are now protected against simultaneous writes.

### 4. Additional Resilience Improvements
- `fsync` / `sync_all()` added after message appends and several other hot write paths (voice samples, knowledge documents, chunks).
- `load_all_messages()` now gracefully skips corrupted/truncated JSON lines (with warnings) instead of failing the entire conversation load.
- `repair_conversation()` command now repairs **both** `messages.ndjson` **and** `metadata.json`, creates backups, and rebuilds metadata when needed.
- Schema `version` field added to `ConversationMetadata` and `StoredCharacter`.
- Basic version checking + logging added on load paths (migration detection foundation).

These changes directly address the most severe "Critical" items from the latest technical audit.

---

**Current Recommended Focus (Post-Phase 0)**
- Expand test coverage aggressively (target: 30+ meaningful tests on core modules).
- Implement proper conversation context management (token accounting → budget system → summarization).
- Complete the remaining legacy chat history cutover (`editMessage`, full removal of direct `callLocalLLM` paths).
- Wire actual semantic embeddings for RAG.
- Backend already had `load_messages_paginated`; main UI paths encouraged to use limited loads. Full history loads are now mostly avoided for rendering.
- Direct LLM fetch path (`callLocalLLM`) is now legacy for new messages.

**Fase 3 (Roadmap - Scoped Execution)**
- RAG improved with relevance filtering and limits (practical stepping stone before full vector DB like HNSW/FAISS).
- Global Mutex usage audited and documented in `inference.rs` with clear future direction (actor model or split read/write locks).
- Path validation hardened further; basic input sanitization comments added across storage and commands.

**Current State**
- `cargo check` passes cleanly.
- All critical security, reliability, and pipeline unification items from the audits are addressed.
- Remaining work is evolutionary (full vector index, actor refactor, comprehensive tests) rather than blocking.

**Legacy Flow Polish (Latest Session)**
- Added `regenerate_last_message` Tauri command.
- Regeneration (`regenerateLastResponse` and `regenerateFrom`) now goes through the full Rust pipeline (RAG + character prompts).
- `editMessage` still uses legacy path (documented with TODO).
- Direct `callLocalLLM` usage has been significantly reduced.

The project is now in a significantly stronger position for limited beta / public testing.

**Next recommended steps after this batch:**
- Consider adding a lightweight per-conversation `Mutex` for the ndjson append path (lower priority now that metadata is atomic).
- Gradually migrate remaining `alert()` calls to the new `showToast()`.

**Documentation Status (2026-10-04):** Canonical dump is **`docs/handbook/ARCHITECTURE_AND_STATUS.md`**. Index: **`docs/README.md`**. Older AUDIT/remediation docs are historical. Packaging: Linux deb without models. No maturity scores — see the criteria table in the Quick Context block.

*Last synced: 2026-10-04 (full sweep: PDF isolation, dep trims, coverage/frontend gates, deny+SBOM, archetype personas, AppImage decision)*

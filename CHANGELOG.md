# Changelog

All notable changes to LocalPersona are documented in this file.

## [0.9.0-rc.1] — 2026-07-25

Release Candidate for closed beta. Ground-truth re-audit fixed product-breaking Q&A identity bugs and aligned governance gates.

### Commercial packaging (P0–P5 prep)
- Installer resources: **no GGUF / no test-models** — only personas, avatars, USER_MANUAL
- Linux bundle targets: **AppImage + deb** only (deb verified ~14 MB in `dist/`)
- Release profile: LTO thin, strip, opt-level 3
- `devtools` optional feature (not default)
- Brand: existing `icons/` neural-profile set; masters archived under `docs/branding/`
- `scripts/package-release.sh`, `docs/packaging/{PACKAGING,INSTALL}.md`, root `LICENSE`
- About modal: commercial product blurb (models not included, MIT, RC limits)
- Documentation consolidated: `docs/README.md` index + **`docs/handbook/ARCHITECTURE_AND_STATUS.md`** full blueprint

### Architecture freeze (stable RC phases A–C)
- Interactive chat is **NDJSON-only** (`conversations/{uuid}/`); legacy `chat_histories/` deprecated for messaging
- Shared inference pipeline for send + regenerate (`compose_system_with_rag`, `post_chat_completion`)
- Diagnostics: last Arena reset reason/time, last RAG status, orphan conversation hints
- Memory-monitor Arena reset records reason `memory_pressure`

### Critical fixes
- **Identity contract**: send/regenerate use conversation UUID + `character_id`; never treat role label `"user"` as a character file id
- Frontend no longer passes character id as `conversation_id` (prevents split-brain chat history)
- Character personality / scenario / writing instructions actually drive inference again

### Q&A fidelity
- Interaction **modes** (adventure, story, …) included in composed system prompts
- **Temperature / max_tokens / top_p** from Settings wired into llama-server requests (clamped)
- Chat history includes speaker names for named assistants
- RAG query uses recent user turns; chunks include **source filename** attribution
- Empty model responses are rejected and not persisted
- Assistant messages store the character’s display name

### Long-context & UX
- Accurate `has_more` from conversation `message_count` vs loaded window
- Context usage pill in the chat header
- Bounded message load + “load earlier” remain the default UI path
- Message **edit** UI removed for this RC (copy remains)
- Token **streaming frozen** (non-stream is the stable path); toggle disabled with honest copy
- Voice / TTS labeled **Experimental**

### Reliability & governance
- Tribunal: `cargo check` + destructive tests + property tests
- Arena Reset: 300 requests + 45 minutes uptime (unchanged, already live)
- ID validators restricted to **ASCII** path-safe characters
- CI clippy policy: correctness/suspicious deny (not total `-D warnings`)
- Destructive battery expanded (identity, sampling clamp, empty parse path)

### Known limitations
- No token streaming in UI
- No in-place message edit
- Voice experimental
- Manual soak + full Q&A matrix required before GA (`docs/handbook/RC_ACCEPTANCE_CHECKLIST.md`)

### Docs
- README, USER_MANUAL, AGENTS.md, RC acceptance checklist updated for honesty vs prior “9.2–9.3” claims

## [0.1.0] — earlier

Initial professional-hardening baseline (atomic writes, dual Arena scaffolding, Tribunal beginnings, GGUF parser foundations).

# Changelog

All notable changes to LocalPersona are documented in this file.

## [Unreleased] — Phase 0 + Phase 1 audit fixes (2026-11-04)

### Fixed — release blockers
- **`app.withGlobalTauri` was never set.** Tauri therefore never injected
  `window.__TAURI__`, `isTauri()` returned false, and *every* `callTauri()` in
  the frontend was a silent no-op: the packaged app could not load characters or
  chat. The flag is now set in `tauri.conf.json` and asserted by
  `tests/ipc_contract.rs` plus a dedicated CI job.
- **`load_messages_for_display` was never registered** in `generate_handler!`
  even though the frontend calls it at three sites — the chat view could never
  load messages. It is now registered, and the `#[allow(dead_code)]` that hid
  the compiler warning is gone. `tests/ipc_contract.rs` enforces parity in both
  directions (every frontend call is registered; every `#[command]` is
  registered).

### Fixed — correctness
- **PDF extraction pipe deadlock.** The worker's stdout was piped but never
  drained, so any extraction larger than the OS pipe buffer (~64 KiB) blocked
  the child until the 30 s timeout and was misreported as a "hostile file".
  Both pipes are now drained concurrently on dedicated threads
  (`drain_capped`); oversize output is rejected as a bound violation, not a
  timeout. Regression suite: `tests/pdf_pipe_regression.rs`.
- **Memory monitor watched the wrong process.** It sampled
  `std::process::id()` (the app shell) instead of the `llama-server` child, so
  the preemptive Arena Reset it exists to provide could never fire for the
  process that actually consumes gigabytes. The monitor now follows the server
  PID and resets its slope baseline when the target changes.
- **`validate_localhost_endpoint` prefix-match bypass.** `http://localhost:1234@evil.com`
  passed a `starts_with` check while a URL parser reads `evil.com` as the host.
  The check now parses scheme/host/port strictly, rejects userinfo, and
  requires an exact loopback host plus a numeric port.
- **Production `expect()` panics removed** from the stream finalizer, the
  NDJSON append path, and app start. `scripts/count_unwraps.py` now counts
  `.expect(` as well, so the "0 production unwrap()" gate can no longer report
  green while panic-on-error calls remain.
- **`has_knowledge_base` was sticky** and there was no way to remove or reindex
  a document — a failed ingest could only be fixed by deleting the character.
  Added `list_character_documents`, `delete_character_document` and
  `reindex_character_knowledge`; the flag is now recomputed from what is
  actually indexed on disk.
- `circuit_breaker.rs` no longer claims SHA-256 hashing (it uses
  `DefaultHasher`; documented as an in-process dedup token only).

### Fixed — supply chain (post-push CI)

- **The `Supply-Chain Audit` job failed on `main`** because seven advisories
  were published against the TLS stack after the last green run (2026-10-04):
  `rustls` 0.23.45 (RUSTSEC-2026-0346/0347/0348/0349), `rustls-webpki`
  0.103.15 (RUSTSEC-2026-0382), `webpki-roots` / `webpki-root-certs` 1.0.7
  (RUSTSEC-2026-0381/0383). All were fixed by a lockfile refresh:
  `rustls` → 0.23.46, `rustls-webpki` → 0.103.16, `webpki-roots` /
  `webpki-root-certs` → 1.0.11, plus `tokio` 1.52.3 → 1.54.1,
  `anyhow` 1.0.102 → 1.0.104 and the yanked `der` 0.8.0 → 0.8.3.
- **The audit config was never being read.** `cargo audit` auto-loads
  `.cargo/audit.toml`, not a root-level `audit.toml`, so the documented
  accepted risks had no effect and CI only passed because the same IDs were
  repeated as `--ignore` flags — two lists that could silently drift. The file
  moved to `.cargo/audit.toml`, CI now runs a bare `cargo audit`, and
  `tests/destructive_tests.rs::test_audit_config_is_in_the_path_cargo_audit_reads`
  fails if a root `audit.toml` reappears or if CI re-duplicates the ignores.

### Added
- `tests/ipc_contract.rs` (5 tests): frontend↔backend command parity, Tauri
  global-API flag, bundled-frontend presence, and a guard against
  `#[allow(dead_code)]` in `commands.rs`.
- `tests/pdf_pipe_regression.rs` (2 tests): 256 KiB extraction completes;
  40 MiB extraction fails fast with a bounds error instead of hanging.
- Tribunal step `[4/4]` + CI job `IPC Contract`; verdict now requires it.

### Removed
- Outdated docs: `docs/handbook/CONTEXT_DUMP.txt` (2026-07 status dump),
  `docs/handbook/SUNRISE_REVIEW.md`, `docs/handbook/OVERNIGHT_RESULTS.md`, and
  `docs/LocalPersona 0 9 0 rc.md` (superseded audit plan).
- `scripts/mock_hits.jsonl` is no longer tracked (runtime mock log; now
  gitignored).
- `frontend/generated/ipc-contract.json` removed: it was a stale (May 2026)
  duplicate of `gen/schemas/localpersona-ipc-schemas.json`, missing
  `StreamEvent` and `EditRecord`, and no code read it. The golden file in
  `gen/schemas/` is the single contract snapshot enforced by
  `tests/destructive_tests.rs::test_ipc_schema_has_no_drift`.

### Notes
- Full suite green: 62 lib tests + 11 integration binaries; frontend smoke 6/6;
  `./scripts/tribunal.sh` green; `scripts/count_unwraps.py` = 0.
- Docs refreshed (README, ROADMAP, AGENTS criteria table, ARCHITECTURE status,
  HANDOVER) to stop under-claiming streaming and to record the audit.

## [Unreleased] — Phase A + B1 (2026-10-30)

### Added
- **Real token streaming (Phase B1)**: `stream_message_with_images` + `abort_inference_stream`
  commands; `llm-stream` Tauri events (`chunk` / `stalled` / `final`) correlated by
  requestId; explicit lifecycle `PENDING → STREAMING → COMPLETED|FAILED|ABORTED`;
  single-owner exactly-once finalization (user + assistant appended in one batch
  at stream end; `ChatMessage.incomplete` flags persisted partial turns;
  zero-content failures persist nothing). Send button doubles as Stop while a stream runs.
- Fuzz corpus seeds (13) for `gguf_parse` / `ndjson_parse` / `pdf_ingest` +
  rot guard suite `tests/fuzz_corpus_seeds.rs` + runbook `fuzz/README.md`
  (run: `cargo +nightly fuzz run <target> fuzz/corpus/<target> -- -max_total_time=60`,
  requires nightly because libFuzzer builds with `-Z` flags).
- GGUF hostile-header sanity caps (kv_count / key length / string length / array
  length) — a corrupt header can no longer drive OOM allocations or ~2^64-iteration loops.

### Fixed
- Documentation drift: AGENTS.md now matches verified code (test counts, CI job
  count, GGUF parser location, models.rs stub status, GGUF context-length already read).
- `fuzz/Cargo.toml` was missing the `serde_json` dependency — the `ndjson_parse`
  fuzz target could never build (latent since the sweep).
- Streaming mock modes (`stream_*`) added to `scripts/mock_llama_server.py` for
  deterministic Tribunal coverage; mock argparse choices now include them.

### Notes
- 7 new streaming chaos tests (normal completion, mid-stream kill, malformed
  frames, error frames, empty stream, cancel-during-slow, concurrent isolation) +
  15 in-crate streaming unit tests. Full suite: 131 Rust test functions (`cargo test`
  green), 6 frontend smoke tests.
- Human RC acceptance / soak checklist rows remain OPEN (unchanged by this work).

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

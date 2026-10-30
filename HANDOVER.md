# LocalPersona — Agent Handover

> Durable, agent-consumable progress record. Update at every coherent
> checkpoint (code commit → this file). Kept deliberately concise; deep
> truth remains in `docs/handbook/ARCHITECTURE_AND_STATUS.md`.

## Current state

| Field | Value |
|---|---|
| Version | `0.9.0-rc.1` (tag exists at `27be170` = HEAD of origin/main) |
| Branch | `main`, clean, in sync with `origin/main` before mission start |
| CI | Tribunal v12 GREEN (last CI run 2026-10-04; 13 jobs) |
| Soak / acceptance | OPEN — human-only (`docs/handbook/RC_ACCEPTANCE_CHECKLIST.md`) |

## Mission plan (2026-10-30 directive)

Dependency order: **A** (RC closure + hygiene) → **B** (streaming + edits) →
**D1/D2** (SillyTavern card import/export) → **C** (deep memory) → **E**
(prefill / structured output / Kokoro TTS) → **F** (observability, ongoing).

## Phase A — RC closure & hygiene — DONE (2026-10-30)

- **A2 verified:** `v0.9.0-rc.1` + `audit-baseline-0.9.0-rc.1` tags both
  point at `27be170` (current HEAD). Nothing to create; no pushes made.
- **A3 docs synchronized:** AGENTS.md now matches verified code (Rust test
  fn count, 6 frontend smoke tests, 13 CI jobs, GGUF parsing = `src/gguf.rs`,
  `models.rs` noted as stub, models.rs claim corrected, tailwind-vendor CSP
  note, GGUF context-length read marked done).
- **A4 fuzz corpus:** 13 meaningful seeds under `fuzz/corpus/{gguf_parse,
  ndjson_parse,pdf_ingest}/`; runbook `fuzz/README.md` (needs
  `cargo +nightly fuzz run`); rot-guard suite `tests/fuzz_corpus_seeds.rs`
  calls the same parse functions as the targets and pins each seed's
  outcome. **Bounded run evidence:** gguf_parse 213k runs/46s no crash;
  ndjson_parse fixed (missing `serde_json` dep in fuzz/Cargo.toml — latent
  since the sweep) then rerun clean: 1.9M runs/46s no crash; pdf unchanged.
- **Fuzz-driven hardening found + fixed:** `src/gguf.rs` consumed unbounded
  `u64` length/count fields → added `MAX_KV_COUNT` (10k), `MAX_KEY_LEN`
  (4096), `MAX_STRING_LEN` (8 MB), `MAX_ARRAY_LEN` (1M) sanity caps;
  regression test `gguf_hostile_headers_fail_fast` pins the two-tier
  contract (header-level claims reject the file; entry-level claims degrade
  to skip-and-continue → `Ok` with all-None metadata).
- **A1 remains HUMAN:** checklist rows 1–9 + soak section stay unchecked
  until actually performed; no automation can discharge this.

## Phase B1 — real token streaming — DONE (2026-10-30)

Architecture (backend first; no competing pipeline):
- `ChatMessage.incomplete` (serde default false): marks persisted partial
  stream turns (ABORTED/FAILED); complete turns never set it; old NDJSON
  still loads.
- Lifecycle `PENDING → STREAMING → COMPLETED|FAILED|ABORTED`; terminal
  exactly-once finalize in the single owning spawned task
  (`stream_message_with_images` in src/commands.rs).
- Transport `post_chat_completion_stream`: one acquisition attempt (no
  silent mid-stream retry), no-total-timeout client + 120s idle-chunk cap +
  cancel flag polled every 150ms inside `tokio::select!`. SSE parsing
  isolated in pure `parse_stream_chunk` (unit-tested: delta, finish frame,
  `[DONE]`, error frames, malformed, role-only, stats-only).
- Testability seam: `consume_llm_sse_stream(response, id, cancel, emit)` is
  generic over an emitter closure (production: Tauri `llm-stream` events;
  tests: mpsc recorder). 7 mock-driven chaos tests in `tests/command_layer.rs`
  (b1_*) incl. cancel-during-slow + concurrent-stream isolation; 15 in-crate
  unit tests (`streaming_tests`) incl. pure policy fn `stream_persistence_decision`.
- Persistence policy (pinned by tests): completed → batch user+assistant;
  failed/aborted WITH content → same batch but `incomplete: true`; zero
  content → append NOTHING (fail closed; no empty assistant rows).
- IPC: `get_ipc_contract` extended additively (ChatMessage.incomplete +
  StreamEvent); golden `gen/schemas/localpersona-ipc-schemas.json` updated
  via the drift test's own dump flow (reviewed diff: additive only).
- Mock: `stream_die_mid|stream_malformed|stream_error_frame|stream_slow|
  stream_empty` modes + argparse choices (missing until now → mock died at
  startup; caught by the new tests).
- Frontend (script.js/index.html): default `stream:true`; ONE `llm-stream`
  listener at init; composer cleared only at finalize (user text kept when
  nothing persisted); send button = Stop while streaming
  (`abort_inference_stream`); toggle real + persisted; interrupted turns
  show "(interrupted)" via `msg.incomplete`. Smoke 6/6.
- Decisions: streams skip `capture::capture` (same as RC); regenerate stays
  non-stream; SSE mock finish frame = real llama.cpp shape (delta {}) +
  [DONE].

Verified: `./scripts/tribunal.sh` green; full `cargo test` green (131 fns:
lib 53×2, command_layer 14, destructive 31, fuzz_corpus_seeds 12,
ctx_budget 8, property 18, pdf_isolation 5, roster 2, stress 3); frontend
smoke 6/6; bounded fuzz above.

## Phase B2 — message editing (NEXT)

Prefer non-destructive revisions over rewriting history — inspect
`src/conversation.rs` `rewrite_conversation_messages` (~line 758) and
`truncate_after_last_user` BEFORE implementing. Requirements: preserve the
original, explicit revisions with provenance + ordering, unambiguous active
version, deterministic reconstruction, crash-safe recovery. Tests: first
edit, multiple edits, regen-after-edit, interrupted write, ordering,
restart. Frontend currently hides edit actions
(`attachMessageActionListeners`).

## Rules for the next agent

- Local commits only; **never push** without explicit authorization.
- Tribunal gates before every commit (`scripts/tribunal.sh`); pre-commit
  hook enforces.
- Do not weaken tests; update docs + `HANDOVER.md` with each phase.
- Anti-roadmap stands: no SQLite/vector DB, no frontend rewrite, no bundled
  models, no AR, no premature MCP/agent-framework work (post-1.0).

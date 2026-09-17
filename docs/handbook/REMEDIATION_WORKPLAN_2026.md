# LocalPersona — Comprehensive Remediation Workplan (v8)
## Closing the Q&A, UI/UX, Performance, Stability & Adversarial Gaps

**Based on:** Full 2026 Audit (Q&A • UI/UX • Performance • Stability • Adversarial)  
**Date:** 2026  
**Governance:** All code changes MUST pass `./scripts/tribunal.sh` (cargo check + full destructive test battery) before being considered complete. No exceptions.  
**Philosophy Alignment:** Maximize creative depth and character fidelity. Preserve total user freedom. Maintain Blast Radius = 0 for data integrity and inference state. File-based ownership first.

**Current Baseline Score (post-audit):** ~8.6–8.9 / 10  
**Target after this workplan:** 9.4–9.6 / 10 (very close to professional 10/10 RC readiness)

---

## Guiding Principles for All Work

1. **Tribunal Gate First** — Every significant change (or phase) must be accompanied by tests that exercise the new behavior under the Tribunal. The test precedes the full solution where possible.
2. **Blast Radius = 0** — No change may increase risk of data corruption, conversation poisoning, orphan processes, or silent failures.
3. **Depth Over Convenience** — Every Q&A change must improve character consistency and creative utility rather than making shallow interactions easier.
4. **Incremental & Reversible** — Prefer small, reviewable edits. Keep legacy paths working until the new path is proven and Tribunal-approved.
5. **Documentation Currency** — Update relevant .md files (README, AGENTS.md, AUDIT, USER_MANUAL, this workplan) as we go.
6. **User Freedom** — Do not add judgment or censorship. Users own their characters, knowledge, and prompts.

---

## Audit Findings Summary (Reference)

### Q&A / Conversation Quality
- **Critical:** `build_system_prompt_for_character` ignores rich editor fields (personality, scenario, writing_instructions, your_character, first_message/greeting). Only raw `systemPrompt` is sent.
- RAG injection is functional but basic (no attribution, limited context in query, cold-start embedding model).
- History messages lose speaker_name and structure for prior turns.
- Token budget protects inference but not character fidelity.

### UI & UX
- **High:** Sidebar/contacts experience is good directionally but incomplete (simulated status is clever; real previews work).
- **High:** Full `load_all_messages` still used for chat rendering/switching (script.js `loadConversationMessages` + `updateChatView`).
- **High:** "Stream Responses" setting and frontend streaming scaffolding exist, but backend unified path hardcodes `stream: false`. No real token streaming.
- Edit message flow incomplete (toast placeholder).
- No visibility when token budget trims history.
- Good: Incremental appendMessageToDOM, rich Diagnostics, callTauri + toast error handling, GGUF metadata in model cards.

### Performance
- **High:** UI full-history load on conversation switch/refresh for long chats (thousands of messages possible with token-budget inference).
- No streaming → poor perceived speed and high time-to-first-token.
- estimate_total_tokens is dead code.
- Cold embedding model init on first RAG use.
- Minor dead-code warnings across gguf, circuit_breaker, autopsy, memory_monitor.

### Stability
- **Strong overall.** Dual ServerState, proactive check_health + try_wait, PID + Drop + start_kill, dual Arena (300 + 45min), atomic writes + fs2 + fsync, repair tools, graceful corrupted line handling, 16-17 Tribunal tests.
- Minor: Some setters are dead code; no persistent Arena reset history; Voice server has less telemetry than LLM.

### Adversarial / Robustness
- **Strong.** Path traversal hardened (validate_* + get_absolute_path with ParentDir strip + starts_with). Size limits everywhere (images 5/10MB, docs 100MB, etc.). Localhost validation. No user-controlled process execution.
- Remaining: No hard resource caps on massive simultaneous knowledge bases; missing the two hardest original v7.2 chaos cases (true partial HTTP mid-generation + mid-request worker SIGKILL with proven conversation integrity).
- Prompt injection surface is intentional (user owns their characters).

---

## Phased Execution Plan

### Phase 0 — Foundation & Quick Wins (Low Risk, High Visibility)
Goal: Clean baseline, update governance artifacts, small safe improvements.

**Tasks:**
0.1 Create this workplan document + update AGENTS.md "Current Technical Priorities" and AUDIT_v7.2 "Current State" section (if needed) to reference it.
0.2 Remove or document dead code surfaced in audit (estimate_total_tokens, unused setters in inference.rs, some autopsy/circuit items). Add `#[allow(dead_code)]` with comments where intentional.
0.3 Improve RAG prompt injection formatting: add document titles/attribution when available, include a short "Relevant knowledge:" header.
0.4 Add one small destructive test if any new behavior is introduced.
0.5 Run full Tribunal as gate. Update this workplan with results.

**Acceptance Criteria:**
- Tribunal passes cleanly.
- No behavior change for existing users.
- Docs reference the new workplan.

**Risk / Blast Radius:** 0. Pure cleanup + docs.

**Owner:** Agent + Tribunal

---

### Phase 1 — Q&A Depth Foundation (Highest Philosophy Impact)
**Primary Finding Addressed:** Rich character fields are not used in actual inference.

**Goal:** Make the detailed Character Editor actually drive model behavior without forcing users to copy-paste into "Custom System Prompt".

**Core Task 1.1 — System Prompt Composer (P0) — ✅ IMPLEMENTED (2026-05-30)**
- New `compose_system_prompt(&StoredCharacter) -> String` added in `src/commands.rs`.
- Rich editor fields now drive the model: Personality (core), Scenario & Lore, User role/description, Writing Instructions (high weight for consistency), Greeting as style reference.
- Full custom `system_prompt` remains a complete power-user override (zero behavior change for existing advanced users).
- Both critical inference paths refactored to load the character once and share it between the rich composer and the RAG decision (minor perf win + consistency).
- 3 new unit tests added and passing (`cargo test prompt_composer_tests` green):
  - Rich fields appear with clear section headers when no custom prompt is set.
  - Custom system_prompt fully overrides everything else.
  - Minimal characters still produce a solid usable prompt.
- Cargo check clean.
- **Next immediate step for this task:** Add a Tribunal-level test (or expand an existing one) that exercises the full `send_message_with_images` path with a character that only has personality/scenario/writing_instructions.
- Small related win also landed: RAG injection header improved to "[Relevant Knowledge from Character's Sources]" with bullet formatting + guidance to the model not to meta-comment on sources.

**Task 1.2 — RAG + History Improvements (P1)**
- Enhance RAG query to include last 1-2 user turns for better relevance (while staying under budget).
- When injecting history, include `speaker_name` for prior assistant turns (helps multi-character and consistency).
- Optional: Truncate very long individual messages gracefully in the prompt builder.

**Task 1.3 — Frontend / Editor Hints (P2)**
- In character editor, add a small "Effective System Prompt Preview" (read-only) button or section that shows what will actually be sent (calls a new Tauri command or computes client-side).

**Task 1.2 — RAG + History Improvements (P1)**
- Enhance RAG query to include last 1-2 user turns for better relevance (while staying under budget).
- When injecting history, include `speaker_name` for prior assistant turns (helps multi-character and consistency).
- Optional: Truncate very long individual messages gracefully in the prompt builder.

**Task 1.3 — Frontend / Editor Hints (P2)**
- In character editor, add a small "Effective System Prompt Preview" (read-only) button or section that shows what will actually be sent (calls a new Tauri command or computes client-side).
- This educates users and builds trust in the editor fields.

**Files Likely Touched:**
- `src/commands.rs` (main composer + call sites)
- `src/storage.rs` (possibly extend StoredCharacter helpers)
- `frontend/script.js` (editor preview, optional)
- New or updated tests (recommend adding to `tests/destructive_tests.rs` or a new `tests/prompt_tests.rs` if it grows)

**New Tribunal Requirements:**
- Add at least one test that verifies a character with only personality/scenario/writing_instructions produces a non-generic system prompt containing key phrases.
- Test that a full custom `system_prompt` still completely overrides.

**Acceptance Criteria:**
- Creating a character with detailed "Bot Personality" + "Scenario" + "Writing Instructions" (no custom system prompt) results in the model receiving a rich, structured system message that includes all three.
- Existing characters with only custom system prompt continue to work identically.
- RAG still works and is appended after the composed prompt.
- Tribunal passes (including new prompt tests).
- No increase in prompt size for simple characters.

**Risk / Blast Radius:** Low (prompt text only; data paths untouched). High creative depth win.

**Estimated Effort:** Medium (careful prompt engineering + tests).

---

### Phase 2 — Long-Context UI Parity & Bounded Rendering (Critical for Usability)
**Primary Finding Addressed:** Token budget protects the model but the UI still loads everything on conversation switch.

**Goal:** Make switching and viewing long conversations fast and memory-efficient while keeping "load more" capability for history review.

**Task 2.1 — Backend Bounded Load Command (P0) — ✅ DONE (2026-05-30)**
- New Tauri command `load_messages_for_display` fully implemented and gated.
- Uses `load_messages_within_token_budget`.
- Tribunal ran after addition → **passed cleanly** (17/17 tests).

**Task 2.2 — Frontend Wiring + "Load Earlier" UX (P0) — ✅ MAJOR PROGRESS (2026-05-30)**
- `loadConversationMessages()` now defaults to bounded token-budget loader.
- Full load-more + prepending + scroll preservation + button management implemented.
- Tribunal-grade test added for the bounded path (`test_load_messages_for_display_bounded_returns_has_more_signal`).
- All changes passed Tribunal.

**Phase 3 Streaming — STARTED**
- Non-stream path explicitly documented as temporary.
- Skeleton planning + awareness added in code comments.
- Real implementation (Tauri events + reqwest streaming + safe abort + atomic final persist) is the next major slice.

**Task 2.2 — Frontend Incremental / Lazy Rendering (P0)**
- Update `loadConversationMessages()` and `updateChatView()` to call the new bounded command by default.
- Implement "Load earlier messages" button (or infinite scroll upward) that fetches the previous window and prepends using `insertAdjacentHTML('afterbegin', ...)` while maintaining the `renderedMessageIds` Set.
- Preserve scroll position intelligently when prepending.
- Show a subtle indicator when history has been trimmed by the budget ("Earlier messages available — click to load").

**Task 2.3 — Token Usage Visibility (P1)**
- Add a small "Context" pill in the chat header or settings that shows estimated tokens used / budget (or % of model ctx when we have dynamic budget from Phase 4).
- Update on message send and on history load.

**Files Likely Touched:**
- `src/commands.rs` (new command + wiring)
- `src/conversation.rs` (possible small enhancements to the budget loader for cursor support)
- `frontend/script.js` (load logic + render + UI controls)
- Possibly `frontend/style.css` for the "load earlier" affordance

**New Tribunal Requirements:**
- Add a test that loads a conversation with > budget messages and confirms the UI-facing load returns a bounded set + has_more=true.
- Property or integration test that prepending older messages does not corrupt order or duplicate IDs.

**Acceptance Criteria:**
- Opening a conversation with 500+ messages is fast (<1s perceived) and uses bounded memory in the webview.
- User can still scroll back arbitrarily far with explicit "load earlier" actions.
- No regression in incremental new-message append.
- Tribunal passes.

**Risk / Blast Radius:** Medium (UI state + scrolling is tricky). Mitigate by keeping full load path behind a debug flag initially.

**Estimated Effort:** Medium-High.

---

### Phase 3 — Real Streaming (Biggest Perceived Performance & Interactivity Win)
**Primary Finding Addressed:** "Stream Responses" is a lie today.

**Goal:** Deliver actual token-by-token streaming in the main unified pipeline with proper abort, error handling, and Arena safety.

**Design Constraints (Non-Negotiable):**
- Must go through the full Rust pipeline (RAG, composed system prompt, token-budget history, vision, health checks, atomic append on completion).
- Must respect Arena Reset and proactive health checks.
- Abort must be clean (do not leave partial assistant message persisted on user cancel).
- On error mid-stream, surface clear error and do not poison conversation state.

**Task 3.1 — Backend Streaming Infrastructure (P0)**
- Options (evaluate in first sub-step):
  A. New command `send_message_stream` that uses `reqwest::Client` with streaming response, parses SSE chunks, and emits Tauri events (`app.emit("llm-token", {token, message_id, ...})`).
  B. Or keep one command but add a streaming flag and use events.
- Handle `abortController` signal from frontend (Tauri supports listening for window close or custom cancel events).
- On stream end: persist the full assistant message atomically (same as today).
- On error or abort: do **not** persist a partial assistant message (or persist with a clear "[incomplete]" marker if we want to allow resume later — decide explicitly).
- Update `regenerate_last_message` to support streaming too (or at minimum keep consistent behavior).

**Task 3.2 — Frontend Streaming Consumer (P0)**
- Wire the existing `APP_STATE.streamingContent`, `isGenerating`, abortController.
- Listen for Tauri events (or use a streaming invoke if Tauri 2 supports it better).
- Render tokens incrementally into a temporary "streaming" bubble (same style as final assistant message).
- On completion: replace the streaming bubble with the final persisted message (or let the normal append path handle it after `updateChatView`).
- Proper cleanup on abort/error.

**Task 3.3 — UI Polish & Settings (P1)**
- Honor the existing "Stream Responses" toggle in Settings.
- Show a clear "Generating..." + stop button during stream.
- Handle vision + streaming (images are sent up-front; tokens come after).

**Files Likely Touched:**
- `src/commands.rs` (new streaming command + event emission)
- `src/inference.rs` (possible small helpers for streaming client)
- `frontend/script.js` (event listener, streaming bubble, abort wiring)
- Possibly `frontend/index.html` for any new UI elements
- Tests: at minimum a happy-path streaming integration (harder to test in pure unit; may use the existing chaos response parser test style)

**New Tribunal Requirements:**
- Test that a successful stream results in exactly one persisted assistant message with the full content.
- Test that user abort mid-stream results in **zero** assistant message persisted (or clearly marked incomplete).
- Test that server death mid-stream is handled cleanly (no partial persist, good error to UI).

**Acceptance Criteria:**
- With a small model, tokens appear progressively in the chat bubble.
- Stop button aborts cleanly with no partial message left in history.
- Error mid-stream (e.g. worker death) shows toast, does not corrupt conversation.
- "Stream Responses" off falls back to non-stream (full response at once).
- Tribunal passes the new streaming invariants.

**Risk / Blast Radius:** Medium-High (streaming + events + abort is a classic source of state bugs). 
**Mitigation:** Implement behind a feature flag or as an opt-in first. Keep the non-stream path as the default until proven. Do not remove the non-stream path.

**Estimated Effort:** High. This is the most complex single piece.

**Recommendation on Sequencing:** Complete Phase 1 + 2 before starting Phase 3, or do Phase 3 in a parallel worktree if the agent can handle it.

---

### Phase 4 — Remaining Polish, Dynamic Budget, Chaos Hardening & Documentation

**Task 4.1 — Dynamic Token Budget from GGUF (P1)**
- When a model is loaded, capture its `context_length` from the GGUF metadata already parsed.
- Expose it via diagnostics / status.
- Use it (with safe headroom, e.g. 80-90%) as the default for `load_messages_within_token_budget` and UI loads.
- Fall back to 8192 (or 4096 for very small models).

**Task 4.2 — Finish Legacy Cutover (P1)**
- Implement real `editMessage` flow using the append-only + repair tools (no more "to be implemented" toast).
- Remove or fully deprecate any remaining direct `callLocalLLM` paths in the main chat flows.
- Ensure regenerate paths go through the new composed prompt + streaming (when Phase 3 done).

**Task 4.3 — Context Usage & Arena Telemetry (P2)**
- Surface estimated context usage in the chat UI (from Phase 2).
- Add last Arena Reset reason + timestamp to `get_diagnostics_snapshot` and the Diagnostics modal.
- Persist minimal Arena history (last 5 resets) in a small JSON (optional, low priority).

**Task 4.4 — Add the Two Hardest Missing Chaos Tests (P0 for full v7.2 closure)**
- True partial/truncated HTTP response during generation (mock the reqwest layer or use a test proxy).
- Mid-request worker SIGKILL + proof that conversation state remains consistent and no partial assistant message is persisted.
- These should be added to `tests/destructive_tests.rs` and must pass in the Tribunal.

**Task 4.5 — Minor Cleanups & Observability**
- Clean dead code or add explicit `#[allow(dead_code)]` comments with rationale.
- Improve Voice server parity in diagnostics.
- Add a few more `log::info!` or metrics around prompt composition and token budget decisions (for future Sovereign Scaling work).

**Task 4.6 — Documentation & Final Audit Refresh**
- Update README "Current Focus", AGENTS.md, AUDIT_v7.2 (append a new "Post-v8 Remediation" section), USER_MANUAL if user-facing changes, this workplan with "Completed" markers.
- Run full end-to-end manual test scenarios (long chat, character with rich fields only, streaming on/off, Arena trigger, corrupted data repair, etc.).
- Final Tribunal run.
- Update the living score in AUDIT and AGENTS Quick Context.

**Acceptance Criteria for Phase 4:**
- Dynamic budget works and is visible.
- Edit message works end-to-end through the modern pipeline.
- The two hard chaos tests exist and pass under Tribunal.
- All docs are current.
- Final Tribunal + manual verification pass.
- No new regressions in existing flows.

---

## Execution Order & Dependencies

1. **Phase 0** (foundation) — can start immediately.
2. **Phase 1** (prompt composer) — independent of others, highest philosophy win. Do early.
3. **Phase 2** (UI bounded load) — benefits from Phase 1 (prompt size awareness) but can run in parallel.
4. **Phase 3** (streaming) — higher risk; ideally after 1+2 are stable. Consider feature flag.
5. **Phase 4** items can be interleaved; the two chaos tests should be added as soon as the relevant code paths are touched.

**Parallelization Opportunities:**
- Agent can work on Phase 1 + Phase 4.1 (dynamic budget) + test additions in one focused slice.
- Frontend-heavy parts (Phase 2 UI, Phase 3 frontend) can be done after backend contracts are defined.

---

## Success Metrics (How We Know We Are Done)

- All P0 findings from the 2026 audit have concrete, Tribunal-gated fixes.
- A user can create a rich character using only the main editor fields (no custom system prompt) and get noticeably better fidelity.
- Opening a 2000-message conversation feels responsive.
- Streaming works and abort is safe.
- The app can survive the two hardest chaos scenarios with Blast Radius 0.
- Updated score in living docs: 9.4–9.6 / 10.
- `./scripts/tribunal.sh` passes cleanly at the end of every phase.

---

## Out of Scope (for this workplan)

- Full vector DB / HNSW for RAG (future after embeddings are proven useful).
- Actor model refactor for the global Mutexes (good future direction, not blocking).
- AR / location social layer (FUTURE_VISION.md).
- Containerized forensic Tribunal (original v7.2 dream) — we can improve the existing script + hook.
- Changing the single-file frontend strategy.

---

## How to Use This Document

- Treat this as the living execution contract for the next development cycle.
- Before starting any task, update its status here and in the todo system.
- Every code change must be accompanied by a note in this file: "Implemented in commit X — Tribunal passed".
- When the plan is complete, append a "Completion Report" section with before/after scores, key diffs, and remaining Tier 2 items.

**This workplan was created immediately after the 2026 Q&A/UI/Perf/Stability/Adversarial audit and is the canonical reference for closing the identified gaps.**

---

*End of initial workplan. Execution begins now.*

---

## Execution Log (Live)

**2026-05-30 Major Session**
- Phase 1 (Rich prompt composer): Fully implemented, 3 tests added, RAG header improved, call sites refactored for single load. All Tribunal passes.
- Phase 2 (Bounded UI loading):
  - `load_messages_for_display` command added.
  - Full frontend wiring: bounded load by default, "Load earlier messages" with prepending + scroll preservation.
  - Tribunal-grade test added.
  - Multiple Tribunal runs: all green (now 18 tests).
- Phase 3 (Streaming): Skeleton + explicit comments added. Real implementation queued.
- Phase 4:
  - `model_context_length` added to `ServerStatus` + both managers.
  - On LLM server start we now parse the GGUF and store the real context length.
  - Ready for smart budgeting (next: use it with headroom in the load calls + surface in Diagnostics).
- All changes passed Tribunal (latest run: 18/18 tests, exit 0, "Blast Radius = 0 verified").

Continuing full-speed execution of the plan.

**Latest achievements (this "go" session):**
- Dynamic GGUF context budget is **live and observable**:
  - Real model_context_length parsed on every LLM start.
  - Used with safe headroom in send_message + regenerate paths.
  - Shown in Diagnostics UI.
- Phase 2 further polished (button state after sends).
- Multiple clean Tribunal passes throughout.

Continuing full execution of the plan.
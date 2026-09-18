# No-GPU slice runbook (post plan-mode)

## 0. Setup
mkdir -p harness docs/handbook/AUDIT_0.9.0-rc.1
Commit kit; chmod +x harness/fake_llama_server.sh harness/mock_ctl.py; .gitignore additions.
Roster (3 models, 5.5G — supersedes "both test-models"): see
docs/handbook/AUDIT_0.9.0-rc.1/MODEL_MANIFEST.md + `cargo test --test gguf_roster_probe`.
A/B verdict pinned: L3.2-Rogue-7B parses llama.ctx 131072 → budget 72,089 = F-001
LIVE-repro for Tier 2 (no parser fix needed); gemma3/qwen35 fall back to 4500.
Port discipline: C-matrix runs against the APP-MANAGED child (Settings →
harness/fake_llama_server.sh); mock_ctl targets the app's configured port, never :18799.

## 1. Tap (10 min)
src/capture.rs + mod lines (main.rs AND lib.rs); call at commands.rs:836;
extend response struct with usage (Option<Usage>) if dropped.
Verify: LOCALPERSONA_CAPTURE_PROMPTS=harness/captures.jsonl cargo tauri dev → send 1 msg → file grows.

## 2. CTX witnesses (5 min)
cargo add --dev proptest (if absent); cargo test --test ctx_budget_property
→ ALL WITNESSES GREEN = F-001/F-002/F-005(F-007) pinned with reproducers.
Then wire actual_* bodies to real fns (lib.rs exports; ADAPT).

## 3. Mock + shim (30 min)
Graft §3.3 into scripts/mock_llama_server.py (4 integration points).
Settings → llama-server path = harness/fake_llama_server.sh (absolute).
Start server in-app → mock_ctl.py <port> show must answer.

## 4. C-matrix (per row: set mode → send in app → observe toast/append/next-send → check mock_requests.jsonl)
| mode            | expected verdict                                              | else |
|-----------------|---------------------------------------------------------------|------|
| normal          | appends; request logged; sampling fields as set               | —    |
| malformed/wrongshape/empty_choices | clean parse error, NO append, next send ok   | S0   |
| empty/whitespace/nullcontent | explicit empty-content rejection, NO append         | S0 (§11.2 claim) |
| 500 / 503 / ctx400 | surfaced error, NO append, exactly 1 POST (HTTP status never retried), next send ok | S1   |
| drip            | error within ≤300s total (≤2×120s timeout + respawn + readiness; record actual duration) | >300s or unbounded = HARD ROW |
| hang            | error within ≤300s total (record actual duration → F-012 evidence; note input responsive?) | >300s or unbounded = HARD ROW |
| redirect        | clean error, NO follow — grep mock_requests.jsonl for /REDIRECT_CANARY hit (=follow=S0-sec) | follow = S0-sec |
| length          | truncation marker VISIBLE in UI (post-hoc :834-835)           | silent = S2 |
| big             | bounded handling, UI responsive                               | S2   |
| reset           | clean error, no append (transport cut → ≤2 POSTs, retry once) | S1   |
| notready→ready  | readiness gating; measure the 15s timeout → F-010 evidence    | —    |
After each row: mock_ctl.py <port> normal.
POST oracle (retry semantics pinned: loop ≤2 iters commands.rs:734; transport errors
retried once :797-803; HTTP-status/parse/content errors never retried :808-833;
auto-restart only on dead child, attempt 0, breaker-guarded :739/inference.rs:601):
success = 1 POST · transport-fail (drip/hang/reset/refused) = ≤2 POSTs + ≤1 respawn
(new PID, same port) · app-fail (4xx/5xx/parse/empty) = exactly 1 POST, no respawn.
>2 POSTs per send = finding (retry-budget anomaly, prima de D2).

## 4c. F-001 live repro (no GPU)
seed_long_history.py (§2.1 → harness/seed_long_history.py) -> ndjson_check clean ->
Settings model=L3.2-Rogue, server=harness/fake_llama_server.sh -> select character,
open 'audit-f001-seed', send 1 message ->
  a) validate_captures --ctx 8192: BUDGET(exact) FAIL with prompt_tokens ≈ 72k  [the deliverable]
  b) mock_ctl ctx400 -> send again -> error surfaced? no append? toast actionable?
  c) mock_ctl normal -> restore
Note: goldens (§7) are unaffected by model choice — short conversations never hit the walk bound.

## 5. T-5 stdout probe
Launch app with MOCK_STDOUT_FLOOD=1 (env inherits to child). Chat 2-3 min.
Server stall / health failure after ~64KB of stdout = F-003 confirmed behaviorally.
Fix is 3 lines (drain forever) — apply immediately.

## 6. B4 partial line
F=<appdata>/conversations/<uuid>/messages.ndjson
truncate -s -1 "$F" → send one message in app → ndjson_check.py --appdata <appdata>
'concatenated' finding = R8/S0. 'truncated'-only + next line clean = pass (skip+warn held).

## 7. QA-1/2/3 golden pass — BATCHED (one sampling per file; validator applies
--expect-sampling to every line). Prep first: 3 disposable in-app characters
(Audit Disposable = default copy + avatar/voice for T-9; Audit Override =
system_prompt set; Audit Minimal = name-only), one small knowledge doc with a
distinctive fact on the G2 base persona (via app UI), fresh logs
(`rm captures.jsonl mock_hits.jsonl mock_state.json`).
| Batch | Content | Sends | Validate | Destination |
|---|---|---|---|---|
| G1 | 7 defaults, default mode | 7 | no --expect-sampling | captures_golden.jsonl |
| G2 | full-factorial on ONE base persona (modes ×4–5, RAG off/on, override, nickname, empty-personality) — compose is persona-independent, ~10 sends cover what 112 would | ~10 | same | append goldens |
| S1 | sweep 0.7/2048/0.9 | 1 | --expect-sampling 0.7,2048,0.9 | append goldens |
| S2 | clamp probe max_tokens 50000 | 1 | manual: sent must be 32768 → F-013 note | append goldens |
| F1 | §4c seed repro (2 sends) | 2 | --ctx 8192 → BUDGET(exact) FAIL **expected** | EVIDENCE_F001.jsonl, NEVER goldens |
Order: §7 BEFORE §4c (the ~72k F-001 line fails BUDGET by design and must not
enter goldens). T-1 freebie: run the first RAG-on cell with network BLOCKED
(fail-closed? chat continues? = T-1 evidence), then unblock for the rest.

## 8. Phase 3 lifecycle
T-2: regen ×5 → reload → exactly one assistant after last user; walk counts live msgs.
T-3: rename character with open conversations → ID preserved?; delete character →
     send in its conversation → actionable error (not hollow persona, not crash).
T-9: export → wipe → import → field diff; binary loss documented (F-006 evidence).
T-10: corrupt a line, OPEN conversation (no repair cmd) → auto-repair or visible drift?
QA-7: pill value vs captured estimator vs usage (when present) — log divergence (F-016).

## 9. Doc patches (CTX_FINDINGS §1.3 text) + F-015 items. 10. Consolidate register; gates review.

## Gates (slice exit)
- CTX-1..5 answered with code evidence (done) and witnesses green
- C-matrix: no append on any failure row; no hang >120s; no redirect follow
- ndjson_check clean after B4 procedure
- Goldens committed green; T-2/T-3 verdicts recorded

# Behavioral session results — no-GPU slice §4–8

## Provenance (F-022 gate — REQUIRED: no full header → no consolidation)
- Operator (human): ___
- Session start/end: ___
- Tree: commit ___ (post-6965905), dirty files: ___
- App mode: dev | binary ; LOCALPERSONA_CAPTURE_PROMPTS=___
- Settings model (§4c requires Rogue): ___
- App port (mock_ctl target): ___
- Closing line counts: captures_golden ___ · EVIDENCE_F001 ___ · mock_requests ___
- Rows run: ___ / skipped + why: ___
- Mock: app-managed via harness/fake_llama_server.sh

## §4 C-matrix (flip mode -> send -> observe -> restore normal)
POST oracle: success=1 · transport-fail ≤2 POSTs + ≤1 respawn · app-fail exactly 1 · >2 POSTs = finding.
| mode | error surfaced? | appended? | next send ok? | POSTs (count in mock_requests) | duration | canary? | verdict | fail→row |
|---|---|---|---|---|---|---|---|---|
| normal | | | | 1 | | — | | — |
| malformed / wrongshape / empty_choices | | | | 1 (no retry on parse) | | — | | C1 |
| empty / whitespace / nullcontent | | | | 1 | | — | | C2 |
| 500 / 503 / reset / ctx400 | | | | 500/503/ctx400→1 · reset→≤2 | | — | | C3 |
| drip | | | | ≤2 | ≤300s req; actual ___ | — | | C4 |
| hang | | | | ≤2 | ≤300s req; actual ___ | — | | A3/C4 |
| redirect | | must NOT follow | | ≤2 (redirect-error retried once) | | /REDIRECT_CANARY hit? ___ (=follow=S0-sec) | | C6 |
| length | marker VISIBLE in UI? | | | 1 | | — | | C5 |
| big | UI responsive? | | | 1 | | — | | C7 |
| notready→ready | readiness behavior; 15s observed ___ | | | — | | — | | T-6 |
Per wait-row: input responsive during wait? (responsive -> F-012 stays S2; frozen -> S1)
Classifier note (record even on pass — feeds the F-004/F-012 Phase-3 rider): reset POSTs = 1 or 2? ___ ; redirect POSTs = 1 or 2? ___

## §4c F-001 live repro (no GPU — Rogue is an ACTIVE trigger, F-001 EN VIVO)
seed: `seed_long_history.py --appdata DIR --learn-from DIR/conversations/<tpl> --character <id>`
  -> `ndjson_check` clean (seed must be clean BEFORE opening app)
  -> Settings model = L3.2-Rogue…gguf, server = harness/fake_llama_server.sh
  -> select character, open 'audit-f001-seed', send 1 message ->
  a) `validate_captures --ctx 8192`: BUDGET(exact) FAIL with prompt_tokens ≈ 72k  [THE deliverable]
  b) `mock_ctl ctx400` -> send again -> error surfaced? ___ no append? ___ toast actionable? ___
  c) `mock_ctl normal` -> restore
prompt_tokens ≈ ___ (expect ~72,089) ; BUDGET(exact) FAIL? ___ ; ctx400 toast text: ___
Note: goldens (§7) unaffected by model choice — short conversations never hit the walk bound.

## §4b T-5 flood: minutes chatted ___ ; stall? ___ ; t-to-stall ___ ; F-003 verdict ___ (apply 3-line drain fix, retest)

## §6 B4: file ___ ; ndjson_check -> [ ] clean [ ] truncated-only (pass) [ ] concatenated (S0/R8)

## §7 goldens (batches separate; F1 NEVER in goldens — see §4c):
G1 (7 defaults): ___ pass / ___ fail ; G2 (single-persona factorial ~10): ___ / ___ ;
S1 --expect-sampling 0.7,2048,0.9: sent ___ ; S2 max_tokens 50000 → sent ___ (expect 32768, F-013) ;
goldens file: ___ ; committed [ ] ; T-1 blocked-net first RAG query: ___ (fail-closed? chat continued?)

## §8 lifecycle: T-2 assistants-after-reload ___ (expect 1) ; T-3 rename id preserved? ___ delete+send actionable? ___ ;
T-9 fields lost ___ binaries lost (expected) ___ ; T-10 auto-repair? ___ ; QA-7 pill ___ vs est ___ vs usage ___ (Δ ___%, F-016)

## §9 new rows (A4)
a) a11y smoke (15–30m): Tab-order chat flow (sidebar→input→send→settings) ___ ; focus visible ___ ; toast/button contrast spot ___ ; Enter vs Ctrl+Enter ___ ; prefers-reduced-motion ___ (if animations)
b) A10 close-mid-generation (use slowdrip… use `drip` mode as the long window): close window during generation → reopen → ndjson_check ___ ; ghost procs (pgrep) ___ 
c) cancel affordance: any way to abort in-flight generation? ___ (expected: none → UX roadmap note, not a finding)
d) ENOSPC (needs sudo: tmpfs 50M + XDG_DATA_HOME, fill, send): clean error? ___ partial line? ___ orphan temps? ___ ; no sudo → DEFERRED ___

## In-session quick wins: [x] clippy triage (67, 4 fixed) [x] git baseline+tag (dd295a4) [x] probe A/B verdict (mixed, Rogue live) [ ] sudo available for §9d? ___

## Gate: [ ] no append after ALL retries exhausted [ ] no hang/drip >300s or unbounded [ ] no /REDIRECT_CANARY hit
[ ] no POST count >2 per send [ ] ndjson clean post-B4 [ ] goldens green+committed [ ] T-2/T-3 recorded [ ] register statuses updated
[ ] seed clean before open (ndjson_check) [ ] **F-001 repro captured** — BUDGET(exact) FAIL ≈ 72k (the FAIL *is* the evidence; paste capture line into register) [ ] ctx400 row: clear error, no append [ ] warm commit time measured → F-014 decision recorded

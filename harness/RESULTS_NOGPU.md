# Behavioral session results — no-GPU slice §4–8
Executor: ___  Date: ___  Tree: audit-baseline-0.9.0-rc.1 + <commits>  App: dev | .deb
Mock: app-managed via harness/fake_llama_server.sh — APP port (mock_ctl target): ___

## §4 C-matrix (flip mode -> send -> observe -> restore normal)
| mode | error surfaced? | appended? | next send ok? | duration | logged in mock_requests.jsonl? | verdict | fail→row |
|---|---|---|---|---|---|---|---|
| normal | | | | | | | — |
| malformed / wrongshape / empty_choices | | | | | | | C1 |
| empty / whitespace / nullcontent | | | | | | | C2 |
| 500 / 503 / reset | | | | | | | C3 |
| drip | | | | ≤120s req | | | C4 |
| hang | | | | ≤120s req; actual ___ | | | A3/C4 |
| redirect | | must NOT follow | | | | | C6 |
| length | marker VISIBLE in UI? | | | | | | C5 |
| big | UI responsive? | | | | | | C7 |
| notready→ready | readiness behavior; 15s observed ___ | | | | | | T-6 |
Per wait-row: input responsive during wait? (responsive -> F-012 stays S2; frozen -> S1)

## §4b T-5 flood: minutes chatted ___ ; stall? ___ ; t-to-stall ___ ; F-003 verdict ___ (apply 3-line drain fix, retest)

## §6 B4: file ___ ; ndjson_check -> [ ] clean [ ] truncated-only (pass) [ ] concatenated (S0/R8)

## §7 goldens: tap lines ___ ; usage present? ___ ; matrix cells ___ ;
validate_captures --ctx 8192 -> ___ pass / ___ fail (paste FAILs) ;
sampling: UI(0.7,2048,0.9) sent ___ ; UI max_tokens 50000 sent ___ (F-013) ; goldens committed [ ]

## §8 lifecycle: T-2 assistants-after-reload ___ (expect 1) ; T-3 rename id preserved? ___ delete+send actionable? ___ ;
T-9 fields lost ___ binaries lost (expected) ___ ; T-10 auto-repair? ___ ; QA-7 pill ___ vs est ___ vs usage ___ (Δ ___%, F-016)

## In-session quick wins: [ ] clippy -D warnings (F-021) [ ] git init + tag (F-020) [ ] probe A/B verdict: ___

## Gate: [ ] no append on failure rows [ ] no hang >120s [ ] no redirect follow
[ ] ndjson clean post-B4 [ ] goldens green+committed [ ] T-2/T-3 recorded [ ] register statuses updated

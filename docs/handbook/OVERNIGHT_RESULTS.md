# Overnight autonomous fix results

**Session end:** 2026-07-25  
**Status:** Automated gates **PASS** — human sunrise smoke still required.

## Gates

| Gate | Result |
|------|--------|
| `node --check frontend/script.js` | OK |
| `cargo test --lib` | 34+ tests OK (incl. new truncate/budget) |
| `cargo test --test property_tests` | 18/18 |
| `cargo test --test destructive_tests` | 21/21 |
| `./scripts/smoke-check.sh` | **PASSED** |

## Deliverables for sunrise

1. Read **`docs/handbook/SUNRISE_REVIEW.md`** — 10-step human smoke.
2. Run `./scripts/smoke-check.sh` again if you pull/rebuild.
3. `cargo tauri dev` and walk the smoke list.

## Fix inventory (this nap)

| # | Fix |
|---|-----|
| 1 | Mid-thread regen: timestamped `.bak` before rewrite + browser confirm |
| 2 | Last-turn regen: replace after last user (no assistant stack) |
| 3 | Block contact switch while generating |
| 4 | Delete character → delete participant conversations |
| 5 | Auto-restart race (`is_resetting`) + one retry inside `post_chat_completion` |
| 6 | RAG: no-embeddings is an error; dim mismatch skipped |
| 7 | Context pill uses model `context_length` when available |
| 8 | Character schema version normalize on load |
| 9 | Smoke script + sunrise docs |

Sleep well. Review at sunrise.

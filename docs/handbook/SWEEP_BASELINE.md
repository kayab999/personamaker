# Sweep baseline (2026-10-04, pre-sweep commit 2b63705)

Numeric ground truth for the full sweep. All gates below ratchet upward —
never lower a floor without a commit explaining why.

| Metric | Baseline | Gate |
|---|---|---|
| `cargo test` (lib+main+6 suites) | 136 passed, 0 failed | CI `test` job |
| Frontend smoke (`node --test frontend/tests/`) | 0 tests (did not exist) | CI `frontend-tests` job |
| Coverage, lines total (`llvm-cov --lib --tests`) | 31.5% | `--fail-under-lines 30` |
| Production `unwrap()` (`scripts/count_unwraps.py`) | 0 | `BASELINE=0` |
| `cargo audit` (with 3 ignores) | exit 0 | CI `audit` job |
| `cargo deny check licenses bans sources` | not run (no deny.toml) | CI `deny` job (new) |
| `cargo clippy` (CI flags) | clean | CI `clippy` job |
| Accepted audit ignores | RUSTSEC-2026-0187, -0195, -0194 | audit.toml + CI flags |
| `.git` size / tracked files | 2.8 MB / 130 | — |
| Default personas with real-person refs | 7 of 7 | 0 after W4 |
| CSP `script-src` | `'self'` only | CI frontend check |
| Remote CDN refs (html/js/conf) | 0 | CI frontend check |

Waves: W1 deps/hardening → W2 gates/tests → W3 supply/packaging → W4 product/docs.

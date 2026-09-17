# Claims-to-Evidence Register — Audit 0.9.0-rc.1 (Phase 0, Day 1)

**Date:** 2026-09-09 | **Scope:** `ARCHITECTURE_AND_STATUS.md` claims → evidence | **Evidence level:** `I1` static + `I4` execution where noted | **OS:** Linux-First

## Execution Evidence (Day 1 Greps)

### 1. Timeout semantics gate (decides A3/C4 S0 vs S0-adjacent) — Re-run Day 3 09:45 UTC
```bash
$ grep -n -E 'Client::builder|\.timeout\(|connect_timeout|Policy::none|redirect' src/commands.rs src/inference.rs
src/commands.rs:35:    reqwest::Client::builder()
src/commands.rs:36:        .timeout(std::time::Duration::from_secs(120))
src/commands.rs:37:        .redirect(reqwest::redirect::Policy::none())
src/inference.rs:1234:    let client = reqwest::Client::builder()
src/inference.rs:1235:        .timeout(Duration::from_secs(2))
src/inference.rs:1236:        .redirect(reqwest::redirect::Policy::none())
$ grep -n "csp" tauri.conf.json
26:      "csp": "default-src 'self'; script-src 'self' 'unsafe-inline'; style-src 'self' 'unsafe-inline' https://fonts.googleapis.com; style-src-elem 'self' 'unsafe-inline' https://fonts.googleapis.com; font-src 'self' https://fonts.gstatic.com data:; img-src 'self' data: blob:; connect-src 'self' http://localhost:* http://127.0.0.1:*"
```
**Verdict:** `HTTP_CLIENT` now `.timeout(120s)` **total** (connect→body) + `Policy::none()`; `wait_for_server_ready` `.timeout(2s)` + `Policy::none()`. No `connect_timeout` separate. Drip (slow body) and hang (no headers) **both** fire at 120s — **S0-adjacent** 2-min spinner, not infinite hang. Before fix: no `Policy::none()` → 302 bypass. After fix: both clients `Policy::none()`.
- **I-level:** I4 verified 2026-09-09T09:45 UTC via grep execution.
- **Action:** `src/commands.rs:35` + `src/inference.rs:1234` patched. **Register labels for A3/C4 now gated: A3=120s spinner (S0-adjacent), C4 drip fires at 120s. If it had been `connect_timeout` only, A3 would be S0 infinite hang.**

### 2. Host bind & CDN contradiction (R1, R5 split)
```bash
$ grep -n "0.0.0.0" docs/VOICE_SETUP.md src/inference.rs
docs/VOICE_SETUP.md:31:  --host 0.0.0.0 \  # pre-fix
# src/inference.rs: no 0.0.0.0 (correct: 127.0.0.1 at :320,830) post-fix 127.0.0.1

$ grep -n "tailwindcss\|fonts.googleapis" tauri.conf.json frontend/index.html
tauri.conf.json:26:  ... https://cdn.tailwindcss.com ...  # pre-fix
frontend/index.html:7: <script src="https://cdn.tailwindcss.com"></script>  # pre-fix
# post-fix 2026-09-09T09:45 UTC:
# tauri.conf.json:26 csp no tailwindcss, still https://fonts.googleapis.com / https://fonts.gstatic.com
# frontend/index.html:7 <script src="vendor/tailwind.js"></script> (398 KB JIT)
```
**Verdict:** App binds `127.0.0.1` correctly (I1), doc contradicted. CDN violates offline-by-design. **Split:** `script-src tailwind` = S0 remote-code (closed Day 1), `font-src/style-src fonts.googleapis` = S2 privacy leak (still open).
- **Fix R1:** `docs/VOICE_SETUP.md:31` → `127.0.0.1` + note.
- **Fix R5-script (S0):** `tauri.conf.json:26` CSP stripped of `https://cdn.tailwindcss.com`. `frontend/vendor/tailwind.js` vendored (Play CDN JIT, 398 KB). `frontend/index.html:7` → `vendor/tailwind.js`. **S0 closed**, but artifact is **development-only JIT** (scans DOM at runtime) — filed as S3 follow-up: replace with `tailwindcss` CLI `--content frontend/**` → `frontend/vendor/tailwind.css` static minified.
- **Fix R5-font (S2):** still open — `tauri.conf.json:26` + `frontend/index.html:9-10` still `https://fonts.googleapis.com`/`gstatic`. Drift test will catch (see Fix/Test Audit § below).

### 3. SSRF redirect bypass (C6)
```bash
$ grep -rn -E "apiEndpoint|ttsEndpoint|api_base|reqwest" src/
src/commands.rs:35: reqwest::Client::builder()
src/commands.rs:47: fn validate_localhost_endpoint
src/inference.rs:1183: reqwest::Client::builder()
# no "redirect" before fix → default Policy::limited(10) follows 302
```
**Verdict:** `validate_localhost_endpoint` only checks initial URL. `reqwest` default follows 10 redirects → 302 to `http://evil.com` bypasses check. **Confirmed.**
- **Fix C6:** Both clients now `.redirect(Policy::none())`. Grade recalibrated S0→S1 (requires hostile localhost process) per review, S0 if auto-discovery lands.
- **Mock verification:** `scripts/mock_llama_server.py` modes `redirect` + `relaylog` provide binary detector via `MOCK_LOG` + `mock_hits.jsonl`.

## Claim Verdicts (Pre/Post-Fix)

| # | Doc claim | Location | Pre-fix I1 | Post-fix | Matrix |
|---|-----------|----------|------------|----------|--------|
| R1 | Voice doc host | `docs/VOICE_SETUP.md:31` vs `src/inference.rs:320` | **False** (doc contradicts code) | **Fixed** — doc → `127.0.0.1` | A-series |
| R2 | HTTP timeout exists | `src/commands.rs:34-42` | **Partial** — 120s total, not infinite, but 2-min spinner | **Mitigated** (total timeout verified 09:45 UTC; abort UX still frozen post-RC) | A3/C4 |
| R3 | Circuit breaker/autopsy wired | `src/circuit_breaker.rs`, `src/autopsy.rs`, `src/commands.rs:95` | **Partial** — wired only via auto-restart path | **Accepted** — not dead code, coverage gap remains | — |
| R4 | Drop → start_kill | `src/inference.rs:1109,1235` | **Risk** — blocking sleep in Drop, no cmdline check | **Hardened** — nonce + `is_llama_server_process` (`/proc/<pid>/cmdline`) | A6 |
| R5-script | Offline CDN (script) | `tauri.conf.json:26`, `frontend/index.html:7` | **False** — CDN `tailwindcss.com` RCE | **Fixed S0** — CSP stripped, `vendor/tailwind.js` JIT vendored (S3 follow-up: replace with CLI `--content` → static CSS) | S-8 |
| R5-font | Offline CDN (fonts) | `tauri.conf.json:26`, `frontend/index.html:9` | **False** — `fonts.googleapis` privacy leak | **Open S2** — still `https://fonts.googleapis.com`/`gstatic` in CSP + HTML; drift test pending | S-8 |
| R6 | Truncation surfaced | `src/commands.rs:828` | **False** — `finish_reason: length` silent | **Fixed** — `post_chat_completion` appends marker via `apply_truncation_marker()`; **Closed** — `test_finish_reason_length_marker` (regression) | C5 |
| R7 | Draft preservation | `frontend/script.js:658` | **Unknown** — need F-2 kill-mid-send | Pending (shell-layer A10) | F-2 |
| R8 | Partial NDJSON + append | `src/conversation.rs:206` | **Risk S0** — no newline guard | **Fixed** — checks last byte `!= \n` inserts separator (B4) **Closed** (2 tests, regression tier) | B4 |
| R9 | Images token budget | `src/commands.rs:586` | **Risk S1** — vision tokens not counted | **Open** — compound IO S1 | — |
| R10 | AppImage space-path | `scripts/package-release.sh` | **Unknown** — shellcheck unavailable | Pending `shellcheck` | — |
| R11 | innerHTML XSS | `frontend/script.js:311,854` | **Risk S0** — `escapeHtml` via `div.textContent` mostly safe, but sink incomplete | **Open** — needs full sink walk + CSP `unsafe-inline` review | S-2 |
| R12 | fsync dir | `src/storage.rs:134` | **Risk** — file fsync only, no dir fsync | **Fixed** — dir `sync_all()` after `persist`; **Closed** — `test_dir_fsync_call_site_exists` (drift-guard tier) | B1 |
| R13 | Arena timer suspend | `src/inference.rs:144 Instant` | **Verified** — monotonic pauses on suspend | **Accepted** — documented | — |
| R14 | Dual instance | `src/main.rs`, no `single-instance` | **Risk S1** — port conflict, dual writers | **Open** — Linux-First freeze | A8 |
| C6 | Redirect policy | `src/commands.rs:35` | **Bypass confirmed** — no `Policy::none` | **Fixed S1** — `Policy::none()` added via `build_http_client()`; **Closed** — `test_c6_redirect_not_followed` now uses real client (regression tier) | C6 |
| gguf:150 | Array seek | `src/gguf.rs:149` | **Bug confirmed** — `seek(8)` rough | **Fixed** — per-type skip; **Closed** — `test_gguf_array_skip_varied_widths` (regression tier, crafted GGUF) | Day1-C |

## Fix/Test Pairing Audit (Day 6 closure rule, applied Day 2-3) — Tiered

**Regression tier** (must fail pre-fix, proves bug existed):
| Day-1 fix | Regression test | Fails pre-fix? | Verdict |
|---|---|---|---|
| B4 newline guard (`conversation.rs:206`) | `test_b4_append_without_trailing_newline` + negative | Yes (concatenated JSON) | **Closed** ✓ |
| A6 PID nonce+cmdline (`inference.rs:28,58`) | `test_a6_pid_reuse_protection` + positive mock | Yes (innocent `sleep` would be unverified) | **Closed** ✓ |
| C6 `Policy::none()` | `test_c6_redirect_not_followed` now uses `build_http_client()` | Yes (would follow 302 pre-fix) | **Closed** ✓ |
| `gguf.rs:142` array-skip | `test_gguf_array_skip_varied_widths` (crafted GGUF with UINT8 + STRING arrays) | Yes (old `seek(8)` misaligned, `context_length` missed) | **Closed** ✓ |
| `finish_reason:"length"` marker (`commands.rs:828`) | `test_finish_reason_length_marker` via `apply_truncation_marker()` | Yes (no marker pre-fix) | **Closed** ✓ |

**Drift-guard tier** (fails on regression, not original bug; counts separately at gate):
| Fix | Drift test | Verdict |
|---|---|---|
| dir fsync (`storage.rs:134`) | `test_dir_fsync_call_site_exists` asserts `dir_file.sync_all()` | **Closed** ✓ (drift-guard) |
| CSP vendoring | `test_csp_no_http_origins_in_script_src` asserts no `cdn.tailwindcss.com`/`https://` in `script-src` + vendored html | **Closed** ✓ (drift-guard, S0); `font-src` S2 still open |
| B9 lock (`conversation.rs:628,670`) | `stress_concurrent_send.rs` concurrent + repair vs append | **Closed** ✓ — `test_stress_concurrent_appends_no_interleaving` + `test_b9_repair_vs_concurrent_append_no_clobber` (Day 3) |
| Command-layer A/C | `tests/command_layer.rs` 7 integration tests via spawned mock (no GUI) | **Closed** ✓ — A1, A3, C1, C2, C3, C5, C6 (harness → command-layer) |

**Still open:** Fonts S2 (`fonts.googleapis.com` in `tauri.conf.json:26` + `frontend/index.html:9` — S2 privacy, not S0), R9/B6 compound IO S1, R11 XSS, R14 dual-instance, A7/A9/A10 shell-layer (xvfb).

## Day 1 Fixes Applied (Build Mode) — with artifact notes

1. **scripts/mock_llama_server.py** — stdlib-only shim, `parse_known_args` absorbs llama-server flags, modes: normal/malformed/empty/whitespace/nullcontent/500/drip/hang/reset/length/big/redirect/relaylog, timing `--bind-delay`/`--slowready`, `--drip-secs` float, `MOCK_LOG` JSONL + stderr `hitlog`, `chmod +x`.
2. **scripts/soak.sh** — 8h harness, 60s sampler RSS/FDs/Disk/Ghost PIDs, summary leak hint, `APP_NAME`/`SOAK_LOG` env, Linux/macOS `date` compat.
3. **docs/VOICE_SETUP.md:31** — `0.0.0.0` → `127.0.0.1` + security note.
4. **tauri.conf.json:26** — CSP `script-src` stripped of `https://cdn.tailwindcss.com` (S0 closed). **Note:** vendored artifact `frontend/vendor/tailwind.js` is Play CDN **JIT compiler** (runtime DOM scan, dev-only per Tailwind docs) — S0 closed but filed as **S3 follow-up**: `tailwindcss` CLI `--content frontend/**` → `frontend/vendor/tailwind.css` static minified. `font-src`/`style-src` still `https://fonts.googleapis.com`/`gstatic` → S2 open.
5. **frontend/vendor/tailwind.js** — 398 KB JIT vendored via `curl`, `frontend/index.html:7` → `vendor/tailwind.js`. **S3 follow-up:** replace with static CSS.
6. **src/commands.rs:35** + **src/inference.rs:1234** — `.redirect(Policy::none())`.
7. **src/gguf.rs:142-153** — array skip per element type (STRING length-aware).
8. **src/storage.rs:134** — dir `sync_all()` after `persist` (best effort, risk-accepted on btrfs/APFS).
9. **src/conversation.rs:14,206** — `Read+Seek` imports + B4 newline guard + B9 repair lock (messages + metadata) via `acquire_sidecar_lock` + `atomic_write_bytes` while held.
10. **src/commands.rs:828** — `finish_reason: "length"` marker + warn log.

## Tribunal Baseline (Post-Fix, I5 — Updated Day 3 11:00 UTC)

```bash
cargo check            # OK, 26 warnings (12 dup), 0 errors
cargo test --test destructive_tests  # 30 passed (0.71s) — 9 new (B4×2, A6×2, C6, gguf, finish_reason, dir_fsync, csp)
cargo test --test command_layer      # 7 passed (2.65s) — command-layer A/C (A1, A3, C1-C3, C5, C6) via spawned mock, no GUI
cargo test --test stress_concurrent_send # 3 passed — B9 + D2-D4 + drift guard
cargo test --test property_tests     # 18 passed
./scripts/tribunal.sh  # ✅ Blast Radius = 0 (gates destructive+property; command_layer/stress are I5 supplement)
cargo +nightly fuzz check  # fuzz targets authored: gguf_parse, ndjson_parse (parse_gguf_bytes, parse_message_line) + dicts
# Background (CARGO_TARGET_DIR=target-mutants): cargo mutants --file src/conversation.rs --file src/storage.rs --file src/gguf.rs &
# Background (fuzz/target): cargo +nightly fuzz run gguf_parse -- -max_total_time=86400 -timeout=25 -dict=fuzz/dict/gguf.dict &
```
shellcheck                          # not installed — deferred
grep Policy::none — confirmed both clients (see §1)
```

## Day 3 Fixes Applied (Build Mode — 2026-09-10)

11. **src/commands.rs:34** — exposed `#[doc(hidden)] pub fn build_http_client()` for C6 regression (now uses real client).
12. **src/commands.rs:828** — factored `#[doc(hidden)] pub fn apply_truncation_marker()` for R6 test.
13. **src/gguf.rs:29** — added `parse_gguf_bytes(&[u8])` + generic `read_kv_pair<R: Read+Seek>` for fuzzing; `fuzz/fuzz_targets/gguf_parse.rs` + `ndjson_parse.rs` authored with `libfuzzer_sys` + dicts (`fuzz/dict/*.dict` with `GGUF`, `role`, `content`, `finish_reason`).
14. **src/conversation.rs:830** — added `parse_message_line(&[u8])` for NDJSON fuzz.
15. **src/memory_monitor.rs:148** — M-1 hysteresis: `consecutive_high_slope = 0` after trigger + 60s cooldown `sleep` to prevent reset-storm (was tight loop every 5s).
16. **src/inference.rs:28,58** — marked `#[doc(hidden)] // pub for audit tests only` for `get_app_nonce`, `read_pid_file_with_nonce`, `is_llama_server_process`.
17. **tests/command_layer.rs** — 7 integration tests (A1, A3, C1-C3, C5, C6) via `spawn_mock_manual` + `wait_for_mock_ready` + `build_http_client()` (no GUI, closes Day-2 app-layer except shell-layer A7/A9/A10).
18. **tests/stress_concurrent_send.rs** — 3 tests: concurrent locked appends, repair vs append, CSP drift guard.
19. **scripts/run_matrix_ab.sh** — hardened to `PASS/FAIL` verdict aggregation, `trap`, `HARNESS_PASS` vs `FAIL`, final `exit 1` on any FAIL; now `PASS=11 FAILS=0` on current run.
20. **fuzz/** — `fuzz/Cargo.toml` with `[[bin]]` gguf_parse/ndjson_parse, `cargo +nightly fuzz build` in progress (Fuzz builds in `fuzz/target/`, not `target/`), `CARGO_TARGET_DIR=target-mutants` isolates `cargo mutants`.

## Open for Day 3-6 (Post Day-3 Kickoff)

- **Regression tier closed** (fail pre-fix): B4, A6, C6, gguf, R6 (5). **Drift-guard tier closed** (fail on regression): dir fsync, CSP script-src, B9, command-layer (4). **Counts tiered separately at Day-6 gate.**
- Still open: R5-font S2 (`fonts.googleapis.com` in CSP + HTML — S2 privacy, drift test will catch when removed), R9/B6 compound IO S1 (O(N) `load_messages_within_token_budget` + `std::fs` in async `commands.rs:892` + `conversation.rs:454`), R11 XSS (needs `innerHTML` sink walk + `unsafe-inline` review), R14 dual-instance (Linux-First freeze), R7/F-2 draft preservation (shell-layer A10), R10 `shellcheck`, A11 `slowready`/`bind-delay`, A7/A9/A10 shell-layer (`WEBKIT_DISABLE_COMPOSITING_MODE=1 xvfb-run -a -s "-screen 0 1280x800x24" cargo tauri dev` — `xvfb`/`dbus-x11` now installed, `cargo-fuzz`/`cargo-mutants` installed, `nightly` ready, fuzz `dict` seeded, `fuzz/target` isolated).
- Next overnight: `cargo +nightly fuzz run gguf_parse -- -max_total_time=86400 -timeout=25 -dict=fuzz/dict/gguf.dict &` + `ndjson_parse` + `CARGO_TARGET_DIR=target-mutants cargo mutants --file src/conversation.rs --file src/storage.rs --file src/gguf.rs &` (half-day saved via parallel background, zero `target/` contention).

---
*Evidence calibrated I1 (source) / I4 (execution via grep & cargo + mock) / I5 (tests). No I0 claims. Fuzz/mutants backgrounded, command-layer closes Day-2 app-layer, xvfb shell-layer is now afternoon scope (A7/A9/A10 only).*

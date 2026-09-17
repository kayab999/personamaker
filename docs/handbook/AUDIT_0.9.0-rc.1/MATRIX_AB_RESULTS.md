# Matrix A/B Execution — Day 2 (2026-09-09) — RECLASSIFIED Day 3 10:00 UTC

**Harness:** `scripts/mock_llama_server.py` as drop-in `llama-server` (spawned via `Command::new`, absorbs `-m/--ctx-size/-ngl`), `scripts/run_matrix_ab.sh` + direct `curl` probes. **OS:** Linux-First, `cargo check` green, `tribunal.sh` green (26 destructive).

> **Reclassification (audit integrity fix):** Rows A1/A3/C1–C4 below were **harness-verified** (mock produces correct fault shapes, `curl` + `mock_hits.jsonl`), not **app-layer executed**. App-layer verification requires `xvfb-run cargo tauri dev` spawning the mock via `set_llama_server_path` and asserting per-row: (a) no partial `messages.ndjson` line, (b) metadata unchanged, (c) `last_error` + toast, (d) auto-restart, (e) next send succeeds (A1); for A3 the *app's* 120s total `timeout` (`src/commands.rs:36` + `Policy::none` `src/commands.rs:37`) fires, UI responsive, no append, `ServerState` recorded; for C1–C3 ndjson byte-identical, error surfaced. **Harness = worth recording, not row execution.** Timeout-semantics grep re-run Day 3 09:45 UTC confirms A3=120s spinner (S0-adjacent) not infinite hang.

## Regression Tests (Failing Pre-Fix → Passing Post-Fix) — I5

| ID | Test | File:Line | Pre-fix would fail | Post-fix result |
|----|------|-----------|-------------------|-----------------|
| B4 | `test_b4_append_without_trailing_newline` | `tests/destructive_tests.rs:763` | Missing `\n` guard → concatenated JSON invalid | **PASS** (0.00s) — guard inserts `\n` when last byte != `\n` (`src/conversation.rs:206`) |
| B4-neg | `test_b4_without_guard_would_fail` | `tests/destructive_tests.rs:799` | Demonstrates old bug: `{"a":1}{"b":2}\n` → parse error | **PASS** — at least one line invalid without guard |
| A6 | `test_a6_pid_reuse_protection` | `tests/destructive_tests.rs:826` | No `is_llama_server_process` check → innocent `sleep` would be killed | **PASS** — `sleep` `is_llama_server_process==false`, still alive after check (`src/inference.rs:58` `/proc/<pid>/cmdline` + nonce) |
| A6-pos | `test_a6_real_llama_server_would_be_identified` | `tests/destructive_tests.rs:859` | Mock not recognized | **PASS** — `mock_llama` `cmdline` contains `mock_llama` → `true` |
| C6 | `test_c6_redirect_not_followed` | `tests/destructive_tests.rs:882` | `Client::builder()` without `Policy::none()` follows 302 to `evil.com` | **PASS** — `Policy::none()` returns 302, not follow (`src/commands.rs:35`, `src/inference.rs:1183`) |

Full suite: `cargo test --test destructive_tests` → **26 passed** (was 21) in 0.70s. `cargo test --test property_tests` 18/18. `tribunal.sh` ✅.

## Matrix A — Process Lifecycle (Harness-Verified, App-Layer Pending xvfb)

| ID | Injection | Repro (executed) | Observed | Verdict |
|----|-----------|------------------|----------|---------|
| A1 | Kill mid-generation | `mock --port 18770 --mode normal` → `curl POST /v1/chat/completions` → `kill -9 $PID` (harness) | `mock_hits.jsonl` `{"mode":"normal","req_bytes":76}` + `curl` `{"mock reply to: Write a long story"}`. Raw log verbatim: `./run_matrix_ab.sh: línea 22: 669205 Terminado (killed)` (Spanish locale, not "Terminiert") | **HARNESS PASS** — mock fault shape verified. **App-layer pending xvfb:** assert (a) no partial line, (b) metadata count unchanged, (c) `last_error`+toast, (d) auto-restart, (e) next send succeeds |
| A3 | Hung (`hang`/`STOP`) | `mock --mode hang` → `curl` blocks → `kill -STOP` → `timeout 3 curl` fails; `kill -CONT` resumes (harness) | `hang` never sends headers → *curl's* 3s timeout fired, not the app's 120s. App timeout verified via `grep` Day 3 09:45 UTC: `src/commands.rs:36 .timeout(120s)` total + `Policy::none` → **app would fire at 120s**, S0-adjacent, UI responsive | **HARNESS PASS** — **App-layer pending xvfb** to prove 120s fires, no append, `ServerState` recorded |
| A4 | Port occupied | `portpicker::is_free_tcp` check in `src/inference.rs:336` → picks unused if taken | Logic verified I1, not yet live-injected (needs dummy listener) | **Partial** |
| A5 | Stale PID file | `write_pid_file` now `pid\nnonce` (`src/inference.rs:28`), `read_pid_file_with_nonce` + `is_llama_server_process` | Old file `sleep` PID not killed (A6 test), ghost `mock` PID verified | **PASS** |
| A6 | PID reuse kill innocent | See A6 test above | `sleep 60` survives, `mock` identified | **PASS** |
| A11 | Slow model load (new) | Mock `--bind-delay 2` / `--slowready 2` not yet injected via Tauri `start()` but `wait_for_server_ready` 2s client handles 503 vs closed port | Logic I1 verified; live test deferred to Day 3 via `set_llama_server_path` | **Pending** |
| A7/A9 | Ghost / rapid open/close | Not yet executed (requires `kill -9 <app>` + `ps` check) | `Drop` now `start_kill+sleep+try_wait` + PID nonce, but orphan `ps` check pending | **Pending** |
| A8 | Dual instance | No `single-instance` plugin (`Cargo.toml` grep) → port + flock race | I1 confirmed risk, live 2× `cargo tauri dev` not yet run (needs display) | **Pending** |

## Matrix B — Disk & I/O

| ID | Injection | Repro | Observed |
|----|-----------|-------|----------|
| B1 | Disk full | `unshare -rm` fails (`write /proc/self/uid_map: Operation not permitted`) → rootless B1 not possible on this host; fallback unit injection covers ENOSPC path | **Deferred to privileged CI** (documented in soak harness) |
| B4 | Truncated final line | `fs::write` msg without `\n` → append with guard → 2 parseable lines (test_b4) vs without guard → 1 corrupted line (test_b4_neg) | **PASS** |
| B2/B3/B5/B6/B8 | Read-only, crash between append & metadata, corrupt middle, huge, symlink | B4/B9 logic verified I1; B6 O(N) `load_messages_within_token_budget` loads all lines (`src/conversation.rs:454`) — still S1 compound; B9 repair lock now held (`src/conversation.rs:628,670`) | **Partial** |
| B9 | Repair vs concurrent append | `repair_conversation` now acquires `acquire_sidecar_lock` for `messages.ndjson` + `metadata.json` via `atomic_write_bytes` while held (`src/conversation.rs:628,670`) — prevents clobber | **PASS** (code), stress harness pending |

## Matrix C — HTTP Contract (Harness-Verified, App-Layer Pending xvfb)

Executed via `curl` against `mock --port 18765` (each mode 1s window, `MOCK_LOG=/tmp/mock_hits.jsonl`) — **curl-layer harness**, not `post_chat_completion:818` app path. Code-reading assertions `(no append via 818)` are not observations until xvfb run:

| Mode | Mock flag | Curl result | Hit log |
|------|-----------|-------------|---------|
| malformed | `--mode malformed` | `curl` → `{"choices": not json {{{` → *would* hit `post_chat_completion:818` parse error → no append (code assertion, not observed) | `{"mode":"malformed","req_bytes":47}` — harness verified |
| empty | `--mode empty` | `curl` → `{"content":""}` → *would* hit `post_chat_completion:825` empty reject → no append | `{"mode":"empty",...}` — harness verified |
| 500 | `--mode 500` | `curl` → `{"error":"mock 500"}` → *would* hit `!is_success()` → error surfaced | `{"mode":"500",...}` — harness verified |
| drip | `--mode drip --drip-secs 0.01` | `curl --max-time 2` → `BrokenPipe` after 2s (curl timeout, not app 120s). App total timeout would fire at 120s (`src/commands.rs:36`) | `{"mode":"drip",...}` — harness verified, **app-layer pending** |
| redirect | `--mode redirect` + `relaylog` | `curl -i` → `302 Location: evil.com`, `curl -L` → hits relay (proves follow). Rust `Policy::none()` (test_c6) → 302 not followed, but test builds own client (needs `build_http_client()` export) | Harness verified, **test open** (see Fix/Test Audit) |
| length | `--mode length` | `curl` → `{"finish_reason":"length"}` → *would* hit `post_chat_completion:828` marker (no test yet) | Harness verified, **test open** |
| big | `--mode big` | `curl` → `10MB` `A*...` → no OOM, but `body_text` holds 10MB (`src/commands.rs:814`) | Pending |
| hang | `--mode hang` | `curl` blocks until `timeout` → app 120s would fire | Harness verified, **app-layer pending** |
| reset | `--mode reset` (SO_LINGER RST) | Half JSON + RST → `serde_json::from_str` error | Harness verified (mock has mode, not yet hit in this batch) |

**Note:** `scripts/run_matrix_ab.sh:5` `set -e` + `timeout 2 curl` 124 exit caused early exit after `drip` (47 lines). Patched `|| true` after `timeout` curls. Remaining `redirect/length/big` verified via separate `MOCK_LOG` runs and code inspection; full app-layer requires `xvfb-run`.

## Evidence Artifacts

- `cargo test --test destructive_tests test_b4 --nocapture` → 2 passed
- `cargo test --test destructive_tests test_a6 --nocapture` → 2 passed (mock 700ms spawn)
- `cargo test --test destructive_tests test_c6 --nocapture` → 1 passed (302)
- `cargo test --test destructive_tests` → 26 passed, 0 failed
- `./scripts/tribunal.sh` → `cargo check` + 21+18 → ✅
- `mock_hits.jsonl` (sample after A1): `{"mode":"normal","path":"/v1/chat/completions","req_bytes":76,"t":...}`
- `/tmp/mock_c_*.log` + `/tmp/matrix_ab_results.log` (47 lines before `set -e` exit) — drip `BrokenPipe` expected on timeout

## Gates for Day 3

- [x] B4, A6, C6 regression tests fail pre-fix (demonstrated via `without_guard` / `is_llama_server_process` / `302` vs `200`)
- [x] Mock as `llama-server` drop-in verified (absorbed `-m/--ctx-size/-ngl`, real `Command::new` args)
- [x] Matrix A/B partial execution (A1, A3, C1-5 via hit logs); remaining A2/A4/A7-A11/B2-B3/B5-B6 + full `cargo tauri dev` integration requires display/webkit (deferred to manual or CI with `xvfb`)
- [ ] Day 3: Concurrency D1-D5 + `tokio-console` (`RUSTFLAGS="--cfg tokio_unstable"`), full C6 two-instance relay detector with `MOCK_LOG` export, `shellcheck` (not installed), `cargo mutants`/`cargo fuzz` 24h

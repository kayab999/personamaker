# LocalPersona `0.9.0-rc.1` — Enterprise Technical Audit Plan

I've read through your architecture dump, README, voice setup, and vision docs. Since I can't execute against the repo directly, this plan is built to be **run by you (or an agent) against the code** — every item names the module, the injection method, and the pass/fail criterion. It's organized around one core idea:

> **Your docs make ~40 verifiable claims. An enterprise audit converts each claim into evidence.** Your own §11.2 "Failure containment" table is effectively a spec — this audit adversarially executes every row of it.

---

## 0. Audit Charter

| Item | Definition |
|---|---|
| **Objectives** | Robustness, non-happy-path stability, resource hygiene, data integrity (fail-closed contract), security posture, release engineering |
| **In scope** | `src/*` (all 12 modules), `frontend/script.js`, `scripts/*.sh`, `tauri.conf.json`, `capabilities/`, `.deb` artifact, data-dir layouts |
| **Out of scope** | Product/UX design, AR vision, streaming (frozen), AppImage tooling beyond diagnosis |
| **Audit principle** | Every finding must have: repro command, evidence, severity, blast-radius classification |

**Severity classes:**

- **S0 — Blocker:** data loss/corruption, unauthenticated network exposure, unrecoverable hang, fail-open on the blast-radius contract
- **S1 — Critical:** crash, unbounded resource leak, zombie/orphan processes, contract near-miss with silent degradation
- **S2 — Major:** misleading errors, recoverable degradation, test coverage gaps on core invariants
- **S3 — Minor:** docs/code mismatch, polish

**Exit criteria for "beta declared":** zero open S0/S1; all S2 owned with dates; fault-injection matrix (§3) executed 100% with expected behavior; automated soak harness green ≥ 8h; fuzz targets clean ≥ 24h core-parser-hours.

---

## 1. Pre-Identified Risk Candidates (from your own docs)

Before touching the code, these are **suspicion anchors** found by cross-referencing your documentation. Each becomes a priority audit item:

| # | Finding candidate | Source | Why it matters |
|---|---|---|---|
| R1 | **`VOICE_SETUP.md` instructs `--host 0.0.0.0`** | Voice doc | Directly contradicts your localhost-only security principle. If the *app* ever spawns with `0.0.0.0`, any LAN device gets an unauthenticated OpenAI-compatible endpoint. Verify what `inference.rs` actually passes; fix the doc regardless. |
| R2 | **HTTP client timeout existence unverified** | §11 flow | If `reqwest` isn't configured with connect/read timeouts, a hung llama-server = infinite spinner. There is no streaming, so a stuck 5-minute generation has no abort path either. |
| R3 | **`circuit_breaker.rs` and `autopsy.rs` are "scaffolding"** | §4 module map | 465 LOC of possibly dead code creates *false confidence* in reliability claims. Verify they're wired into hot paths or remove them from the architecture claims. |
| R4 | **`Drop` → `start_kill` in async context** | §6.3 | Blocking kills inside a `Drop` that runs during tokio runtime shutdown is a classic deadlock/leak source. Also: PID reuse — does the kill path verify process identity before `kill(pid)`? |
| R5 | **CDN Tailwind/fonts** | §12.8 | "Offline by design" product phones home for styling. Privacy principle violation + degraded UI offline. |
| R6 | **Truncation silently accepted?** | §4 pipeline step 5 | Contract rejects *empty* content. But `finish_reason: "length"` (max_tokens hit) produces a non-empty, cut-off message. Is that surfaced to the user or silently appended? |
| R7 | **User message lost on inference failure** | §4 pipeline | User + assistant are appended *after* a successful response. On failure: no append (correct blast radius) — but does the frontend preserve the draft, or does the user retype? |
| R8 | **Partial NDJSON line + next append** | §5.1 | Crash mid-write leaves a half-line. `load` skips it — but does the *next append* prefix a `\n` if the file doesn't end with one? If not: two corrupt lines, and repair may destroy both. Byte-level check required. |
| R9 | **Images excluded from token budget math** | §4 step 2 | Budget = ctx × 0.82 over text history only. Vision tokens aren't counted → context overflow → llama-server 400 on image-heavy chats. |
| R10 | **AppImage failure on space-paths** | §12.4 | Smells like shell quoting in build scripts. `shellcheck scripts/*.sh`. |
| R11 | **`innerHTML` in a 3.3k-LOC untyped JS file** | §7 | Model output is attacker-controllable (prompt injection via imported character files). Stored XSS in the WebView with IPC access is your highest-severity security surface. |
| R12 | **fsync of parent dir after rename** | §5.4 | `atomic_write` does temp+rename+fsync(file) presumably — but POSIX crash-durability of rename requires fsync of the *directory*. Verify. |
| R13 | **Suspend pauses the 45-min Arena timer** | §6.2 | If uptime uses `Instant` (CLOCK_MONOTONIC), laptop lid-close freezes the timer — a 3-day-old server never resets. Acceptable, but decide and document. |
| R14 | **Dual-instance launch** | §6.3 | Two app instances → port conflict + two writers on app data. What actually happens? |

These are hypotheses, not findings — the matrix below verifies each.

---

## 2. Phase 0 — Claims-to-Evidence Reconciliation (≈1 day)

Go through `ARCHITECTURE_AND_STATUS.md` and mark every assertion as **Verified / Partially / False**. Build the traceability table:

| Doc claim | Evidence to collect | Verifier |
|---|---|---|
| "Atomic writes, locks, PID hygiene" (Phase 0) | Code review of `atomic_write` (rename + fsync + dir fsync?), lock scope | §4.2, §5 checklist |
| "No silent corruption, no empty assistant rows" | Fault matrix B, C rows | §3 |
| "Worker dead → error string → toast; no append" | A1, A3 injections | §3 |
| "RAG fail → chat continues" | Corrupt `chunks.json`, remove embedding model | §3 F-row |
| "Arena background stop/start; reason stored" | Force triggers; inspect Diagnostics + logs | §6 |
| "validate_localhost_endpoint" | Grep all configurable URLs — is TTS endpoint, embeddings, any character-imported URL covered? | §5 |
| "Path IDs: ASCII + no ../, no slashes" | Property test review + fuzz the validator | §7 |
| Golden IPC "no drift" | Run `test_ipc_schema_has_no_drift` + cross-check FE calls against schema | §6 |
| "Window close handler stops both managers" | K-series injections (close during in-flight work) | §3 |

**Deliverable:** claims register with per-claim verdict. Anything unverified becomes a matrix row.

---

## 3. Fault-Injection Matrix (the core execution)

Every row: **inject → observe → verdict**. Expected behavior is always your §11.2 contract: *fail closed, error surfaced, no partial data, app remains usable.*

### A — Process lifecycle (maps to `inference.rs`, `memory_monitor.rs`)

| ID | Scenario | Injection | Expected | Failure = |
|---|---|---|---|---|
| A1 | Server dies mid-generation | `kill -9 $(pgrep -f llama-server)` while a long generation runs | Health detects, toast, no append, server restarts, next send works | S0 (data) / S1 |
| A2 | Server dies mid-*startup* (model load) | Kill during `Starting` state | No state-machine deadlock; retry permitted; UI informed | S1 |
| A3 | Server hung, not dead | `kill -STOP <pid>` during request | **Request timeout fires** (R2!), error toast, app responsive. Infinite spinner = **S0** | S0 |
| A4 | Port occupied | Pre-start a dummy listener on the target port | Clean startup error, no busy-loop, user can change config | S2 |
| A5 | Stale PID file | Delete server process, keep PID file, restart app | Detected, cleaned, fresh start | S2 |
| A6 | **PID reuse kills innocent process** | Edit PID file → point at sacrificial `sleep 9999`; close app | `sleep` survives (kill verifies cmdline/name) — if it dies: **S1** | S1 |
| A7 | App SIGKILLed → ghost servers | `kill -9 <app pid>`; inspect processes | Orphan-detection on next start (adopt or kill by recorded PID+identity). Ghost llama-server = S1 | S1 |
| A8 | Two instances launched | Launch twice rapidly | Second instance either refuses, or coexists safely (distinct ports, shared file locks hold) | S1 |
| A9 | Rapid open/close ×50 | Script loop | No fd growth, no zombies (`ps` check), no temp/PID file accumulation | S1 |
| A10 | Close during in-flight request | Send message, close window immediately | Close handler completes or aborts gracefully; conversation on disk is consistent | S0/S1 |

### B — Disk & I/O (maps to `storage.rs`, `conversation.rs`)

| ID | Scenario | Injection | Expected | Failure = |
|---|---|---|---|---|
| B1 | Disk full | Run app with data dir on small tmpfs: `mount -t tmpfs -o size=50M tmpfs /tmp/tiny && XDG_DATA_HOME=/tmp/tiny cargo tauri dev`; then fill | Clear ENOSPC error; **no partial NDJSON line**; no temp-file litter | S0 |
| B2 | Data dir read-only mid-session | `chmod -R a-w` on app data mid-run | atomic_write fails cleanly, no orphan temp files | S1 |
| B3 | Crash between message append & metadata update | Instrument or SIGKILL at the window; reload | `repair_conversation` reconciles count; message present | S0/S1 |
| B4 | **Truncated final line (R8)** | `f=$(...)/messages.ndjson; truncate -s $(( $(stat -c%s $f) - 1 )) $f`; then send a message; `tail -c 200 $f \| xxd` | New line starts after `\n`; repair truncates the bad line. Concatenation onto the half-line = **S0** | S0 |
| B5 | Corrupt middle lines | Random byte flips / invalid JSON lines | Skip+warn; pagination & budget walk unaffected; count drift detected | S1 |
| B6 | Huge conversation | Generate 50MB / 100k-line NDJSON | Load bounded, UI responsive, budget walk not O(all) per send | S2 |
| B7 | Paths with spaces/unicode | Model path `/tmp/my models/qwen.gguf`; data dir with spaces | argv passed via `Command` (safe) — verify no shell-string assembly; this likely also explains R10 | S1 |
| B8 | Symlink swap on data subdirs | Swap `characters/` → `/etc` symlink mid-run | Path-safety validators hold (absolute-under-base check) | S1 |

### C — HTTP contract (maps to `inference.rs`, golden schema)

| ID | Scenario | Injection | Expected | Failure = |
|---|---|---|---|---|
| C1 | 200 + malformed JSON | Mock server (tiny `socat`/Python) returning garbage | Parse error → no append (your claim — verify) | S0 |
| C2 | 200 + empty / whitespace-only content | Mock | Explicit rejection (your claim) — also test whitespace-only and `"content": null` | S0 |
| C3 | 500 / 503 / connection reset mid-body | Mock | Error surfaced, no append, retry possible | S1 |
| C4 | Slowloris / drip response | Mock with delayed body | **Timeout fires** (pairs with A3 — R2) | S0 |
| C5 | `finish_reason: "length"` | Mock with max_tokens-truncated content | **User-visible truncation marker** (R6). Silent append = S2 | S2 |
| C6 | 302 redirect off-localhost | Mock returning redirect to external host | Redirect policy: disabled or re-validated. `reqwest` follows redirects by default — **explicitly verify** this can't bypass `validate_localhost_endpoint` | **S0 (security)** |
| C7 | 10MB content response | Mock | No UI freeze/OOM; size cap or chunked handling | S2 |

### D — Race conditions (code review + stress, see §4)

| ID | Scenario | Method | Expected |
|---|---|---|---|
| D1 | Arena reset while request in-flight | Force threshold mid-generation | Request fails gracefully; state machine never enters invalid combo (Restarting×Starting) |
| D2 | Send while send in-flight | Hammer send button / scripted | Button guarded or queued; NDJSON ordering: user→assistant pairs never interleave |
| D3 | Switch character/conversation mid-generation | UI automation | Response lands in the **origin** conversation (R0 identity under race) |
| D4 | Regenerate + send concurrently | UI automation | No double-append, no lock inversion |
| D5 | Memory-pressure reset during model load | Artificially trip monitor during `Starting` | Rejected or queued; no double-spawn of llama-server |

### E — RAG / embeddings failure containment (your §11.2 claim)

| ID | Scenario | Injection | Expected |
|---|---|---|---|
| E1 | Corrupt `chunks.json` | Hand-edit to invalid JSON | Chat continues, diagnostics flags RAG |
| E2 | Embedding model missing/unloadable | Rename model file | First-query failure degrades gracefully; cold-start path doesn't block sends |
| E3 | Adversarial knowledge doc | 1MB single line; 0-byte; binary garbage; 10k tiny chunks | Chunker bounded; no OOM; size caps enforced |

---

## 4. Concurrency & Async-Correctness Review (static, `src/`)

Targeted review pass over the exact hot paths. Checklist:

1. **Lock scope across `.await`** — grep for `.lock()` / `.read()` / `.write()` followed by `.await` while held. std sync primitives held across await = runtime stall; tokio primitives held across long awaits = contention/deadlock candidates. Check `commands.rs` → `conversation.rs` → `inference.rs` call chain specifically.
2. **`ServerState` transition guard** — who mutates state (request path, Arena task, health checker, Drop, window-close handler)? One mutex? Can `Stopping` race with `Starting`?
3. **`Drop` impls calling blocking kill** — `start_kill` in Drop: does it run on the tokio runtime thread? During runtime shutdown? Use `tokio::task::block_in_place` or spawn a dedicated thread for shutdown kills. Also: zombie reaping — after `kill`, is `waitpid`/`try_wait` guaranteed to run, or do children zombie until app exit?
4. **Process-group kill** — spawn children via `pre_exec` `setpgid(0,0)` and kill the group, not just the PID (belt-and-braces vs A6/A7).
5. **`std::fs`/`fsync` in async fns** — must be `spawn_blocking`, else you stall the runtime on every message append (your fsync-per-append design makes this a *hot-path* stall).
6. **Lock ordering** — metadata lock vs messages lock vs manager-state lock: extract the acquisition graph, assert no cycles (manual or via a lock-order lint pass).
7. **Tooling:** run under [`tokio-console`](https://github.com/tokio-rs/console) during a stress session — it directly surfaces task/lock stalls.

**Stress harness (turn D2–D4 into automation):** a Rust integration test that drives `send_message_with_images` concurrently ×N against one conversation, plus interleaved `regenerate`, asserting: every user message has ≤1 matching assistant append, no interleaved NDJSON lines, conversation parses fully afterward.

---

## 5. Security Review (local app ≠ no threat model)

Your threat model: *imported character files and model output are untrusted input; the machine may be multi-user; the LAN exists.*

| ID | Check | Method |
|---|---|---|
| S-1 | **Server bind address** (R1) | `grep -n "host" src/inference.rs` — what does the app pass to `llama-server`? App must bind `127.0.0.1`. Fix `VOICE_SETUP.md` example (`0.0.0.0` → `127.0.0.1`). |
| S-2 | **XSS via model output** (R11) | `grep -n "innerHTML\|insertAdjacentHTML\|document.write" frontend/script.js`; trace every sink: does character name, message text, knowledge preview, or diagnostics string reach an HTML sink unescaped? A malicious imported persona can emit `<img onerror=...>` through the model. **This is the #1 security item.** |
| S-3 | **Tauri hardening** | Review `tauri.conf.json` CSP (is it set at all? default is often permissive), `capabilities/` — least privilege per window, no wildcard plugin grants. |
| S-4 | **Endpoint validation coverage** | Enumerate every user-configurable URL (LLM endpoint, TTS endpoint, anything in imported characters). All through `validate_localhost_endpoint`? + C6 redirect check. |
| S-5 | **Path validators under fuzz** | Property tests exist for ID validation — extend to: unicode homoglyphs, `..`, `a/../b`, `\` (Windows-style on Linux), NUL bytes, empty, 4096-char IDs, `.` and `..` exact. |
| S-6 | **Binary validation on import** | Avatar/voice uploads: is content actually decoded as image/audio (magic bytes / decoder), or just renamed files? Decompression bombs: are image *dimensions* capped before decode (a 10000×10000 PNG = 400MB RGBA)? |
| S-7 | **GGUF parser fuzz** (230 LOC hand parser = fuzz target) | `cargo fuzz` target: malformed headers, absurd field counts (alloc-size attacks → OOM). |
| S-8 | **Network egress assertion** (R5 + privacy) | Run with `strace -f -e trace=connect` (or a sink proxy); assert connections are only: localhost endpoints + WebView's known CDN. For "no telemetry" claim this is your *evidence*. Document the CDN exception or vendor the assets. |
| S-9 | **Log hygiene** | Do logs contain message content? Privacy principle violation if so. Grep log calls in `commands.rs`/`conversation.rs`. |
| S-10 | Temp PID files | Symlink pre-creation in world-writable tmp (multi-user machines); file perms. |

---

## 6. Frontend Audit (`script.js`, 3.3k LOC)

| ID | Check | Method |
|---|---|---|
| F-1 | Unhandled rejections | DevTools: `window.addEventListener('unhandledrejection')` during full session; every `callTauri` rejection → toast (claimed) |
| F-2 | **Draft preservation on failure** (R7) | Kill server mid-send (A1); is the user's typed message still in the input box? |
| F-3 | DOM growth | 2000-message session: heap snapshot + node count. Are offscreen messages virtualized/removed? |
| F-4 | Listener leaks | Open/close panels, switch characters ×100; listener count stable? |
| F-5 | Global state races (G3) | `currentConversationId` mutation during any `await` — enumerate every read point |
| F-6 | Offline UI (R5) | Block network (devtools offline / firewall): app must remain *usable*, not error-looping |
| F-7 | IPC vs golden schema | Script that extracts every `invoke("...")` call name + args from `script.js` and diffs against `gen/schemas/localpersona-ipc-schemas.json` — your drift test covers the Rust side; do the same for the JS side |
| F-8 | Long-op re-entrancy | Send/regenerate button states during in-flight; double-click behavior |

---

## 7. Test Engineering Uplift (audit of the Tribunal itself)

Current: 21 destructive + 18 property + 31 unit. Audit for **invariant coverage**, not count:

1. **Coverage matrix:** invariant (from §4/§5/§11 of your docs) × test that proves it. Gaps I already suspect have no test: crash-mid-append (B3), kill-server-mid-request (A1), PID reuse (A6), redirect policy (C6), truncation marker (C5), fd/memory over time (F-3).
2. **Mutation testing** — measures whether your tests actually *kill* mutants in core modules:
   ```bash
   cargo install cargo-mutants
   cargo mutants --in-place src/conversation.rs src/storage.rs src/inference.rs
   ```
   Enterprise-grade signal: if flipping `fsync` off or inverting a lock guard isn't caught, the suite is decorative.
3. **Fuzz targets:** `gguf::parse`, NDJSON line parser, character import JSON, IPC payload decode. Run 24h in CI nightly.
4. **CI parity & port hygiene:** do tests that bind ports use ephemeral allocation? Parallel runs collide?
5. **Static battery:**
   ```bash
   cargo clippy --all-targets -- -D warnings
   cargo audit && cargo deny check && cargo machete
   shellcheck scripts/*.sh        # likely explains the AppImage/space-path failure (R10)
   ```

---

## 8. Packaging & Release Verification

```bash
lintian --pedantic --info dist/0.9.0-rc.1/*.deb
desktop-file-validate <extracted .desktop>
```

| ID | Check |
|---|---|
| P-1 | `dpkg -i` **upgrade** over an existing install with user data → data preserved |
| P-2 | `dpkg -r` (remove) **must not** touch `~/.local/share/com.localpersona.studio`; document purge semantics |
| P-3 | Remove while app running → processes, `.desktop`, autostart all cleaned; no ghost llama-server (pairs with A7) |
| P-4 | Fresh-machine matrix: Ubuntu 22.04, 24.04, Debian 12 (webkit2gtk-4.1 availability) |
| P-5 | First-run with zero models/characters: BYO guidance, no dead-end errors |
| P-6 | `CHECKSUMS.txt`: sha256s verified; add minisign/gpg signature before public beta (unsigned RC is acceptable, must be stated) |
| P-7 | StartupWMClass in `.desktop` matches window → correct taskbar grouping |

---

## 9. Automated Soak Harness (replaces the manual 2–4h human soak)

Your §13 lists the soak as "P0 human." Convert it to a harness that produces a report — the human then reviews the *report*, not the 4 hours:

**Spec:** loop for N hours: random send (payload sizes 10B–50KB, images p=0.2), p=0.05 kill llama-server (A1), p=0.02 SIGSTOP 60s (A3), p=0.01 arena-force, every 60s sample RSS/VMS, fd count, disk delta, temp-file count. **Pass:** monotonic-ish RSS (bounded growth), fd count plateau, zero ghost processes at end, every conversation post-soak passes full NDJSON parse + `repair_conversation` is a no-op (i.e., nothing needed repair).

```bash
watch -n 60 'echo "$(date +%T) rss=$(ps -o rss= -p $(pgrep -f localpersona)) \
  fds=$(ls /proc/$(pgrep -f localpersona)/fd | wc -l) \
  disk=$(du -s ~/.local/share/com.localpersona.studio)"' >> soak.log
```

For memory profiling on the failure cases: `heaptrack` the app during the soak.

---

## 10. Execution Order & Effort

| Day | Work | Output |
|---|---|---|
| 1 | Phase 0 claims reconciliation + §7 static battery + R1/R2/R12 greps | Claims register, quick wins |
| 2 | Fault matrix A + B (process + disk) | Highest-severity findings expected here |
| 3 | Fault matrix C + D stress + §4 concurrency review | |
| 4 | §5 security (S-1/S-2 priority) + §6 frontend | |
| 5 | §8 packaging + soak harness setup → overnight 8h soak | |
| 6 | Report: findings register + traceability matrix + fix plan | RC gate decision |

**Deliverable format per finding:** `ID | Severity | Module | Repro command | Evidence | Expected vs Actual | Suggested fix | Owner`. Keep it in `docs/handbook/AUDIT_0.9.0-rc.1/` — consistent with your documentation-governance rule (it becomes historical once remediated, exactly like your v7.2 audit).

---

### Where I'd bet the real bugs are, ranked

1. **R2 (no/generous HTTP timeout)** — combined with frozen streaming, a hung server is an unrecoverable UI. Cheapest S0 to test (A3/C4), most likely to exist.
2. **R8 (partial-line append concatenation)** — a genuine data-corruption path that "skip+warn on load" masks until the next write.
3. **R11 (XSS via model output)** — imported personas are untrusted; WebView XSS + IPC = the worst case for a local-first product's trust story.
4. **R6 (silent truncation)** — violates the spirit of your fail-closed contract even though the letter ("reject empty") passes.
5. **R4 (Drop/PID hygiene)** — zombie/ghost-server class bugs that only your A7/A9/A10 injections will catch.

If you want, I can draft any single piece in executable detail next — e.g., the mock llama-server for the C-series (a ~50-line Python server that emits malformed/truncated/redirect responses on demand), the concurrency stress test, or the soak harness as a full script.

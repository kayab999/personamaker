# Findings Register (seed) — Companion audit, no-GPU slice

All rows carry verified line-level evidence. Status `V` = verified by code
reading; `P` = pending behavioral confirmation in this slice.

| ID | Sev | Ref | Module | Evidence | Expected vs Actual | Fix | St |
|---|---|---|---|---|---|---|---|
| F-001 | S1 | CTX-1+2 | inference.rs:379-381, 438-439; commands.rs:597-604; script.js:2969,3324,3849-3852 | Budget from GGUF×0.55 (crossover ≥14,897) vs server hardcoded 8192 | Budget ≤ server ctx vs 18k–70k packed on ≥32k llama/qwen models | One ctx source (expose in Settings); reserve-based budget | V |
| F-002 | S1 | CTX-3 | commands.rs:880-881→932-939; conversation.rs:157-163 | No reserves for system/RAG/turn/max_tokens/template; byte/4 estimator undercounts CJK ≥25% structurally | Sum(prompt+completion) ≤ ctx vs 9,548 > 8,192 on fallback path | Reserve-based budget; usage.prompt_tokens recalibration | V |
| F-004 | S1 | T-4 | commands.rs:36-39 | No `.no_proxy()`; reqwest honors HTTP(S)_PROXY env | Localhost never proxied vs total inference failure + prompt egress on corporate proxy machines | `.no_proxy()` (+ connect timeout ≤10s) | V |
| F-006 | S1 | T-9 | export/import path | JSON-only round-trip; avatar/voice-sample/knowledge binaries lost | Lossless round-trip vs silent binary loss on migration | Bundle binaries in export archive or explicit loss warning | V |
| F-003 | S2 | T-5 | inference.rs:1283-1286 | stdout reader breaks at READY; post-ready stdout undrained → 64KB pipe fill → child blocks mid-generation | Drain for process lifetime vs hard hang (narrow trigger: verbose builds) | Don't break; daemon drain thread. Fix now (3 lines) | V |
| F-005 | S2 | T-14 | — | No single-instance guard | Refuse/focus second launch vs port conflict + split-brain UI (fs2 locks mitigate data corruption) | `tauri-plugin-single-instance` | V |
| F-007 | S2 | CTX-5 | commands.rs:458-466, :729 | max_tokens clamp 16..=32768, never vs ctx | Clamp ≤ ctx−prompt vs deterministic 400 at high settings | ctx-bound clamp | V |
| F-008 | S2 | CTX-4 | gguf.rs:249 | A/B RESOLVED 2026-09-17 (empirical, `cargo test --test gguf_roster_probe`): allowlist is exactly `"llama.context_length" \| "qwen2.context_length" \| "qwen3.context_length"` (Uint32/Uint64 only). Roster: `llama=131072` READ → budget 72,089; `gemma3=32768` NOT read → 4500; `qwen35=262144` NOT read → 4500. Pass-2 "all fallback" claim was wrong (typeless python dump); pass-1 arm reading was right but incomplete (missed that gemma3/qwen35 keys differ). | Budget from model metadata vs 1/3 models live-overflow, 2/3 accidental fallback | Generic `{arch}.context_length` read (arch from `general.architecture`) — **gated on F-001, see F-019** | V |
| F-009 | S2 | T-1 | rag.rs:88-89 | fastembed downloads AllMiniLM from HF at first RAG use (fails closed) | Offline-first: vendored or opt-in vs surprise network dependency | Vendor model in resources, or opt-in with offline-aware message | V |
| F-010 | S2 | T-6 | inference.rs (readiness) | Fixed 15s/10s readiness, no model-size scaling | Big-model CPU load succeeds vs false `Error` state | Scale timeout by param count (GGUF parser already provides it) | V |
| F-011 | S2 | T-16 | — | No llama-server version detection / documented minimum | BYO-binary contract: detect + warn below min vs silent degradation | `--version` probe at spawn; min-version in INSTALL.md | V |
| F-012 | S2 | A3/C4 | commands.rs:36-39 | Single 120s timeout, no connect timeout | ≤30s connect / ≤60s total vs 2-min frozen UI on hang | Split timeouts; shorten | V |
| F-013 | S2 | QA-3 | commands.rs:458-466 | Sampling params silently clamped 16..=32768 | Explicit clamp feedback vs silent mutation of user settings | Surface clamped values in Settings UI | V |
| F-014 | S2 | Gov | scripts/tribunal.sh | Gate runs check+destructive+property only; command_layer/stress are CI-only | Commit gate = full battery (or documented fast/full split) vs partial gate | Add to tribunal.sh or split fast/full | V |
| F-015 | S3 | Docs | ARCHITECTURE_AND_STATUS.md:194; storage.rs:69-91; script.js:2674-2679 | 0.82/8192 claim false; "stale-lock 30s" claim false (design rejects mtime); "16 tests" UI claim vs 58 real | Docs = ground truth | Apply CTX_FINDINGS §1.3 patch; remove test-count from UI; fix lock claim | V |
| F-016 | S3 | QA-7 | script.js vs conversation.rs:157-163 | JS `.length` (UTF-16 units) vs Rust byte-len token divergence | Pill within ±15% of real | Unify estimator; gate with usage.prompt_tokens | V |
| F-017 | S3 | Hygiene | test-models/ | Roster drift: 2 GGUF + incomplete `.crdownload` → 3 GGUF / 5.5G (+L3.2-Rogue-7B-Q3_k_m, 2026-09-17). No manifest → Tier-2 irreproducible. `qwen35.*` arch string nonstandard (Qwen3 normally emits `qwen3.*`) — verify conversion source before QA-4 use. | Pinned roster + manifest vs drifting ad-hoc dir | MODEL_MANIFEST.md + roster probe test (`gguf_roster_probe`); manifest updates ride in the same change as roster changes | V |
| F-018 | S3 | T-2 | conversation.rs:749-828 | Regen = atomic rewrite + .bak (no tombstones) — *expected pass* | ×5 regen + reload → exactly one assistant; walk counts live only | — (test to confirm) | P |
| F-019 | S1 (live trigger on roster) / constraint | CTX coupling | commands.rs:597-604 ⇄ gguf.rs:249 | PARTIAL compensating pair (corrected 2026-09-17): parser gap shields gemma3/qwen35 at 4500, but L3.2-Rogue already parses → budget 72,089 vs server 8192 TODAY. Parser extension alone would add 18,022/144,179 for the other two. Witnesses feed synthetic ctx — pin arithmetic, not wiring. Guard: `tests/gguf_roster_probe.rs::pinned_ab_verdict` (green = trigger pinned). | Fix order F-001→F-008 (or atomic) + wiring test vs unconstrained "easy parser fix" | Ordering constraint; post-F-001 integration test `budget(gguf_file) ≤ server_ctx` for full roster | V |
| F-020 | S2 | Gov | .git/ (hooks only, no repo) | No git repo → pre-commit Tribunal hook inert, CI has no remote, every Tribunal run manual. ARCHITECTURE_AND_STATUS §13 P1 "restore git history" is actually "create git history". | Tagged, diffable, hook-enforced tree vs untracked workspace | `git init` + baseline commit (incl. audit artifacts) + tag `audit-baseline-0.9.0-rc.1` **before behavioral session** | V |
| F-021 | S3 | Hygiene | build | 26 preexisting rustc warnings (dead-code); `cargo clippy --all-targets` 2026-09-17: 71 warnings, dominated by dead-code/never-read, plus actionable lints (`map().flatten()`, needless `u64→u64` casts ×2, `sort_by_key`, needless `Ok`+`?`) | Zero-warning baseline vs slow rot | Triage actionable clippy lints during behavioral session (5 min) | V |

**Claims that PASSED verification** (for the claims register — audit honesty cuts
both ways): redirect `Policy::none` ✓ (verify behaviorally), 120s timeout exists
✓, mmproj supported (`:403-405`) ✓, `finish_reason:"length"` post-hoc marker
exists ✓, RAG fail-closed on missing embedding model ✓, regen atomic-with-backup
design ✓, raw-JSON tap adaptation (no struct churn, `data["choices"][0]…` +
`data["usage"]`) compiles + `cargo check` green ✓ (app-level 1-line capture
pending `cargo tauri dev`).

### Blast table v3 — 2026-09-17 ground truth (supersedes v2)

Server ctx: 8192 (frontend hardcode). Budget: max(1024, gguf×0.55), fallback 4500.
Crossover: any GGUF ctx ≥ 14,895 breaks the server (0.55·c > 8192).
Empirical source: `cargo test --test gguf_roster_probe -- --nocapture`.

| Roster model | ctx key in file | Read by gguf.rs:249? | Budget today | vs 8192 | After F-008 parser fix WITHOUT F-001 |
|---|---|---|---|---|---|
| Gemma-3-1B Q5_k_m (812M) | gemma3.context_length = 32768 | No | 4500 | under-use; F-002 overflow still reachable (4500+1200+1500+2048+300 ≈ 9.5k) | 18,022 → deterministic overflow |
| L3.2-Rogue-7B Q3_k_m (3.5G) | llama.context_length = 131072 | **YES (live)** | **72,089** | **OVERFLOW — F-001 trigger on current roster** | stays 72,089 (already broken) |
| Qwen3.8-2B Q4_K_M (1.2G) | qwen35.context_length = 262144 | No (`qwen35` ≠ `qwen3`; arch string nonstandard) | 4500 | under-use; same F-002 path | 144,179 → catastrophic |

Exact allowlist (`src/gguf.rs:249`):

```rust
"llama.context_length" | "qwen2.context_length" | "qwen3.context_length" => {
```

The register story for the CTX cluster: **one defect already fires live
(L3.2-Rogue), a second defect accidentally shields the other two; fix in F-001 →
F-008 order or convert an accidental safety into a deterministic outage.**

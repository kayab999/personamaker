# CTX Findings — Context-Overflow Triangle (Phase-1 deliverable)

## 1.1 CTX-1..5, answered (code evidence)

**CTX-1 — VERIFIED, defect.** `--ctx-size` is emitted only from
`ServerStartRequest.ctx_size` (`src/inference.rs:379-381`), which the frontend
hardcodes to 8192 (`frontend/script.js:2969, 3324, 3849-3852`). GGUF metadata is
read post-spawn for budget math only (`src/inference.rs:438-439`).
**The server always runs at 8192 regardless of model; the user has no UI control
over it.**

**CTX-2 — VERIFIED, defect (merged with CTX-1 as F-001).** Budget =
`max(1024, gguf_ctx × 0.55)`, fallback 4500 (`src/commands.rs:597-604`) —
derived from a *different source* than the server's actual ctx. Crossover at gguf
ctx ≥ 14,895 (= ⌈8192/0.55⌉; float truncation in code shifts the observable edge
to 14,897 — pinned by `witness_ctx12_exact_crossover`) → any llama/qwen2/qwen3 model ≥ 32k packs up to 18k–70k history
tokens against an 8192 server → llama-server 400 (or silent truncation on some
builds) on every send past a history depth. Docs claim ×0.82 / 8192
(`docs/handbook/ARCHITECTURE_AND_STATUS.md:194`) — both stale; no 0.82 exists
anywhere in `src/`.

**CTX-3 — VERIFIED, defect.** History is loaded before system/RAG are composed
(`src/commands.rs:880-881 → 932-939`); current turn, max_tokens, and per-message
template overhead are never reserved. Estimator is byte-based `(len+3)/4`
(`src/conversation.rs:157-163`), which structurally undercounts CJK ≥ 25%
(3 bytes/char → 0.75 est/char vs ≥ 1.0 real). Even the "safe" fallback path
overflows: 4500 + system 1200 + RAG 1500 + max_tokens 2048 + turn 300 =
**9,548 > 8,192**.

**CTX-4 — VERIFIED, partial-defect.** Fallback is 4500, not the documented 8192
(`src/commands.rs:604`); UI shows 8192 display-only (`:1288`). Parser covers only
llama/qwen2/qwen3 `context_length` (`src/gguf.rs:249`) — all other families
silently take the 4500 fallback (accidentally safe, but budget under-use + doc
mismatch).

**CTX-5 — VERIFIED, defect.** `resolve_sampling_params` clamps max_tokens to
16..=32768 (`src/commands.rs:458-466`), forwarded verbatim (`:729`); never clamped
against ctx → any setting > ~8k guarantees 400. Mitigating: a post-hoc
`finish_reason:"length"` marker exists (`:834-835, 1037-1041`) — R6/C5 is
*partially closed*; verify UI visibility during QA-1.

## 1.2 Unified fix (target, encoded in `tests/ctx_budget_property.rs`)

Single source of ctx truth = the value passed to `--ctx-size` (expose it in
Settings; kill the frontend hardcode). Reserve-based budget:
`history = server_ctx − system − RAG − max_tokens − (msgs×8) − 32 − margin`.
Clamp `max_tokens ≤ server_ctx − prompt − 1`. Use `response.usage.prompt_tokens`
to recalibrate or gate.

## 1.3 Doc patch text (paste into `ARCHITECTURE_AND_STATUS.md` §4 step 2 + `CONTEXT_DUMP.txt`)

> **Token budget (current behavior — KNOWN DEFECT):** `max(1024, GGUF ctx × 0.55)`,
> fallback 4500 when metadata is unreadable. ⚠ The server itself always starts
> with `--ctx-size 8192` hardcoded by the frontend, and the budget reserves
> nothing for system prompt, RAG, current turn, max_tokens, or template overhead.
> For GGUFs with ctx ≥ ~14.9k (llama/qwen2/qwen3 families) the budget can exceed
> the server context → 400 errors on long conversations. See
> `AUDIT_0.9.0-rc.1/FINDINGS_REGISTER.md` F-001/F-002. Planned fix: reserve-based
> budget derived from the actual server ctx.

# Sunrise review — overnight autonomous fix batch

**When:** 2026-07-25 (overnight session)  
**For:** Human RC smoke after sleep  

## What was fixed overnight

### Data safety
- **Mid-thread regenerate** writes `messages.ndjson.bak.{timestamp}` (+ metadata bak) before rewrite.
- **FE confirm** dialog before mid-thread regenerate when later turns would be dropped.
- **Last-turn regenerate** replaces assistants after the last user turn (no stack bloat).

### Identity / delete
- **Delete contact** also removes conversations where that character is a participant.

### Inference
- **Auto-restart** sets `is_resetting` before background spawn (less dual-start race).
- **post_chat_completion** detects dead worker, restarts once, retries HTTP once.

### RAG
- Chunks without embeddings return an **explicit error** (reindex message) instead of silent empty retrieval.
- Cosine similarity skips dimension-mismatched vectors.

### UX
- **Cannot switch contacts while generating** (toast + early return).
- Send binds conversation/character ids at send time; only refreshes UI if still on that chat.
- **Context pill** prefers live `model_context_length` from inference status when available.
- Character load **bumps schema version** and re-persists when below current.

### Tooling
- `scripts/smoke-check.sh` — cargo + JS automated gate.

## Automated verification

```bash
./scripts/smoke-check.sh
```

Expected: all five steps green.

## Human smoke (do this at sunrise) — ~10 min

1. `cargo tauri dev` (or launch installed build).
2. **Settings** opens.
3. **Select contact** → chat shows.
4. **Start server** → not false “failed”; becomes Running.
5. **Send message** → persists after reload.
6. **Regenerate** last → single new assistant (replaces prior last reply).
7. **Mid-thread regen** → confirm dialog; later turns gone; check `conversations/{id}/messages.ndjson.bak.*`.
8. **Switch contact while generating** → blocked with toast.
9. **Delete contact** → conversations for that participant gone.
10. Optional: knowledge re-upload if RAG says missing embeddings.

## Remaining known gaps (acceptable for RC if smoke passes)

- No headless WebView e2e of real `invoke`.
- Voice auto-restart still weaker than LLM.
- Nested `ServerStartRequest` fields stay snake_case (correct for serde).
- Streaming still frozen by design.

## If something fails

1. Check DevTools console for `Error in <command>:`.
2. Run `./scripts/smoke-check.sh`.
3. Logs: terminal running `cargo tauri dev`.

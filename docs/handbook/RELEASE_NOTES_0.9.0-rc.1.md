# LocalPersona 0.9.0-rc.1 — Release Notes

**Status:** Release Candidate (closed beta)  
**Date:** 2026-07-25  
**Technical dump:** [ARCHITECTURE_AND_STATUS.md](./ARCHITECTURE_AND_STATUS.md)

## Who this is for

Creators and engineers who want **local, file-owned AI personas** with GGUF models. Not a cloud companion app.

## Highlights

1. **Personas work again end-to-end** — rich editor fields (personality, scenario, writing instructions) reach the model.
2. **Settings matter** — temperature, max tokens, and top-p apply to local inference.
3. **Stable chat path** — non-streaming completion; no half-finished stream state.
4. **Safer data plane** — conversation UUID + character id contract; ASCII id validation; empty replies not saved.
5. **Honest UX** — streaming and edit are not advertised as ready; voice is experimental.

## How to validate this RC

1. Create a contact with only personality + scenario + writing instructions (leave custom system empty). Chat and confirm voice/style match.
2. Attach a knowledge document; ask something only the doc answers.
3. Change temperature in Settings; send again; confirm behavior shifts.
4. Open a long conversation; confirm context pill and “Load earlier” only when needed.
5. Stop/start llama-server mid-use; confirm toast + recovery without corrupt history.
6. Run `./scripts/tribunal.sh` after any local code change.

Full checklist: [RC_ACCEPTANCE_CHECKLIST.md](./RC_ACCEPTANCE_CHECKLIST.md)

## Not in this RC

- Live token streaming  
- Message edit / branch  
- AR / location / social layer  
- Full vector DB for RAG  

## Install / build

```bash
cargo tauri build
# or development
cargo tauri dev
```

Requires a local `llama-server` (llama.cpp) and your own `.gguf` models.

## Upgrade notes

If you previously chatted while the identity bug was present, you may have orphan directories under `conversations/{characterId}/` separate from UUID conversations. Prefer the conversation shown in the messenger list for that contact. A merge tool may land post-RC if needed.

## Version

- Cargo / Tauri: `0.9.0-rc.1`

# LocalPersona — Architecture (short map)

**For the full architectural blueprint and living status dump, see:**

→ **[ARCHITECTURE_AND_STATUS.md](./ARCHITECTURE_AND_STATUS.md)** (canonical)

## One-screen summary

```
Tauri 2 UI (vanilla JS)
    │ invoke
    ▼
commands.rs  ──► conversation (NDJSON) + storage (characters/knowledge)
    │
    ▼
inference.rs ──► llama-server children (LLM + experimental Voice)
    │
    ▼
User-owned app data files · BYO GGUF + llama-server
```

## Key facts (0.9.0-rc.1)

- **Chat SoT:** `conversations/{uuid}/messages.ndjson` only (interactive)
- **Identity:** `character_id` for prompts/RAG; conversation UUID for history
- **Arena:** 300 requests + 45 min uptime on both servers
- **Package:** `.deb` ~14 MB, no models; see `docs/packaging/`
- **Gate:** `./scripts/tribunal.sh`

## Principles

Local ownership · offline · depth over virality · external inference · fail closed on data integrity

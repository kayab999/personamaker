# Roadmap: Persona Maker

## Vision (Long-Term)
To evolve Persona Maker into a decentralized, location-aware, AR-powered social layer for AI-native personas, maintaining a local-first architecture.

## Roadmap Phases

### Phase 0: Stabilization & Foundation (Completed)
- Robust crash safety, atomic data persistence, and worker lifecycle management (Arena Reset).
- Implementation of the Tribunal (automated governance/testing).

### Phase 0.9: RC-Stable (Current — 0.9.0-rc.1)
- Identity contract (conversation UUID + character_id) fixed.
- Rich system prompt composition + modes + sampling settings.
- Real token streaming (Phase B1) with abort + idle detection + partial preservation.
- Non-destructive message revisions backend (Phase B2): `revisions.ndjson` + fold.
- Tribunal: check + destructive + property + **IPC contract** tests.
- Commercial packaging: Linux `.deb` (~14 MB), no models; packaging docs + brand icons.
- Canonical technical dump: `docs/handbook/ARCHITECTURE_AND_STATUS.md`.
- Closed beta after manual acceptance checklist + soak (human gate still open).

### Phase 1: Interop & Knowledge Depth (Next)
- Character-card interop: CCv2/CCv3 JSON + PNG cards + CHARX import, CCv3 export.
- Knowledge-base management UI (list / delete / reindex) — backend commands landed.
- Message-edit UI on top of the Phase B2 revision backend.
- Lorebook / World Info (keyword-triggered entries) alongside semantic RAG.
- Intelligent summarization for very long chats.
- Orphan conversation repair tool (pre-RC identity bug survivors).

### Phase 2: Conversation Depth
- Branching / forking from any message; single-message delete; conversation rename.
- Author's note + injection-depth control.
- Group chat (2–4 characters) on the existing `participant_ids` schema.
- Wider sampling surface (repetition penalty, min_p, seed, stop strings).

### Phase 3: Location & Discovery
- Character spawning and location anchoring.
- Geofenced discovery (local).

### Phase 4: AR & Experience
- Camera-based AR interaction.
- Character encounter mechanics.

### Phase 5: Decentralized Social Layer
- Peer-to-peer sharing and discovery.
- Reputation mechanisms (quality-based).

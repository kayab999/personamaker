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
- Non-stream stable path; streaming deferred.
- Tribunal: check + destructive + property tests.
- Commercial packaging: Linux `.deb` (~14 MB), no models; packaging docs + brand icons.
- Canonical technical dump: `docs/handbook/ARCHITECTURE_AND_STATUS.md`.
- Closed beta after manual acceptance checklist + soak (human gate still open).

### Phase 1: Feature Expansion & Optimization (Next)
- Real token streaming (post-RC) with abort + Tribunal invariants.
- Message edit / branch on append-only history.
- Intelligent summarization for very long chats.
- Orphan conversation repair tool (pre-RC identity bug survivors).

### Phase 2: Location & Discovery
- Character spawning and location anchoring.
- Geofenced discovery (local).

### Phase 3: AR & Experience
- Camera-based AR interaction.
- Character encounter mechanics.

### Phase 4: Decentralized Social Layer
- Peer-to-peer sharing and discovery.
- Reputation mechanisms (quality-based).

# LocalPersona documentation index

Start here to find the right doc without reading everything.

## Canonical (read these first)

| Doc | Audience | Purpose |
|-----|----------|---------|
| [../HANDOVER.md](../HANDOVER.md) | Agents / maintainers | **Durable progress + mission state record** (update per checkpoint) |
| [handbook/ARCHITECTURE_AND_STATUS.md](./handbook/ARCHITECTURE_AND_STATUS.md) | Engineers, agents | **Full architectural blueprint + current project status** |
| [../README.md](../README.md) | Everyone | Product overview, features, build entry points |
| [../AGENTS.md](../AGENTS.md) | AI agents / contributors | Working conventions and codebase briefing |
| [../assets/USER_MANUAL.md](../assets/USER_MANUAL.md) | End users | In-app help (shipped in installer) |
| [../CHANGELOG.md](../CHANGELOG.md) | Everyone | What changed by version |

## Packaging & install

| Doc | Purpose |
|-----|---------|
| [packaging/INSTALL.md](./packaging/INSTALL.md) | How to install closed-beta packages |
| [packaging/PACKAGING.md](./packaging/PACKAGING.md) | How to build `.deb` / release kit |
| [../dist/0.9.0-rc.1/](../dist/0.9.0-rc.1/) | Built artifacts (if present; gitignored) |

## Release / QA

| Doc | Purpose |
|-----|---------|
| [handbook/RC_ACCEPTANCE_CHECKLIST.md](./handbook/RC_ACCEPTANCE_CHECKLIST.md) | Manual + automated gates for RC |
| [handbook/RELEASE_NOTES_0.9.0-rc.1.md](./handbook/RELEASE_NOTES_0.9.0-rc.1.md) | Closed-beta release notes |

## Architecture (short + deep)

| Doc | Purpose |
|-----|---------|
| [handbook/ARCHITECTURE_AND_STATUS.md](./handbook/ARCHITECTURE_AND_STATUS.md) | **Deep dump (preferred)** |
| [handbook/ARCHITECTURE.md](./handbook/ARCHITECTURE.md) | Shorter map; points to deep dump |

## Historical / forensic (do not treat scores as current)

These remain for audit trail. Prefer `ARCHITECTURE_AND_STATUS.md` for “what is true now.”

| Doc | Notes |
|-----|--------|
| [handbook/AUDIT_v7.2_LOCALPERSONA_RC_READINESS.md](./handbook/AUDIT_v7.2_LOCALPERSONA_RC_READINESS.md) | Large forensic audit + loop log |
| [handbook/AUDIT_0.9.0-rc.1/](./handbook/AUDIT_0.9.0-rc.1/) | 0.9.0-rc.1 audit bundle (claims register, findings, matrix) |
| [handbook/REMEDIATION_WORKPLAN_2026.md](./handbook/REMEDIATION_WORKPLAN_2026.md) | Q&A/UI remediation plan log |
| [handbook/PHASE0_STABILIZATION.md](./handbook/PHASE0_STABILIZATION.md) | Stabilization checkpoint |
| [handbook/PHASE0_RECOVERY_GUIDE.md](./handbook/PHASE0_RECOVERY_GUIDE.md) | Recovery notes after restarts |
| [handbook/SWEEP_BASELINE.md](./handbook/SWEEP_BASELINE.md) | Numeric ratchet baseline (2026-10-04) |

## Vision & experimental

| Doc | Purpose |
|-----|---------|
| [handbook/FUTURE_VISION.md](./handbook/FUTURE_VISION.md) | Long-term AR / social idea (out of RC scope) |
| [VOICE_SETUP.md](./VOICE_SETUP.md) | Experimental TTS notes |
| [SANDBOX.md](./SANDBOX.md) | Sandbox notes |
| [branding/](./branding/) | Icon master archive |

## Governance & contracts

| Doc / gate | Purpose |
|------------|---------|
| [COLLABORATION_MANIFEST.md](./COLLABORATION_MANIFEST.md) | Project values, objectives, working agreements |
| [../scripts/tribunal.sh](../scripts/tribunal.sh) | Local gate: `cargo check` + destructive + property + IPC contract |
| [../.github/workflows/tribunal.yml](../.github/workflows/tribunal.yml) | CI Tribunal v12.1 (14 jobs, required on `main`) |
| [../gen/schemas/localpersona-ipc-schemas.json](../gen/schemas/localpersona-ipc-schemas.json) | **Single** IPC contract snapshot (golden file for the schema-drift test) |
| [../tests/ipc_contract.rs](../tests/ipc_contract.rs) | Frontend↔backend command parity + `withGlobalTauri` gate |

## Roadmap

| Doc | Purpose |
|-----|---------|
| [../ROADMAP.md](../ROADMAP.md) | High-level product phases |

---

**Documentation hygiene:** When status changes (version, SoT, packaging), update `ARCHITECTURE_AND_STATUS.md` and `CHANGELOG.md` first, then skim README/AGENTS for stale claims.

# LocalPersona documentation index

Start here to find the right doc without reading everything.

## Canonical (read these first)

| Doc | Audience | Purpose |
|-----|----------|---------|
| [handbook/CONTEXT_DUMP.txt](./handbook/CONTEXT_DUMP.txt) | Everyone / paste into agents | **Plain-text current status dump** |
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
| [handbook/REMEDIATION_WORKPLAN_2026.md](./handbook/REMEDIATION_WORKPLAN_2026.md) | Q&A/UI remediation plan log |
| [handbook/PHASE0_STABILIZATION.md](./handbook/PHASE0_STABILIZATION.md) | Stabilization checkpoint |
| [handbook/PHASE0_RECOVERY_GUIDE.md](./handbook/PHASE0_RECOVERY_GUIDE.md) | Recovery notes after restarts |

## Vision & experimental

| Doc | Purpose |
|-----|---------|
| [handbook/FUTURE_VISION.md](./handbook/FUTURE_VISION.md) | Long-term AR / social idea (out of RC scope) |
| [VOICE_SETUP.md](./VOICE_SETUP.md) | Experimental TTS notes |
| [SANDBOX.md](./SANDBOX.md) | Sandbox notes |
| [branding/](./branding/) | Icon master archive |

## Roadmap

| Doc | Purpose |
|-----|---------|
| [../ROADMAP.md](../ROADMAP.md) | High-level product phases |

---

**Documentation hygiene:** When status changes (version, SoT, packaging), update `ARCHITECTURE_AND_STATUS.md` and `CHANGELOG.md` first, then skim README/AGENTS for stale claims.

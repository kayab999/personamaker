# LocalPersona RC Acceptance Checklist

**Target tag:** `v0.9.0-rc.1`  
**Governance:** `./scripts/tribunal.sh` must be green before tag.  
**Date baseline:** 2026-07 re-audit + R0–R3 remediation.

## Automated gates (required)

- [x] `cargo check` clean
- [x] Destructive / invariant tests green (identity, Arena, schema, chaos parse, sampling clamp)
- [x] Property tests green (ASCII ID validators, truncate, circuit breaker)
- [x] Pre-commit Tribunal runs check + destructive + property
- [x] Clippy CI policy: correctness/suspicious deny (not total `-D warnings`)
- [x] Version set to `0.9.0-rc.1` (Cargo.toml + tauri.conf.json)
- [x] CHANGELOG.md + RELEASE_NOTES_0.9.0-rc.1.md

## Product correctness (manual before tag)

| # | Scenario | Pass? |
|---|----------|-------|
| 1 | Character with only personality + scenario + writing_instructions (no custom system) reflects those traits | |
| 2 | Custom system_prompt fully overrides rich fields | |
| 3 | Knowledge doc: relevant answer without meta-commentary when natural | |
| 4 | 100+ turns: UI stays responsive; Load earlier works only when needed | |
| 5 | Messages land in one conversation UUID per contact (no orphan char-id dirs on new sends) | |
| 6 | Temperature / max tokens change affects requests (settings save + send) | |
| 7 | Worker kill mid-reply: toast, no corrupt ndjson, next send works | |
| 8 | Arena reset (300 req or 45m): recovery without data loss | |

## Known limitations (document in release notes)

- Token streaming frozen (non-stream only) — post-RC
- Message edit deferred
- Voice / TTS experimental
- AR / social layer out of scope

## Architecture phases (code — required)

- [x] Phase A: single chat SoT (NDJSON); legacy history path removed from interactive FE
- [x] Phase B7: shared send/regenerate inference helpers
- [x] Phase C2: Arena last_reset_reason in diagnostics
- [x] Phase C3: RAG status recorded for diagnostics
- [x] Phase C5: Enter key respects isGenerating
- [x] Orphan conversation hints command + Diagnostics card

## Soak (Phase D — human)

- [ ] 2–4 hour session: no ghost `llama-server`, Arena fires, memory stable
- [ ] Clean app exit ×10: no orphan processes
- [ ] Manual Q&A matrix 8/8 above

## Packaging (code — done 2026-07-25)

- [x] No GGUF in installer resources
- [x] Linux `.deb` produced under `dist/0.9.0-rc.1/` (~14 MB)
- [x] INSTALL.md + CHECKSUMS + LICENSE in dist kit
- [x] Canonical blueprint: `ARCHITECTURE_AND_STATUS.md`
- [ ] AppImage (optional; linuxdeploy may fail on this host)

## Tag

- [x] Version / CHANGELOG / USER_MANUAL known-limitations aligned (code)
- [ ] Tag `v0.9.0-rc.1` and closed beta (requires full git + human soak)

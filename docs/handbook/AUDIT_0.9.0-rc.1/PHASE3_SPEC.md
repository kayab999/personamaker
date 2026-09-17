# Phase 3 spec — F-001 fix, design pinned pre-session (2026-09-17)

Binding for the Phase-3 implementation turn. Session → consolidation → fixes →
Tier-2 order is inviolable; this file removes Phase-3 improvisation.

## a) Single source of ctx

The value the backend passes to `--ctx-size` is stored in manager state at spawn;
`commands.rs` budget reads from there — never from GGUF. The GGUF parser
degrades to informational: model ctx shown as a hint in Settings, **no
auto-prefill** (prefilling 131072 on a 7B = KV cache that kills normal machines;
default 8192 stays). The context pill (F-016/F-008 display) reads the same
field — rides free in this batch.

## b) Scope: source + reserves + clamp (the 0.55 multiplier dies with this fix)

Source-only fix is insufficient — the arithmetic:

- Source-only: budget = 8192 × 0.55 = **4505** history.
- Rich persona (system ≈1200) + RAG (≈1500) + turn (≈300) = 7505 fixed tokens.
- Overflow as soon as max_tokens ≥ **688** — a normal user setting on a rich
  persona + knowledge base 400s exactly like today, at ~1.17× instead of ~9×.

The reserve-based budget (`history = server_ctx − system − RAG − max_tokens −
overhead − margin`) is already specified and proptested in
`tests/ctx_budget_property.rs`. Note on clamp computability: exact prompt tokens
don't exist pre-send, so `max_tokens ≤ server_ctx − prompt − 1` operates against
the *estimate*; the margin absorbs estimator error and the rehearsed ctx400 row
covers the residual. **Phase 3 = single source + reserves + clamp.** The three
changes the proptests already encode — honest scope, not extra scope.

## c) Witness lifecycle (anti "fit the test to the code")

Current `witness_*` tests pin the *defect* (green = bug confirmed). In the fix
commit they are **inverted and renamed**:

```rust
// before (pins the defect):
fn witness_ctx12_budget_exceeds_server_on_32k() { assert!(actual_budget(Some(32_768)) > SERVER_CTX); }
// after (pins the invariant — the name tells the direction):
fn regression_budget_never_exceeds_server_ctx() {
    assert!(budget_for_server_ctx(32_768) <= SERVER_CTX);   // any GGUF, any ctx
}
```

Register rule: post-fix tests pin **relations** (`budget ≤ server_ctx` for every
roster file, via probe), never implementation constants. A renamed test asserting
whatever number the code happens to produce = decorative witness.

## d) Migration

New Settings field `ctx_size` with `#[serde(default = ...)]` = 8192 →
existing users see identical behavior, no migration path.

## e) Post-F-001, F-019 inverts

Once the fix lands, the parser extension (F-008) flips from forbidden to
**safe and pending** — first commit post-Phase-3, not "someday". The probe
already holds the mechanism: asserts flip to `budget(file) ≤ server_ctx`.

## Exit gate — Phase 3

- [ ] All tests green with witnesses inverted/renamed (same commit as the fix)
- [ ] Probe: `budget(file) ≤ server_ctx` for **all 3** roster files (Gemma3 and
      Qwen3.5 included — reserves protect them even while still on fallback)
- [ ] **F1 seed re-run as the fix's regression demo**: same 96k seed, same
      harness, fresh capture → `validate_captures --ctx 8192` must **PASS** with
      prompt ≤ 8192. The repro becomes the fix's end-to-end regression test —
      same evidence machine, inverted direction. Most valuable item on this list.
- [ ] Goldens G1/G2/S1/S2 re-validated post-fix (compose markers unchanged; the
      budget gate must now pass on normal conversations)
- [ ] Pill shows real ctx; ctx400 row re-rehearsed (error handling is still the
      residual's blast radius)
- [ ] Tribunal in hook (warm 6–11s, measured)

## Session input Phase 3 needs

Runbook pins S1 = 2048 and S2 = 50000→32768; the session only *confirms* what the
app actually sent (`--expect-sampling` line + F-013 note). Pending inputs beyond
the evidence itself: effectively zero.

## Post-fix UX consequence (for Phase-3 implementation + release notes)

Honest reserves on ctx 8192 + rich persona (~1200) + RAG (~1500) + max_tokens
2048 leave **~3k history tokens**. Not a bug — the truth of an 8k ctx — but users
currently "enjoying" the broken 72k budget will read it as a memory regression.
Mitigation: the pill showing the real history budget (already riding in §a); one
release-notes line to close the expectation.

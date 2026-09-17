//! tests/gguf_roster_probe.rs — settles F-008 Scenario A/B empirically; once
//! asserts are added (step 2/3) it guards the F-001⇄F-008 wiring (F-019).
//! CI-safe: skips gracefully when test-models/ is absent.
//! Run: cargo test --test gguf_roster_probe -- --nocapture

use std::path::Path;

fn budget_for(ctx: Option<u32>) -> u64 {
    match ctx {
        Some(c) => ((c as f64 * 0.55) as u64).max(1024),
        None => 4500,
    }
}

#[test]
fn roster_ctx_ground_truth() {
    let dir = Path::new("test-models");
    let Ok(entries) = std::fs::read_dir(dir) else {
        eprintln!("test-models/ absent — skipping");
        return;
    };
    let mut found = false;
    for entry in entries.flatten() {
        let p = entry.path();
        if p.extension().and_then(|e| e.to_str()) != Some("gguf") {
            continue;
        }
        found = true;
        match localpersona::gguf::read_gguf_metadata(&p) {
            Ok(m) => eprintln!(
                "{} -> parsed ctx = {:?} (arch = {:?}) | budget = {} | server_ctx = 8192 | {}",
                p.display(),
                m.context_length,
                m.architecture,
                budget_for(m.context_length),
                if budget_for(m.context_length) > 8192 {
                    "OVERFLOW vs server"
                } else {
                    "fits server"
                }
            ),
            Err(e) => eprintln!("{} -> PARSE ERROR: {e}", p.display()),
        }
    }
    assert!(
        found,
        "test-models/ present but contains no .gguf — update MODEL_MANIFEST.md"
    );
    // STEP 2 — after first run, pin the verdict with asserts:
    //   Scenario A (nothing read): all parsed ctx None -> budget 4500 universally.
    //   Scenario B (llama read): L3.2-Rogue parses Some(131_072).
    // STEP 3 — post-F-001 fix: assert budget(gguf_file) <= SERVER_CTX for every roster model.
}

/// Pinned A/B verdict (2026-09-17, empirical via real parser):
/// mixed — `llama` arm reads, `gemma3`/`qwen35` keys are NOT in the allowlist.
/// L3.2-Rogue is a LIVE F-001 trigger (budget 72,089 vs server 8192).
/// This test is the F-019 wiring guard: extending the parser (F-008) without
/// landing F-001 first MUST break these asserts loudly. Post-F-001, rewrite to
/// assert budget(file) <= SERVER_CTX for every roster model.
#[test]
fn pinned_ab_verdict() {
    let dir = Path::new("test-models");
    let Ok(entries) = std::fs::read_dir(dir) else {
        eprintln!("test-models/ absent — skipping");
        return;
    };
    let mut rogue_ctx = None;
    let mut gemma_ctx = Some(0);
    let mut qwen_ctx = Some(0);
    for entry in entries.flatten() {
        let p = entry.path();
        if p.extension().and_then(|e| e.to_str()) != Some("gguf") {
            continue;
        }
        let m = localpersona::gguf::read_gguf_metadata(&p)
            .unwrap_or_else(|e| panic!("{} should parse: {e}", p.display()));
        let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if name.starts_with("L3.2-Rogue") {
            rogue_ctx = m.context_length;
        } else if name.starts_with("Gemma-3") {
            gemma_ctx = m.context_length;
        } else if name.starts_with("Qwen3.8") {
            qwen_ctx = m.context_length;
        }
    }
    assert_eq!(
        rogue_ctx,
        Some(131_072),
        "F-001 LIVE trigger: L3.2-Rogue must parse 131072 (Scenario B)"
    );
    assert_eq!(
        budget_for(rogue_ctx),
        72_089,
        "rogue budget 131072*0.55 must exceed server 8192"
    );
    assert!(
        budget_for(rogue_ctx) > 8192,
        "pinned: rogue budget overflows server ctx until F-001 lands"
    );
    assert_eq!(gemma_ctx, None, "gemma3 key not in allowlist (gguf.rs:249)");
    assert_eq!(qwen_ctx, None, "qwen35 key not in allowlist (gguf.rs:249)");
}

//! CTX invariants v2 — pinned to VERIFIED constants (ground-truth pass).
//! WITNESS tests (witness_*) pin CURRENT behavior: GREEN witness = confirmed bug.
//! When the reserve-based fix lands, swap adapters and update witnesses to pin
//! the FIXED behavior (a failing witness after a refactor = constants drifted).
//!
//! Verified: server ctx 8192 (frontend hardcode, script.js:2969,3324,3849-3852
//! → inference.rs:379-381) · budget max(1024, gguf×0.55) else 4500
//! (commands.rs:597-604) · estimator (byte_len+3)/4 (conversation.rs:157-163)
//! · max_tokens clamp 16..=32768 (commands.rs:458-466,:729) · GGUF parser reads
//! llama/qwen2/qwen3 only (gguf.rs:249).
//!
//! ADAPT: point the actual_* bodies at the real fns (via lib.rs exports) so
//! refactors can't silently change constants without a witness failing.

use proptest::prelude::*;

const SERVER_CTX: u64 = 8192;

// ----- current implementation (wire to real fns) ------------------------------
fn actual_budget(gguf_ctx: Option<u64>) -> u64 {
    let raw = match gguf_ctx {
        Some(c) => (c as f64 * 0.55) as u64,
        None => 4500,
    };
    raw.max(1024)
}
fn actual_estimate_bytes(s: &str) -> u64 {
    (s.len() as u64 + 3) / 4
}
fn actual_max_tokens_clamp(v: u64) -> u64 {
    v.clamp(16, 32768)
}

// ----- reference fix (lift into commands.rs/inference.rs) ---------------------
const TPL_PER_MSG: u64 = 8;
const TPL_BASE: u64 = 32;
const MARGIN: u64 = 128;

pub fn history_budget_tokens(
    server_ctx: u64,
    system: u64,
    rag: u64,
    max_tokens: u64,
    msgs: u64,
) -> u64 {
    let overhead = msgs * TPL_PER_MSG + TPL_BASE;
    server_ctx.saturating_sub(system + rag + max_tokens + overhead + MARGIN)
}
pub fn clamp_max_tokens(requested: u64, server_ctx: u64, prompt_tokens: u64) -> u64 {
    requested.min(server_ctx.saturating_sub(prompt_tokens).saturating_sub(1))
}

// ----- WITNESSES: green = bug confirmed ---------------------------------------

#[test]
fn witness_ctx12_budget_exceeds_server_on_32k() {
    assert!(actual_budget(Some(32_768)) > SERVER_CTX); // 18,022 > 8,192
    assert!(actual_budget(Some(16_384)) > SERVER_CTX); //  9,011 > 8,192
}

#[test]
fn witness_ctx12_exact_crossover() {
    let mut c = 1024u64;
    while c < 32_768 && actual_budget(Some(c)) <= SERVER_CTX {
        c += 1;
    }
    assert!(
        c < 16_384,
        "crossover at GGUF ctx = {c}: every llama/qwen model ≥ {c} breaks (F-001)"
    );
}

#[test]
fn witness_ctx3_no_reserves_overflow_on_fallback() {
    let (system, rag, max_tokens, turn) = (1_200u64, 1_500, 2_048, 300);
    assert!(actual_budget(None) + system + rag + max_tokens + turn > SERVER_CTX); // 9,548 > 8,192
}

#[test]
fn witness_ctx5_max_tokens_range_not_ctx_bound() {
    assert!(actual_max_tokens_clamp(32_768) > SERVER_CTX); // 32k allowed on an 8k server
}

#[test]
fn witness_qa2_cjk_byte_estimator_undercounts_structurally() {
    let cjk = "今日はとても良い天気ですね。一緒に散歩に行きましょう。";
    let chars = cjk.chars().count() as f64;
    let est = actual_estimate_bytes(cjk) as f64;
    // pure CJK = 3 bytes/char → est = 0.75 × chars; common BPE tokenizers ≥ 1.0 × chars
    assert!(
        est < chars,
        "byte/4 gives {est} for {chars} CJK chars — structural ≥25% undercount (F-002)"
    );
}

// ----- fix invariants (target behavior) ---------------------------------------
proptest! {
    #[test]
    fn prop_total_never_exceeds_ctx(ctx in 512u64..131_072, sys in 0u64..6_000,
        rag in 0u64..4_000, mt in 1u64..16_384, msgs in 0u64..500) {
        let b = history_budget_tokens(ctx, sys, rag, mt, msgs);
        prop_assert!(b == 0 || b + sys + rag + mt + msgs * TPL_PER_MSG + TPL_BASE + MARGIN <= ctx);
    }
    #[test]
    fn prop_budget_monotonic_in_ctx(ctx in 512u64..100_000, d in 1u64..4_096,
        sys in 0u64..6_000, rag in 0u64..4_000, mt in 1u64..16_384, msgs in 0u64..500) {
        prop_assert!(history_budget_tokens(ctx + d, sys, rag, mt, msgs)
                     >= history_budget_tokens(ctx, sys, rag, mt, msgs));
    }
    #[test]
    fn prop_clamped_max_tokens_fits(ctx in 1_024u64..131_072, prompt in 0u64..131_072, mt in 1u64..32_768) {
        let c = clamp_max_tokens(mt, ctx, prompt);
        prop_assert!(c <= mt && (c == 0 || prompt + c + 1 <= ctx));
    }
}

// ----- QA-2 real-token table (fill when a real llama-server exists) -----------
#[test]
#[ignore = "fill real counts via real llama-server /tokenize or captured usage.prompt_tokens, then run with -- --ignored"]
fn cjk_real_drift_table() {
    let en = "The quick brown fox jumps over the lazy dog. ".repeat(5);
    let cases: &[(&str, u64)] = &[
        (en.as_str(), 0),
        ("今日はとても良い天気ですね。一緒に散歩に行きましょう。", 0),
        ("这是一段用于测试的中文文本，包含一些常见词汇和表达方式。", 0),
        ("안녕하세요, 반갑습니다. 오늘 날씨가 정말 좋네요.", 0),
        ("🤖✨😄🔥🎉🚀💡🎧🌟💯😂", 0),
    ];
    for (text, real) in cases {
        if *real == 0 {
            continue;
        }
        let est = actual_estimate_bytes(text) as f64;
        assert!(
            (*real as f64 - est) / *real as f64 <= 0.30,
            "undercount >30% (F-002)"
        );
    }
}

//! Phase A (2026-10-30): fuzz-corpus seed rot guard.
//!
//! The seeds under `fuzz/corpus/<target>/` drive `cargo fuzz run <target>`.
//! If a parser change invalidates them silently, fuzzing starts from garbage
//! (or stops reaching its intended paths). This suite calls the SAME parse
//! functions the fuzz targets call (`src/gguf.rs`, `src/conversation.rs`,
//! `src/storage.rs`) and pins each seed's semantic outcome on disk.
//!
//! Rule: a commit that changes parser behavior must update seeds AND this
//! guard in the same commit. See fuzz/README.md for run instructions.

use std::fs;
use std::path::PathBuf;

fn corpus(target: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("fuzz")
        .join("corpus")
        .join(target)
}

// ---------------------------------------------------------------- gguf_parse

#[test]
fn gguf_seed_valid_llama_meta_parses_with_full_metadata() {
    let bytes =
        fs::read(corpus("gguf_parse").join("seed_valid_llama_meta")).expect("seed exists");
    let m = localpersona::gguf::parse_gguf_bytes(&bytes).expect("valid seed must parse");
    assert_eq!(m.architecture.as_deref(), Some("llama"));
    assert_eq!(m.name.as_deref(), Some("TestModel 8B"));
    assert_eq!(m.parameter_count, Some(8_000_000_000));
    assert_eq!(m.context_length, Some(8192));
}

#[test]
fn gguf_seed_string_array_skips_without_misalignment() {
    let bytes = fs::read(corpus("gguf_parse").join("seed_array_strings")).expect("seed exists");
    let m = localpersona::gguf::parse_gguf_bytes(&bytes)
        .expect("string arrays must be skipped without corrupting the stream");
    assert!(m.architecture.is_none());
}

#[test]
fn gguf_seed_u8_array_skips_without_misalignment() {
    let bytes = fs::read(corpus("gguf_parse").join("seed_array_u8")).expect("seed exists");
    assert!(
        localpersona::gguf::parse_gguf_bytes(&bytes).is_ok(),
        "numeric array must be skipped cleanly"
    );
}

#[test]
fn gguf_seed_truncated_kv_readable_header_partial_metadata() {
    // Real contract (verified 2026-10-30): KV extraction is best-effort — a
    // KV whose payload is missing emits a per-entry error that the loop
    // skips, but a readable magic+version header still yields Ok with
    // all-None fields so model discovery can fall back to filename
    // heuristics. Pin that behavior; do not "fix" it into a rejection
    // without changing scan_for_models assumptions too.
    let bytes = fs::read(corpus("gguf_parse").join("seed_truncated_kv")).expect("seed exists");
    let m = localpersona::gguf::parse_gguf_bytes(&bytes)
        .expect("readable header, truncated payloads: still parses");
    assert!(m.architecture.is_none());
    assert!(m.name.is_none());
    assert!(m.context_length.is_none());
}

#[test]
fn gguf_seed_trailing_junk_is_ignored() {
    let bytes = fs::read(corpus("gguf_parse").join("seed_trailing_junk")).expect("seed exists");
    assert!(
        localpersona::gguf::parse_gguf_bytes(&bytes).is_ok(),
        "bytes past the declared KV section must be ignored"
    );
}

// -------------------------------------------------------------- ndjson_parse

#[test]
fn ndjson_seed_valid_user_message() {
    let bytes = fs::read(corpus("ndjson_parse").join("seed_valid_user")).expect("seed exists");
    let msg = localpersona::conversation::parse_message_line(&bytes).expect("valid line");
    assert_eq!(msg.role, "user");
    assert_eq!(msg.speaker_id, "user");
    assert_eq!(msg.token_count, 3);
    assert!(msg.images.is_empty());
}

#[test]
fn ndjson_seed_assistant_with_image_reference() {
    let bytes =
        fs::read(corpus("ndjson_parse").join("seed_valid_with_images")).expect("seed exists");
    let msg =
        localpersona::conversation::parse_message_line(&bytes).expect("valid line");
    assert_eq!(msg.role, "assistant");
    assert_eq!(msg.images.len(), 1, "voice for the full ImageReference shape");
    assert_eq!(msg.images[0].mime_type, "image/png");
    assert_eq!(msg.images[0].width, Some(640));
}

#[test]
fn ndjson_seed_narrator_third_role() {
    let bytes = fs::read(corpus("ndjson_parse").join("seed_valid_narrator")).expect("seed exists");
    let msg = localpersona::conversation::parse_message_line(&bytes).expect("valid line");
    assert_eq!(msg.role, "narrator");
}

#[test]
fn ndjson_seed_legacy_unknown_fields_tolerated() {
    let bytes =
        fs::read(corpus("ndjson_parse").join("seed_valid_legacy_extra_fields")).expect("seed exists");
    let msg = localpersona::conversation::parse_message_line(&bytes)
        .expect("serde forward-compat: unknown fields ignored");
    assert_eq!(msg.content, "legacy line");
}

#[test]
fn ndjson_seed_crash_cut_line_rejected() {
    let bytes =
        fs::read(corpus("ndjson_parse").join("seed_corrupt_partial")).expect("seed exists");
    assert!(
        localpersona::conversation::parse_message_line(&bytes).is_err(),
        "truncated JSON line must be rejected (loader skips it, never crashes)"
    );
}

// ---------------------------------------------------------------- pdf_ingest

fn pin_worker() {
    // Same helper as tests/pdf_isolation.rs: pin the real app binary so the
    // disposable --extract-pdf worker path is used in tests.
    std::env::set_var(
        "LOCALPERSONA_PDF_WORKER_BIN",
        PathBuf::from(env!("CARGO_BIN_EXE_localpersona")),
    );
}

#[test]
fn pdf_seed_minimal_hello_extracts() {
    let bytes = fs::read(corpus("pdf_ingest").join("seed_minimal_hello.pdf")).expect("seed exists");
    let dir = std::env::temp_dir().join(format!(
        "lp-fuzzseed-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos().wrapping_add(std::process::id() as u128))
            .unwrap_or(0)
    ));
    fs::create_dir_all(&dir).expect("temp dir");
    let path = dir.join("seed_minimal_hello.pdf");
    fs::write(&path, &bytes).expect("write seed copy");
    pin_worker();
    let text = localpersona::storage::extract_text_from_pdf(&path)
        .unwrap_or_else(|e| panic!("valid seed PDF must extract, got Err: {}", e));
    assert!(
        text.contains("Hello fuzz corpus"),
        "expected seed text, got: {:?}",
        text.chars().take(200).collect::<String>()
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn pdf_seed_partial_truncated_never_panics_in_worker() {
    // Mirrors the fuzz target's actual contract: pdf_fuzz_entry must not
    // panic on truncated input (in-process, panic-isolating wrapper).
    let bytes =
        fs::read(corpus("pdf_ingest").join("seed_partial_truncated.pdf")).expect("seed exists");
    localpersona::storage::pdf_fuzz_entry(&bytes);
}

//! Phase B2 — non-destructive message revisions (revisions.ndjson + fold).
//!
//! Exercises the directory-scoped edit/fold seams directly (no Tauri
//! AppHandle exists in this suite — same convention as stress_concurrent_send;
//! the AppHandle wrappers are thin `get_conversation_dir` + delegate):
//!   - `append_edit_record_in_dir`   (edit write path: lock + B4 guard + fsync + preview refresh)
//!   - `load_revisions_from_dir`     (revision side file, corrupt-line tolerance)
//!   - `apply_revisions`             (pure fold)
//!   - `load_all_messages_core_from_dir` (raw parser)
//!   - `load_messages_within_token_budget_in_dir` (B2-aware budget loader)
//!
//! Pinned contracts:
//!   1. Editing NEVER rewrites messages.ndjson (raw bytes bit-identical).
//!   2. Active version = LAST record for a message id in append order.
//!   3. Supersession chain is honest: record[i].old_content == record[i-1].new_content.
//!   4. Fold is idempotent, also after a mid-thread regenerate bakes edits
//!      back into messages.ndjson (rewrite_conversation_messages precedent).
//!   5. Edits clear the stream `incomplete` flag and set `edited` (user took
//!      ownership of the text).
//!   6. Corrupt/truncated revision lines degrade to skip-and-continue; a
//!      crash-truncated tail gets a B4 separator before the next append.
//!   7. Only metadata preview/updated_at refresh when the edited row IS the
//!      last message; other edits leave metadata.json byte-identical.
//!   8. Stale revision records never resurrect truncated/regenerated turns.

use localpersona::conversation::{
    append_edit_record_in_dir, apply_revisions, load_all_messages_core_from_dir,
    load_messages_within_token_budget_in_dir, load_revisions_from_dir, ChatMessage, EditRecord,
};
use std::fs;

// ==================== harness ====================

fn msg(conv_id: &str, id: &str, role: &str, content: &str) -> ChatMessage {
    ChatMessage {
        id: id.to_string(),
        conversation_id: conv_id.to_string(),
        speaker_id: if role == "user" { "user".to_string() } else { "char-1".to_string() },
        speaker_name: if role == "user" { "You".to_string() } else { "Guide".to_string() },
        content: content.to_string(),
        timestamp: 1_700_000_000_000 + id.len() as u64,
        role: role.to_string(),
        images: vec![],
        token_count: 0,
        incomplete: false,
        edited: false,
    }
}

fn msg_line(m: &ChatMessage) -> String {
    let mut line = serde_json::to_string(m).unwrap();
    line.push('\n');
    line
}

fn edit_record_line(r: &EditRecord) -> String {
    let mut line = serde_json::to_string(r).unwrap();
    line.push('\n');
    line
}

fn seed_dir_conv(messages: &[ChatMessage]) -> ConvFixture {
    let tmp = tempfile::TempDir::new().unwrap();
    let dir = tmp.path().join(messages[0].conversation_id.as_str());
    fs::create_dir_all(&dir).unwrap();

    let msgs_path = dir.join("messages.ndjson");
    let payload: String = messages.iter().map(msg_line).collect();
    fs::write(&msgs_path, payload).unwrap();
    fs::write(dir.join("revisions.ndjson"), "").unwrap();
    write_metadata(&dir, &messages[0].conversation_id, messages.len() as u32);

    ConvFixture {
        conv_id: messages[0].conversation_id.clone(),
        raw_msgs_path: msgs_path,
        raw_meta_path: dir.join("metadata.json"),
        dir,
        _tmp: tmp, // keep the tempdir alive
    }
}

fn write_metadata(dir: &std::path::Path, conv_id: &str, count: u32) {
    let meta = serde_json::json!({
        "version": 1,
        "id": conv_id,
        "name": "Test conversation",
        "participant_ids": [],
        "created_at": 1_700_000_000_000u64,
        "updated_at": 1_700_000_000_000u64,
        "message_count": count,
        "last_message_preview": null,
        "last_message_timestamp": null,
    });
    fs::write(dir.join("metadata.json"), serde_json::to_string_pretty(&meta).unwrap()).unwrap();
}

struct ConvFixture {
    dir: std::path::PathBuf,
    conv_id: String,
    raw_meta_path: std::path::PathBuf,
    raw_msgs_path: std::path::PathBuf,
    _tmp: tempfile::TempDir,
}

impl ConvFixture {
    fn raw_msgs(&self) -> Vec<ChatMessage> {
        load_all_messages_core_from_dir(&self.dir).unwrap()
    }
    fn revisions(&self) -> Vec<EditRecord> {
        load_revisions_from_dir(&self.dir).unwrap()
    }
    fn folded(&self) -> Vec<ChatMessage> {
        let mut m = self.raw_msgs();
        apply_revisions(&mut m, &self.revisions());
        m
    }
    /// Simulates what mid-thread regen's `rewrite_conversation_messages` does:
    /// replace messages.ndjson (and metadata's message_count) with the folded
    /// view — edits become part of the raw rows.
    fn bake(&self) {
        let folded = self.folded();
        let payload: String = folded.iter().map(msg_line).collect();
        fs::write(&self.raw_msgs_path, payload).unwrap();
        let meta: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&self.raw_meta_path).unwrap()).unwrap();
        let with_count = {
            let mut m = meta.clone();
            m["message_count"] = serde_json::json!(folded.len());
            m
        };
        fs::write(
            &self.raw_meta_path,
            serde_json::to_string_pretty(&with_count).unwrap(),
        )
        .unwrap();
    }
}

// ==================== 1. first edit ====================

#[test]
fn first_edit_appends_record_raw_file_untouched() {
    let fx = seed_dir_conv(&[
        msg("conv-1", "m1", "user", "hello there"),
        msg("conv-1", "m2", "assistant", "initial answer"),
    ]);
    let raw_before = fs::read(&fx.raw_msgs_path).unwrap();

    let rec = append_edit_record_in_dir(&fx.dir, &fx.conv_id, "m2", "revised answer").unwrap();

    // Raw rows bit-identical — edits never rewrite history.
    assert_eq!(fs::read(&fx.raw_msgs_path).unwrap(), raw_before);
    // Provenance: old = the content actually replaced (the active view).
    assert_eq!(rec.old_content, "initial answer");
    assert_eq!(rec.new_content, "revised answer");
    assert_eq!(rec.message_id, "m2");
    assert_eq!(rec.schema_version, 1);
    assert!(rec.previous_revision_id.is_none());
    assert!(rec.edited_at > 0);
    assert!(!rec.revision_id.is_empty());
    // Exactly one record on disk.
    let all = fx.revisions();
    assert_eq!(all.len(), 1);
    assert_eq!(all[0].revision_id, rec.revision_id);
    // Fold shows the new content; untouched rows carry no view flag.
    let folded = fx.folded();
    assert_eq!(folded.iter().find(|m| m.id == "m2").unwrap().content, "revised answer");
    assert!(folded.iter().find(|m| m.id == "m2").unwrap().edited);
    assert!(!folded.iter().find(|m| m.id == "m1").unwrap().edited);
}

// ==================== 2. multiple edits chain ====================

#[test]
fn multiple_edits_form_honest_supersession_chain() {
    let fx = seed_dir_conv(&[msg("conv-2", "m1", "user", "draft one")]);

    let r1 = append_edit_record_in_dir(&fx.dir, &fx.conv_id, "m1", "draft two").unwrap();
    let r2 = append_edit_record_in_dir(&fx.dir, &fx.conv_id, "m1", "final draft").unwrap();

    // Chain: each record replaces the PREVIOUS active version.
    assert_eq!(r2.previous_revision_id.as_deref(), Some(r1.revision_id.as_str()));
    assert_eq!(r1.previous_revision_id, None);
    assert_eq!(r1.old_content, "draft one");
    assert_eq!(
        r2.old_content, "draft two",
        "second edit must record the intermediate active version"
    );
    // Append order preserved on disk.
    let all = fx.revisions();
    assert_eq!(all.len(), 2);
    assert_eq!(all[0].revision_id, r1.revision_id);
    assert_eq!(all[1].revision_id, r2.revision_id);
    // Active version = last record.
    assert_eq!(fx.folded()[0].content, "final draft");
}

// ==================== 3-5. rejection cases ====================

#[test]
fn edit_unknown_message_id_errors() {
    let fx = seed_dir_conv(&[msg("conv-3", "m1", "user", "hi")]);
    let err = append_edit_record_in_dir(&fx.dir, &fx.conv_id, "ghost-id", "x").unwrap_err();
    assert!(err.contains("not found"), "got: {err}");
    assert!(fx.revisions().is_empty(), "failed edits must persist nothing");
}

#[test]
fn edit_empty_content_errors() {
    let fx = seed_dir_conv(&[msg("conv-4", "m1", "user", "hi")]);
    for bad in ["", "   ", "\n", "\r\n"] {
        let err = append_edit_record_in_dir(&fx.dir, &fx.conv_id, "m1", bad).unwrap_err();
        assert!(err.contains("must not be empty"), "got: {err}");
    }
    assert!(fx.revisions().is_empty());
}

#[test]
fn edit_identical_content_rejected() {
    let fx = seed_dir_conv(&[msg("conv-5", "m1", "user", "same text")]);
    let err = append_edit_record_in_dir(&fx.dir, &fx.conv_id, "m1", "same text").unwrap_err();
    assert!(err.contains("No change"), "got: {err}");
    assert!(fx.revisions().is_empty());
    // Trailing-newline-only diffs normalize away (textarea round-trips).
    let err2 = append_edit_record_in_dir(&fx.dir, &fx.conv_id, "m1", "same text\n").unwrap_err();
    assert!(err2.contains("No change"), "got: {err2}");
}

// ==================== 6. fold idempotence + regen bake ====================

#[test]
fn fold_is_idempotent_and_survives_regen_bake() {
    let fx = seed_dir_conv(&[
        msg("conv-6", "m1", "user", "question"),
        msg("conv-6", "m2", "assistant", "original answer"),
    ]);

    // Edit assistant turn, then a (simulated) mid-thread regenerate bakes it.
    let r1 = append_edit_record_in_dir(&fx.dir, &fx.conv_id, "m2", "edited answer").unwrap();
    assert_eq!(fx.folded().iter().find(|m| m.id == "m2").unwrap().content, "edited answer");

    // Applying the fold twice changes nothing (pure wholesale replace).
    let once = fx.folded();
    let mut twice = fx.folded();
    apply_revisions(&mut twice, &fx.revisions());
    assert_eq!(
        serde_json::to_string(&once).unwrap(),
        serde_json::to_string(&twice).unwrap(),
        "fold must be idempotent"
    );

    fx.bake();

    // The stale record still folds to the SAME content: replace is a no-op now.
    let after_bake = fx.folded();
    assert_eq!(
        after_bake.iter().find(|m| m.id == "m2").unwrap().content,
        "edited answer",
        "baked content must equal the folded view (rewrite+fold consistency)"
    );
    assert!(after_bake.iter().find(|m| m.id == "m2").unwrap().edited);
    // Provenance of the pre-bake edit survives the bake.
    assert_eq!(r1.old_content, "original answer");
}

#[test]
fn edit_after_bake_matches_baked_provenance() {
    let fx = seed_dir_conv(&[msg("conv-7", "m1", "user", "old question")]);

    // Edit, then regen bakes it into the raw row.
    append_edit_record_in_dir(&fx.dir, &fx.conv_id, "m1", "new question").unwrap();
    fx.bake();
    assert_eq!(fx.raw_msgs()[0].content, "new question");

    // A second edit records the baked (active) version as its old_content.
    let r2 = append_edit_record_in_dir(&fx.dir, &fx.conv_id, "m1", "newer question").unwrap();
    assert_eq!(r2.old_content, "new question");
    assert_eq!(fx.folded()[0].content, "newer question");
}

// ==================== 8. incomplete cleared on edit ====================

#[test]
fn edit_clears_incomplete_flag_and_sets_edited() {
    let fx = seed_dir_conv(&[msg("conv-8", "m1", "assistant", "partial turn…")]);
    // Mark the row as a persisted aborted-stream fragment (Phase B1 on disk).
    let mut flagged = msg("conv-8", "m1", "assistant", "partial turn…");
    flagged.incomplete = true;
    fs::write(&fx.raw_msgs_path, msg_line(&flagged)).unwrap();

    append_edit_record_in_dir(&fx.dir, &fx.conv_id, "m1", "finished turn").unwrap();

    let folded = fx.folded();
    let m1 = folded.iter().find(|m| m.id == "m1").unwrap();
    assert!(!m1.incomplete, "user edit takes ownership: partial marker must clear");
    assert!(m1.edited);
    // token_count re-estimated from the edited content.
    assert_eq!(m1.token_count, ("finished turn".len() + 3) / 4);
}

// ==================== 9. corruption tolerance ====================

#[test]
fn crash_truncated_tail_recovers_with_separator() {
    let fx = seed_dir_conv(&[msg("conv-10", "m1", "user", "hello")]);
    let replay_path = fx.dir.join("revisions.ndjson");
    // 1) Two valid edits.
    let r1 = append_edit_record_in_dir(&fx.dir, &fx.conv_id, "m1", "hello v2").unwrap();
    let r2 = append_edit_record_in_dir(&fx.dir, &fx.conv_id, "m1", "hello v3").unwrap();
    // 2) Simulate a crash mid-write of a third record (truncated, no newline).
    let recovery = format!(
        "{}{}{{\"partial\": tru",
        edit_record_line(&r1),
        edit_record_line(&r2)
    );
    fs::write(&replay_path, recovery).unwrap();

    // Loader: both valid records survive, corrupt tail skipped.
    let all = fx.revisions();
    assert_eq!(all.len(), 2);
    assert_eq!(all[0].revision_id, r1.revision_id);
    assert_eq!(all[1].revision_id, r2.revision_id);

    // 3) A fresh edit after the crash inserts the B4 newline separator first —
    //    the next record lands on its own line (no line-joining corruption).
    let r3 = append_edit_record_in_dir(&fx.dir, &fx.conv_id, "m1", "hello v4").unwrap();
    let content = fs::read_to_string(&replay_path).unwrap();
    let r2_line = {
        let mut l = serde_json::to_string(&r2).unwrap();
        l.push('\n');
        l
    };
    assert!(
        content.contains(&format!("{r2_line}{{\"partial\": tru\n")),
        "corrupt tail must be isolated by the B4 separator; file:\n{content}"
    );
    let all = fx.revisions();
    assert_eq!(all.len(), 3, "r3 must be readable after recovery");
    assert_eq!(all[2].revision_id, r3.revision_id);
    // Chain skips the corrupt line: last valid revision for m1 is r2.
    assert_eq!(r3.previous_revision_id.as_deref(), Some(r2.revision_id.as_str()));
    assert_eq!(fx.folded()[0].content, "hello v4");
}

#[test]
fn mid_file_corrupt_revision_line_skipped() {
    let fx = seed_dir_conv(&[msg("conv-11", "m1", "user", "hello")]);
    let replay_path = fx.dir.join("revisions.ndjson");
    let r1 = append_edit_record_in_dir(&fx.dir, &fx.conv_id, "m1", "hello v2").unwrap();

    // Garbage line in the MIDDLE of the side file (not a crash tail): a
    // handcrafted valid record follows it.
    let r2 = EditRecord {
        schema_version: 1,
        revision_id: "handcrafted-r2".to_string(),
        message_id: "m1".to_string(),
        old_content: "hello v2".to_string(),
        new_content: "hello v3".to_string(),
        edited_at: 42,
        previous_revision_id: Some(r1.revision_id.clone()),
    };
    let with_garbage = format!(
        "{}GARBAGE-NOT-JSON\n{}",
        edit_record_line(&r1),
        edit_record_line(&r2)
    );
    fs::write(&replay_path, with_garbage).unwrap();

    let all = fx.revisions();
    assert_eq!(all.len(), 2, "garbage line skipped, both records readable");
    assert_eq!(all[0].revision_id, r1.revision_id);
    assert_eq!(all[1].revision_id, r2.revision_id);
    assert_eq!(fx.folded()[0].content, "hello v3");
}

// ==================== 10. metadata preview refresh ====================

#[test]
fn edit_of_last_message_refreshes_metadata_preview() {
    let fx = seed_dir_conv(&[
        msg("conv-12", "m1", "user", "question"),
        msg("conv-12", "m2", "assistant", "the answer"),
    ]);

    append_edit_record_in_dir(&fx.dir, &fx.conv_id, "m2", "the corrected answer").unwrap();

    let meta: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&fx.raw_meta_path).unwrap()).unwrap();
    assert_eq!(
        meta["last_message_preview"], "the corrected answer",
        "conversation preview must reflect the edited last turn"
    );
    assert_eq!(meta["message_count"], 2, "edit must not change message_count");
    assert_eq!(meta["id"], "conv-12");
}

#[test]
fn edit_of_non_last_message_leaves_metadata_byte_identical() {
    let fx = seed_dir_conv(&[
        msg("conv-13", "m1", "user", "younger question"),
        msg("conv-13", "m2", "assistant", "the answer"),
    ]);
    let meta_before = fs::read(&fx.raw_meta_path).unwrap();

    append_edit_record_in_dir(&fx.dir, &fx.conv_id, "m1", "older question (fixed)").unwrap();

    assert_eq!(
        fs::read(&fx.raw_meta_path).unwrap(),
        meta_before,
        "non-last edits must not touch metadata (sidebar previews keep pointing at the true last row)"
    );
    // The view still shows the edit when loading history.
    assert_eq!(fx.folded()[0].content, "older question (fixed)");
}

#[test]
fn long_edited_preview_truncates_like_native_previews() {
    let fx = seed_dir_conv(&[msg("conv-14", "m1", "user", "short")]);
    let long_text = "x".repeat(200);
    append_edit_record_in_dir(&fx.dir, &fx.conv_id, "m1", &long_text).unwrap();
    let meta: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&fx.raw_meta_path).unwrap()).unwrap();
    let preview = meta["last_message_preview"].as_str().unwrap();
    assert!(preview.ends_with("..."), "truncation shape must match native previews: {preview}");
    assert_eq!(preview.chars().count(), 60);
}

// ==================== 11. budget loader folds edits ====================

#[test]
fn budget_loader_returns_folded_content() {
    let fx = seed_dir_conv(&[
        msg("conv-15", "m1", "user", "question"),
        msg("conv-15", "m2", "assistant", "short answer"),
    ]);
    // Give m1 an explicit token cost so the budget walk can exclude it.
    let with_cost = {
        let mut m = msg("conv-15", "m1", "user", "question");
        m.token_count = 10;
        m
    };
    fs::write(
        &fx.raw_msgs_path,
        format!("{}{}", msg_line(&with_cost), msg_line(&msg("conv-15", "m2", "assistant", "short answer"))),
    )
    .unwrap();
    append_edit_record_in_dir(&fx.dir, &fx.conv_id, "m2", "brand new answer").unwrap();

    // Generous budget: every message present, fold applied.
    let all = load_messages_within_token_budget_in_dir(&fx.dir, 4096).unwrap();
    assert_eq!(all.len(), 2);
    assert_eq!(all.iter().find(|m| m.id == "m2").unwrap().content, "brand new answer");

    // Tiny budget: window is the tail — still the FOLDED tail.
    // m2 estimate = (16+3)/4 = 4 tokens; adding m1 (10) exceeds 6 → break.
    let tail = load_messages_within_token_budget_in_dir(&fx.dir, 6).unwrap();
    assert_eq!(tail.len(), 1);
    assert_eq!(tail[0].id, "m2");
    assert_eq!(
        tail[0].content, "brand new answer",
        "inference must see the edited content, never stale raw text"
    );
    assert!(tail[0].edited);
}

// ==================== 12. raw rows never carry view-only flags ====================

#[test]
fn raw_parser_preserves_provenance_flags_and_view_clears_them() {
    let fx = seed_dir_conv(&[msg("conv-16", "m1", "user", "q")]);
    // Persisted partial turn (B1 shape) instead of a clean row.
    let mut partial = msg("conv-16", "m1", "user", "q");
    partial.incomplete = true;
    fs::write(&fx.raw_msgs_path, msg_line(&partial)).unwrap();

    append_edit_record_in_dir(&fx.dir, &fx.conv_id, "m1", "user-completed turn").unwrap();

    let raw = fx.raw_msgs();
    let raw_m1 = raw.iter().find(|m| m.id == "m1").unwrap();
    assert_eq!(raw_m1.content, "q", "raw content must stay the original fragment");
    assert!(raw_m1.incomplete, "raw provenance keeps the B1 marker");
    // `edited` is a VIEW-only flag; raw rows never carry it.
    assert!(!raw.iter().any(|m| m.edited));

    // View clears incomplete and flags edited.
    let folded = fx.folded();
    let folded_m1 = folded.iter().find(|m| m.id == "m1").unwrap();
    assert_eq!(folded_m1.content, "user-completed turn");
    assert!(!folded_m1.incomplete);
    assert!(folded_m1.edited);
}

// ==================== 13. stale revisions after truncate-regen ====================

#[test]
fn stale_revision_records_never_resurrect_truncated_turns() {
    let fx = seed_dir_conv(&[
        msg("conv-17", "m1", "user", "q1"),
        msg("conv-17", "m2", "assistant", "a1"),
    ]);
    append_edit_record_in_dir(&fx.dir, &fx.conv_id, "m2", "a1-fixed").unwrap();

    // Simulate a destructive regen that DROPPED the m2 row entirely.
    fs::write(&fx.raw_msgs_path, msg_line(&msg("conv-17", "m1", "user", "q1"))).unwrap();

    let folded = fx.folded();
    assert_eq!(folded.len(), 1, "no ghost row may be resurrected by a stale revision");
    assert_eq!(folded[0].id, "m1");
    assert_eq!(folded[0].content, "q1");
}

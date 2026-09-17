//! Property-Based Tests (proptest) — Fuzzing for invariant verification.
//!
//! These tests verify that core functions maintain their invariants
//! under arbitrary input, not just hand-crafted happy paths.

use proptest::prelude::*;

// Import the functions we want to test
use localpersona::conversation::validate_conv_id;
use localpersona::storage::{validate_character_id, validate_owner_id};

// ==================== validate_conv_id ====================

proptest! {
    #[test]
    fn conv_id_rejects_double_dots(s in ".*\\.\\..*") {
        // Any string containing ".." should be rejected
        prop_assert!(validate_conv_id(&s).is_err(), "Should reject: {}", s);
    }

    #[test]
    fn conv_id_rejects_slashes(s in ".*/.*") {
        // Any string containing / should be rejected
        prop_assert!(validate_conv_id(&s).is_err(), "Should reject: {}", s);
    }

    #[test]
    fn conv_id_rejects_backslashes(s in ".*\\\\.*") {
        // Any string containing \ should be rejected
        prop_assert!(validate_conv_id(&s).is_err(), "Should reject: {}", s);
    }

    #[test]
    fn conv_id_accepts_alphanumeric(s in "[a-zA-Z0-9_-]{1,64}") {
        // Pure alphanumeric with hyphens/underscores should be accepted
        prop_assert!(validate_conv_id(&s).is_ok(), "Should accept: {}", s);
    }

    #[test]
    fn conv_id_rejects_empty(s in "") {
        prop_assert!(validate_conv_id(&s).is_err());
    }
}

// ==================== validate_character_id ====================

proptest! {
    #[test]
    fn char_id_rejects_traversal(s in ".*\\.\\..*") {
        prop_assert!(validate_character_id(&s).is_err(), "Should reject: {}", s);
    }

    #[test]
    fn char_id_rejects_slashes(s in ".*/.*") {
        prop_assert!(validate_character_id(&s).is_err(), "Should reject: {}", s);
    }

    // Safe charset without consecutive dots: validators correctly reject ".." path traversal.
    // Generator must not emit ".." or the accepts_safe property is contradictory.
    #[test]
    fn char_id_accepts_safe(s in "[a-zA-Z0-9_\\-]{1,64}") {
        prop_assert!(validate_character_id(&s).is_ok(), "Should accept: {}", s);
    }

    #[test]
    fn char_id_rejects_special_chars(s in "[^a-zA-Z0-9_\\-\\.]+") {
        // Filter empty: the charset can produce empty strings which validators also reject,
        // but "only special chars" property should stay meaningful.
        prop_assume!(!s.is_empty());
        prop_assert!(validate_character_id(&s).is_err(), "Should reject: {}", s);
    }
}

// ==================== validate_owner_id ====================

proptest! {
    #[test]
    fn owner_id_rejects_traversal(s in ".*\\.\\..*") {
        prop_assert!(validate_owner_id(&s).is_err(), "Should reject: {}", s);
    }

    #[test]
    fn owner_id_accepts_safe(s in "[a-zA-Z0-9_\\-]{1,64}") {
        prop_assert!(validate_owner_id(&s).is_ok(), "Should accept: {}", s);
    }
}

// ==================== safe_truncate ====================

proptest! {
    #[test]
    fn truncate_never_panics(s in ".*", max_chars in 0usize..1000) {
        // safe_truncate should never panic regardless of input
        let _ = localpersona::conversation::safe_truncate(&s, max_chars);
    }

    #[test]
    fn truncate_output_len_at_most_max(s in ".*", max_chars in 0usize..500) {
        let result = localpersona::conversation::safe_truncate(&s, max_chars);
        prop_assert!(
            result.chars().count() <= max_chars,
            "Truncated string {} has {} chars, max was {}",
            result,
            result.chars().count(),
            max_chars
        );
    }

    #[test]
    fn truncate_preserves_content(s in "[a-zA-Z0-9 ]{1,100}", max_chars in 1usize..100) {
        let result = localpersona::conversation::safe_truncate(&s, max_chars);
        // The truncated string should be a prefix of the original
        prop_assert!(
            s.starts_with(&result),
            "Truncated '{}' is not a prefix of '{}'",
            result,
            s
        );
    }
}

// ==================== Circuit Breaker ====================

proptest! {
    #[test]
    fn circuit_breaker_opens_after_n_failures(count in 1usize..100) {
        let mut cb = localpersona::circuit_breaker::CircuitBreaker::new(3, std::time::Duration::from_secs(60));

        for _ in 0..count {
            cb.record_failure("test_hash");
        }

        if count >= 3 {
            prop_assert!(!cb.should_allow("test_hash"), "Circuit should be OPEN after {} failures", count);
        }
    }

    #[test]
    fn circuit_breaker_resets_on_success(count in 1usize..100) {
        let mut cb = localpersona::circuit_breaker::CircuitBreaker::new(3, std::time::Duration::from_secs(60));

        for _ in 0..count {
            cb.record_failure("test_hash");
        }
        cb.record_success("test_hash");

        prop_assert!(cb.should_allow("test_hash"), "Circuit should be CLOSED after success");
    }
}

// ==================== atomic_delete ====================

proptest! {
    #[test]
    fn atomic_delete_removes_file(content in "[a-zA-Z0-9 ]{1,100}") {
        let ts = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let dir = std::env::temp_dir().join(format!("proptest_atomic_del_{}", ts));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let file_path = dir.join("test.txt");
        std::fs::write(&file_path, &content).unwrap();
        prop_assert!(file_path.exists());

        let result = localpersona::storage::atomic_delete(&file_path);
        prop_assert!(result.is_ok(), "atomic_delete should succeed: {:?}", result);
        prop_assert!(!file_path.exists(), "File should be removed after atomic_delete");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn atomic_delete_noop_on_missing(path in "[a-zA-Z0-9_/]{1,50}") {
        let p = std::path::Path::new(&path);
        if !p.exists() {
            let result = localpersona::storage::atomic_delete(p);
            prop_assert!(result.is_ok(), "atomic_delete on missing path should succeed: {:?}", result);
        }
    }
}

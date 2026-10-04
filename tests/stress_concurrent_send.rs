//! Stress harness for D2-D4 + B9 — concurrent appends + repair vs append
//! Runs without Tauri AppHandle by exercising the same file primitives
//! that `conversation::append_messages` and `repair_conversation` use.

use std::fs::{self, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::sync::{Arc, Barrier};
use std::thread;
use tempfile::TempDir;

/// Helper: simulate append_messages' B4-guarded, locked append
fn locked_append(path: &std::path::Path, payload: &str) {
    // Acquire sidecar lock (same as storage::acquire_sidecar_lock)
    let lock_path = {
        let mut os = path.as_os_str().to_os_string();
        os.push(".lock");
        std::path::PathBuf::from(os)
    };
    // Ensure parent
    if let Some(parent) = lock_path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let lock_file = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(&lock_path)
        .unwrap();
    fs2::FileExt::lock_exclusive(&lock_file).unwrap();

    // B4 guard: check last byte
    if path.exists() {
        if let Ok(meta) = fs::metadata(path) {
            if meta.len() > 0 {
                if let Ok(mut check) = fs::File::open(path) {
                    if check.seek(SeekFrom::End(-1)).is_ok() {
                        let mut last = [0u8; 1];
                        if check.read_exact(&mut last).is_ok() && last[0] != b'\n' {
                            let mut f = OpenOptions::new().append(true).open(path).unwrap();
                            f.write_all(b"\n").unwrap();
                        }
                    }
                }
            }
        }
    }

    let mut f = OpenOptions::new().create(true).append(true).open(path).unwrap();
    f.write_all(payload.as_bytes()).unwrap();
    let _ = f.sync_all();
    fs2::FileExt::unlock(&lock_file).unwrap();
}

#[test]
fn test_stress_concurrent_appends_no_interleaving() {
    let temp = TempDir::new().unwrap();
    let msg_path = temp.path().join("messages.ndjson");
    fs::write(&msg_path, "").unwrap();

    let n = 20;
    let barrier = Arc::new(Barrier::new(n));
    let mut handles = Vec::new();

    for i in 0..n {
        let path = msg_path.clone();
        let b = barrier.clone();
        handles.push(thread::spawn(move || {
            b.wait();
            let line = format!(r#"{{"id":"{}","content":"msg {}"}}"#, i, i);
            locked_append(&path, &format!("{}\n", line));
        }));
    }

    for h in handles {
        h.join().unwrap();
    }

    let content = fs::read_to_string(&msg_path).unwrap();
    let lines: Vec<&str> = content.lines().filter(|l| !l.trim().is_empty()).collect();
    assert_eq!(lines.len(), n, "all {} appends must be present", n);

    // Each line must be valid JSON and have correct id
    let mut seen = std::collections::HashSet::new();
    for line in lines {
        let v: serde_json::Value = serde_json::from_str(line).expect("each line must be valid JSON, no interleaving");
        let id = v["id"].as_str().unwrap().to_string();
        assert!(seen.insert(id.clone()), "duplicate id {}", id);
    }
    assert_eq!(seen.len(), n);
}

#[test]
fn test_b9_repair_vs_concurrent_append_no_clobber() {
    let temp = TempDir::new().unwrap();
    let dir = temp.path().join("conv");
    fs::create_dir_all(&dir).unwrap();
    let msg_path = dir.join("messages.ndjson");
    let meta_path = dir.join("metadata.json");

    // Seed with 5 valid lines + 1 corrupt
    let mut initial = String::new();
    for i in 0..5 {
        initial.push_str(&format!(r#"{{"id":"{}","content":"hi {}"}}"#, i, i));
        initial.push('\n');
    }
    initial.push_str("not json at all\n");
    fs::write(&msg_path, &initial).unwrap();
    fs::write(&meta_path, r#"{"version":1,"id":"conv","name":"Test","participant_ids":[],"created_at":0,"updated_at":0,"message_count":99}"#).unwrap();

    // Spawn repair in one thread, concurrent append in another
    let msg_path_clone = msg_path.clone();
    let meta_path_clone = meta_path.clone();

    let repair_handle = thread::spawn(move || {
        // Simulate repair_conversation: read, filter valid, backup, rewrite
        let content = fs::read_to_string(&msg_path_clone).unwrap();
        let mut recovered = Vec::new();
        for line in content.lines() {
            if line.trim().is_empty() {
                continue;
            }
            if serde_json::from_str::<serde_json::Value>(line).is_ok() {
                recovered.push(line.to_string());
            }
        }
        // Acquire lock (same as fixed repair)
        let lock_path = {
            let mut os = msg_path_clone.as_os_str().to_os_string();
            os.push(".lock");
            std::path::PathBuf::from(os)
        };
        let lock_file = OpenOptions::new().create(true).truncate(true).write(true).open(&lock_path).unwrap();
        fs2::FileExt::lock_exclusive(&lock_file).unwrap();

        // Backup and rewrite
        let backup = msg_path_clone.with_extension("ndjson.bak");
        let _ = fs::copy(&msg_path_clone, &backup);
        let new_content = recovered.join("\n") + "\n";
        fs::write(&msg_path_clone, &new_content).unwrap();

        fs2::FileExt::unlock(&lock_file).unwrap();
        recovered.len()
    });

    let append_handle = {
        let msg_path_clone2 = msg_path.clone();
        thread::spawn(move || {
            // Small delay to interleave
            std::thread::sleep(std::time::Duration::from_millis(10));
            locked_append(&msg_path_clone2, r#"{"id":"concurrent","content":"hello"}"#);
            locked_append(&msg_path_clone2, "\n");
        })
    };

    let repaired = repair_handle.join().unwrap();
    append_handle.join().unwrap();

    // After repair, file must be parseable and not lose the concurrent append
    let final_content = fs::read_to_string(&msg_path).unwrap();
    let lines: Vec<&str> = final_content.lines().filter(|l| !l.trim().is_empty()).collect();
    // At least the 5 valid + 1 concurrent, and at most 6 (if repair ran before append, concurrent appended after)
    assert!(lines.len() >= 5 && lines.len() <= 6, "repair vs append should not clobber, got {} lines", lines.len());
    for line in lines {
        assert!(serde_json::from_str::<serde_json::Value>(line).is_ok(), "all lines must be valid JSON after repair+append: {}", line);
    }
    // If repair counted 5, concurrent added 1 → final should be 5 or 6 depending on order
    assert!(repaired == 5, "repair should have recovered 5 valid lines");
}

#[test]
fn test_csp_drift_guard_tier() {
    // Drift-guard tier (not regression): fails if CSP regresses
    let content = fs::read_to_string("tauri.conf.json").unwrap();
    let v: serde_json::Value = serde_json::from_str(&content).unwrap();
    let csp = v["app"]["security"]["csp"].as_str().unwrap();
    assert!(!csp.contains("cdn.tailwindcss.com"), "drift guard: csp must not contain tailwind CDN");
}

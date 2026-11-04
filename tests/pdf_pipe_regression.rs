//! Regression: PDF worker pipe deadlock (P1-1, found in the 0.9.0-rc.1 audit).
//!
//! Original defect: `extract_text_from_pdf` spawned the worker with piped
//! stdout and then polled `try_wait()` in a loop **without reading the pipe**.
//! Any extraction larger than the OS pipe buffer (~64 KiB on Linux) blocked the
//! child on `write()` forever; the parent hit its 30 s timeout and reported the
//! user's perfectly valid book as a "hostile file".
//!
//! This lives in its own integration-test binary because it mutates
//! `LOCALPERSONA_PDF_WORKER_BIN` process-wide, which would race with
//! `tests/pdf_isolation.rs` if they shared a process.

#![cfg(unix)]

use std::io::Write as _;
use std::path::PathBuf;
use std::sync::Mutex;

/// `LOCALPERSONA_PDF_WORKER_BIN` is process-wide, and cargo runs tests in
/// threads inside one process. Serialize the mutation so each case observes
/// its own worker.
static ENV_LOCK: Mutex<()> = Mutex::new(());

fn temp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "lp-pdfpipe-{}-{}",
        tag,
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    std::fs::create_dir_all(&dir).expect("temp dir");
    dir
}

/// Writes an executable shell script that ignores its args and emits `bytes`
/// of filler on stdout, then exits 0. Stands in for a worker whose extraction
/// is larger than the pipe buffer.
fn fake_worker(dir: &PathBuf, tag: &str, bytes: usize) -> PathBuf {
    let path = dir.join(format!("fake-worker-{tag}.sh"));
    let mut f = std::fs::File::create(&path).expect("create script");
    // head -c avoids shell-quoting issues and streams without buffering all
    // of it first, which is exactly the behaviour we need to reproduce.
    writeln!(f, "#!/bin/sh").unwrap();
    writeln!(f, "exec head -c {} /dev/zero | tr '\\000' 'x'", bytes).unwrap();
    f.sync_all().unwrap();
    drop(f);
    use std::os::unix::fs::PermissionsExt;
    let mut perms = std::fs::metadata(&path).expect("stat script").permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(&path, perms).expect("chmod script");
    path
}

fn dummy_pdf(dir: &PathBuf) -> PathBuf {
    let path = dir.join("input.pdf");
    std::fs::write(&path, b"%PDF-1.4\n% fixture; content is irrelevant to the fake worker\n")
        .expect("write pdf fixture");
    path
}

/// The headline regression: 256 KiB of extracted text (4x the Linux pipe
/// buffer) must come back intact instead of timing out.
#[test]
fn large_extraction_does_not_deadlock() {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = temp_dir("large");
    let worker = fake_worker(&dir, "large", 256 * 1024);
    std::env::set_var("LOCALPERSONA_PDF_WORKER_BIN", &worker);

    let pdf = dummy_pdf(&dir);
    let started = std::time::Instant::now();
    let text = localpersona::storage::extract_text_from_pdf(&pdf)
        .unwrap_or_else(|e| panic!("256 KiB extraction must succeed, got: {e}"));

    assert_eq!(text.len(), 256 * 1024, "all extracted bytes must survive");
    assert!(
        text.bytes().all(|b| b == b'x'),
        "content must not be corrupted by the drain path"
    );
    assert!(
        started.elapsed() < std::time::Duration::from_secs(20),
        "must not sit on the 30s timeout; took {:?}",
        started.elapsed()
    );
}

/// Draining must continue past the 32 MiB cap rather than stalling the child.
/// Oversize output is still rejected, but as a clean bound violation — not a
/// timeout, and not a hang.
#[test]
fn oversize_extraction_is_rejected_not_hung() {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = temp_dir("oversize");
    let worker = fake_worker(&dir, "oversize", 40 * 1024 * 1024);
    std::env::set_var("LOCALPERSONA_PDF_WORKER_BIN", &worker);

    let pdf = dummy_pdf(&dir);
    let started = std::time::Instant::now();
    let err = localpersona::storage::extract_text_from_pdf(&pdf)
        .expect_err("40 MiB extraction must be rejected");

    assert!(
        err.contains("sane bounds"),
        "expected a bounds rejection, got: {err}"
    );
    assert!(
        !err.contains("timed out"),
        "must not be reported as a timeout (that was the old bug): {err}"
    );
    assert!(
        started.elapsed() < std::time::Duration::from_secs(25),
        "must fail fast, took {:?}",
        started.elapsed()
    );
}

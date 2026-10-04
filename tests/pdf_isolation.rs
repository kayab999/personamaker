//! Sweep W1: PDF worker isolation tests (F-02 residual, RUSTSEC-2026-0187).
//!
//! Each case spawns the disposable `--extract-pdf` worker and asserts the
//! hostile document is REJECTED while the parent (this test process) stays
//! alive to run the next case. A stack-overflow in the child dies by signal;
//! the parent maps any non-zero exit / timeout to Err.

use std::io::Write as _;
use std::path::{Path, PathBuf};

fn worker_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_localpersona"))
}

fn pin_worker() {
    // Same value on every call => idempotent under parallel test threads.
    std::env::set_var("LOCALPERSONA_PDF_WORKER_BIN", worker_bin());
}

fn write_fixture(name: &str, bytes: &[u8]) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "lp-pdfiso-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(name);
    let mut f = std::fs::File::create(&path).unwrap();
    f.write_all(bytes).unwrap();
    f.sync_all().unwrap();
    path
}

fn extract(path: &Path) -> Result<String, String> {
    pin_worker();
    localpersona::storage::extract_text_from_pdf(path)
}

#[test]
fn hostile_garbage_rejected_parent_alive() {
    // 8 KiB of non-PDF bytes.
    let mut junk = Vec::with_capacity(8192);
    for i in 0..8192u64 {
        junk.push(((i.wrapping_mul(2654435761)) % 251) as u8);
    }
    let path = write_fixture("garbage.pdf", &junk);
    let res = extract(&path);
    assert!(res.is_err(), "garbage must be rejected, got Ok");
    // Parent-survival proof: we are still here asserting.
    assert!(path.exists());
}

#[test]
fn hostile_truncated_rejected_parent_alive() {
    let path = write_fixture("truncated.pdf", b"%PDF-1.4\n1 0 obj\n<< /Type /Cata");
    let res = extract(&path);
    assert!(res.is_err(), "truncated PDF must be rejected, got Ok");
}

#[test]
fn hostile_deep_nesting_rejected_parent_alive() {
    // RUSTSEC-2026-0187 shape: pathologically nested array objects.
    // In-process this risks stack exhaustion; in the worker it dies alone.
    let depth = 100_000usize;
    let mut doc = b"%PDF-1.4\n1 0 obj\n".to_vec();
    doc.extend(std::iter::repeat(b'[').take(depth));
    doc.extend(std::iter::repeat(b']').take(depth));
    doc.extend_from_slice(b"\nendobj\ntrailer\n<< /Root 1 0 R >>\n");
    let path = write_fixture("deepnest.pdf", &doc);
    let res = extract(&path);
    assert!(res.is_err(), "deep-nesting PDF must be rejected, got Ok");
}

#[test]
fn valid_minimal_pdf_extracts() {
    // Minimal but xref-valid PDF (offsets computed programmatically).
    let stream = b"BT /F1 12 Tf 50 150 Td (Hello isolation) Tj ET\n";
    let mut doc: Vec<u8> = Vec::new();
    let mut offsets: Vec<usize> = Vec::new();
    let mut obj = |doc: &mut Vec<u8>, offsets: &mut Vec<usize>, body: &[u8]| {
        offsets.push(doc.len());
        doc.extend_from_slice(body);
    };
    doc.extend_from_slice(b"%PDF-1.4\n");
    obj(
        &mut doc,
        &mut offsets,
        b"1 0 obj<</Type/Catalog/Pages 2 0 R>>endobj\n",
    );
    obj(
        &mut doc,
        &mut offsets,
        b"2 0 obj<</Type/Pages/Kids[3 0 R]/Count 1>>endobj\n",
    );
    obj(
        &mut doc,
        &mut offsets,
        b"3 0 obj<</Type/Page/Parent 2 0 R/MediaBox[0 0 200 200]/Contents 4 0 R/Resources<</Font<</F1 5 0 R>>>>>>endobj\n",
    );
    obj(
        &mut doc,
        &mut offsets,
        format!("4 0 obj<</Length {}>>stream\n", stream.len()).as_bytes(),
    );
    // Note: stream bytes are part of object 4 (offsets point at "4 0 obj").
    doc.extend_from_slice(stream);
    doc.extend_from_slice(b"endstream\nendobj\n");
    obj(
        &mut doc,
        &mut offsets,
        b"5 0 obj<</Type/Font/Subtype/Type1/BaseFont/Helvetica>>endobj\n",
    );
    let xref_pos = doc.len();
    doc.extend_from_slice(format!("xref\n0 {}\n", offsets.len() + 1).as_bytes());
    doc.extend_from_slice(b"0000000000 65535 f \n");
    for off in &offsets {
        doc.extend_from_slice(format!("{:010} 00000 n \n", off).as_bytes());
    }
    doc.extend_from_slice(
        format!(
            "trailer<</Size {}/Root 1 0 R>>\nstartxref\n{}\n%%EOF\n",
            offsets.len() + 1,
            xref_pos
        )
        .as_bytes(),
    );
    let path = write_fixture("hello.pdf", &doc);
    let res = extract(&path);
    let text = res.unwrap_or_else(|e| panic!("valid PDF must extract, got Err: {}", e));
    assert!(
        text.contains("Hello isolation"),
        "expected extracted greeting, got: {:?}",
        text.chars().take(200).collect::<String>()
    );
}

#[test]
fn oversize_pdf_refused_without_spawn() {
    // Pre-check path: file over the cap is rejected before any worker spawn.
    // (Sparse file: size metadata huge, disk cost ~0.)
    let dir = std::env::temp_dir().join(format!(
        "lp-pdfiso-big-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("big.pdf");
    let f = std::fs::File::create(&path).unwrap();
    f.set_len(300 * 1024 * 1024).unwrap();
    drop(f);
    pin_worker();
    let res = localpersona::storage::extract_text_from_pdf(&path);
    assert!(res.is_err());
    assert!(res.unwrap_err().contains("too large"));
}

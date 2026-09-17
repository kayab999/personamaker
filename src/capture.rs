// src/capture.rs — audit harness A: env-guarded no-op prompt/response capture.
// Register `mod capture;` in BOTH main.rs and lib.rs.
use std::io::Write;
use std::sync::Mutex;

static LOCK: Mutex<()> = Mutex::new(());

pub fn capture(entry: &serde_json::Value) {
    let Ok(path) = std::env::var("LOCALPERSONA_CAPTURE_PROMPTS") else {
        return;
    };
    let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let Ok(f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
    else {
        return;
    };
    let _ = writeln!(std::io::BufWriter::new(f), "{entry}");
}

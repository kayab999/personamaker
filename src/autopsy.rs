//! AutopsyDump — Structured crash dump system for forensic analysis.
//!
//! When a panic or unrecoverable error occurs, this module generates a JSON dump
//! containing process state, memory usage, and context for post-mortem analysis.
//! Dumps are stored in `app_data/autopsy/` as immutable forensic evidence.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tauri::AppHandle;
use tauri::Manager;

/// A structured crash dump for forensic analysis.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutopsyDump {
    /// ISO 8601 timestamp of the crash
    pub timestamp: String,
    /// Process ID that crashed
    pub pid: u32,
    /// Type of failure (panic, signal, timeout, etc.)
    pub failure_type: String,
    /// Human-readable error message
    pub message: String,
    /// Optional backtrace (if RUST_BACKTRACE=1)
    pub backtrace: Option<String>,
    /// Current memory usage in KB at time of crash
    pub memory_usage_kb: Option<u64>,
    /// Server status at time of crash (running/starting/stopped)
    pub server_status: Option<String>,
    /// Active conversation ID if any
    pub conversation_id: Option<String>,
    /// Module where the crash originated
    pub source_module: String,
}

impl AutopsyDump {
    /// Creates a new autopsy dump from a panic info.
    pub fn from_panic(info: &std::panic::PanicHookInfo) -> Self {
        let pid = std::process::id();
        let timestamp = chrono_timestamp();
        let message = info.payload().downcast_ref::<&str>().unwrap_or(&"unknown panic").to_string();
        let location = info.location()
            .map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column()))
            .unwrap_or_else(|| "unknown location".to_string());

        Self {
            timestamp,
            pid,
            failure_type: "panic".to_string(),
            message: format!("{} at {}", message, location),
            backtrace: std::env::var("RUST_BACKTRACE").ok().and_then(|_| {
                // Only capture backtrace if RUST_BACKTRACE is set
                Some(format!("Backtrace requested (RUST_BACKTRACE=1) — check stderr for full trace"))
            }),
            memory_usage_kb: read_process_memory_kb(),
            server_status: None,
            conversation_id: None,
            source_module: location,
        }
    }

    /// Creates a new autopsy dump for a process failure.
    pub fn from_process_failure(failure_type: &str, message: &str, source_module: &str) -> Self {
        Self {
            timestamp: chrono_timestamp(),
            pid: std::process::id(),
            failure_type: failure_type.to_string(),
            message: message.to_string(),
            backtrace: None,
            memory_usage_kb: read_process_memory_kb(),
            server_status: None,
            conversation_id: None,
            source_module: source_module.to_string(),
        }
    }

    /// Sets the server status context.
    pub fn with_server_status(mut self, status: &str) -> Self {
        self.server_status = Some(status.to_string());
        self
    }

    /// Sets the active conversation context.
    pub fn with_conversation(mut self, conv_id: &str) -> Self {
        self.conversation_id = Some(conv_id.to_string());
        self
    }
}

/// Writes an autopsy dump to disk as an immutable JSON file.
/// Returns the path where the dump was written.
pub fn write_autopsy_dump(app: &AppHandle, dump: &AutopsyDump) -> Result<PathBuf, String> {
    let app_data = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let autopsy_dir = app_data.join("autopsy");
    std::fs::create_dir_all(&autopsy_dir).map_err(|e| e.to_string())?;

    let filename = format!("{}_pid{}.json", dump.timestamp.replace(':', "-"), dump.pid);
    let path = autopsy_dir.join(&filename);

    let json = serde_json::to_string_pretty(dump).map_err(|e| e.to_string())?;
    crate::storage::atomic_write(&path, &json)?;

    log::error!("AutopsyDump written to: {}", path.display());
    Ok(path)
}

/// Lists all autopsy dumps, most recent first.
pub fn list_autopsy_dumps(app: &AppHandle) -> Result<Vec<AutopsyDump>, String> {
    let app_data = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let autopsy_dir = app_data.join("autopsy");

    if !autopsy_dir.exists() {
        return Ok(vec![]);
    }

    let mut dumps = Vec::new();
    for entry in std::fs::read_dir(&autopsy_dir).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) == Some("json") {
            if let Ok(content) = std::fs::read_to_string(&path) {
                if let Ok(dump) = serde_json::from_str::<AutopsyDump>(&content) {
                    dumps.push(dump);
                }
            }
        }
    }

    dumps.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
    Ok(dumps)
}

/// Installs a custom panic hook that generates AutopsyDump on panic.
pub fn install_panic_hook() {
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let dump = AutopsyDump::from_panic(info);

        // Write to stderr (always works)
        eprintln!("╔══════════════════════════════════════════╗");
        eprintln!("║         AUTOPSY DUMP GENERATED           ║");
        eprintln!("╠══════════════════════════════════════════╣");
        eprintln!("║ Type: {}", dump.failure_type);
        eprintln!("║ PID:  {}", dump.pid);
        eprintln!("║ Msg:  {}", dump.message);
        eprintln!("║ Src:  {}", dump.source_module);
        if let Some(mem) = dump.memory_usage_kb {
            eprintln!("║ RSS:  {} KB", mem);
        }
        eprintln!("╚══════════════════════════════════════════╝");

        // Try to write to disk (best effort — may fail if the filesystem is the problem)
        if let Some(app_data) = dirs::data_local_dir() {
            let autopsy_dir = app_data.join("LocalPersona").join("autopsy");
            let _ = std::fs::create_dir_all(&autopsy_dir);
            let filename = format!("{}_pid{}.json", dump.timestamp.replace(':', "-"), dump.pid);
            let path = autopsy_dir.join(&filename);
            if let Ok(json) = serde_json::to_string_pretty(&dump) {
                // Use atomic write (tempfile + rename) to prevent corruption if panic recurs during write
                let tmp_path = path.with_extension("json.tmp");
                if std::fs::write(&tmp_path, &json).is_ok() {
                    let _ = std::fs::rename(&tmp_path, &path);
                }
                eprintln!("Dump written to: {}", path.display());
            }
        }

        // Call the default hook (prints to stderr with backtrace)
        default_hook(info);
    }));
}

/// Reads current process RSS memory in KB from /proc/self/status (Linux only).
fn read_process_memory_kb() -> Option<u64> {
    #[cfg(target_os = "linux")]
    {
        let status = std::fs::read_to_string("/proc/self/status").ok()?;
        for line in status.lines() {
            if line.starts_with("VmRSS:") {
                let kb: u64 = line.split_whitespace().nth(1)?.parse().ok()?;
                return Some(kb);
            }
        }
    }
    None
}

/// Returns a simple timestamp string for filenames using ISO 8601 date.
fn chrono_timestamp() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    // Simple UTC timestamp: YYYYMMDD_HHMMSS using days-since-epoch with leap-year-aware calculation
    let days = secs / 86400;
    let time_of_day = secs % 86400;
    let hours = time_of_day / 3600;
    let minutes = (time_of_day % 3600) / 60;
    let seconds = time_of_day % 60;

    // Civil date from days since epoch (accounts for leap years)
    // Algorithm: https://howardhinnant.github.io/date_algorithms.html
    let z = days as i64 + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let year = (if m <= 2 { y + 1 } else { y }) as u32;

    format!("{:04}{:02}{:02}_{:02}{:02}{:02}", year, m, d, hours, minutes, seconds)
}

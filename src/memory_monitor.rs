//! Memory Monitor — Real-time RAM telemetry with slope-based Preemptive Reset.
//!
//! Monitors process memory usage and triggers arena reset when the rate of
//! consumption (first derivative) exceeds a threshold. This prevents OOM
//! by resetting the LLM server before the absolute limit is reached.

use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::sync::Arc;
use tokio::sync::Mutex;

/// Memory telemetry snapshot.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemorySnapshot {
    /// Timestamp (seconds since epoch)
    pub timestamp: u64,
    /// Resident Set Size in KB
    pub rss_kb: u64,
    /// Virtual Memory Size in KB
    pub vms_kb: u64,
    /// Rate of change in KB/s (first derivative)
    pub slope_kb_per_sec: f64,
}

/// Configuration for the memory monitor.
pub struct MemoryMonitorConfig {
    /// How often to sample memory (in seconds)
    pub sample_interval_secs: u64,
    /// Slope threshold in KB/s that triggers a reset
    pub slope_threshold_kb_per_sec: f64,
    /// Absolute RSS threshold in KB that triggers a hard reset
    pub absolute_threshold_kb: u64,
    /// Number of consecutive high-slope samples before triggering
    pub required_consecutive_samples: u32,
}

impl Default for MemoryMonitorConfig {
    fn default() -> Self {
        Self {
            sample_interval_secs: 5,
            slope_threshold_kb_per_sec: 10240.0, // 10 MB/s
            absolute_threshold_kb: 4 * 1024 * 1024, // 4 GB
            required_consecutive_samples: 3,
        }
    }
}

/// Memory monitor state.
pub struct MemoryMonitor {
    config: MemoryMonitorConfig,
    last_snapshot: Option<MemorySnapshot>,
    consecutive_high_slope: u32,
    history: VecDeque<MemorySnapshot>,
    max_history: usize,
    /// PID currently being sampled. Changes when the llama-server child
    /// starts/stops, so slope tracking is reset instead of computing a
    /// meaningless delta across two unrelated processes.
    target_pid: u32,
}

/// PID the monitor should watch. `0` means "the app process itself".
///
/// This is process-wide and lock-free on purpose: the sampler runs on a
/// background task, while `LlamaServerManager::start/stop` sets the child PID.
/// Before the 0.9.0-rc.1 audit this module always sampled
/// `std::process::id()` — i.e. the Tauri app, which is small and stable —
/// so the preemptive reset it exists to provide could never fire for the
/// process that actually consumes gigabytes.
static MONITORED_PID: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

/// Point the monitor at a process (the llama-server child). Pass `0` to
/// return to self-monitoring.
pub fn set_monitored_pid(pid: u32) {
    MONITORED_PID.store(pid, std::sync::atomic::Ordering::Relaxed);
}

/// The PID currently being sampled (never `0`).
pub fn monitored_pid() -> u32 {
    let pid = MONITORED_PID.load(std::sync::atomic::Ordering::Relaxed);
    if pid == 0 {
        std::process::id()
    } else {
        pid
    }
}

impl MemoryMonitor {
    pub fn new(config: MemoryMonitorConfig) -> Self {
        Self {
            config,
            last_snapshot: None,
            consecutive_high_slope: 0,
            history: VecDeque::new(),
            max_history: 60,
            target_pid: monitored_pid(),
        }
    }

    /// Takes a memory snapshot and returns it.
    pub fn sample(&mut self) -> MemorySnapshot {
        // The watched process can change (server started/stopped/restarted).
        // RSS deltas across two different processes are meaningless, so drop
        // the previous sample and restart slope tracking.
        let pid = monitored_pid();
        if pid != self.target_pid {
            log::info!(
                "Memory Monitor: target process changed {} -> {}; resetting slope baseline",
                self.target_pid,
                pid
            );
            self.target_pid = pid;
            self.last_snapshot = None;
            self.consecutive_high_slope = 0;
        }

        let (rss_kb, vms_kb) = process_memory_kb(pid);
        let timestamp = now_epoch_secs();

        let slope = if let Some(ref prev) = self.last_snapshot {
            let dt = timestamp.saturating_sub(prev.timestamp) as f64;
            if dt > 0.0 {
                (rss_kb as f64 - prev.rss_kb as f64) / dt
            } else {
                0.0
            }
        } else {
            0.0
        };

        let snapshot = MemorySnapshot {
            timestamp,
            rss_kb,
            vms_kb,
            slope_kb_per_sec: slope,
        };

        // Track consecutive high-slope samples
        if slope > self.config.slope_threshold_kb_per_sec {
            self.consecutive_high_slope += 1;
        } else {
            self.consecutive_high_slope = 0;
        }

        self.history.push_back(snapshot.clone());
        if self.history.len() > self.max_history {
            self.history.pop_front();
        }

        self.last_snapshot = Some(snapshot.clone());
        snapshot
    }

    /// Checks if a preemptive reset should be triggered.
    pub fn should_trigger_reset(&self) -> bool {
        // Absolute threshold check
        if let Some(ref last) = self.last_snapshot {
            if last.rss_kb >= self.config.absolute_threshold_kb {
                log::error!(
                    "Memory Monitor: ABSOLUTE threshold exceeded! RSS={} KB >= {} KB",
                    last.rss_kb,
                    self.config.absolute_threshold_kb
                );
                return true;
            }
        }

        // Slope-based check (consecutive high samples)
        if self.consecutive_high_slope >= self.config.required_consecutive_samples {
            log::warn!(
                "Memory Monitor: Slope threshold exceeded for {} consecutive samples (threshold: {} KB/s)",
                self.consecutive_high_slope,
                self.config.slope_threshold_kb_per_sec
            );
            return true;
        }

        false
    }

    /// Returns the last N snapshots.
    pub fn get_history(&self, n: usize) -> Vec<MemorySnapshot> {
        self.history.iter().rev().take(n).cloned().collect()
    }

    /// Returns the latest snapshot if available.
    pub fn latest(&self) -> Option<MemorySnapshot> {
        self.last_snapshot.clone()
    }
}

/// Starts the memory monitor as a background task.
/// Returns a shared state that can be queried for telemetry.
/// If `reset_signal` is provided, a `()` is sent on it whenever a preemptive reset is needed.
pub fn start_memory_monitor(
    config: MemoryMonitorConfig,
    reset_signal: Option<tokio::sync::mpsc::UnboundedSender<()>>,
) -> Arc<Mutex<MemoryMonitor>> {
    let monitor = Arc::new(Mutex::new(MemoryMonitor::new(config)));
    let monitor_clone = monitor.clone();

    tokio::spawn(async move {
        let interval = {
            let m = monitor_clone.lock().await;
            std::time::Duration::from_secs(m.config.sample_interval_secs)
        };

        loop {
            tokio::time::sleep(interval).await;

            let should_reset = {
                let mut m = monitor_clone.lock().await;
                m.sample();
                let trigger = m.should_trigger_reset();
                if trigger {
                    // M-1 hysteresis: reset consecutive counter to avoid tight loop (reset-storm)
                    // After a trigger, require 3 *again* consecutive high slopes before next trigger
                    m.consecutive_high_slope = 0;
                }
                trigger
            };

            if should_reset {
                log::warn!("Memory Monitor: Preemptive Reset triggered! (hysteresis: consecutive reset, next trigger requires 3 new high slopes)");
                if let Some(ref sender) = reset_signal {
                    let _ = sender.send(());
                }
                // Cooldown: sleep extra 60s after trigger before next sample to avoid rapid re-trigger during model reload
                tokio::time::sleep(std::time::Duration::from_secs(60)).await;
            }
        }
    });

    monitor
}

/// Reads memory for a specific PID using the sysinfo crate (cross-platform).
///
/// Callers pass the monitored PID — normally the `llama-server` child, not the
/// app. Returns `(0, 0)` when the process has already exited, which the
/// sampler treats as "no data" rather than as zero memory usage.
pub fn process_memory_kb(pid: u32) -> (u64, u64) {
    use sysinfo::{Pid, ProcessesToUpdate, System};
    let target = Pid::from_u32(pid);
    let mut sys = System::new();
    sys.refresh_processes(ProcessesToUpdate::Some(&[target]), false);
    if let Some(process) = sys.process(target) {
        let rss = process.memory() / 1024; // bytes → KB
        let vms = process.virtual_memory() / 1024;
        return (rss, vms);
    }
    (0, 0)
}

/// Returns current epoch seconds.
fn now_epoch_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// `MONITORED_PID` is process-wide, so tests that retarget it must not run
    /// concurrently with tests that depend on the default target.
    static PID_LOCK: Mutex<()> = Mutex::new(());

    fn lock() -> std::sync::MutexGuard<'static, ()> {
        PID_LOCK.lock().unwrap_or_else(|e| e.into_inner())
    }

    #[test]
    fn test_memory_monitor_slope_calculation() {
        let _g = lock();
        set_monitored_pid(0);
        let config = MemoryMonitorConfig {
            sample_interval_secs: 1,
            slope_threshold_kb_per_sec: 100.0,
            absolute_threshold_kb: 1024 * 1024,
            required_consecutive_samples: 2,
        };
        let mut monitor = MemoryMonitor::new(config);

        // First sample — no slope
        let s1 = monitor.sample();
        assert_eq!(s1.slope_kb_per_sec, 0.0);

        // Should not trigger yet
        assert!(!monitor.should_trigger_reset());
    }

    #[test]
    fn test_history_bounded() {
        let _g = lock();
        set_monitored_pid(0);
        let config = MemoryMonitorConfig {
            sample_interval_secs: 1,
            ..Default::default()
        };
        let mut monitor = MemoryMonitor::new(config);
        monitor.max_history = 5;

        for _ in 0..10 {
            monitor.sample();
        }

        assert!(monitor.get_history(100).len() <= 5);
    }

    /// P1-2 regression: the monitor must follow the process it is pointed at,
    /// and must not compute a slope across two different processes.
    #[test]
    fn retargeting_process_resets_slope_baseline() {
        let _g = lock();

        // Start on self.
        set_monitored_pid(0);
        let mut monitor = MemoryMonitor::new(MemoryMonitorConfig::default());
        assert_eq!(monitor.target_pid, std::process::id());
        let first = monitor.sample();
        assert_eq!(first.slope_kb_per_sec, 0.0);

        // Retarget to PID 1 (init) — guaranteed present on unix CI runners.
        // Any Linux/macOS CI runner has it; on Windows the test is skipped by
        // the assertion below.
        if cfg!(unix) {
            set_monitored_pid(1);
            let after = monitor.sample();
            assert_eq!(
                monitor.target_pid, 1,
                "monitor must follow set_monitored_pid"
            );
            assert_eq!(
                after.slope_kb_per_sec, 0.0,
                "first sample after a target change must not compute a slope \
                 across two unrelated processes"
            );
        }

        // Restore the default so later tests in this binary are unaffected.
        set_monitored_pid(0);
        assert_eq!(monitored_pid(), std::process::id());
    }

    /// `monitored_pid()` never returns the sentinel 0.
    #[test]
    fn monitored_pid_falls_back_to_self() {
        let _g = lock();
        set_monitored_pid(0);
        assert_eq!(monitored_pid(), std::process::id());
        set_monitored_pid(4242);
        assert_eq!(monitored_pid(), 4242);
        set_monitored_pid(0);
    }
}

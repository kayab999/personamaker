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
}

impl MemoryMonitor {
    pub fn new(config: MemoryMonitorConfig) -> Self {
        Self {
            config,
            last_snapshot: None,
            consecutive_high_slope: 0,
            history: VecDeque::new(),
            max_history: 60,
        }
    }

    /// Takes a memory snapshot and returns it.
    pub fn sample(&mut self) -> MemorySnapshot {
        let (rss_kb, vms_kb) = read_process_memory();
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

/// Reads current process memory using the sysinfo crate for cross-platform support.
/// Falls back to /proc/self/status on Linux, sysinfo on macOS/Windows.
fn read_process_memory() -> (u64, u64) {
    // Use sysinfo for cross-platform memory reading
    use sysinfo::{Pid, System, ProcessesToUpdate};
    let mut sys = System::new();
    sys.refresh_processes(ProcessesToUpdate::Some(&[Pid::from_u32(std::process::id())]), false);
    if let Some(process) = sys.process(Pid::from_u32(std::process::id())) {
        let rss = process.memory() / 1024; // bytes → KB
        let vms = process.virtual_memory() / 1024;
        return (rss as u64, vms as u64);
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

    #[test]
    fn test_memory_monitor_slope_calculation() {
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
}

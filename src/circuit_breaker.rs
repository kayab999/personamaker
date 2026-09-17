//! Circuit Breaker — SHA-256 hash-based deduplication for agent iterations.
//!
//! Prevents the system from wasting resources on identical repeated failures.
//! When a diff hash fails N times within a cooldown window, the circuit opens
//! and blocks further attempts until the cooldown expires.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Status of a circuit breaker channel.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum CircuitStatus {
    /// Circuit is closed — requests are allowed
    Closed,
    /// Circuit is open — requests are blocked due to repeated failures
    Open,
    /// Circuit is half-open — one test request is allowed
    HalfOpen,
}

/// Entry tracking failures for a specific diff hash.
#[derive(Debug, Clone)]
struct FailureEntry {
    count: u32,
    first_failure: Instant,
    last_failure: Instant,
}

/// Circuit breaker that blocks repeated identical failures.
pub struct CircuitBreaker {
    failures: HashMap<String, FailureEntry>,
    max_failures: u32,
    cooldown: Duration,
}

/// Global circuit breaker instance.
static CIRCUIT_BREAKER: once_cell::sync::Lazy<Mutex<CircuitBreaker>> =
    once_cell::sync::Lazy::new(|| {
        Mutex::new(CircuitBreaker::new(3, Duration::from_secs(60)))
    });

/// Status report for the circuit breaker.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CircuitBreakerStatus {
    pub total_hashes_tracked: usize,
    pub open_circuits: Vec<String>,
    pub max_failures: u32,
    pub cooldown_secs: u64,
}

impl CircuitBreaker {
    /// Creates a new circuit breaker with the given thresholds.
    pub fn new(max_failures: u32, cooldown: Duration) -> Self {
        Self {
            failures: HashMap::new(),
            max_failures,
            cooldown,
        }
    }

    /// Records a failure for the given diff hash.
    pub fn record_failure(&mut self, diff_hash: &str) {
        let now = Instant::now();
        let entry = self.failures.entry(diff_hash.to_string()).or_insert_with(|| FailureEntry {
            count: 0,
            first_failure: now,
            last_failure: now,
        });
        entry.count += 1;
        entry.last_failure = now;
    }

    /// Records a success — resets the failure counter for this hash.
    pub fn record_success(&mut self, diff_hash: &str) {
        self.failures.remove(diff_hash);
    }

    /// Checks if a request with this diff hash should be allowed.
    pub fn should_allow(&mut self, diff_hash: &str) -> bool {
        let now = Instant::now();

        if let Some(entry) = self.failures.get(diff_hash) {
            // Check if cooldown has expired
            if now.duration_since(entry.last_failure) > self.cooldown {
                // Cooldown expired — allow and reset
                self.failures.remove(diff_hash);
                return true;
            }

            // Check if under the failure threshold
            if entry.count >= self.max_failures {
                return false; // Circuit is OPEN
            }
        }

        true // Circuit is CLOSED
    }

    /// Returns the current status of a specific hash.
    pub fn get_status_for(&self, diff_hash: &str) -> CircuitStatus {
        if let Some(entry) = self.failures.get(diff_hash) {
            let now = Instant::now();
            if now.duration_since(entry.last_failure) > self.cooldown {
                CircuitStatus::Closed
            } else if entry.count >= self.max_failures {
                CircuitStatus::Open
            } else {
                CircuitStatus::Closed
            }
        } else {
            CircuitStatus::Closed
        }
    }

    /// Returns a status report.
    pub fn status_report(&self) -> CircuitBreakerStatus {
        let open: Vec<String> = self.failures.iter()
            .filter(|(_, e)| e.count >= self.max_failures)
            .map(|(h, _)| h.clone())
            .collect();

        CircuitBreakerStatus {
            total_hashes_tracked: self.failures.len(),
            open_circuits: open,
            max_failures: self.max_failures,
            cooldown_secs: self.cooldown.as_secs(),
        }
    }

    /// Cleans up expired entries.
    pub fn cleanup_expired(&mut self) {
        let now = Instant::now();
        self.failures.retain(|_, entry| {
            now.duration_since(entry.last_failure) <= self.cooldown
        });
    }
}

/// Computes a deduplication key for the given content using DefaultHasher.
/// Used by the circuit breaker to distinguish different error sources.
pub fn compute_dedup_key(content: &str) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let mut hasher = DefaultHasher::new();
    content.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

// ==================== Global API ====================

/// Records a failure for the given diff hash globally.
pub fn record_failure(diff_hash: &str) {
    if let Ok(mut cb) = CIRCUIT_BREAKER.lock() {
        cb.record_failure(diff_hash);
    }
}

/// Records a success for the given diff hash globally.
pub fn record_success(diff_hash: &str) {
    if let Ok(mut cb) = CIRCUIT_BREAKER.lock() {
        cb.record_success(diff_hash);
    }
}

/// Checks if a request with this diff hash should be allowed globally.
pub fn should_allow(diff_hash: &str) -> bool {
    CIRCUIT_BREAKER.lock()
        .map(|mut cb| cb.should_allow(diff_hash))
        .unwrap_or(true) // If lock fails, allow (fail-open)
}

/// Returns the global circuit breaker status.
pub fn get_status() -> CircuitBreakerStatus {
    CIRCUIT_BREAKER.lock()
        .map(|cb| cb.status_report())
        .unwrap_or(CircuitBreakerStatus {
            total_hashes_tracked: 0,
            open_circuits: vec![],
            max_failures: 3,
            cooldown_secs: 60,
        })
}

/// Cleans up expired entries globally.
pub fn cleanup() {
    if let Ok(mut cb) = CIRCUIT_BREAKER.lock() {
        cb.cleanup_expired();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_circuit_opens_after_max_failures() {
        let mut cb = CircuitBreaker::new(3, Duration::from_secs(60));

        assert!(cb.should_allow("hash1"));
        cb.record_failure("hash1");
        assert!(cb.should_allow("hash1"));
        cb.record_failure("hash1");
        assert!(cb.should_allow("hash1"));
        cb.record_failure("hash1");
        assert!(!cb.should_allow("hash1")); // Now OPEN
    }

    #[test]
    fn test_success_resets_counter() {
        let mut cb = CircuitBreaker::new(3, Duration::from_secs(60));

        cb.record_failure("hash1");
        cb.record_failure("hash1");
        cb.record_success("hash1");
        assert!(cb.should_allow("hash1")); // Reset
    }

    #[test]
    fn test_cooldown_reopens_circuit() {
        let mut cb = CircuitBreaker::new(2, Duration::from_millis(100));

        cb.record_failure("hash1");
        cb.record_failure("hash1");
        assert!(!cb.should_allow("hash1"));

        // Wait for cooldown
        std::thread::sleep(Duration::from_millis(150));
        assert!(cb.should_allow("hash1")); // Cooldown expired
    }

    #[test]
    fn test_diff_hash_deterministic() {
        let h1 = compute_dedup_key("test content");
        let h2 = compute_dedup_key("test content");
        assert_eq!(h1, h2);
    }

    #[test]
    fn test_diff_hash_different_for_different_content() {
        let h1 = compute_dedup_key("content A");
        let h2 = compute_dedup_key("content B");
        assert_ne!(h1, h2);
    }
}

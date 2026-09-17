//! Inference Backend Controller
//!
//! This module is responsible for the most critical piece of "one-click" experience:
//! starting, monitoring, and stopping the llama-server process that actually runs
//! the GGUF model.
//!
//! Design goals for v1:
//! - Reliable process lifecycle (especially on Linux/Windows)
//! - Automatic port selection
//! - Readiness detection (wait until server actually answers HTTP requests)
//! - Clean shutdown (SIGTERM → SIGKILL)
//! - Structured configuration
//! - Future-proof for log streaming to the frontend

use anyhow::{anyhow, Context, Result};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;

fn get_pid_file_path(port: u16) -> PathBuf {
    // Store PID files in a temp location or app data. Using /tmp for simplicity in this phase.
    std::env::temp_dir().join(format!("localpersona-llama-server-{}.pid", port))
}

fn write_pid_file(pid: u32, port: u16) -> Result<(), String> {
    let pid_path = get_pid_file_path(port);
    // A6 hardening: include per-app nonce to distinguish stale files across restarts
    let nonce = get_app_nonce();
    let content = format!("{}\n{}", pid, nonce);
    crate::storage::atomic_write_bytes(&pid_path, content.as_bytes())
}

fn read_pid_file(port: u16) -> Option<u32> {
    let pid_path = get_pid_file_path(port);
    if !pid_path.exists() {
        return None;
    }
    let raw = std::fs::read_to_string(pid_path).ok()?;
    // Support both legacy "pid" and new "pid\nnonce" format — parse first line
    let first = raw.lines().next().unwrap_or("").trim();
    first.parse().ok()
}

#[doc(hidden)] // pub for audit tests only
pub fn read_pid_file_with_nonce(port: u16) -> Option<(u32, String)> {
    let pid_path = get_pid_file_path(port);
    let raw = std::fs::read_to_string(pid_path).ok()?;
    let mut lines = raw.lines();
    let pid: u32 = lines.next()?.trim().parse().ok()?;
    let nonce = lines.next().unwrap_or("").to_string();
    Some((pid, nonce))
}

fn cleanup_stale_pid_file(port: u16) {
    let pid_path = get_pid_file_path(port);
    if pid_path.exists() {
        if let Err(e) = std::fs::remove_file(&pid_path) {
            log::warn!("Failed to clean stale PID file {}: {}", pid_path.display(), e);
        }
    }
}

#[doc(hidden)] // pub for audit tests only
pub fn get_app_nonce() -> String {
    use once_cell::sync::Lazy;
    static NONCE: Lazy<String> = Lazy::new(|| uuid::Uuid::new_v4().to_string());
    NONCE.clone()
}

/// A6 hardening: verify PID actually belongs to llama-server/mock before acting.
/// Reads /proc/<pid>/cmdline on Linux; falls back to allowing cleanup on other OS
/// but never kills an unverified PID.
// pub for audit tests only
#[doc(hidden)]
pub fn is_llama_server_process(pid: u32) -> bool {
    #[cfg(target_os = "linux")]
    {
        let path = format!("/proc/{}/cmdline", pid);
        if let Ok(bytes) = std::fs::read(&path) {
            let cmd = String::from_utf8_lossy(&bytes);
            return cmd.contains("llama-server") || cmd.contains("mock_llama");
        }
        // If cmdline unreadable (process dead or permission), treat as not ours
        return false;
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = pid;
        // On non-Linux, we cannot verify — default to false to avoid killing innocent
        return false;
    }
}
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::Mutex;

/// Configuration for starting a llama-server instance.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerStartRequest {
    /// Absolute path to the .gguf model file
    pub model_path: PathBuf,

    /// Context size (0 = let the server/model decide, usually 4096 or model default)
    pub ctx_size: Option<u32>,

    /// Number of GPU layers to offload (-1 = all possible, 0 = CPU only)
    pub gpu_layers: Option<i32>,

    /// CPU threads to use (None = let llama.cpp decide, usually half of cores)
    pub threads: Option<u32>,

    /// Flash attention (usually good to enable on modern models)
    pub flash_attn: bool,

    /// Preferred port. If 0 or taken, we will auto-pick a free port.
    pub preferred_port: Option<u16>,

    /// Optional path to the multimodal projector (mmproj) file.
    /// Required for Vision-Language Models (VLMs) like Qwen-VL, Llava, etc.
    pub mmproj_path: Option<PathBuf>,
}

/// Professional-grade explicit state machine for server lifecycle.
/// This replaces scattered booleans and makes shutdown races, restarts,
/// and observability much cleaner (core part of WS3 remediation).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ServerState {
    #[default]
    /// No server instance exists.
    Idle,
    /// Process has been spawned, waiting for HTTP readiness.
    Starting,
    /// Server is healthy and accepting requests.
    Running,
    /// Graceful or forced shutdown in progress.
    Stopping,
    /// Server process died unexpectedly or failed to start.
    Error,
    /// Currently performing an Arena Reset (stop + start cycle).
    Restarting,
}

/// Current status of the inference server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerStatus {
    pub running: bool,
    /// True while the server is in the process of starting (after spawn, before ready)
    pub starting: bool,
    /// Explicit professional state machine (new in architecture hardening phase)
    #[serde(default)]
    pub state: ServerState,
    pub model_path: Option<String>,
    pub port: Option<u16>,
    pub api_base: Option<String>,
    pub pid: Option<u32>,
    pub last_error: Option<String>,

    /// Whether the server was started with a multimodal projector (vision capable)
    pub vision_enabled: bool,

    // Phase 4: Dynamic context budget from GGUF
    /// Context length parsed from the loaded model's GGUF metadata (if available)
    #[serde(default)]
    pub model_context_length: Option<u32>,

    // Phase 1.5: Arena Reset tracking
    /// Number of inference requests served since last server start
    #[serde(default)]
    pub request_count: u64,
    /// Maximum requests before auto-respawn is recommended (0 = disabled)
    #[serde(default)]
    pub max_requests_before_reset: u64,
}

/// Default max inference requests before Arena Reset triggers a respawn.
/// 
/// Chosen as chat-realistic value (per v7.2 Tribunal audit):
/// A heavy persona conversation session typically generates 50-250 inferences.
/// 300 ensures the server is periodically respawned in normal long-running use
/// (instead of the previous 5000 which effectively never fired for this workload).
/// This directly addresses principle #12: no heavy worker lives eternally.
const DEFAULT_ARENA_RESET_THRESHOLD: u64 = 300;

/// Professional Grade: Default maximum continuous uptime before forcing Arena Reset.
/// 45 minutes is a conservative value that balances memory health against
/// the cost of restarting inference (model reload time).
const DEFAULT_MAX_UPTIME_BEFORE_RESET: std::time::Duration = std::time::Duration::from_secs(45 * 60);

/// The main manager that owns the llama-server child process.
pub struct LlamaServerManager {
    child: Option<Child>,
    current_port: Option<u16>,
    current_model: Option<PathBuf>,
    last_error: Option<String>,
    /// We keep the path to the llama-server binary once we resolve it.
    server_binary: Option<PathBuf>,
    /// Path to the multimodal projector (if vision was enabled for this session)
    current_mmproj: Option<PathBuf>,

    // Phase 1.5 + Professional Grade Hardening: Arena Reset policy
    /// Number of successful inference requests served since last start/reset
    request_count: u64,
    /// Max requests before auto-respawn. 0 = disabled.
    max_requests_before_reset: u64,
    /// The last start request config, stored for Arena Reset respawn
    pub(crate) last_start_request: Option<ServerStartRequest>,
    /// Prevents concurrent resets from check_arena_reset and memory monitor
    pub(crate) is_resetting: bool,
    /// R03: timestamp when start() was called (for tracking actual starting state)
    starting_since: Option<std::time::Instant>,

    // Professional Grade: Time-based Arena Reset (complements request count)
    /// When the current server instance was started (for uptime-based reset)
    last_started_at: Option<std::time::Instant>,
    /// Max continuous uptime before we force an Arena Reset (0 = disabled).
    /// Default: 45 minutes. Prevents long-running memory fragmentation even on low-traffic personas.
    max_uptime_before_reset: std::time::Duration,

    // Phase 4: Dynamic context budget
    /// Context length from the currently loaded GGUF model (for smart token budgeting)
    model_context_length: Option<u32>,

    // Phase C2: last Arena Reset observability
    /// Human-readable reason for the most recent Arena Reset (if any)
    last_reset_reason: Option<String>,
    /// Unix epoch millis when the last Arena Reset completed
    last_reset_at_unix_ms: Option<u64>,
}

impl LlamaServerManager {
    pub fn new() -> Self {
        Self {
            child: None,
            current_port: None,
            current_model: None,
            last_error: None,
            server_binary: None,
            current_mmproj: None,
            request_count: 0,
            max_requests_before_reset: DEFAULT_ARENA_RESET_THRESHOLD,
            last_start_request: None,
            is_resetting: false,
            starting_since: None,
            last_started_at: None,
            max_uptime_before_reset: DEFAULT_MAX_UPTIME_BEFORE_RESET,
            model_context_length: None,
            last_reset_reason: None,
            last_reset_at_unix_ms: None,
        }
    }

    /// Finds the llama-server binary using a robust, cross-platform search strategy.
    ///
    /// Search order (highest priority first):
    /// 1. Explicit path previously set via `set_binary_path` (user override)
    /// 2. Bundled inside app resources (future-proof for shipping llama-server)
    /// 3. Standard locations in PATH (`llama-server`, `server`)
    /// 4. Common installation directories used by llama.cpp users
    pub async fn resolve_binary(&mut self) -> Result<PathBuf> {
        // 1. User override (highest priority)
        if let Some(path) = &self.server_binary {
            if path.exists() {
                log::info!("Using user-configured llama-server at {:?}", path);
                return Ok(path.clone());
            } else {
                log::warn!("Previously configured llama-server path no longer exists: {:?}", path);
            }
        }

        // 2. Check if we have a bundled version inside the app resources
        if let Ok(resource_dir) = std::env::current_exe().and_then(|p| {
            // In dev this may not be perfect, but works well in bundled apps
            p.parent()
                .map(|p| p.join("resources"))
                .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "no parent"))
        }) {
            let bundled = if cfg!(windows) {
                resource_dir.join("llama-server.exe")
            } else {
                resource_dir.join("llama-server")
            };
            if bundled.exists() {
                log::info!("Found bundled llama-server at {:?}", bundled);
                self.server_binary = Some(bundled.clone());
                return Ok(bundled);
            }
        }

        // 3. Standard PATH lookup (most common for developers)
        if let Ok(path) = which::which("llama-server") {
            log::info!("Found llama-server in PATH: {:?}", path);
            self.server_binary = Some(path.clone());
            return Ok(path);
        }
        if let Ok(path) = which::which("server") {
            log::info!("Found 'server' binary in PATH (treating as llama-server): {:?}", path);
            self.server_binary = Some(path.clone());
            return Ok(path);
        }

        // 4. Common locations used by the llama.cpp community
        let candidates = get_common_llama_server_locations();
        for candidate in candidates {
            if candidate.exists() {
                log::info!("Found llama-server at common location: {:?}", candidate);
                self.server_binary = Some(candidate.clone());
                return Ok(candidate);
            }
        }

        Err(anyhow!(
            "Could not locate llama-server binary.\n\n\
             Searched PATH and common installation directories.\n\n\
             Solutions:\n\
             • Install llama.cpp and ensure 'llama-server' is in your PATH\n\
             • Use the 'Browse for llama-server' button in Settings (coming soon)\n\
             • Place a copy of llama-server next to the LocalPersona executable"
        ))
    }

    /// Allows the frontend to explicitly set (or override) the llama-server binary path.
    pub fn set_binary_path(&mut self, path: PathBuf) {
        self.server_binary = Some(path);
    }

    /// Starts the llama-server with the given configuration.
    /// This is an async operation because we wait for the server to become ready.
    pub async fn start(&mut self, req: ServerStartRequest) -> Result<ServerStatus> {
        if self.is_running() {
            self.stop().await?;
        }

        let binary = self.resolve_binary().await?;

        // Choose port
        let port = if let Some(p) = req.preferred_port {
            if portpicker::is_free_tcp(p) {
                p
            } else {
                portpicker::pick_unused_port().context("No free TCP ports available")?
            }
        } else {
            portpicker::pick_unused_port().context("No free TCP ports available")?
        };

        // Phase 0: Check for stale PID from previous crash (A6 hardened)
        if let Some((old_pid, old_nonce)) = read_pid_file_with_nonce(port).or_else(|| read_pid_file(port).map(|p| (p, String::new()))) {
            let current_nonce = get_app_nonce();
            let is_same_nonce = !old_nonce.is_empty() && old_nonce == current_nonce;
            if is_same_nonce {
                log::warn!("PID file for port {} has same nonce (likely same app instance), reusing check skipped", port);
            } else if is_llama_server_process(old_pid) {
                log::warn!(
                    "Found stale PID file for port {} (PID {} verified as llama-server/mock). Ghost detected — cleaning.",
                    port, old_pid
                );
            } else {
                log::warn!(
                    "Found stale PID file for port {} (PID {} not verified as llama-server — possibly innocent or dead). Cleaning file only, not killing.",
                    port, old_pid
                );
            }
            cleanup_stale_pid_file(port);
        }

        let mut cmd = Command::new(&binary);

        cmd.arg("--model")
            .arg(&req.model_path)
            .arg("--port")
            .arg(port.to_string())
            .arg("--host")
            .arg("127.0.0.1");

        // Context size
        if let Some(ctx) = req.ctx_size {
            if ctx > 0 {
                cmd.arg("--ctx-size").arg(ctx.to_string());
            }
        }

        // GPU layers
        if let Some(ngl) = req.gpu_layers {
            cmd.arg("-ngl").arg(ngl.to_string());
        } else {
            // Sensible default: try to offload as much as possible
            cmd.arg("-ngl").arg("-1");
        }

        // Threads
        if let Some(t) = req.threads {
            cmd.arg("--threads").arg(t.to_string());
        }

        if req.flash_attn {
            cmd.arg("--flash-attn");
        }

        // Vision support: pass mmproj if this is a multimodal model
        if let Some(mmproj) = &req.mmproj_path {
            if mmproj.exists() {
                cmd.arg("--mmproj").arg(mmproj);
                log::info!("Starting with vision support using mmproj: {:?}", mmproj);
            } else {
                log::warn!("mmproj path provided but file does not exist: {:?}", mmproj);
            }
        }

        // Good defaults for a persona/chat app
        cmd.arg("--batch-size").arg("512");
        cmd.arg("--ubatch-size").arg("512");
        cmd.arg("--parallel").arg("1"); // We can increase later

        // Important: we want to see when the server is ready
        cmd.arg("--verbose-prompt").arg("false");

        // Capture output so we can detect readiness
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        log::info!("Starting llama-server: {:?}", cmd);

        let mut child = cmd
            .spawn()
            .with_context(|| format!("Failed to spawn llama-server at {:?}", binary))?;

        let child_pid = child.id().unwrap_or(0);

        // Phase 0: PID file for orphan detection
        if let Err(e) = write_pid_file(child_pid, port) {
            log::warn!("Failed to write PID file for port {}: {}", port, e);
        }

        // Phase 4: Try to read GGUF context length for dynamic token budgeting
        if let Ok(meta) = crate::gguf::read_gguf_metadata(&req.model_path) {
            self.model_context_length = meta.context_length;
            if let Some(ctx) = meta.context_length {
                log::info!("Loaded model context length from GGUF: {} tokens", ctx);
            }
        } else {
            log::warn!("Could not parse GGUF metadata for context length (will use default budget)");
        }

        // Take stdout so we can monitor it for the "listening" message
        let stdout = child.stdout.take()
            .ok_or_else(|| anyhow!("Failed to capture llama-server stdout (process may have exited)"))?;

        // Phase 1.5: Take stderr to prevent pipe buffer full → child deadlock
        if let Some(stderr) = child.stderr.take() {
            tokio::spawn(async move {
                use tokio::io::{AsyncBufReadExt, BufReader};
                let reader = BufReader::new(stderr);
                let mut lines = reader.lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    log::debug!("[llama-server stderr] {}", line);
                }
            });
        }

        // Store the child
        self.child = Some(child);
        self.current_port = Some(port);
        self.current_model = Some(req.model_path.clone());
        self.current_mmproj = req.mmproj_path.clone();
        self.last_error = None;

        // Phase 1.5 + Professional Hardening: Arena Reset state
        self.request_count = 0;
        self.last_start_request = Some(req.clone());
        // R03: Track when start was initiated for deriving `starting` state
        self.starting_since = Some(std::time::Instant::now());
        self.last_started_at = Some(std::time::Instant::now());

        // Spawn a background task that monitors the process output for readiness
        let port_for_monitor = port;
        tokio::spawn(async move {
            monitor_server_startup(stdout, port_for_monitor).await;
        });

        // Phase 0 Fix: Return immediately with starting=true.
        // The long wait_for_server_ready is no longer blocking the Mutex.
        // The frontend should poll get_inference_status until running becomes true.
        // We keep a short best-effort wait in background (non-blocking for the command).
        let port_for_wait = port;
        tokio::spawn(async move {
            if let Err(e) = wait_for_server_ready(port_for_wait, Duration::from_secs(15)).await {
                log::warn!("Background readiness wait failed for port {}: {}", port_for_wait, e);
            }
        });

        // Return quickly so the command releases the Mutex
        Ok(ServerStatus {
            running: false,
            starting: true,
            state: ServerState::Starting,
            model_path: Some(req.model_path.to_string_lossy().to_string()),
            port: Some(port),
            api_base: Some(format!("http://127.0.0.1:{}/v1", port)),
            pid: self.child.as_ref().and_then(|c| c.id()),
            last_error: None,
            vision_enabled: self.current_mmproj.is_some(),
            model_context_length: self.model_context_length,
            request_count: 0,
            max_requests_before_reset: self.max_requests_before_reset,
        })
    }

    /// Stops the running server (if any).
    /// Phase 1.5: Tries graceful shutdown with timeout, then SIGKILL.
    pub async fn stop(&mut self) -> Result<()> {
        if let Some(mut child) = self.child.take() {
            log::info!("Stopping llama-server (pid: {:?})", child.id());

            // Phase 1.5: Try graceful exit first — drop stdin/stdout handles + wait with timeout
            // On most platforms, closing the parent's pipe handles signals the child to exit.
            drop(child.stdin.take());
            drop(child.stdout.take());
            drop(child.stderr.take());

            // Wait up to 3 seconds for graceful exit
            match tokio::time::timeout(Duration::from_secs(3), child.wait()).await {
                Ok(Ok(status)) => {
                    log::info!("llama-server exited gracefully: {}", status);
                }
                _ => {
                    // Graceful shutdown timed out — force kill
                    log::warn!("llama-server did not exit gracefully, sending SIGKILL");
                    if let Err(e) = child.kill().await {
                        log::warn!("Failed to send SIGKILL to llama-server: {}", e);
                    }
                    if let Err(e) = child.wait().await {
                        log::warn!("Failed to wait for llama-server after SIGKILL: {}", e);
                    }
                    log::info!("llama-server stopped (SIGKILL)");
                }
            }
        }

        // Clean PID file
        if let Some(port) = self.current_port {
            cleanup_stale_pid_file(port);
        }

        self.current_port = None;
        self.current_model = None;
        self.current_mmproj = None;
        self.request_count = 0;
        self.last_start_request = None;
        self.starting_since = None;
        self.last_started_at = None;
        Ok(())
    }

    pub fn is_running(&self) -> bool {
        self.child.is_some()
    }

    /// FIX-C02: Actively checks if the child process has exited (crashed).
    /// Call this periodically to detect silent crashes. Returns true if the
    /// process was found to have died and state was cleaned up.
    /// R01: Triggers auto-restart with circuit breaker to prevent infinite restart loops.
    pub fn check_health(&mut self) -> bool {
        if let Some(child) = self.child.as_mut() {
            match child.try_wait() {
                Ok(Some(status)) => {
                    log::warn!(
                        "llama-server process exited unexpectedly with status: {}",
                        status
                    );
                    let port = self.current_port;
                    self.child = None;
                    self.current_port = None;
                    self.current_model = None;
                    self.current_mmproj = None;
                    self.last_error = Some(format!("Server process exited with status: {}", status));
                    self.request_count = 0;
                    // Keep last_start_request so auto_restart_if_needed / Arena Reset can recover.
                    self.starting_since = None;
                    self.last_started_at = None;
                    if let Some(p) = port {
                        cleanup_stale_pid_file(p);
                    }
                    true
                }
                Ok(None) => false, // Still running
                Err(e) => {
                    log::warn!("Failed to check llama-server status: {}", e);
                    false
                }
            }
        } else {
            false
        }
    }

    /// R01: Attempts auto-restart if a start request is stored (circuit-breaker guarded).
    /// Returns true if the restart was initiated.
    pub fn auto_restart_if_needed(&mut self) -> bool {
        if self.last_start_request.is_some() && !self.is_resetting {
            // Stable key shared with get_inference_status record_failure/success
            let model = self
                .last_start_request
                .as_ref()
                .map(|r| r.model_path.display().to_string())
                .unwrap_or_else(|| "unknown".into());
            let diff_hash = format!("auto_restart_{}", model);
            if crate::circuit_breaker::should_allow(&diff_hash) {
                log::info!("Auto-restart triggered for llama-server");
                true
            } else {
                log::warn!("Auto-restart blocked by circuit breaker (hash: {})", diff_hash);
                false
            }
        } else {
            false
        }
    }

    pub fn status(&self) -> ServerStatus {
        // R03 + WS3: Derive professional state machine.
        // Note: We intentionally do *not* call check_health() here to avoid side effects in a read-only status call.
        // Callers who want fresh health should call check_health() explicitly.
        let is_child_alive = self.child.is_some();
        // Child alive ⇒ process is up. `starting` is a UI hint only (first ~15s after spawn).
        // post_chat_completion still fails cleanly if HTTP is not ready yet.
        let starting = is_child_alive
            && self
                .starting_since
                .map_or(false, |t| t.elapsed() < std::time::Duration::from_secs(15));

        let state = if self.is_resetting {
            ServerState::Restarting
        } else if !is_child_alive && self.last_error.is_some() {
            ServerState::Error
        } else if starting {
            ServerState::Starting
        } else if is_child_alive {
            ServerState::Running
        } else {
            ServerState::Idle
        };

        ServerStatus {
            // Allow chat once the process exists; HTTP readiness is checked on request.
            running: is_child_alive,
            starting,
            state,
            model_path: self.current_model.as_ref().map(|p| p.to_string_lossy().to_string()),
            port: self.current_port,
            api_base: self.current_port.map(|p| format!("http://127.0.0.1:{}/v1", p)),
            pid: self.child.as_ref().and_then(|c| c.id()),
            last_error: self.last_error.clone(),
            vision_enabled: self.current_mmproj.is_some(),
            model_context_length: self.model_context_length,
            request_count: self.request_count,
            max_requests_before_reset: self.max_requests_before_reset,
        }
    }

    /// Returns the current API base URL if the server is running.
    pub fn api_base(&self) -> Option<String> {
        self.current_port.map(|p| format!("http://127.0.0.1:{}/v1", p))
    }

    // ==================== Phase 1.5: Arena Reset ====================

    /// Increments the request counter and returns `true` if the threshold is hit
    /// (indicating an Arena Reset is advisable).
    pub fn increment_and_check_reset(&mut self) -> bool {
        if self.max_requests_before_reset == 0 {
            return false; // disabled
        }
        self.request_count += 1;
        let hit = self.request_count >= self.max_requests_before_reset;
        if hit {
            log::warn!(
                "Arena Reset threshold hit: {} requests served (max: {}). Will respawn server.",
                self.request_count,
                self.max_requests_before_reset
            );
        }
        hit
    }

    /// Record why an Arena Reset is about to run (diagnostics).
    pub fn record_reset_reason(&mut self, reason: &str) {
        self.last_reset_reason = Some(reason.to_string());
    }

    /// Stops the current server and restarts with the last stored config.
    /// After a successful reset, the request counter is cleared.
    /// Always clears `is_resetting` so a failed reset cannot stick forever.
    pub async fn reset(&mut self) -> Result<ServerStatus> {
        if self.is_resetting {
            log::warn!("Arena Reset already in progress, skipping...");
            return Ok(self.status());
        }
        self.is_resetting = true;

        let result = async {
            let req = self
                .last_start_request
                .clone()
                .ok_or_else(|| anyhow!("No stored start request for Arena Reset"))?;

            log::info!("Arena Reset: stopping server for respawn...");
            // Preserve start config across stop() so a failed restart can still recover.
            let preserved = self.last_start_request.clone();
            self.stop().await?;
            if self.last_start_request.is_none() {
                self.last_start_request = preserved;
            }

            log::info!("Arena Reset: restarting server...");
            let status = self.start(req).await?;

            self.last_reset_at_unix_ms = Some(
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis() as u64,
            );
            if self.last_reset_reason.is_none() {
                self.last_reset_reason = Some("scheduled".to_string());
            }
            log::info!(
                "Arena Reset completed successfully (reason={:?}).",
                self.last_reset_reason
            );
            Ok(status)
        }
        .await;

        self.is_resetting = false;
        result
    }

    /// Configures the maximum requests before an Arena Reset is triggered.
    /// Set to 0 to disable auto-respawn.
    pub fn set_arena_reset_threshold(&mut self, max: u64) {
        self.max_requests_before_reset = max;
    }

    /// Returns the current (request_count, max_requests_before_reset) tuple.
    pub fn arena_reset_info(&self) -> (u64, u64) {
        (self.request_count, self.max_requests_before_reset)
    }

    /// Last Arena Reset reason + unix ms timestamp (for Diagnostics).
    pub fn last_reset_info(&self) -> (Option<String>, Option<u64>) {
        (self.last_reset_reason.clone(), self.last_reset_at_unix_ms)
    }

    /// Returns seconds since last start (for uptime-based Arena logic).
    pub fn uptime_seconds(&self) -> Option<u64> {
        self.last_started_at.map(|t| t.elapsed().as_secs())
    }

    /// Professional Grade: Returns true if the server has been running longer than
    /// the configured max uptime. This complements request-count-based resets.
    pub fn should_reset_due_to_uptime(&self) -> bool {
        if self.max_uptime_before_reset.as_secs() == 0 {
            return false;
        }
        if let Some(started) = self.last_started_at {
            started.elapsed() >= self.max_uptime_before_reset
        } else {
            false
        }
    }

    /// Sets the maximum continuous uptime before a time-based Arena Reset.
    /// Set to 0 to disable time-based resets.
    pub fn set_max_uptime_before_reset(&mut self, duration: std::time::Duration) {
        self.max_uptime_before_reset = duration;
    }

    /// Phase 4: Voice doesn't use GGUF context length (always None).
    pub fn model_context_length(&self) -> Option<u32> {
        self.model_context_length
    }
}

/// Voice / TTS server manager.
/// Separate from the main LLM server so both can run independently.
pub struct VoiceServerManager {
    child: Option<Child>,
    current_port: Option<u16>,
    current_model: Option<PathBuf>,
    last_error: Option<String>,
    server_binary: Option<PathBuf>,

    // Phase 1.5 + Professional Grade: Arena Reset tracking
    request_count: u64,
    max_requests_before_reset: u64,
    last_start_request: Option<ServerStartRequest>,
    /// R03/R04: timestamp when start() was called
    starting_since: Option<std::time::Instant>,
    /// M08: Prevents concurrent voice arena resets
    is_resetting: bool,

    // Professional Grade: Time-based Arena Reset
    last_started_at: Option<std::time::Instant>,
    max_uptime_before_reset: std::time::Duration,

    // Phase 4: Not used for voice, but needed for uniform ServerStatus
    model_context_length: Option<u32>,

    last_reset_reason: Option<String>,
    last_reset_at_unix_ms: Option<u64>,
}

impl VoiceServerManager {
    pub fn new() -> Self {
        Self {
            child: None,
            current_port: None,
            current_model: None,
            last_error: None,
            server_binary: None,
            request_count: 0,
            max_requests_before_reset: DEFAULT_ARENA_RESET_THRESHOLD,
            last_start_request: None,
            starting_since: None,
            is_resetting: false,
            last_started_at: None,
            max_uptime_before_reset: DEFAULT_MAX_UPTIME_BEFORE_RESET,
            model_context_length: None,
            last_reset_reason: None,
            last_reset_at_unix_ms: None,
        }
    }

    pub fn set_binary_path(&mut self, path: PathBuf) {
        self.server_binary = Some(path);
    }

    pub async fn resolve_binary(&mut self) -> Result<PathBuf> {
        if let Some(path) = &self.server_binary {
            if path.exists() {
                return Ok(path.clone());
            }
        }

        // Reuse the same discovery logic as the main LLM server
        if let Ok(path) = which::which("llama-server") {
            self.server_binary = Some(path.clone());
            return Ok(path);
        }
        if let Ok(path) = which::which("server") {
            self.server_binary = Some(path.clone());
            return Ok(path);
        }

        let candidates = get_common_llama_server_locations();
        for candidate in candidates {
            if candidate.exists() {
                self.server_binary = Some(candidate.clone());
                return Ok(candidate);
            }
        }

        Err(anyhow!("Could not locate llama-server binary for voice/TTS server"))
    }

    pub async fn start(&mut self, req: ServerStartRequest) -> Result<ServerStatus> {
        if self.is_running() {
            self.stop().await?;
        }

        let binary = self.resolve_binary().await?;

        let port = if let Some(p) = req.preferred_port {
            if portpicker::is_free_tcp(p) { p } else { portpicker::pick_unused_port().context("No free ports")? }
        } else {
            portpicker::pick_unused_port().context("No free ports")?
        };

        let mut cmd = Command::new(&binary);
        cmd.arg("--model").arg(&req.model_path)
           .arg("--port").arg(port.to_string())
           .arg("--host").arg("127.0.0.1");

        if let Some(ngl) = req.gpu_layers {
            cmd.arg("-ngl").arg(ngl.to_string());
        } else {
            cmd.arg("-ngl").arg("-1");
        }

        if req.flash_attn {
            cmd.arg("--flash-attn");
        }

        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        let mut child = cmd.spawn().with_context(|| format!("Failed to spawn TTS llama-server at {:?}", binary))?;

        let child_pid = child.id().unwrap_or(0);
        if let Err(e) = write_pid_file(child_pid, port) {
            log::warn!("Failed to write voice server PID file for port {}: {}", port, e);
        }

        // R04: Take stdout for readiness monitoring (mirrors LlamaServerManager pattern)
        let stdout = child.stdout.take()
            .ok_or_else(|| anyhow!("Failed to capture voice server stdout (process may have exited)"))?;

        // Phase 1.5: Take stderr to prevent pipe buffer full → child deadlock
        if let Some(stderr) = child.stderr.take() {
            tokio::spawn(async move {
                use tokio::io::{AsyncBufReadExt, BufReader};
                let reader = BufReader::new(stderr);
                let mut lines = reader.lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    log::debug!("[TTS llama-server stderr] {}", line);
                }
            });
        }

        self.child = Some(child);
        self.current_port = Some(port);
        self.current_model = Some(req.model_path.clone());
        self.last_error = None;
        self.request_count = 0;
        self.last_start_request = Some(req.clone());
        self.starting_since = Some(std::time::Instant::now());
        self.last_started_at = Some(std::time::Instant::now());

        // R04: Spawn stdout readiness monitor (same as LLM server)
        let port_for_monitor = port;
        tokio::spawn(async move {
            monitor_server_startup(stdout, port_for_monitor).await;
        });

        // Spawn readiness check as background task (matching LLM server pattern).
        // Returns immediately so the Mutex is released quickly.
        let port_for_wait = port;
        tokio::spawn(async move {
            if let Err(e) = wait_for_server_ready(port_for_wait, Duration::from_secs(10)).await {
                log::warn!("Voice server readiness wait timed out: {}", e);
            }
        });

        Ok(ServerStatus {
            running: false,
            starting: true,
            state: ServerState::Starting,
            model_path: self.current_model.as_ref().map(|p| p.to_string_lossy().to_string()),
            port: self.current_port,
            api_base: self.current_port.map(|p| format!("http://127.0.0.1:{}/v1", p)),
            pid: self.child.as_ref().and_then(|c| c.id()),
            last_error: None,
            vision_enabled: false,
            model_context_length: self.model_context_length,
            request_count: 0,
            max_requests_before_reset: self.max_requests_before_reset,
        })
    }

    /// FIX-I01: Graceful shutdown for voice server (same pattern as LlamaServerManager).
    pub async fn stop(&mut self) -> Result<()> {
        if let Some(mut child) = self.child.take() {
            log::info!("Stopping voice server (pid: {:?})", child.id());

            // Try graceful exit first — drop pipe handles + wait with timeout
            drop(child.stdin.take());
            drop(child.stdout.take());
            drop(child.stderr.take());

            match tokio::time::timeout(Duration::from_secs(3), child.wait()).await {
                Ok(Ok(status)) => {
                    log::info!("Voice server exited gracefully: {}", status);
                }
                _ => {
                    log::warn!("Voice server did not exit gracefully, sending SIGKILL");
                    if let Err(e) = child.kill().await {
                        log::warn!("Failed to send SIGKILL to voice server: {}", e);
                    }
                    if let Err(e) = child.wait().await {
                        log::warn!("Failed to wait for voice server after SIGKILL: {}", e);
                    }
                    log::info!("Voice server stopped (SIGKILL)");
                }
            }
        }
        if let Some(port) = self.current_port {
            cleanup_stale_pid_file(port);
        }
        self.current_port = None;
        self.current_model = None;
        self.starting_since = None;
        self.last_started_at = None;
        Ok(())
    }

    pub fn is_running(&self) -> bool {
        self.child.is_some()
    }

    /// FIX-C02: Actively checks if the voice child process has exited (crashed).
    pub fn check_health(&mut self) -> bool {
        if let Some(child) = self.child.as_mut() {
            match child.try_wait() {
                Ok(Some(status)) => {
                    log::warn!(
                        "Voice server process exited unexpectedly with status: {}",
                        status
                    );
                    let port = self.current_port;
                    self.child = None;
                    self.current_port = None;
                    self.current_model = None;
                    self.last_error = Some(format!("Voice server exited with status: {}", status));
                    self.starting_since = None;
                    if let Some(p) = port {
                        cleanup_stale_pid_file(p);
                    }
                    true
                }
                Ok(None) => false,
                Err(e) => {
                    log::warn!("Failed to check voice server status: {}", e);
                    false
                }
            }
        } else {
            false
        }
    }

    pub fn status(&self) -> ServerStatus {
        let is_child_alive = self.is_running();
        // UI hint only — process may still be warming HTTP for a few seconds after spawn.
        let is_starting = is_child_alive
            && self
                .starting_since
                .map_or(false, |t| t.elapsed() < std::time::Duration::from_secs(15));

        let state = if self.is_resetting {
            ServerState::Restarting
        } else if !is_child_alive && self.last_error.is_some() {
            ServerState::Error
        } else if is_starting {
            ServerState::Starting
        } else if is_child_alive {
            ServerState::Running
        } else {
            ServerState::Idle
        };

        ServerStatus {
            running: is_child_alive,
            starting: is_starting,
            state,
            model_path: self.current_model.as_ref().map(|p| p.to_string_lossy().to_string()),
            port: self.current_port,
            api_base: self.current_port.map(|p| format!("http://127.0.0.1:{}/v1", p)),
            pid: self.child.as_ref().and_then(|c| c.id()),
            last_error: self.last_error.clone(),
            vision_enabled: false,
            model_context_length: None, // Voice server doesn't use GGUF context length for budgeting
            request_count: self.request_count,
            max_requests_before_reset: self.max_requests_before_reset,
        }
    }

    // ==================== Arena Reset ====================

    pub fn increment_and_check_reset(&mut self) -> bool {
        if self.max_requests_before_reset == 0 {
            return false;
        }
        self.request_count += 1;
        let hit = self.request_count >= self.max_requests_before_reset;
        if hit {
            log::warn!(
                "Voice Arena Reset threshold hit: {} requests served (max: {}). Will respawn server.",
                self.request_count,
                self.max_requests_before_reset
            );
        }
        hit
    }

    pub fn record_reset_reason(&mut self, reason: &str) {
        self.last_reset_reason = Some(reason.to_string());
    }

    pub async fn reset(&mut self) -> Result<ServerStatus> {
        if self.is_resetting {
            return Err(anyhow!("Voice arena reset already in progress"));
        }
        self.is_resetting = true;

        let req = self.last_start_request.clone()
            .ok_or_else(|| anyhow!("No stored start request for Voice Arena Reset"))?;

        log::info!("Voice Arena Reset: stopping server for respawn...");
        let result = async {
            self.stop().await?;
            log::info!("Voice Arena Reset: restarting server...");
            let status = self.start(req).await?;
            log::info!("Voice Arena Reset completed successfully.");
            Ok::<ServerStatus, anyhow::Error>(status)
        }.await;

        self.is_resetting = false;
        if result.is_ok() {
            self.last_reset_at_unix_ms = Some(
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis() as u64,
            );
            if self.last_reset_reason.is_none() {
                self.last_reset_reason = Some("scheduled".to_string());
            }
        }
        result
    }

    pub fn set_arena_reset_threshold(&mut self, max: u64) {
        self.max_requests_before_reset = max;
    }

    pub fn last_reset_info(&self) -> (Option<String>, Option<u64>) {
        (self.last_reset_reason.clone(), self.last_reset_at_unix_ms)
    }

    pub fn arena_reset_info(&self) -> (u64, u64) {
        (self.request_count, self.max_requests_before_reset)
    }

    /// Returns seconds since last start.
    pub fn uptime_seconds(&self) -> Option<u64> {
        self.last_started_at.map(|t| t.elapsed().as_secs())
    }

    /// Professional Grade: Time-based reset decision for Voice server.
    pub fn should_reset_due_to_uptime(&self) -> bool {
        if self.max_uptime_before_reset.as_secs() == 0 {
            return false;
        }
        if let Some(started) = self.last_started_at {
            started.elapsed() >= self.max_uptime_before_reset
        } else {
            false
        }
    }

    pub fn set_max_uptime_before_reset(&mut self, duration: std::time::Duration) {
        self.max_uptime_before_reset = duration;
    }

    /// Phase 4: Voice doesn't use GGUF context length (always None).
    pub fn model_context_length(&self) -> Option<u32> {
        self.model_context_length
    }
}

impl Drop for VoiceServerManager {
    fn drop(&mut self) {
        // Phase 1.5: Unified cleanup pattern — best effort kill + brief wait + reap
        if let Some(mut child) = self.child.take() {
            let _ = child.start_kill();
            std::thread::sleep(std::time::Duration::from_millis(50));
            let _ = child.try_wait();
        }
    }
}

/// Shared state for the voice/TTS server
pub type SharedVoiceServer = Arc<Mutex<VoiceServerManager>>;

/// Returns a list of likely locations for llama-server on the current platform.
/// Used by `resolve_binary`.
fn get_common_llama_server_locations() -> Vec<PathBuf> {
    let mut paths = vec![];

    // Linux common locations
    #[cfg(target_os = "linux")]
    {
        paths.push(PathBuf::from("/usr/local/bin/llama-server"));
        paths.push(PathBuf::from("/usr/bin/llama-server"));
        paths.push(PathBuf::from("/opt/llama.cpp/bin/llama-server"));
        // Common user build locations
        if let Some(home) = std::env::var_os("HOME") {
            let home = PathBuf::from(home);
            paths.push(home.join("llama.cpp/build/bin/llama-server"));
            paths.push(home.join("llama.cpp-prism/build/bin/llama-server"));
            paths.push(home.join("llama.cpp-main/build/bin/llama-server"));
            paths.push(home.join(".local/bin/llama-server"));
        }
    }

    // macOS common locations
    #[cfg(target_os = "macos")]
    {
        paths.push(PathBuf::from("/usr/local/bin/llama-server"));
        paths.push(PathBuf::from("/opt/homebrew/bin/llama-server"));
        if let Some(home) = std::env::var_os("HOME") {
            let home = PathBuf::from(home);
            paths.push(home.join("llama.cpp/build/bin/llama-server"));
            paths.push(home.join("llama.cpp-prism/build/bin/llama-server"));
            paths.push(home.join("llama.cpp-main/build/bin/llama-server"));
        }
    }

    // Windows common locations
    #[cfg(target_os = "windows")]
    {
        if let Some(home) = std::env::var_os("USERPROFILE") {
            let home = PathBuf::from(home);
            paths.push(home.join("llama.cpp\\build\\bin\\llama-server.exe"));
            paths.push(home.join("llama.cpp\\build\\bin\\Release\\llama-server.exe"));
            paths.push(home.join("llama.cpp-prism\\build\\bin\\llama-server.exe"));
        }
        paths.push(PathBuf::from("C:\\Program Files\\llama.cpp\\llama-server.exe"));
        paths.push(PathBuf::from("C:\\llama.cpp\\llama-server.exe"));
    }

    // Also check next to the current executable (good for portable distributions)
    if let Ok(exe_path) = std::env::current_exe() {
        if let Some(dir) = exe_path.parent() {
            paths.push(dir.join(if cfg!(windows) { "llama-server.exe" } else { "llama-server" }));
        }
    }

    paths
}

/// Actively waits until the llama-server HTTP endpoint responds successfully.
/// This is far more reliable than a fixed sleep.
async fn wait_for_server_ready(port: u16, timeout: Duration) -> Result<(), String> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(2))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|e| format!("Failed to build HTTP client: {}", e))?;

    let url = format!("http://127.0.0.1:{}/v1/models", port);
    let start = std::time::Instant::now();

    while start.elapsed() < timeout {
        match client.get(&url).send().await {
            Ok(resp) if resp.status().is_success() => {
                log::info!("Server on port {} is ready (responded to /v1/models)", port);
                return Ok(());
            }
            Ok(resp) => {
                log::debug!("Server on port {} responded with status {}", port, resp.status());
            }
            Err(e) => {
                log::debug!("Waiting for server on port {}: {}", port, e);
            }
        }
        tokio::time::sleep(Duration::from_millis(180)).await;
    }

    Err(format!(
        "Server on port {} did not become ready within {:?}",
        port, timeout
    ))
}

/// FIX-M04: Statically compiled regex for server readiness detection.
static READY_REGEX: once_cell::sync::Lazy<Regex> = once_cell::sync::Lazy::new(|| {
    Regex::new(r"(?i)(listening|server.*ready|http server started|listening on)").unwrap()
});

/// Background task that watches the server stdout looking for the
/// "listening on port" message that modern llama-server prints when ready.
async fn monitor_server_startup(stdout: tokio::process::ChildStdout, port: u16) {
    let reader = BufReader::new(stdout);
    let mut lines = reader.lines();

    while let Ok(Some(line)) = lines.next_line().await {
        log::debug!("[llama-server] {}", line);

        if READY_REGEX.is_match(&line) {
            log::info!("llama-server appears ready on port {}", port);
            // In a future version we will emit a Tauri event here so the frontend knows instantly.
            break;
        }
    }
}

impl Drop for LlamaServerManager {
    fn drop(&mut self) {
        // Phase 1.5: Unified cleanup pattern — best effort kill + brief wait + reap
        if let Some(mut child) = self.child.take() {
            let _ = child.start_kill();
            std::thread::sleep(std::time::Duration::from_millis(50));
            let _ = child.try_wait();
        }
    }
}

/// Shared state that Tauri will hold for the whole application lifetime.
///
/// NOTE (Fase 3 audit): This global Mutex is a source of contention for long-running
/// operations. Future improvement: consider splitting into read-heavy status vs
/// write (start/stop) or moving to an actor model with mpsc channels.
pub type SharedLlamaServer = Arc<Mutex<LlamaServerManager>>;

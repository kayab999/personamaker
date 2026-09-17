// LocalPersona
// This file exists so the crate can be both a binary and (in the future) a library.
// During early development we keep most implementation in the binary root (main.rs).

pub mod commands;
pub mod inference;
pub mod storage;
pub mod conversation;   // Multi-persona conversations (Phase 2)
pub mod rag;
pub mod autopsy;        // Phase 12.0: Structured crash dumps
pub mod circuit_breaker; // Phase 12.0: Hash-based deduplication
pub mod memory_monitor;  // Phase 12.0: RAM telemetry + Preemptive Reset

pub mod gguf; // GGUF metadata parsing (10/10 remediation - automatic model understanding)
pub mod capture; // Audit harness A: env-guarded prompt/response capture tap

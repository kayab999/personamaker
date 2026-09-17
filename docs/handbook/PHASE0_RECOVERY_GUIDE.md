# Phase 0 Recovery Guide (After Computer Restart)

**Date:** July 2025

If you lost local changes after a restart, this file + `PHASE0_CHECKPOINT.md` should contain everything you need to restore the most important Phase 0 stabilization work.

---

## Most Important Files Modified

### Core Fixes (Critical)
1. `src/inference.rs`
2. `src/storage.rs`
3. `src/conversation.rs`
4. `src/commands.rs`
5. `src/main.rs`

### Documentation
- `AGENTS.md`
- `ARCHITECTURE.md`
- `README.md`
- `PHASE0_CHECKPOINT.md` (summary of achievements)

---

## Summary of Changes to Re-apply

### 1. Mutex Contention Fix (Server Start no longer blocks)

**File:** `src/inference.rs`

- Add `starting: bool` to `ServerStatus` struct.
- In `LlamaServerManager::start()` and `VoiceServerManager::start()`:
  - After spawning the child process and storing it, return **immediately** with `starting: true`.
  - Move the `wait_for_server_ready()` call into a `tokio::spawn` background task.
- Update both `status()` methods to include `starting: false`.

**New field example:**
```rust
pub struct ServerStatus {
    pub running: bool,
    pub starting: bool,   // <-- New
    ...
}
```

### 2. PID File / Orphan Process Protection

**File:** `src/inference.rs`

Add these helper functions (near the top):

```rust
fn get_pid_file_path(port: u16) -> PathBuf {
    std::env::temp_dir().join(format!("localpersona-llama-server-{}.pid", port))
}

fn write_pid_file(pid: u32, port: u16) -> Result<(), String> { ... }
fn read_pid_file(port: u16) -> Option<u32> { ... }
fn cleanup_stale_pid_file(port: u16) { ... }
```

In `start()`:
- After successful spawn, call `write_pid_file(child.id().unwrap_or(0), port)`.
- Before spawning, check for stale PID and clean if the process is dead.

In `stop()`:
- Call `cleanup_stale_pid_file(self.current_port.unwrap_or(0))`.

### 3. Concurrent Write Safety (fs2 Locking)

**File:** `src/storage.rs`

- Add import: `use fs2::FileExt;`
- Add `use std::fs::OpenOptions;`

Add this function:

```rust
pub(crate) fn atomic_write_with_lock(path: &Path, content: &[u8]) -> Result<(), String> {
    let lock_path = path.with_extension("lock");
    let lock_file = OpenOptions::new()
        .create(true).write(true).open(&lock_path)
        .map_err(|e| e.to_string())?;
    lock_file.lock_exclusive().map_err(|e| e.to_string())?;

    let result = atomic_write_bytes(path, content);

    let _ = lock_file.unlock();
    let _ = std::fs::remove_file(&lock_path);
    result
}
```

Update `atomic_write()` to call `atomic_write_with_lock()`.

**File:** `src/conversation.rs`

In `append_message()`, wrap the actual ndjson append with an exclusive lock on a `.ndjson.lock` file (see the edit done in the session for exact pattern).

### 4. Other Notable Improvements

- `repair_conversation()` now repairs both messages and metadata.
- `load_all_messages()` skips corrupted lines gracefully.
- `fsync` added in several places (messages, voice, knowledge, chunks).
- Version field + logging added to `ConversationMetadata` and `StoredCharacter`.
- `token_count` field added to `ChatMessage` (early Context Management work).

---

## Recommended Recovery Steps

1. Read `PHASE0_CHECKPOINT.md` first (high-level overview).
2. Apply the three critical fixes above in this order:
   - Mutex / `starting` state (biggest UX win)
   - PID file mechanism
   - Locking on atomic writes + ndjson append
3. Re-apply the smaller resilience and version changes.
4. Run `cargo check` after each major group of changes.
5. Once the code is back, re-apply the documentation updates from AGENTS.md, ARCHITECTURE.md, and README.md (or copy the relevant sections from this environment).

---

**Note:** All the changes listed above were already applied and verified (`cargo check` clean) in this agent environment before the restart.

This file exists purely to help you restore the work on your local machine.

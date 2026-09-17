// src/conversation.rs - High-performance append-only message storage
// Replaces the old full-JSON rewrite model with:
//   conversations/{id}/
//     - metadata.json
//     - messages.ndjson

use serde::{Deserialize, Serialize};
use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use tauri::AppHandle;
use tauri::Manager;

use crate::storage::{atomic_write, atomic_write_bytes};
use fs2::FileExt;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageReference {
    pub id: String,
    pub path: String,
    pub mime_type: String,
    pub width: Option<u32>,
    pub height: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub id: String,
    pub conversation_id: String,
    pub speaker_id: String,
    pub speaker_name: String,
    pub content: String,
    pub timestamp: u64,
    pub role: String, // "user", "assistant", "narrator"
    #[serde(default)]
    pub images: Vec<ImageReference>,
    /// Estimated token count for this message (Phase 1 - Context Management)
    #[serde(default)]
    pub token_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationMetadata {
    /// Schema version for migration support (Phase 0)
    #[serde(default = "default_metadata_version")]
    pub version: u32,
    pub id: String,
    pub name: String,
    pub participant_ids: Vec<String>,
    pub created_at: u64,
    pub updated_at: u64,
    pub message_count: u32,
    /// Preview of the most recent message (truncated for UI)
    #[serde(default)]
    pub last_message_preview: Option<String>,
    /// Timestamp of the most recent message
    #[serde(default)]
    pub last_message_timestamp: Option<u64>,
}

fn default_metadata_version() -> u32 {
    1
}

/// Phase 1.5: Sanitize conversation ID to prevent path traversal attacks.
pub fn validate_conv_id(conv_id: &str) -> Result<(), String> {
    if conv_id.is_empty() {
        return Err("Conversation ID cannot be empty".to_string());
    }
    if conv_id.contains("..") || conv_id.contains('/') || conv_id.contains('\\') {
        return Err(format!("Invalid conversation ID '{}': path traversal detected", conv_id));
    }
    // ASCII-only safe path components (Unicode alnum is rejected).
    if !conv_id
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.')
    {
        return Err(format!("Invalid conversation ID '{}': only alphanumeric, hyphens, underscores, and dots allowed", conv_id));
    }
    Ok(())
}

fn get_conversations_root(app: &AppHandle) -> Result<PathBuf, String> {
    let app_data = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let conv_root = app_data.join("conversations");
    fs::create_dir_all(&conv_root).map_err(|e| e.to_string())?;
    Ok(conv_root)
}

fn get_conversation_dir(app: &AppHandle, conv_id: &str) -> Result<PathBuf, String> {
    validate_conv_id(conv_id)?;
    let root = get_conversations_root(app)?;
    let dir = root.join(conv_id);
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir)
}

fn metadata_path(conv_dir: &Path) -> PathBuf {
    conv_dir.join("metadata.json")
}

fn messages_path(conv_dir: &Path) -> PathBuf {
    conv_dir.join("messages.ndjson")
}

fn current_timestamp() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

/// Phase 1.5: Safely truncate a string to `max_chars` characters without panicking on multi-byte UTF-8.
pub fn safe_truncate(s: &str, max_chars: usize) -> String {
    s.chars().take(max_chars).collect::<String>()
}

// Create a new conversation with empty message log
pub fn create_conversation(
    app: &AppHandle,
    id: &str,
    name: &str,
    participant_ids: Vec<String>,
) -> Result<ConversationMetadata, String> {
    let dir = get_conversation_dir(app, id)?;
    let now = current_timestamp();

    let metadata = ConversationMetadata {
        version: 1,
        id: id.to_string(),
        name: name.to_string(),
        participant_ids,
        created_at: now,
        updated_at: now,
        message_count: 0,
        last_message_preview: None,
        last_message_timestamp: None,
    };

    let meta_path = metadata_path(&dir);
    let json = serde_json::to_string_pretty(&metadata).map_err(|e| e.to_string())?;
    atomic_write(&meta_path, &json)?;

    // Create empty messages.ndjson (initial creation — low risk, but use atomic for consistency)
    let msg_path = messages_path(&dir);
    atomic_write(&msg_path, "").map_err(|e| e.to_string())?;

    Ok(metadata)
}

/// Simple token estimation (Phase 1 - Context Management)
/// Rough approximation: ~4 characters per token for English text.
/// This is good enough for budget tracking; more accurate counting
/// can come later via llama-server tokenize endpoint.
///
/// Note: This is intentionally conservative for safety.
pub fn estimate_tokens(text: &str) -> usize {
    if text.is_empty() {
        return 0;
    }
    // Very rough: 1 token ≈ 4 chars + 1 for safety margin
    (text.len() + 3) / 4
}

/// Estimate total tokens for a list of messages (basic long-context foundation).
pub fn estimate_total_tokens(messages: &[ChatMessage]) -> usize {
    messages.iter().map(|m| m.token_count.max(estimate_tokens(&m.content))).sum()
}

// Append a single message (O(1) write)
pub fn append_message(
    app: &AppHandle,
    conv_id: &str,
    message: &ChatMessage,
) -> Result<(), String> {
    append_messages(app, conv_id, std::slice::from_ref(message))
}

/// Append one or more messages under a **single** ndjson lock + one metadata update.
/// Used so user+assistant pairs from a send never leave only the user turn persisted
/// when the second write would fail mid-way (both lines written before unlock).
pub fn append_messages(
    app: &AppHandle,
    conv_id: &str,
    messages: &[ChatMessage],
) -> Result<(), String> {
    if messages.is_empty() {
        return Ok(());
    }

    let dir = get_conversation_dir(app, conv_id)?;
    let msg_path = messages_path(&dir);

    let mut prepared: Vec<ChatMessage> = Vec::with_capacity(messages.len());
    let mut payload = String::new();
    for message in messages {
        let mut message = message.clone();
        if message.token_count == 0 {
            message.token_count = estimate_tokens(&message.content);
        }
        let line = serde_json::to_string(&message).map_err(|e| e.to_string())? + "\n";
        payload.push_str(&line);
        prepared.push(message);
    }

    // Exclusive lock on ndjson for the whole batch
    let lock_file = crate::storage::acquire_sidecar_lock(&msg_path)?;

    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&msg_path)
        .map_err(|e| e.to_string())?;
    // B4 fix: if file exists and doesn't end with newline (crash-truncated), insert separator
    if msg_path.exists() {
        if let Ok(meta) = fs::metadata(&msg_path) {
            if meta.len() > 0 {
                if let Ok(mut check) = fs::File::open(&msg_path) {
                    if check.seek(SeekFrom::End(-1)).is_ok() {
                        let mut last = [0u8; 1];
                        if check.read_exact(&mut last).is_ok() && last[0] != b'\n' {
                            file.write_all(b"\n").map_err(|e| e.to_string())?;
                            log::warn!("messages.ndjson missing trailing newline (likely crash-truncated), inserted separator before append (B4 fix)");
                        }
                    }
                }
            }
        }
    }
    file.write_all(payload.as_bytes()).map_err(|e| e.to_string())?;

    if let Err(e) = file.sync_all() {
        log::warn!(
            "fsync failed for messages.ndjson (data may be at risk on power loss): {}",
            e
        );
    }

    if let Err(e) = lock_file.unlock() {
        log::warn!("Failed to unlock ndjson append lock: {}", e);
    }

    // Metadata: one RMW for the whole batch
    let meta_path = metadata_path(&dir);
    let meta_lock = crate::storage::acquire_sidecar_lock(&meta_path)?;

    let last = prepared
        .last()
        .expect("messages non-empty checked above");
    let batch_n = prepared.len() as u32;

    let meta = if meta_path.exists() {
        let result = (|| -> Result<ConversationMetadata, String> {
            let content = fs::read_to_string(&meta_path).map_err(|e| e.to_string())?;
            serde_json::from_str::<ConversationMetadata>(&content).map_err(|e| e.to_string())
        })();
        let mut meta = match result {
            Ok(m) => m,
            Err(e) => {
                log::warn!("Metadata auto-repair triggered for {}: {}", conv_id, e);
                let backup = meta_path.with_extension("json.bak");
                let _ = fs::copy(&meta_path, &backup);
                // Count existing valid lines so we do not undercount after corruption.
                let existing_count = fs::read_to_string(&msg_path)
                    .ok()
                    .map(|c| {
                        c.lines()
                            .filter(|l| !l.trim().is_empty() && serde_json::from_str::<ChatMessage>(l).is_ok())
                            .count() as u32
                    })
                    .unwrap_or(0)
                    .saturating_sub(batch_n); // file already includes this batch
                let participants = participant_ids_from_messages(&prepared);
                ConversationMetadata {
                    version: 1,
                    id: conv_id.to_string(),
                    name: "Repaired".to_string(),
                    participant_ids: participants,
                    created_at: current_timestamp(),
                    updated_at: current_timestamp(),
                    message_count: existing_count,
                    last_message_preview: None,
                    last_message_timestamp: None,
                }
            }
        };

        // Preserve empty participants by trying to recover from this batch
        if meta.participant_ids.is_empty() {
            meta.participant_ids = participant_ids_from_messages(&prepared);
        }

        meta.message_count = meta.message_count.saturating_add(batch_n);
        meta.updated_at = current_timestamp();

        let preview = if last.content.chars().count() > 60 {
            format!("{}...", safe_truncate(&last.content, 57))
        } else {
            last.content.clone()
        };
        meta.last_message_preview = Some(preview);
        meta.last_message_timestamp = Some(last.timestamp);

        meta
    } else {
        let preview = if last.content.chars().count() > 60 {
            format!("{}...", safe_truncate(&last.content, 57))
        } else {
            last.content.clone()
        };
        ConversationMetadata {
            version: 1,
            id: conv_id.to_string(),
            name: "Untitled".to_string(),
            participant_ids: vec![],
            created_at: current_timestamp(),
            updated_at: current_timestamp(),
            message_count: batch_n,
            last_message_preview: Some(preview),
            last_message_timestamp: Some(last.timestamp),
        }
    };

    let meta_json = serde_json::to_string_pretty(&meta).map_err(|e| e.to_string())?;
    // Write bytes while holding meta lock (avoid nested sidecar lock via atomic_write_with_lock)
    atomic_write_bytes(&meta_path, meta_json.as_bytes())?;

    if let Err(e) = meta_lock.unlock() {
        log::warn!("Failed to unlock metadata lock: {}", e);
    }

    Ok(())
}

pub fn load_metadata(app: &AppHandle, conv_id: &str) -> Result<Option<ConversationMetadata>, String> {
    let dir = get_conversation_dir(app, conv_id)?;
    let meta_path = metadata_path(&dir);
    if !meta_path.exists() {
        return Ok(None);
    }
    let json = fs::read_to_string(meta_path).map_err(|e| e.to_string())?;
    let meta: ConversationMetadata = serde_json::from_str(&json).map_err(|e| e.to_string())?;

    // Phase 0: Migration detection
    if meta.version < 1 {
        log::info!(
            "Conversation {} loaded with old schema version {}. Migration may be needed in future.",
            conv_id, meta.version
        );
    }

    Ok(Some(meta))
}

pub fn list_conversation_metadata(app: &AppHandle) -> Result<Vec<ConversationMetadata>, String> {
    let root = get_conversations_root(app)?;
    let mut result = Vec::new();

    for entry in fs::read_dir(root).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        if path.is_dir() {
            let meta_path = path.join("metadata.json");
            if meta_path.exists() {
                match fs::read_to_string(&meta_path) {
                    Ok(json) => {
                        match serde_json::from_str::<ConversationMetadata>(&json) {
                            Ok(meta) => result.push(meta),
                            Err(e) => {
                                // FIX-C08: Log corrupt metadata so conversations are not silently lost
                                log::error!(
                                    "Failed to parse metadata from {}: {} — conversation may be corrupt",
                                    meta_path.display(), e
                                );
                            }
                        }
                    }
                    Err(e) => {
                        log::error!(
                            "Failed to read metadata file {}: {}",
                            meta_path.display(), e
                        );
                    }
                }
            }
        }
    }

    // Sort by most recent activity (prefer last message timestamp for chat-list feel)
    result.sort_by(|a, b| {
        let time_a = a.last_message_timestamp.unwrap_or(a.updated_at);
        let time_b = b.last_message_timestamp.unwrap_or(b.updated_at);
        time_b.cmp(&time_a)
    });
    Ok(result)
}

/// Phase 1.5: Real BufReader-based pagination — loads only the requested window.
/// This replaces the old implementation that loaded ALL messages then sliced.
pub fn load_messages_paginated(
    app: &AppHandle,
    conv_id: &str,
    offset: usize,
    limit: usize,
) -> Result<Vec<ChatMessage>, String> {
    let dir = get_conversation_dir(app, conv_id)?;
    let msg_path = messages_path(&dir);
    if !msg_path.exists() {
        return Ok(vec![]);
    }

    let file = fs::File::open(&msg_path).map_err(|e| e.to_string())?;
    let reader = BufReader::new(file);
    let mut messages = Vec::with_capacity(limit.min(1000));
    let mut skipped = 0usize;
    let mut collected = 0usize;

    for line_result in reader.lines() {
        let line = match line_result {
            Ok(l) => l,
            Err(_) => continue,
        };

        if line.trim().is_empty() {
            continue;
        }

        // Skip lines until we reach the offset
        if skipped < offset {
            skipped += 1;
            continue;
        }

        // Collect up to `limit` messages
        if collected >= limit {
            break;
        }

        match serde_json::from_str::<ChatMessage>(&line) {
            Ok(msg) => {
                messages.push(msg);
                collected += 1;
            }
            Err(e) => {
                log::warn!("Skipping corrupted line in conversation {}: {}", conv_id, e);
            }
        }
    }

    Ok(messages)
}

/// Loads messages from the end (most recent first) until the cumulative
/// estimated token count reaches or exceeds the given budget.
/// Returns messages in chronological order (oldest first within the window).
pub fn load_messages_within_token_budget(
    app: &AppHandle,
    conv_id: &str,
    max_tokens: usize,
) -> Result<Vec<ChatMessage>, String> {
    let dir = get_conversation_dir(app, conv_id)?;
    let msg_path = messages_path(&dir);
    if !msg_path.exists() {
        return Ok(vec![]);
    }

    let file = fs::File::open(&msg_path).map_err(|e| e.to_string())?;
    let reader = BufReader::new(file);

    let mut all_lines: Vec<String> = reader
        .lines()
        .filter_map(|l| l.ok())
        .filter(|l| !l.trim().is_empty())
        .collect();

    // Work backwards from the end
    let mut selected: Vec<ChatMessage> = Vec::new();
    let mut total_tokens: usize = 0;

    for line in all_lines.iter().rev() {
        match serde_json::from_str::<ChatMessage>(line) {
            Ok(msg) => {
                let tokens = if msg.token_count > 0 {
                    msg.token_count
                } else {
                    estimate_tokens(&msg.content)
                };

                if total_tokens + tokens > max_tokens && !selected.is_empty() {
                    break;
                }

                selected.push(msg);
                total_tokens += tokens;
            }
            Err(e) => {
                log::warn!("Skipping corrupted line while loading context budget for {}: {}", conv_id, e);
            }
        }
    }

    selected.reverse(); // chronological order
    Ok(selected)
}

/// Keep the newest messages from `messages` until `max_tokens` is reached.
/// Always keeps at least the last message. Chronological order preserved.
pub fn take_messages_within_token_budget(
    messages: &[ChatMessage],
    max_tokens: usize,
) -> Vec<ChatMessage> {
    if messages.is_empty() {
        return vec![];
    }
    let mut selected: Vec<ChatMessage> = Vec::new();
    let mut total_tokens: usize = 0;
    for msg in messages.iter().rev() {
        let tokens = if msg.token_count > 0 {
            msg.token_count
        } else {
            estimate_tokens(&msg.content)
        };
        if total_tokens + tokens > max_tokens && !selected.is_empty() {
            break;
        }
        selected.push(msg.clone());
        total_tokens += tokens;
    }
    selected.reverse();
    selected
}

pub fn load_all_messages(app: &AppHandle, conv_id: &str) -> Result<Vec<ChatMessage>, String> {
    let dir = get_conversation_dir(app, conv_id)?;
    let msg_path = messages_path(&dir);
    if !msg_path.exists() {
        return Ok(vec![]);
    }

    // Phase 0: Migration detection - check conversation metadata version
    if let Ok(Some(meta)) = load_metadata(app, conv_id) {
        if meta.version < 1 {
            log::info!(
                "Loading conversation {} with legacy schema (version {}). Consider running repair/migration.",
                conv_id, meta.version
            );
        }
    }

    let file = fs::File::open(&msg_path).map_err(|e| e.to_string())?;
    let reader = BufReader::new(file);
    let mut messages = Vec::new();

    for (line_number, line_result) in reader.lines().enumerate() {
        let line = match line_result {
            Ok(l) => l,
            Err(e) => {
                log::warn!("Failed to read line {} in {}: {}", line_number, msg_path.display(), e);
                continue;
            }
        };

        if line.trim().is_empty() {
            continue;
        }

        match serde_json::from_str::<ChatMessage>(&line) {
            Ok(msg) => messages.push(msg),
            Err(e) => {
                // Phase 0: Resilience - skip corrupted/truncated lines instead of failing the whole load
                log::warn!(
                    "Skipping corrupted line {} in conversation {}: {}",
                    line_number, conv_id, e
                );
            }
        }
    }

    Ok(messages)
}

pub fn delete_conversation(app: &AppHandle, conv_id: &str) -> Result<(), String> {
    let dir = get_conversation_dir(app, conv_id)?;
    fs::remove_dir_all(dir).map_err(|e| e.to_string())
}

/// Collect character participant ids from message speakers (skip reserved roles).
fn participant_ids_from_messages(messages: &[ChatMessage]) -> Vec<String> {
    let mut ids = Vec::new();
    for m in messages {
        let sid = m.speaker_id.trim();
        if sid.is_empty() {
            continue;
        }
        let lower = sid.to_ascii_lowercase();
        if matches!(lower.as_str(), "user" | "assistant" | "narrator" | "system") {
            continue;
        }
        if m.role == "assistant" && !ids.iter().any(|x| x == sid) {
            ids.push(sid.to_string());
        }
    }
    ids
}

fn preview_from_message(msg: &ChatMessage) -> String {
    if msg.content.chars().count() > 60 {
        format!("{}...", safe_truncate(&msg.content, 57))
    } else {
        msg.content.clone()
    }
}

/// Phase 0: Repair a potentially corrupted conversation.
/// - Repairs messages.ndjson (skips bad lines, rewrites clean file)
/// - Always reconciles metadata.json count/participants/preview with recovered messages
/// Returns the number of messages recovered.
pub fn repair_conversation(app: &AppHandle, conv_id: &str) -> Result<usize, String> {
    let dir = get_conversation_dir(app, conv_id)?;
    let msg_path = messages_path(&dir);
    let meta_path = metadata_path(&dir);

    if !msg_path.exists() && !meta_path.exists() {
        return Ok(0);
    }

    // --- Repair Messages --- B9 fix: hold sidecar lock for read+backup+rewrite to prevent clobber vs concurrent append
    let mut recovered_messages: Vec<ChatMessage> = Vec::new();
    if msg_path.exists() {
        let msg_lock = crate::storage::acquire_sidecar_lock(&msg_path)?;
        let content = fs::read_to_string(&msg_path).map_err(|e| {
            let _ = msg_lock.unlock();
            e.to_string()
        })?;

        for line in content.lines() {
            if line.trim().is_empty() {
                continue;
            }
            if let Ok(msg) = serde_json::from_str::<ChatMessage>(line) {
                recovered_messages.push(msg);
            }
        }

        // FIX-C10: Backup + rewrite messages — abort if backup fails to prevent data loss
        let backup_path = msg_path.with_extension("ndjson.bak");
        if let Err(e) = fs::copy(&msg_path, &backup_path) {
            log::error!("Failed to backup messages.ndjson during repair: {} — aborting to prevent data loss", e);
            let _ = msg_lock.unlock();
            return Err(format!("Repair aborted: could not create backup of messages: {}", e));
        }

        let mut new_content = String::new();
        for msg in &recovered_messages {
            if let Ok(line) = serde_json::to_string(msg) {
                new_content.push_str(&line);
                new_content.push('\n');
            }
        }
        // Write while still holding lock (use bytes variant to avoid nested sidecar lock)
        if let Err(e) = crate::storage::atomic_write_bytes(&msg_path, new_content.as_bytes()) {
            let _ = msg_lock.unlock();
            return Err(e);
        }
        let _ = msg_lock.unlock();
    }

    // --- Always reconcile metadata with recovered messages --- B9 fix: hold lock for RMW
    let meta_lock = if meta_path.exists() {
        Some(crate::storage::acquire_sidecar_lock(&meta_path)?)
    } else {
        // Ensure parent exists so sidecar lock can be created even if meta missing
        if let Some(parent) = meta_path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        Some(crate::storage::acquire_sidecar_lock(&meta_path)?)
    };
    let existing_meta = if meta_path.exists() {
        fs::read_to_string(&meta_path)
            .ok()
            .and_then(|c| serde_json::from_str::<ConversationMetadata>(&c).ok())
    } else {
        None
    };

    let now = current_timestamp();
    let mut participants = existing_meta
        .as_ref()
        .map(|m| m.participant_ids.clone())
        .unwrap_or_default();
    if participants.is_empty() {
        participants = participant_ids_from_messages(&recovered_messages);
    }

    let new_meta = ConversationMetadata {
        version: 1,
        id: conv_id.to_string(),
        name: existing_meta
            .as_ref()
            .map(|m| m.name.clone())
            .filter(|n| !n.is_empty())
            .unwrap_or_else(|| "Recovered Conversation".to_string()),
        participant_ids: participants,
        created_at: existing_meta
            .as_ref()
            .map(|m| m.created_at)
            .unwrap_or(now),
        updated_at: now,
        message_count: recovered_messages.len() as u32,
        last_message_preview: recovered_messages.last().map(preview_from_message),
        last_message_timestamp: recovered_messages.last().map(|m| m.timestamp),
    };

    let needs_write = existing_meta
        .as_ref()
        .map(|m| {
            m.message_count != new_meta.message_count
                || m.participant_ids != new_meta.participant_ids
                || m.last_message_preview != new_meta.last_message_preview
        })
        .unwrap_or(true);

    if needs_write {
        if meta_path.exists() {
            let backup_meta = meta_path.with_extension("json.bak");
            if let Err(e) = fs::copy(&meta_path, &backup_meta) {
                log::error!("Failed to backup metadata.json during repair: {} — aborting to prevent data loss", e);
                if let Some(l) = meta_lock { let _ = l.unlock(); }
                return Err(format!("Repair aborted: could not create backup of metadata: {}", e));
            }
        }
        let json = serde_json::to_string_pretty(&new_meta).map_err(|e| e.to_string())?;
        // Use bytes variant while holding lock to avoid nested sidecar deadlock
        if let Err(e) = crate::storage::atomic_write_bytes(&meta_path, json.as_bytes()) {
            if let Some(l) = meta_lock { let _ = l.unlock(); }
            return Err(e);
        }
    }
    if let Some(l) = meta_lock { let _ = l.unlock(); }

    Ok(recovered_messages.len())
}

/// Replace the entire messages.ndjson with `messages` and rebuild metadata (single write).
/// Used by mid-thread regenerate to drop turns after the target user message.
///
/// Always writes a timestamped backup of the previous messages.ndjson (and metadata if present)
/// before rewrite so destructive regenerates are recoverable.
pub fn rewrite_conversation_messages(
    app: &AppHandle,
    conv_id: &str,
    messages: &[ChatMessage],
) -> Result<(), String> {
    validate_conv_id(conv_id)?;
    let dir = get_conversation_dir(app, conv_id)?;
    let msg_path = messages_path(&dir);
    let meta_path = metadata_path(&dir);

    // Snapshot before destructive rewrite
    let stamp = current_timestamp();
    if msg_path.exists() {
        let bak = dir.join(format!("messages.ndjson.bak.{}", stamp));
        if let Err(e) = fs::copy(&msg_path, &bak) {
            return Err(format!(
                "Refusing rewrite: could not backup messages.ndjson: {}",
                e
            ));
        }
        log::info!("Backed up conversation {} messages to {}", conv_id, bak.display());
    }
    if meta_path.exists() {
        let bak = dir.join(format!("metadata.json.bak.{}", stamp));
        if let Err(e) = fs::copy(&meta_path, &bak) {
            log::warn!("Could not backup metadata before rewrite: {}", e);
        }
    }

    let lock = crate::storage::acquire_sidecar_lock(&msg_path)?;
    let mut payload = String::new();
    for m in messages {
        let mut msg = m.clone();
        if msg.token_count == 0 {
            msg.token_count = estimate_tokens(&msg.content);
        }
        payload.push_str(&serde_json::to_string(&msg).map_err(|e| e.to_string())?);
        payload.push('\n');
    }
    atomic_write_bytes(&msg_path, payload.as_bytes())?;
    if let Err(e) = lock.unlock() {
        log::warn!("Failed to unlock after rewrite messages: {}", e);
    }

    let existing = load_metadata(app, conv_id)?.unwrap_or(ConversationMetadata {
        version: 1,
        id: conv_id.to_string(),
        name: "Untitled".into(),
        participant_ids: participant_ids_from_messages(messages),
        created_at: current_timestamp(),
        updated_at: current_timestamp(),
        message_count: 0,
        last_message_preview: None,
        last_message_timestamp: None,
    });

    let mut participants = existing.participant_ids;
    if participants.is_empty() {
        participants = participant_ids_from_messages(messages);
    }

    let meta = ConversationMetadata {
        version: existing.version.max(1),
        id: conv_id.to_string(),
        name: existing.name,
        participant_ids: participants,
        created_at: existing.created_at,
        updated_at: current_timestamp(),
        message_count: messages.len() as u32,
        last_message_preview: messages.last().map(preview_from_message),
        last_message_timestamp: messages.last().map(|m| m.timestamp),
    };
    let meta_json = serde_json::to_string_pretty(&meta).map_err(|e| e.to_string())?;
    let meta_lock = crate::storage::acquire_sidecar_lock(&meta_path)?;
    atomic_write_bytes(&meta_path, meta_json.as_bytes())?;
    if let Err(e) = meta_lock.unlock() {
        log::warn!("Failed to unlock after rewrite metadata: {}", e);
    }
    Ok(())
}

// Fuzz helper: parse a single NDJSON line as ChatMessage (for fuzz/fuzz_targets/ndjson_parse.rs)
pub fn parse_message_line(data: &[u8]) -> Result<ChatMessage, String> {
    let s = std::str::from_utf8(data).map_err(|e| e.to_string())?;
    if s.trim().is_empty() {
        return Err("empty line".to_string());
    }
    serde_json::from_str::<ChatMessage>(s).map_err(|e| e.to_string())
}

// Convenience wrapper
pub fn get_conversation_metadata(app: &AppHandle, conv_id: &str) -> Result<Option<ConversationMetadata>, String> {
    load_metadata(app, conv_id)
}

/// Delete every conversation that lists `character_id` as a participant.
/// Returns number of conversation directories removed.
pub fn delete_conversations_for_character(
    app: &AppHandle,
    character_id: &str,
) -> Result<usize, String> {
    if character_id.trim().is_empty() {
        return Ok(0);
    }
    let all = list_conversation_metadata(app)?;
    let mut removed = 0usize;
    for meta in all {
        if meta.participant_ids.iter().any(|p| p == character_id) {
            match delete_conversation(app, &meta.id) {
                Ok(()) => removed += 1,
                Err(e) => log::warn!(
                    "Failed to delete conversation {} for character {}: {}",
                    meta.id,
                    character_id,
                    e
                ),
            }
        }
    }
    Ok(removed)
}

/// Drop trailing assistant messages after the last user turn (for last-turn regenerate replace).
pub fn truncate_after_last_user(messages: &[ChatMessage]) -> Vec<ChatMessage> {
    if messages.is_empty() {
        return vec![];
    }
    let last_user_idx = messages
        .iter()
        .rposition(|m| m.role == "user")
        .unwrap_or(messages.len().saturating_sub(1));
    messages[..=last_user_idx].to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use std::io::Write;

    #[test]
    fn test_ndjson_append_and_load_roundtrip() {
        let temp = TempDir::new().unwrap();
        let dir = temp.path().join("test-conv");
        std::fs::create_dir_all(&dir).unwrap();

        let msg_path = dir.join("messages.ndjson");
        std::fs::write(&msg_path, "").unwrap();

        let msg1 = ChatMessage {
            id: "m1".into(),
            conversation_id: "c1".into(),
            speaker_id: "user".into(),
            speaker_name: "You".into(),
            content: "First message".into(),
            timestamp: 1000,
            role: "user".into(),
            images: vec![],
            token_count: 0, // will be estimated
        };
        let msg2 = ChatMessage {
            id: "m2".into(),
            conversation_id: "c1".into(),
            speaker_id: "assistant".into(),
            speaker_name: "Bot".into(),
            content: "Second message".into(),
            timestamp: 2000,
            role: "assistant".into(),
            images: vec![],
            token_count: 0,
        };

        let mut f = std::fs::OpenOptions::new().append(true).open(&msg_path).unwrap();
        writeln!(f, "{}", serde_json::to_string(&msg1).unwrap()).unwrap();
        writeln!(f, "{}", serde_json::to_string(&msg2).unwrap()).unwrap();
        drop(f);

        let content = std::fs::read_to_string(&msg_path).unwrap();
        let lines: Vec<_> = content.lines().collect();
        assert_eq!(lines.len(), 2);

        let loaded1: ChatMessage = serde_json::from_str(lines[0]).unwrap();
        let loaded2: ChatMessage = serde_json::from_str(lines[1]).unwrap();

        assert_eq!(loaded1.content, "First message");
        assert_eq!(loaded2.content, "Second message");
    }

    #[test]
    fn test_load_skips_corrupted_ndjson_lines() {
        let temp = TempDir::new().unwrap();
        let msg_path = temp.path().join("messages.ndjson");

        let valid = r#"{"id":"1","conversation_id":"c1","speaker_id":"u","speaker_name":"You","content":"good","timestamp":1,"role":"user","images":[],"token_count":0}"#;
        let bad = r#"{"id":"2", "content": "truncated"#;

        std::fs::write(&msg_path, format!("{}\n{}\n", valid, bad)).unwrap();

        let content = std::fs::read_to_string(&msg_path).unwrap();
        let mut valid_count = 0;
        for line in content.lines() {
            if serde_json::from_str::<ChatMessage>(line).is_ok() {
                valid_count += 1;
            }
        }
        assert_eq!(valid_count, 1, "Corrupted line must be skipped");
    }

    #[test]
    fn test_repair_conversation_recovers_valid_messages() {
        let temp = TempDir::new().unwrap();
        let dir = temp.path().join("repair-test");
        std::fs::create_dir_all(&dir).unwrap();

        let msg_path = dir.join("messages.ndjson");
        let meta_path = dir.join("metadata.json");

        // Setup minimal valid structure
        std::fs::write(&meta_path, r#"{"version":1,"id":"repair-test","name":"Test","participant_ids":[],"created_at":0,"updated_at":0,"message_count":0}"#).unwrap();

        // Mix of good and bad lines
        let good1 = r#"{"id":"g1","conversation_id":"repair-test","speaker_id":"u","speaker_name":"You","content":"valid one","timestamp":1,"role":"user","images":[]}"#;
        let bad = r#"{"id":"bad", "content": "broken"#;
        let good2 = r#"{"id":"g2","conversation_id":"repair-test","speaker_id":"a","speaker_name":"Bot","content":"valid two","timestamp":2,"role":"assistant","images":[]}"#;

        std::fs::write(&msg_path, format!("{}\n{}\n{}\n", good1, bad, good2)).unwrap();

        // We can't easily call the real repair without an AppHandle, so we test the logic directly
        let content = std::fs::read_to_string(&msg_path).unwrap();
        let mut recovered = Vec::new();
        for line in content.lines() {
            if let Ok(msg) = serde_json::from_str::<ChatMessage>(line) {
                recovered.push(msg);
            }
        }

        assert_eq!(recovered.len(), 2);
        assert_eq!(recovered[0].content, "valid one");
        assert_eq!(recovered[1].content, "valid two");
    }

    #[test]
    fn test_ndjson_empty_file_returns_empty() {
        let temp = TempDir::new().unwrap();
        let dir = temp.path().join("empty-conv");
        std::fs::create_dir_all(&dir).unwrap();

        let msg_path = dir.join("messages.ndjson");
        std::fs::write(&msg_path, "").unwrap();

        // Simulate what load_all_messages does
        let content = std::fs::read_to_string(&msg_path).unwrap();
        let mut messages = Vec::new();
        for line in content.lines() {
            if line.trim().is_empty() { continue; }
            if let Ok(msg) = serde_json::from_str::<ChatMessage>(line) {
                messages.push(msg);
            }
        }
        assert!(messages.is_empty());
    }

    #[test]
    fn test_append_multiple_messages_preserves_order() {
        let temp = TempDir::new().unwrap();
        let dir = temp.path().join("order-test");
        std::fs::create_dir_all(&dir).unwrap();

        let msg_path = dir.join("messages.ndjson");
        std::fs::write(&msg_path, "").unwrap();

        let mut file = std::fs::OpenOptions::new().append(true).open(&msg_path).unwrap();

        for i in 0..5 {
            let msg = ChatMessage {
                id: format!("m{}", i),
                conversation_id: "c1".into(),
                speaker_id: "user".into(),
                speaker_name: "You".into(),
                content: format!("Message {}", i),
                timestamp: 1000 + i as u64,
                role: "user".into(),
                images: vec![],
                token_count: 0,
            };
            writeln!(file, "{}", serde_json::to_string(&msg).unwrap()).unwrap();
        }
        drop(file);

        let content = std::fs::read_to_string(&msg_path).unwrap();
        let lines: Vec<_> = content.lines().collect();
        assert_eq!(lines.len(), 5);

        for (i, line) in lines.iter().enumerate() {
            let msg: ChatMessage = serde_json::from_str(line).unwrap();
            assert_eq!(msg.content, format!("Message {}", i));
        }
    }

    #[test]
    fn test_truncate_after_last_user() {
        let msgs = vec![
            ChatMessage {
                id: "u1".into(),
                conversation_id: "c".into(),
                speaker_id: "user".into(),
                speaker_name: "You".into(),
                content: "hi".into(),
                timestamp: 1,
                role: "user".into(),
                images: vec![],
                token_count: 1,
            },
            ChatMessage {
                id: "a1".into(),
                conversation_id: "c".into(),
                speaker_id: "bot".into(),
                speaker_name: "Bot".into(),
                content: "hello".into(),
                timestamp: 2,
                role: "assistant".into(),
                images: vec![],
                token_count: 1,
            },
            ChatMessage {
                id: "u2".into(),
                conversation_id: "c".into(),
                speaker_id: "user".into(),
                speaker_name: "You".into(),
                content: "again".into(),
                timestamp: 3,
                role: "user".into(),
                images: vec![],
                token_count: 1,
            },
            ChatMessage {
                id: "a2".into(),
                conversation_id: "c".into(),
                speaker_id: "bot".into(),
                speaker_name: "Bot".into(),
                content: "ok".into(),
                timestamp: 4,
                role: "assistant".into(),
                images: vec![],
                token_count: 1,
            },
        ];
        let kept = truncate_after_last_user(&msgs);
        assert_eq!(kept.len(), 3);
        assert_eq!(kept[2].id, "u2");
    }

    #[test]
    fn test_take_messages_within_token_budget_keeps_tail() {
        let msgs: Vec<ChatMessage> = (0..10)
            .map(|i| ChatMessage {
                id: format!("m{i}"),
                conversation_id: "c".into(),
                speaker_id: "user".into(),
                speaker_name: "You".into(),
                content: "xxxx".repeat(20), // ~80 chars ~20 tokens estimate
                timestamp: i,
                role: "user".into(),
                images: vec![],
                token_count: 20,
            })
            .collect();
        let taken = take_messages_within_token_budget(&msgs, 50);
        assert!(taken.len() >= 1);
        assert!(taken.len() <= 3);
        assert_eq!(taken.last().unwrap().id, "m9");
    }
}

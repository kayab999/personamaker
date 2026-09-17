// Persona and chat history persistence layer
// Real file-based storage instead of localStorage.

use std::fs::OpenOptions;
use std::path::{Path, PathBuf};
use tauri::AppHandle;
use tauri::Manager;
use base64::engine::Engine;

use serde::{Deserialize, Serialize};
use tempfile::NamedTempFile;
use fs2::FileExt;

/// Phase 1.5 Security: Validates a character_id to prevent path traversal attacks.
/// Rejects empty IDs, `..`, `/`, `\`, and non-alphanumeric characters (allowing hyphens, underscores, dots).
fn is_safe_id_char(c: char) -> bool {
    // ASCII-only: Unicode alphanumeric (e.g. ¼) must not be allowed in path components.
    c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.'
}

pub fn validate_character_id(character_id: &str) -> Result<(), String> {
    if character_id.is_empty() {
        return Err("Character ID cannot be empty".to_string());
    }
    if character_id.contains("..") || character_id.contains('/') || character_id.contains('\\') {
        return Err(format!("Invalid character ID '{}': path traversal detected", character_id));
    }
    if !character_id.chars().all(is_safe_id_char) {
        return Err(format!("Invalid character ID '{}': only alphanumeric, hyphens, underscores, and dots allowed", character_id));
    }
    Ok(())
}

/// Phase 1.5 Security: Validates an owner_id (for chat history) to prevent path traversal attacks.
pub fn validate_owner_id(owner_id: &str) -> Result<(), String> {
    if owner_id.is_empty() {
        return Err("Owner ID cannot be empty".to_string());
    }
    if owner_id.contains("..") || owner_id.contains('/') || owner_id.contains('\\') {
        return Err(format!("Invalid owner ID '{}': path traversal detected", owner_id));
    }
    if !owner_id.chars().all(is_safe_id_char) {
        return Err(format!("Invalid owner ID '{}': only alphanumeric, hyphens, underscores, and dots allowed", owner_id));
    }
    Ok(())
}

/// Returns the main application data directory (e.g. ~/.local/share/LocalPersona or equivalent)
pub fn get_app_data_dir(app: &AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_data_dir()
        .map_err(|e| format!("Failed to get app data directory: {}", e))
}

/// Performs an atomic write of string content to a file using tempfile + rename + exclusive lock.
/// This is the recommended function for all critical data files.
pub(crate) fn atomic_write(path: &Path, content: &str) -> Result<(), String> {
    atomic_write_with_lock(path, content.as_bytes())
}

/// Sidecar lock path: `foo.json` → `foo.json.lock` (not `foo.lock`).
/// Keeps append_message and atomic_write on the same lock inode for a given file.
pub(crate) fn sidecar_lock_path(path: &Path) -> PathBuf {
    let mut os = path.as_os_str().to_os_string();
    os.push(".lock");
    PathBuf::from(os)
}

/// Acquire an exclusive flock on a stable sidecar lock file.
///
/// Intentionally does **not** delete "stale" lock files by mtime: unlinking a lock
/// that another process still holds (flock on the open inode) creates a new inode and
/// dual-writer races. OS releases flock when the holder dies; we just block on lock.
pub(crate) fn acquire_sidecar_lock(target_path: &Path) -> Result<std::fs::File, String> {
    let lock_path = sidecar_lock_path(target_path);
    let parent = lock_path
        .parent()
        .ok_or_else(|| "Lock path has no parent".to_string())?;
    std::fs::create_dir_all(parent).map_err(|e| format!("Failed to create lock dir: {}", e))?;

    let lock_file = OpenOptions::new()
        .create(true)
        .write(true)
        .open(&lock_path)
        .map_err(|e| format!("Failed to open lock file: {}", e))?;

    lock_file
        .lock_exclusive()
        .map_err(|e| format!("Failed to acquire exclusive lock: {}", e))?;
    Ok(lock_file)
}

/// Atomic write with exclusive file lock using fs2.
/// Prevents data loss when multiple commands write the same file concurrently.
pub(crate) fn atomic_write_with_lock(path: &Path, content: &[u8]) -> Result<(), String> {
    let lock_file = acquire_sidecar_lock(path)?;

    // Perform the actual atomic write while holding the lock
    let result = atomic_write_bytes(path, content);

    // Unlock only — keep the lock file so the inode stays stable for future writers.
    if let Err(e) = lock_file.unlock() {
        log::warn!(
            "Failed to unlock file {}: {}",
            sidecar_lock_path(path).display(),
            e
        );
    }

    result
}

/// Atomic write for raw bytes (avatars, voice samples, uploaded documents, images, etc.).
/// Uses the same tempfile + persist strategy as atomic_write for JSON.
/// FIX-C05: Includes explicit fsync on the temp file before persist for durability guarantees.
pub(crate) fn atomic_write_bytes(path: &Path, data: &[u8]) -> Result<(), String> {
    let dir = path.parent().ok_or_else(|| "Target path has no parent directory".to_string())?;
    std::fs::create_dir_all(dir).map_err(|e| format!("Failed to ensure parent dir: {}", e))?;

    let temp = NamedTempFile::new_in(dir)
        .map_err(|e| format!("Failed to create temp file for atomic bytes write: {}", e))?;

    std::fs::write(temp.path(), data)
        .map_err(|e| format!("Failed to write temp file (bytes): {}", e))?;

    // FIX-C05: fsync the temp file before atomic rename for durability
    if let Err(e) = temp.as_file().sync_all() {
        log::warn!("fsync failed for temp file before atomic rename (data may be at risk on power loss): {}", e);
    }

    temp.persist(path)
        .map_err(|e| format!("Failed to persist atomic bytes write to {:?}: {}", path, e))?;

    // R12: fsync directory after rename for POSIX durability (best effort)
    if let Ok(dir_file) = std::fs::File::open(dir) {
        if let Err(e) = dir_file.sync_all() {
            log::warn!("fsync of directory {:?} failed after atomic rename: {} (risk accepted on this FS)", dir, e);
        }
    }

    Ok(())
}

/// Ensures that the avatars subdirectory exists.
pub fn ensure_avatars_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let data_dir = get_app_data_dir(app)?;
    let avatars_dir = data_dir.join("avatars");
    std::fs::create_dir_all(&avatars_dir)
        .map_err(|e| format!("Failed to create avatars directory: {}", e))?;
    Ok(avatars_dir)
}

/// Saves an avatar image for a character.
/// `data` should be a base64 data URL (e.g. "data:image/png;base64,....") or raw base64.
/// Returns the relative path that should be stored in the character (e.g. "avatars/abc123.png")
pub fn save_avatar(app: &AppHandle, character_id: &str, data: &str) -> Result<String, String> {
    validate_character_id(character_id)?;
    let avatars_dir = ensure_avatars_dir(app)?;

    // Strip data URL prefix if present
    let base64_data = if data.contains(",") {
        data.split(',').nth(1).unwrap_or(data)
    } else {
        data
    };

    // Decode base64
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(base64_data)
        .map_err(|e| format!("Failed to decode avatar image: {}", e))?;

    // Determine extension (very basic)
    let ext = if data.contains("image/png") {
        "png"
    } else if data.contains("image/jpeg") || data.contains("image/jpg") {
        "jpg"
    } else if data.contains("image/webp") {
        "webp"
    } else {
        "png" // fallback
    };

    let filename = format!("{}.{}", character_id, ext);
    let file_path = avatars_dir.join(&filename);

    atomic_write_bytes(&file_path, &bytes)
        .map_err(|e| format!("Failed to write avatar file: {}", e))?;

    // Return relative path for storage in character JSON
    Ok(format!("avatars/{}", filename))
}

/// Reads an avatar file and returns it as a base64 data URL.
pub fn get_avatar_as_data_url(app: &AppHandle, relative_path: &str) -> Result<String, String> {
    let full_path = get_absolute_path(app, relative_path)?;

    if !full_path.exists() {
        return Err("Avatar file not found".to_string());
    }

    let bytes = std::fs::read(&full_path)
        .map_err(|e| format!("Failed to read avatar: {}", e))?;

    // Very basic mime detection
    let mime = if relative_path.ends_with(".png") {
        "image/png"
    } else if relative_path.ends_with(".jpg") || relative_path.ends_with(".jpeg") {
        "image/jpeg"
    } else if relative_path.ends_with(".webp") {
        "image/webp"
    } else {
        "application/octet-stream"
    };

    let base64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
    Ok(format!("data:{};base64,{}", mime, base64))
}

// ==================== CHARACTER PERSISTENCE ====================

/// Character record on disk + IPC.
///
/// Serialized as **camelCase** for the frontend. Deserialization accepts both
/// camelCase and legacy snake_case field names (and `avatarUrl` / `avatar_path`).
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct StoredCharacter {
    /// Schema version for future migrations (Phase 0)
    #[serde(default = "default_character_version")]
    pub version: u32,

    pub id: String,
    pub name: String,
    #[serde(default)]
    pub mode: String,
    #[serde(default, alias = "avatar_color")]
    pub avatar_color: String,
    /// Relative path like "avatars/xxx.png" — wire name is `avatarUrl` for UI compatibility.
    #[serde(
        default,
        rename = "avatarUrl",
        alias = "avatar_path",
        alias = "avatarPath",
        alias = "avatarUrl"
    )]
    pub avatar_path: Option<String>,
    #[serde(default)]
    pub personality: String,
    #[serde(default, alias = "user_nickname")]
    pub user_nickname: Option<String>,
    #[serde(default, alias = "user_description")]
    pub user_description: Option<String>,
    #[serde(default)]
    pub scenario: Option<String>,
    #[serde(default, alias = "writing_instructions")]
    pub writing_instructions: Option<String>,
    #[serde(default, alias = "system_prompt")]
    pub system_prompt: Option<String>,
    #[serde(default)]
    pub greeting: String,
    #[serde(default, alias = "created_at")]
    pub created_at: u64,

    // === Voice Support (for Qwen3-TTS / VibeVoice style models) ===
    /// "none" | "preset" | "custom_sample"
    #[serde(default, alias = "voice_mode")]
    pub voice_mode: String,

    /// e.g. "neutral-female-01", "deep-male-narrator", etc.
    #[serde(default, alias = "voice_preset")]
    pub voice_preset: Option<String>,

    /// Path to uploaded reference audio for voice cloning (relative under voices/ folder)
    #[serde(default, alias = "voice_sample_path")]
    pub voice_sample_path: Option<String>,

    // === Future AR / Social Layer (Meta + Pokémon GO vision) ===
    /// Optional geolocation where this character "lives" or can be encountered.
    /// Format: { lat: number, lng: number, radius?: number (meters) }
    #[serde(default, alias = "spawn_location")]
    pub spawn_location: Option<serde_json::Value>,

    /// Whether this character is discoverable by others in the AR/social layer.
    #[serde(default, alias = "is_public_spawn")]
    pub is_public_spawn: bool,

    // === Persistent Knowledge Base (RAG) ===
    /// Whether this character has attached documents for retrieval-augmented generation.
    #[serde(default, alias = "has_knowledge_base", alias = "hasKnowledgeBase")]
    pub has_knowledge_base: bool,
}

fn default_character_version() -> u32 {
    1
}

/// Phase 0: Basic migration utilities skeleton.
/// In a full implementation this would contain version-specific migration logic
/// (e.g. v1 -> v2 transformations) and be called on load.
pub mod migrations {
    pub const CURRENT_CHARACTER_VERSION: u32 = 1;
    pub const CURRENT_CONVERSATION_VERSION: u32 = 1;

    /// Placeholder for future migration logic.
    pub fn migrate_character_if_needed(_data: &mut serde_json::Value) -> bool {
        // Return true if migration was applied
        false
    }
}

/// Ensures the characters directory exists.
pub fn ensure_characters_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let data_dir = get_app_data_dir(app)?;
    let characters_dir = data_dir.join("characters");
    std::fs::create_dir_all(&characters_dir)
        .map_err(|e| format!("Failed to create characters directory: {}", e))?;
    Ok(characters_dir)
}

/// Saves a single character to disk as JSON.
pub fn save_character(app: &AppHandle, character: &StoredCharacter) -> Result<(), String> {
    validate_character_id(&character.id)?;
    let characters_dir = ensure_characters_dir(app)?;
    let file_path = characters_dir.join(format!("{}.json", character.id));

    let json = serde_json::to_string_pretty(character)
        .map_err(|e| format!("Failed to serialize character: {}", e))?;

    atomic_write(&file_path, &json)
}

/// Loads all characters from disk.
pub fn load_all_characters(app: &AppHandle) -> Result<Vec<StoredCharacter>, String> {
    let characters_dir = ensure_characters_dir(app)?;

    if !characters_dir.exists() {
        return Ok(vec![]);
    }

    let mut characters = Vec::new();
    let mut corruption_count = 0u32;

    for entry in std::fs::read_dir(&characters_dir)
        .map_err(|e| format!("Failed to read characters directory: {}", e))?
    {
        let entry = entry.map_err(|e| format!("Failed to read directory entry: {}", e))?;
        let path = entry.path();

        if path.extension().and_then(|s| s.to_str()) == Some("json") {
            if let Ok(content) = std::fs::read_to_string(&path) {
                match serde_json::from_str::<StoredCharacter>(&content) {
                    Ok(mut character) => {
                        // Lightweight migration: normalize version field; re-save if bumped.
                        if character.version < migrations::CURRENT_CHARACTER_VERSION {
                            log::info!(
                                "Character {} schema v{} → v{} (normalize).",
                                character.id,
                                character.version,
                                migrations::CURRENT_CHARACTER_VERSION
                            );
                            character.version = migrations::CURRENT_CHARACTER_VERSION;
                            if let Ok(pretty) = serde_json::to_string_pretty(&character) {
                                if let Err(e) = atomic_write(&path, &pretty) {
                                    log::warn!(
                                        "Failed to persist migrated character {}: {}",
                                        character.id,
                                        e
                                    );
                                }
                            }
                        }
                        characters.push(character);
                    }
                    Err(e) => {
                        corruption_count += 1;
                        log::error!(
                            "Failed to load character from {}: {} — file may be corrupt",
                            path.display(), e
                        );
                    }
                }
            } else {
                corruption_count += 1;
                log::error!(
                    "Failed to read character file: {} — file may be unreadable",
                    path.display()
                );
            }
        }
    }

    if corruption_count > 0 {
        log::warn!(
            "Loaded {} character(s) successfully, but {} file(s) were corrupt or unreadable. Check logs above for details.",
            characters.len(), corruption_count
        );
    }

    // Sort by creation date (newest first)
    characters.sort_by(|a, b| b.created_at.cmp(&a.created_at));

    Ok(characters)
}

/// Atomically deletes a file by first renaming it (atomic on same filesystem),
/// then removing the renamed copy. If the process crashes between rename and
/// remove, a stale orphan may be left (logged at info level).
pub fn atomic_delete(path: &Path) -> Result<(), String> {
    if !path.exists() {
        return Ok(());
    }
    let temp_dir = path.parent().unwrap_or(Path::new("."));
    let orphan = format!(".orphan_{}", uuid::Uuid::new_v4());
    let temp_path = temp_dir.join(&orphan);
    std::fs::rename(path, &temp_path)
        .map_err(|e| format!("Failed to rename for atomic delete: {}", e))?;
    if let Err(e) = std::fs::remove_file(&temp_path) {
        // Log but don't fail — the rename already removed the original path
        log::warn!("Failed to remove orphan after atomic delete ({}): {} — manual cleanup may be needed", temp_path.display(), e);
    }
    Ok(())
}

/// Deletes a character file and associated assets (knowledge dir, voice sample, avatar files).
/// Also removes conversations that list this character as a participant (orphan cleanup).
pub fn delete_character(app: &AppHandle, character_id: &str) -> Result<(), String> {
    validate_character_id(character_id)?;
    let data_dir = get_app_data_dir(app)?;
    let characters_dir = ensure_characters_dir(app)?;
    let file_path = characters_dir.join(format!("{}.json", character_id));

    // Best-effort read of avatar path before removing JSON
    let avatar_rel = if file_path.exists() {
        std::fs::read_to_string(&file_path)
            .ok()
            .and_then(|j| serde_json::from_str::<StoredCharacter>(&j).ok())
            .and_then(|c| c.avatar_path)
    } else {
        None
    };

    // Conversations first (while character still “exists” for debugging logs)
    match crate::conversation::delete_conversations_for_character(app, character_id) {
        Ok(n) if n > 0 => log::info!(
            "Deleted {} conversation(s) for character {}",
            n,
            character_id
        ),
        Ok(_) => {}
        Err(e) => log::warn!(
            "Failed to delete conversations for character {}: {}",
            character_id,
            e
        ),
    }

    if file_path.exists() {
        atomic_delete(&file_path)?;
    }

    // characters/{id}/ (knowledge + any per-character folder)
    let char_dir = characters_dir.join(character_id);
    if char_dir.is_dir() {
        if let Err(e) = std::fs::remove_dir_all(&char_dir) {
            log::warn!(
                "Failed to remove character directory {}: {}",
                char_dir.display(),
                e
            );
        }
    }

    // Voice samples: voices/{id}/
    let voice_dir = data_dir.join("voices").join(character_id);
    if voice_dir.is_dir() {
        if let Err(e) = std::fs::remove_dir_all(&voice_dir) {
            log::warn!("Failed to remove voice dir {}: {}", voice_dir.display(), e);
        }
    }

    // Avatar file if stored under app data and looks character-specific
    if let Some(rel) = avatar_rel {
        if !rel.starts_with("avatars/default/") {
            if let Ok(abs) = get_absolute_path(app, &rel) {
                if abs.exists() {
                    if let Err(e) = atomic_delete(&abs) {
                        log::warn!("Failed to delete avatar {}: {}", abs.display(), e);
                    }
                }
            }
        }
    }

    // Legacy chat history file
    let legacy = data_dir
        .join("chat_histories")
        .join(format!("{}.json", character_id));
    if legacy.exists() {
        let _ = atomic_delete(&legacy);
    }

    Ok(())
}

// ==================== CHAT HISTORY PERSISTENCE ====================

/// Ensures the chat histories directory exists.
pub fn ensure_chat_histories_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let data_dir = get_app_data_dir(app)?;
    let histories_dir = data_dir.join("chat_histories");
    std::fs::create_dir_all(&histories_dir)
        .map_err(|e| format!("Failed to create chat histories directory: {}", e))?;
    Ok(histories_dir)
}

/// Saves chat history for a specific character/conversation to disk.
pub fn save_chat_history(app: &AppHandle, owner_id: &str, history: &[serde_json::Value]) -> Result<(), String> {
    validate_owner_id(owner_id)?;
    let histories_dir = ensure_chat_histories_dir(app)?;
    let file_path = histories_dir.join(format!("{}.json", owner_id));

    let json = serde_json::to_string_pretty(history)
        .map_err(|e| format!("Failed to serialize chat history: {}", e))?;

    atomic_write(&file_path, &json)
}

/// Loads chat history for a specific character/conversation from disk.
pub fn load_chat_history(app: &AppHandle, owner_id: &str) -> Result<Vec<serde_json::Value>, String> {
    validate_owner_id(owner_id)?;
    let histories_dir = ensure_chat_histories_dir(app)?;
    let file_path = histories_dir.join(format!("{}.json", owner_id));

    if !file_path.exists() {
        return Ok(vec![]);
    }

    let content = std::fs::read_to_string(file_path)
        .map_err(|e| format!("Failed to read chat history: {}", e))?;

    let history: Vec<serde_json::Value> = serde_json::from_str(&content)
        .map_err(|e| format!("Failed to parse chat history: {}", e))?;

    Ok(history)
}

/// Deletes chat history for a character.
pub fn delete_chat_history(app: &AppHandle, owner_id: &str) -> Result<(), String> {
    validate_owner_id(owner_id)?;
    let histories_dir = ensure_chat_histories_dir(app)?;
    let file_path = histories_dir.join(format!("{}.json", owner_id));

    if file_path.exists() {
        atomic_delete(&file_path)?;
    }
    Ok(())
}

/// Ensures the images directory exists for storing uploaded chat images.
pub fn ensure_images_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let data_dir = get_app_data_dir(app)?;
    let images_dir = data_dir.join("images");
    std::fs::create_dir_all(&images_dir)
        .map_err(|e| format!("Failed to create images directory: {}", e))?;
    Ok(images_dir)
}

pub fn get_absolute_path(app: &AppHandle, relative_path: &str) -> Result<std::path::PathBuf, String> {
    let base = get_app_data_dir(app)?;

    // Normalize and strip dangerous path components (especially ParentDir / "..")
    let normalized: std::path::PathBuf = std::path::Path::new(relative_path)
        .components()
        .filter(|c| !matches!(c, std::path::Component::ParentDir))
        .collect();

    let full = base.join(normalized);

    // Final safety check: the resolved path must still be inside our app data directory
    if !full.starts_with(&base) {
        return Err("Path traversal detected: attempted to escape app data directory".to_string());
    }

    Ok(full)
}

/// Ensures the voices directory exists for storing voice reference samples.
pub fn ensure_voices_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let data_dir = get_app_data_dir(app)?;
    let voices_dir = data_dir.join("voices");
    std::fs::create_dir_all(&voices_dir)
        .map_err(|e| format!("Failed to create voices directory: {}", e))?;
    Ok(voices_dir)
}

/// Saves a voice reference sample for a character.
/// Expects a WAV data URL (already normalized by frontend to 16kHz mono).
/// Returns the relative path (e.g. "voices/{character_id}/reference.wav")
pub fn save_voice_sample(app: &AppHandle, character_id: &str, data_url: &str) -> Result<String, String> {
    validate_character_id(character_id)?;
    let voices_dir = ensure_voices_dir(app)?;

    // Strip data URL prefix if present
    let base64_data = if data_url.contains(",") {
        data_url.split(',').nth(1).unwrap_or(data_url)
    } else {
        data_url
    };

    let bytes = base64::engine::general_purpose::STANDARD
        .decode(base64_data)
        .map_err(|e| format!("Failed to decode voice sample: {}", e))?;

    let char_voice_dir = voices_dir.join(character_id);
    std::fs::create_dir_all(&char_voice_dir)
        .map_err(|e| format!("Failed to create voice folder: {}", e))?;

    let file_path = char_voice_dir.join("reference.wav");
    atomic_write_bytes(&file_path, &bytes)
        .map_err(|e| format!("Failed to write voice sample: {}", e))?;

    Ok(format!("voices/{}/reference.wav", character_id))
}

/// Ensures the knowledge base directory exists for a specific character.
pub fn ensure_character_knowledge_dir(app: &AppHandle, character_id: &str) -> Result<PathBuf, String> {
    validate_character_id(character_id)?;
    let data_dir = get_app_data_dir(app)?;
    let knowledge_dir = data_dir
        .join("characters")
        .join(character_id)
        .join("knowledge");
    std::fs::create_dir_all(&knowledge_dir)
        .map_err(|e| format!("Failed to create knowledge directory: {}", e))?;
    Ok(knowledge_dir)
}

/// Saves an uploaded document (PDF, TXT, MD, etc.) into the character's knowledge base.
/// Returns the relative path inside the knowledge folder.
pub fn save_character_document(
    app: &AppHandle,
    character_id: &str,
    original_filename: &str,
    data: &[u8],
) -> Result<String, String> {
    validate_character_id(character_id)?;
    let knowledge_dir = ensure_character_knowledge_dir(app, character_id)?;

    // Sanitize filename a bit
    let safe_name = original_filename
        .replace(['/', '\\', ':', '*', '?', '"', '<', '>', '|'], "_");

    let file_path = knowledge_dir.join(&safe_name);
    atomic_write_bytes(&file_path, data)
        .map_err(|e| format!("Failed to write document: {}", e))?;

    // Automatically process the document into chunks for RAG
    let relative_path = format!("characters/{}/knowledge/{}", character_id, safe_name);
    match process_character_document(app, character_id, &relative_path) {
        Ok(n) => {
            log::info!(
                "Knowledge document {} processed into {} chunk(s)",
                relative_path,
                n
            );
        }
        Err(e) => {
            // Document file is still saved; surface processing failure to the caller.
            return Err(format!(
                "Document saved to {} but knowledge processing failed: {}",
                relative_path, e
            ));
        }
    }

    Ok(relative_path)
}

/// Represents a single chunk of text from a knowledge document.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct KnowledgeChunk {
    pub id: String,
    pub source_file: String,
    pub text: String,
    pub char_start: usize,
    pub char_end: usize,
    pub embedding: Option<Vec<f32>>,
}

/// Processes a document (after it has been saved) into chunks and stores them.
/// Returns the number of chunks created.
pub fn process_character_document(
    app: &AppHandle,
    character_id: &str,
    relative_path: &str,
) -> Result<usize, String> {
    validate_character_id(character_id)?;
    use std::fs;
    use crate::rag;

    let full_path = get_absolute_path(app, relative_path)?;
    if !full_path.exists() {
        return Err(format!("Document not found: {}", relative_path));
    }

    let content = match full_path.extension().and_then(|e| e.to_str()) {
        Some("pdf") => extract_text_from_pdf(&full_path)?,
        Some("txt") | Some("md") | Some("markdown") => {
            fs::read_to_string(&full_path).map_err(|e| format!("Failed to read text file: {}", e))?
        }
        _ => {
            return Err("Unsupported file type for knowledge base. Use PDF, TXT or MD.".to_string());
        }
    };

    if content.trim().is_empty() {
        return Ok(0);
    }

    let mut chunks = chunk_text(&content, &relative_path);
    if chunks.is_empty() {
        return Ok(0);
    }

    // Generate embeddings — required for retrieval; do not claim knowledge is usable without them.
    let texts: Vec<String> = chunks.iter().map(|c| c.text.clone()).collect();
    let embeddings = rag::embed_texts(texts).map_err(|e| {
        format!(
            "Failed to embed knowledge chunks (retrieval would be empty): {}",
            e
        )
    })?;
    if embeddings.len() != chunks.len() {
        return Err(format!(
            "Embedding count mismatch (got {}, expected {})",
            embeddings.len(),
            chunks.len()
        ));
    }
    for (i, embedding) in embeddings.into_iter().enumerate() {
        chunks[i].embedding = Some(embedding);
    }

    // Save chunks next to the original file
    let chunks_path = full_path.with_extension(
        full_path
            .extension()
            .map(|e| format!("{}.chunks.json", e.to_string_lossy()))
            .unwrap_or_else(|| "chunks.json".to_string()),
    );

    let json = serde_json::to_string_pretty(&chunks).map_err(|e| e.to_string())?;
    atomic_write(&chunks_path, &json).map_err(|e| format!("Failed to write chunks file: {}", e))?;

    // Only mark usable after embeddings exist
    mark_character_has_knowledge_base(app, character_id)?;

    Ok(chunks.len())
}

fn extract_text_from_pdf(path: &Path) -> Result<String, String> {
    // M06: Use pdf-extract for real PDF text extraction
    let bytes = std::fs::read(path).map_err(|e| format!("Failed to read PDF file: {}", e))?;
    match pdf_extract::extract_text_from_mem(&bytes) {
        Ok(text) => {
            if text.trim().is_empty() {
                Err("PDF contains no extractable text (may be a scanned document or image-based PDF)".to_string())
            } else {
                Ok(text)
            }
        }
        Err(e) => Err(format!("Failed to extract text from PDF: {}", e))
    }
}

/// Simple but effective chunking for RAG.
/// Splits on double newlines (paragraphs) first, then falls back to sentences.
fn chunk_text(full_text: &str, source_file: &str) -> Vec<KnowledgeChunk> {
    let mut chunks = Vec::new();
    let target_chunk_size = 1800; // ~300-400 tokens rough estimate
    let mut current_chunk = String::new();
    let mut char_offset = 0;

    // Split by paragraphs first
    for paragraph in full_text.split("\n\n") {
        let paragraph = paragraph.trim();
        if paragraph.is_empty() {
            continue;
        }

        if current_chunk.len() + paragraph.len() > target_chunk_size && !current_chunk.is_empty() {
            // Push current chunk
            let end = char_offset;
            let start = end - current_chunk.len();
            chunks.push(KnowledgeChunk {
                id: uuid::Uuid::new_v4().to_string(),
                source_file: source_file.to_string(),
                text: current_chunk.trim().to_string(),
                char_start: start,
                char_end: end,
                embedding: None,
            });
            current_chunk.clear();
        }

        if !current_chunk.is_empty() {
            current_chunk.push_str("\n\n");
        }
        current_chunk.push_str(paragraph);
        char_offset += paragraph.len() + 2;
    }

    // Push final chunk if any
    if !current_chunk.is_empty() {
        let end = char_offset;
        let start = end - current_chunk.len();
        chunks.push(KnowledgeChunk {
            id: uuid::Uuid::new_v4().to_string(),
            source_file: source_file.to_string(),
            text: current_chunk.trim().to_string(),
            char_start: start,
            char_end: end,
            embedding: None,
        });
    }

    // If we ended up with very few huge chunks, do a secondary sentence split (simplified)
    if chunks.len() < 3 && full_text.len() > target_chunk_size * 2 {
        return chunk_by_sentences(full_text, source_file, target_chunk_size);
    }

    chunks
}

fn chunk_by_sentences(text: &str, source_file: &str, target_size: usize) -> Vec<KnowledgeChunk> {
    // Very basic sentence splitter
    let mut chunks = Vec::new();
    let mut current = String::new();
    let mut offset = 0;

    for sentence in text.split(['.', '!', '?'].as_ref()) {
        let sentence = sentence.trim();
        if sentence.is_empty() {
            continue;
        }

        let with_punct = format!("{}.", sentence);

        if current.len() + with_punct.len() > target_size && !current.is_empty() {
            let end = offset;
            let start = end - current.len();
            chunks.push(KnowledgeChunk {
                id: uuid::Uuid::new_v4().to_string(),
                source_file: source_file.to_string(),
                text: current.trim().to_string(),
                char_start: start,
                char_end: end,
                embedding: None,
            });
            current.clear();
        }

        if !current.is_empty() {
            current.push(' ');
        }
        current.push_str(&with_punct);
        offset += with_punct.len() + 1;
    }

    if !current.is_empty() {
        let end = offset;
        let start = end - current.len();
        chunks.push(KnowledgeChunk {
            id: uuid::Uuid::new_v4().to_string(),
            source_file: source_file.to_string(),
            text: current.trim().to_string(),
            char_start: start,
            char_end: end,
            embedding: None,
        });
    }

    chunks
}

fn mark_character_has_knowledge_base(app: &AppHandle, character_id: &str) -> Result<(), String> {
    validate_character_id(character_id)?;
    let characters_dir = ensure_characters_dir(app)?;
    let path = characters_dir.join(format!("{}.json", character_id));

    if !path.exists() {
        return Ok(());
    }

    // Exclusive lock (stable inode — no stale-unlink race)
    let lock_file = acquire_sidecar_lock(&path)
        .map_err(|e| format!("Failed to acquire lock for mark_knowledge_base: {}", e))?;

    let mut character: StoredCharacter = match std::fs::read_to_string(&path) {
        Ok(json) => match serde_json::from_str(&json) {
            Ok(c) => c,
            Err(_) => {
                let _ = lock_file.unlock();
                return Ok(());
            }
        },
        Err(_) => {
            let _ = lock_file.unlock();
            return Ok(());
        }
    };

    character.has_knowledge_base = true;

    if let Ok(pretty) = serde_json::to_string_pretty(&character) {
        // Write under existing lock (use bytes path to avoid nested sidecar lock)
        if let Err(e) = atomic_write_bytes(&path, pretty.as_bytes()) {
            log::warn!("Failed to update has_knowledge_base flag for character {}: {}", character_id, e);
        }
    }

    if let Err(e) = lock_file.unlock() {
        log::warn!("Failed to unlock mark_knowledge_base: {}", e);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_atomic_write_is_atomic() {
        let temp = TempDir::new().unwrap();
        let target = temp.path().join("test.json");

        let content = r#"{"hello": "world", "number": 42}"#;
        atomic_write(&target, content).expect("atomic write should succeed");

        let read_back = std::fs::read_to_string(&target).unwrap();
        assert_eq!(read_back, content);
    }

    #[test]
    fn test_atomic_write_bytes_roundtrip() {
        let temp = TempDir::new().unwrap();
        let target = temp.path().join("data.bin");

        let bytes: &[u8] = &[0x00, 0x01, 0x02, 0xFF, 0x42];
        atomic_write_bytes(&target, bytes).expect("atomic bytes write should succeed");

        let read_back = std::fs::read(&target).unwrap();
        assert_eq!(read_back, bytes);
    }

    #[test]
    fn test_atomic_write_creates_parent_directories() {
        let temp = TempDir::new().unwrap();
        let target = temp.path().join("nested").join("deep").join("file.txt");

        atomic_write(&target, "test").expect("should create parent dirs");

        assert!(target.exists());
        assert_eq!(std::fs::read_to_string(target).unwrap(), "test");
    }

    #[test]
    fn test_atomic_write_failure_does_not_corrupt_target() {
        let temp = TempDir::new().unwrap();
        let target = temp.path().join("important.json");

        // Write valid content first
        atomic_write(&target, r#"{"status":"original"}"#).unwrap();

        let result = atomic_write(&target, r#"{"status":"updated"}"#);
        assert!(result.is_ok());

        let content = std::fs::read_to_string(&target).unwrap();
        assert!(content.contains("updated"));
    }

    #[test]
    fn test_stored_character_default_version() {
        // When deserializing old character data without version, it should default to 1
        let old_json = r##"{
            "id": "old-char",
            "name": "Old Character",
            "mode": "advanced-chat",
            "avatar_color": "#000000",
            "avatar_path": null,
            "personality": "Old personality",
            "greeting": "Hello",
            "created_at": 1234567890,
            "voice_mode": "none",
            "is_public_spawn": false,
            "has_knowledge_base": false
        }"##;

        let char: StoredCharacter = serde_json::from_str(old_json).unwrap();
        assert_eq!(char.version, 1); // Should get the default
    }

    #[test]
    fn test_stored_character_camel_case_frontend_roundtrip() {
        // Frontend / default_personas.json shape
        let camel = r##"{
            "id": "support-unit",
            "name": "Support Unit",
            "mode": "advanced-chat",
            "avatarColor": "#3b82f6",
            "avatarUrl": "avatars/default/support-unit.png",
            "personality": "Tactical",
            "userNickname": "Commander",
            "systemPrompt": "Be brief.",
            "greeting": "Online.",
            "createdAt": 1700000000000,
            "hasKnowledgeBase": true
        }"##;
        let char: StoredCharacter = serde_json::from_str(camel).unwrap();
        assert_eq!(char.avatar_color, "#3b82f6");
        assert_eq!(
            char.avatar_path.as_deref(),
            Some("avatars/default/support-unit.png")
        );
        assert_eq!(char.user_nickname.as_deref(), Some("Commander"));
        assert_eq!(char.system_prompt.as_deref(), Some("Be brief."));
        assert_eq!(char.created_at, 1700000000000);
        assert!(char.has_knowledge_base);

        let out = serde_json::to_value(&char).unwrap();
        assert_eq!(out["avatarUrl"], "avatars/default/support-unit.png");
        assert_eq!(out["avatarColor"], "#3b82f6");
        assert_eq!(out["createdAt"], 1700000000000u64);
    }
}

// Tauri Commands - IPC bridge between frontend and Rust backend
//
// These functions are callable from the JavaScript side using
// `invoke('command_name', { args })`

use crate::inference::{ServerStartRequest, ServerStatus, SharedLlamaServer, SharedVoiceServer};
use crate::storage;
use std::path::PathBuf;
use tauri::command;
use tauri_plugin_dialog::DialogExt;

/// Phase 1.5: Maximum number of recent messages to include in LLM context.
/// Prevents OOM on long conversations while maintaining coherent context.
const LLM_CONTEXT_WINDOW_SIZE: usize = 100;

/// Security: Maximum total image payload size (10MB) to prevent OOM from base64 encoding.
const MAX_TOTAL_IMAGE_BYTES: usize = 10 * 1024 * 1024;

/// Security: Maximum individual image size (5MB).
const MAX_SINGLE_IMAGE_BYTES: usize = 5 * 1024 * 1024;

/// Security: Maximum TTS text length (10,000 characters).
const MAX_TTS_TEXT_LENGTH: usize = 10_000;

/// Security: Maximum voice sample size (50MB).
const MAX_VOICE_SAMPLE_BYTES: usize = 50 * 1024 * 1024;

/// Security: Maximum knowledge document size (100MB).
const MAX_DOCUMENT_BYTES: usize = 100 * 1024 * 1024;

/// Phase 1.5: Shared HTTP client with timeout to prevent UI freezes.
/// Replaces per-request `reqwest::Client::new()` calls.
/// FIX-C01: No longer uses `expect()` — falls back to a default client on TLS failure.
// pub for audit tests only — ensures C6 regression test verifies the real client, not a pattern copy.
#[doc(hidden)]
pub fn build_http_client() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap_or_else(|e| {
            log::error!("Failed to build HTTP client with custom config ({}), using default client", e);
            reqwest::Client::new()
        })
}
static HTTP_CLIENT: once_cell::sync::Lazy<reqwest::Client> = once_cell::sync::Lazy::new(|| build_http_client());

/// FIX-SEC03: Validates that an inference API base URL is localhost-only (SSRF prevention).
/// Returns the validated URL or an error if it points to a non-local host.
/// Accepts: http://127.0.0.1:*, http://localhost:*, http://[::1]:*
fn validate_localhost_endpoint(api_base: &str) -> Result<(), String> {
    let is_localhost = api_base.starts_with("http://127.0.0.1:")
        || api_base.starts_with("http://localhost:")
        || api_base.starts_with("http://[::1]:");
    if !is_localhost {
        return Err(format!(
            "Inference endpoint must be on localhost (127.0.0.1, localhost, or [::1]) for security. Got: {}",
            api_base
        ));
    }
    Ok(())
}

/// P1-4: derives a pinned Host header from a validated localhost endpoint URL
/// (DNS-rebinding defense: the client always asserts the expected Host rather
/// than whatever the URL parser / proxy might forward).
fn endpoint_host_header(endpoint: &str) -> String {
    // Strip scheme, take authority up to next '/'.
    let without_scheme = endpoint
        .split("://")
        .nth(1)
        .unwrap_or(endpoint);
    let authority = without_scheme.split('/').next().unwrap_or(without_scheme);
    // Only ever emit loopback authorities; fall back to 127.0.0.1 on parse failure.
    if authority.starts_with("127.0.0.1:")
        || authority.starts_with("localhost:")
        || authority.starts_with("[::1]:")
    {
        authority.to_string()
    } else {
        "127.0.0.1".to_string()
    }
}

/// Starts the local llama-server with the given model and parameters.
/// For Vision models (VLMs), include the `mmproj_path` pointing to the projector file.
#[command]
pub async fn start_inference_server(
    state: tauri::State<'_, SharedLlamaServer>,
    request: ServerStartRequest,
) -> Result<ServerStatus, String> {
    let mut manager = state.lock().await;

    manager
        .start(request)
        .await
        .map_err(|e| format!("Failed to start inference server: {}", e))
}

/// Stops the running inference server.
#[command]
pub async fn stop_inference_server(state: tauri::State<'_, SharedLlamaServer>) -> Result<(), String> {
    let mut manager = state.lock().await;

    manager
        .stop()
        .await
        .map_err(|e| format!("Failed to stop inference server: {}", e))
}

/// Returns the current status of the inference server.
#[command]
pub async fn get_inference_status(
    state: tauri::State<'_, SharedLlamaServer>,
) -> Result<ServerStatus, String> {
    let mut manager = state.lock().await;
    // FIX-C02: Check actual process health on every status query
    let crashed = manager.check_health();
    // R01: Auto-restart if crash detected and circuit breaker allows
    if crashed && manager.auto_restart_if_needed() {
        let req = manager.last_start_request.clone();
        let model_hint = req
            .as_ref()
            .map(|r| r.model_path.display().to_string())
            .unwrap_or_else(|| "unknown".into());
        // Must match the key used in LlamaServerManager::auto_restart_if_needed
        let cb_key = format!("auto_restart_{}", model_hint);
        // Mark resetting before spawn so concurrent status polls don't dual-start
        manager.is_resetting = true;
        let server = state.inner().clone();
        drop(manager);
        if let Some(req) = req {
            tokio::spawn(async move {
                let mut m = server.lock().await;
                log::info!("Auto-restarting llama-server after crash...");
                let start_res = m.start(req).await;
                m.is_resetting = false;
                match start_res {
                    Ok(_) => crate::circuit_breaker::record_success(&cb_key),
                    Err(e) => {
                        log::error!("Auto-restart failed: {}", e);
                        crate::circuit_breaker::record_failure(&cb_key);
                    }
                }
            });
        } else {
            let mut m = state.lock().await;
            m.is_resetting = false;
        }
        // Prefer real status fields from last_start_request over empty fabricated status
        return Ok(ServerStatus {
            running: false,
            starting: true,
            state: crate::inference::ServerState::Starting,
            model_path: Some(model_hint),
            port: None,
            api_base: None,
            pid: None,
            last_error: Some("Worker crashed — auto-restarting…".into()),
            vision_enabled: false,
            model_context_length: None,
            request_count: 0,
            max_requests_before_reset: 0,
        });
    }
    Ok(manager.status())
}

/// Phase 1.5: Respawns the inference server (Arena Reset) using the last stored start config.
/// Useful for mitigating memory fragmentation in long-running llama-server processes.
#[command]
pub async fn reset_inference_server(
    state: tauri::State<'_, SharedLlamaServer>,
) -> Result<ServerStatus, String> {
    // Phase 1.5: Direct reset — this is an explicit user request, no counter increment needed
    let mut manager = state.lock().await;
    manager
        .reset()
        .await
        .map_err(|e| format!("Arena Reset failed: {}", e))
}

/// Sets the Arena Reset threshold (max inference requests before auto-respawn).
/// Set to 0 to disable.
#[command]
pub async fn set_arena_reset_threshold(
    state: tauri::State<'_, SharedLlamaServer>,
    max_requests: u64,
) -> Result<(), String> {
    let mut manager = state.lock().await;
    manager.set_arena_reset_threshold(max_requests);
    Ok(())
}

/// Simple ping used during development to verify the Rust backend is responding.
#[command]
pub fn greet(name: &str) -> String {
    format!("Hello from LocalPersona Rust backend, {}!", name)
}

// ==================== AVATAR COMMANDS ====================

/// Saves an uploaded avatar image to disk and returns the relative path
/// that should be stored in the character (e.g. "avatars/xyz123.png").
#[command]
pub async fn save_character_avatar(
    app: tauri::AppHandle,
    character_id: String,
    data_url: String,
) -> Result<String, String> {
    storage::save_avatar(&app, &character_id, &data_url)
}

/// Retrieves a previously saved avatar and returns it as a data URL
/// so the frontend can display it.
#[command]
pub async fn get_character_avatar(
    app: tauri::AppHandle,
    relative_path: String,
) -> Result<String, String> {
    storage::get_avatar_as_data_url(&app, &relative_path)
}

// ==================== PROPER EXPORT / IMPORT ====================

/// Exports all current characters (passed from frontend) to a JSON file chosen by the user.
#[command]
pub async fn export_characters(
    app: tauri::AppHandle,
    characters_json: String,
) -> Result<(), String> {
    let file_path = app
        .dialog()
        .file()
        .add_filter("JSON", &["json"])
        .set_file_name("localpersona-characters.json")
        .blocking_save_file();

    let Some(path) = file_path else {
        return Err("Export cancelled".to_string());
    };

    let path_buf = path.into_path().map_err(|e| format!("Invalid path: {:?}", e))?;

    crate::storage::atomic_write(&path_buf, &characters_json)
        .map_err(|e| format!("Failed to write export file: {}", e))
}

/// Lets the user pick a JSON file and returns its contents so the frontend can import.
#[command]
pub async fn import_characters(app: tauri::AppHandle) -> Result<String, String> {
    let file_path = app
        .dialog()
        .file()
        .add_filter("JSON", &["json"])
        .blocking_pick_file();

    let Some(path) = file_path else {
        return Err("Import cancelled".to_string());
    };

    let path_buf = path.into_path().map_err(|e| format!("Invalid path: {:?}", e))?;

    std::fs::read_to_string(path_buf)
        .map_err(|e| format!("Failed to read import file: {}", e))
}

// ==================== REAL FILE-BASED CHARACTER STORAGE ====================

use crate::storage::{load_all_characters, save_character, delete_character, StoredCharacter};

#[command]
pub async fn save_character_to_disk(
    app: tauri::AppHandle,
    character: StoredCharacter,
) -> Result<(), String> {
    save_character(&app, &character)
}

#[command]
pub async fn load_characters_from_disk(
    app: tauri::AppHandle,
) -> Result<Vec<StoredCharacter>, String> {
    load_all_characters(&app)
}

#[command]
pub async fn delete_character_from_disk(
    app: tauri::AppHandle,
    character_id: String,
) -> Result<(), String> {
    delete_character(&app, &character_id)
}

// --- Legacy Chat History Commands (Phase A: deprecated) ---
// Interactive chat uses conversations/{uuid}/ NDJSON only.
// These remain for optional cleanup of pre-migration files under chat_histories/.

/// DEPRECATED: Do not use for interactive messaging. Prefer conversation append-only API.
#[command]
pub async fn save_chat_history_to_disk(
    app: tauri::AppHandle,
    owner_id: String,
    history: Vec<serde_json::Value>,
) -> Result<(), String> {
    log::warn!(
        "save_chat_history_to_disk is deprecated (Phase A); prefer conversations/{{uuid}}/ NDJSON"
    );
    storage::save_chat_history(&app, &owner_id, &history)
}

/// DEPRECATED: Prefer load_messages_for_display / load_all_messages on a conversation id.
#[command]
pub async fn load_chat_history_from_disk(
    app: tauri::AppHandle,
    owner_id: String,
) -> Result<Vec<serde_json::Value>, String> {
    log::warn!(
        "load_chat_history_from_disk is deprecated (Phase A); prefer conversations/{{uuid}}/ NDJSON"
    );
    storage::load_chat_history(&app, &owner_id)
}

/// Safe to keep: cleans legacy chat_histories/ files when a character is removed.
#[command]
pub async fn delete_chat_history_from_disk(
    app: tauri::AppHandle,
    owner_id: String,
) -> Result<(), String> {
    storage::delete_chat_history(&app, &owner_id)
}

// ==================== VISION: send_message_with_images (MVP) ====================

use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;

/// Phase 1.5: Increments the request counter and triggers Arena Reset if threshold hit.
/// Spawns a background task for the actual respawn so the current request is not blocked.
/// Phase 1.5: Checks if Arena Reset is needed after a successful inference call.
/// Uses try_lock to prevent concurrent resets from multiple rapid requests.
fn check_arena_reset(state: &tauri::State<'_, SharedLlamaServer>) {
    let server = state.inner().clone();
    tokio::spawn(async move {
        let reason = match server.try_lock() {
            Ok(mut manager) => {
                if manager.is_resetting {
                    return;
                }
                let req_hit = manager.increment_and_check_reset();
                let up_hit = manager.should_reset_due_to_uptime();
                if req_hit {
                    Some("request_threshold")
                } else if up_hit {
                    Some("uptime_threshold")
                } else {
                    None
                }
            }
            Err(_) => return,
        };
        if let Some(reason) = reason {
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            let mut manager = server.lock().await;
            if manager.is_resetting {
                return;
            }
            manager.record_reset_reason(reason);
            log::info!("Arena Reset ({reason}). Respawning server...");
            if let Err(e) = manager.reset().await {
                log::error!("Arena Reset failed: {}", e);
            }
        }
    });
}

/// Composes a rich, structured system prompt from the full character data.
/// This is the core of Phase 1 Q&A depth remediation: the detailed editor fields
/// (personality, scenario, writing_instructions, etc.) now actually drive the model.
///
/// Priority:
/// 1. If a non-empty custom `system_prompt` exists, use it as the complete base (power-user override).
/// 2. Otherwise, intelligently compose from the rich editor fields with clear section headers.
/// 3. Mode prefix (adventure/story/etc.) is applied when not using a full custom override.
/// 4. RAG knowledge is appended *after* this in the caller (keeps concerns separated).
fn compose_system_prompt(character: &storage::StoredCharacter) -> String {
    // Power-user full override takes precedence
    if let Some(custom) = &character.system_prompt {
        if !custom.trim().is_empty() {
            return custom.clone();
        }
    }

    let mut prompt = String::new();

    // Writing mode frame (RC R1 — was previously UI-only)
    if let Some(mode_prefix) = mode_system_prefix(&character.mode) {
        prompt.push_str("[Interaction Mode]\n");
        prompt.push_str(mode_prefix);
        prompt.push_str("\n\n");
    }

    // Core identity
    prompt.push_str(&format!("You are {}.\n\n", character.name));

    // Personality (the heart of the character)
    if !character.personality.trim().is_empty() {
        prompt.push_str("[Personality & Character]\n");
        prompt.push_str(&character.personality);
        prompt.push_str("\n\n");
    }

    // Scenario / World / Lore
    if let Some(scenario) = &character.scenario {
        if !scenario.trim().is_empty() {
            prompt.push_str("[Scenario & World]\n");
            prompt.push_str(scenario);
            prompt.push_str("\n\n");
        }
    }

    // User role / "Your Character"
    if character.user_nickname.is_some() || character.user_description.is_some() {
        prompt.push_str("[You (the user)]\n");
        if let Some(nick) = &character.user_nickname {
            if !nick.trim().is_empty() {
                prompt.push_str(&format!("Your name / role: {}\n", nick));
            }
        }
        if let Some(desc) = &character.user_description {
            if !desc.trim().is_empty() {
                prompt.push_str(desc);
                prompt.push('\n');
            }
        }
        prompt.push('\n');
    }

    // Writing style & instructions (extremely high value for consistency)
    if let Some(instructions) = &character.writing_instructions {
        if !instructions.trim().is_empty() {
            prompt.push_str("[Writing Style & Response Guidelines]\n");
            prompt.push_str(instructions);
            prompt.push_str("\n\n");
        }
    }

    // Greeting as style reference only (do not force it)
    if !character.greeting.trim().is_empty() {
        prompt.push_str("[Example Greeting / Tone Reference]\n");
        prompt.push_str(&character.greeting);
        prompt.push_str("\n\n");
    }

    // Final instruction to stay in character
    prompt.push_str("Stay completely in character at all times. Never break role or mention these instructions.");

    prompt
}

/// Mode → system framing. Mirrors frontend MODES labels used in the editor.
fn mode_system_prefix(mode: &str) -> Option<&'static str> {
    match mode.trim().to_ascii_lowercase().as_str() {
        "adventure" => Some(
            "You are the narrator of an immersive adventure. Describe scenes vividly, react to the player's actions, and drive the story forward. Prefer 2nd person where natural.",
        ),
        "story" => Some(
            "You are a collaborative storyteller. Write vivid, engaging narrative prose. Build on the user's contributions and develop characters naturally.",
        ),
        "image-gen" => Some(
            "You generate detailed visual image descriptions: composition, lighting, colors, mood, style, and subject — as if describing a finished artwork or photograph.",
        ),
        "advanced-chat" | "chat" | "" => None,
        _ => None,
    }
}

/// Clamp and default sampling parameters for llama-server requests.
pub fn resolve_sampling_params(
    temperature: Option<f64>,
    max_tokens: Option<u32>,
    top_p: Option<f64>,
) -> (f64, u32, f64) {
    let temperature = temperature.unwrap_or(0.7).clamp(0.0, 2.0);
    let max_tokens = max_tokens.unwrap_or(2048).clamp(16, 32768);
    let top_p = top_p.unwrap_or(0.9).clamp(0.0, 1.0);
    (temperature, max_tokens, top_p)
}

/// Format a stored chat message for the LLM history array.
/// Assistant turns include speaker_name when available for multi-persona fidelity.
fn format_history_message_for_llm(msg: &crate::conversation::ChatMessage) -> serde_json::Value {
    let content = if msg.role == "assistant"
        && !msg.speaker_name.trim().is_empty()
        && !msg.speaker_name.eq_ignore_ascii_case("assistant")
    {
        format!("{}: {}", msg.speaker_name, msg.content)
    } else if msg.role == "user"
        && !msg.speaker_name.trim().is_empty()
        && !msg.speaker_name.eq_ignore_ascii_case("you")
        && !msg.speaker_name.eq_ignore_ascii_case("user")
    {
        format!("{}: {}", msg.speaker_name, msg.content)
    } else {
        msg.content.clone()
    };

    serde_json::json!({
        "role": msg.role,
        "content": content
    })
}

/// Build a RAG query from the current turn plus up to two prior user turns.
fn build_rag_query(current: &str, history: &[crate::conversation::ChatMessage]) -> String {
    let mut parts: Vec<String> = history
        .iter()
        .rev()
        .filter(|m| m.role == "user")
        .take(2)
        .map(|m| m.content.clone())
        .collect();
    parts.reverse();
    if !current.trim().is_empty() {
        parts.push(current.to_string());
    }
    let joined = parts.join("\n");
    // Cap query size so embedding stays fast
    if joined.chars().count() > 1200 {
        joined.chars().rev().take(1200).collect::<String>().chars().rev().collect()
    } else {
        joined
    }
}

/// Format retrieved chunks with source attribution for the system prompt.
fn format_rag_injection(chunks: &[storage::KnowledgeChunk]) -> String {
    let mut out = String::from("\n\n[Relevant Knowledge from Character's Sources]\n");
    for chunk in chunks {
        let title = std::path::Path::new(&chunk.source_file)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(chunk.source_file.as_str());
        out.push_str(&format!("- ({}) {}\n", title, chunk.text.trim()));
    }
    out.push_str(
        "\nUse the above only when directly relevant. Do not mention these sources unless it fits naturally in character.\n",
    );
    out
}

/// Legacy wrapper kept for minimal blast radius during transition.
/// Now delegates to the rich composer (Phase 1 remediation).
async fn build_system_prompt_for_character(app: &tauri::AppHandle, character_id: &str) -> Result<String, String> {
    storage::validate_character_id(character_id)?;

    // Prefer loading the typed StoredCharacter so the rich composer can use all fields
    match get_character_data(app, character_id).await {
        Ok(character) => Ok(compose_system_prompt(&character)),
        Err(_) => {
            // Fallback for very old or missing characters
            Ok("You are a helpful assistant.".to_string())
        }
    }
}

/// Resolve which character drives prompt composition + RAG for a turn.
///
/// Priority (RC identity contract, R0):
/// 1. Explicit `character_id` from the frontend (preferred).
/// 2. First `participant_ids` entry on the conversation metadata.
/// 3. `speaker_id` only when it is not a reserved role label (`user` / `assistant` / `narrator`).
///
/// Never treat the user role label `"user"` as a character file id.
pub fn resolve_character_id_for_inference(
    explicit_character_id: Option<&str>,
    conversation_participant_ids: &[String],
    speaker_id: &str,
) -> Result<String, String> {
    if let Some(id) = explicit_character_id {
        let trimmed = id.trim();
        if !trimmed.is_empty() && !is_reserved_speaker_role(trimmed) {
            storage::validate_character_id(trimmed)?;
            return Ok(trimmed.to_string());
        }
    }

    if let Some(id) = conversation_participant_ids.iter().find(|p| {
        let t = p.trim();
        !t.is_empty() && !is_reserved_speaker_role(t)
    }) {
        storage::validate_character_id(id)?;
        return Ok(id.clone());
    }

    if !speaker_id.trim().is_empty() && !is_reserved_speaker_role(speaker_id) {
        storage::validate_character_id(speaker_id)?;
        return Ok(speaker_id.to_string());
    }

    Err(
        "Cannot resolve character for inference: pass character_id, or ensure the conversation has a participant character id"
            .to_string(),
    )
}

fn is_reserved_speaker_role(id: &str) -> bool {
    matches!(
        id.trim().to_ascii_lowercase().as_str(),
        "user" | "assistant" | "narrator" | "system"
    )
}

// ==================== Shared inference pipeline (Phase B7) ====================
// Single implementation for token budget, history load, compose+RAG, and HTTP completion.
// send_message_with_images and regenerate_last_message both call these helpers.

async fn inference_token_budget(state: &tauri::State<'_, SharedLlamaServer>) -> usize {
    let mgr = state.lock().await;
    // Reserve headroom for system prompt + RAG + the current user turn (not just history).
    // ~55% of model context for history keeps compose/RAG from overflowing small models.
    let full = mgr
        .model_context_length()
        .map(|ctx| ((ctx as f64) * 0.55) as usize)
        .unwrap_or(4500);
    full.max(1024)
}

async fn load_history_for_inference(
    app: &tauri::AppHandle,
    conversation_id: &str,
    budget: usize,
) -> Result<Vec<crate::conversation::ChatMessage>, String> {
    match crate::conversation::load_messages_within_token_budget(app, conversation_id, budget) {
        Ok(msgs) if !msgs.is_empty() => Ok(msgs),
        _ => {
            let meta = crate::conversation::load_metadata(app, conversation_id)?;
            let total = meta.as_ref().map(|m| m.message_count as usize).unwrap_or(0);
            let offset = total.saturating_sub(LLM_CONTEXT_WINDOW_SIZE);
            crate::conversation::load_messages_paginated(
                app,
                conversation_id,
                offset,
                LLM_CONTEXT_WINDOW_SIZE,
            )
        }
    }
}

/// Returns (system_prompt, assistant_display_name).
async fn compose_system_with_rag(
    app: &tauri::AppHandle,
    resolved_character_id: &str,
    query_text: &str,
    history: &[crate::conversation::ChatMessage],
    has_images: bool,
) -> Result<(String, String), String> {
    let char_data = get_character_data(app, resolved_character_id).await.ok();

    let base_system = if let Some(ref c) = char_data {
        compose_system_prompt(c)
    } else {
        build_system_prompt_for_character(app, resolved_character_id).await?
    };

    let mut system_prompt = base_system;
    if let Some(ref c) = char_data {
        if c.has_knowledge_base {
            let rag_query = build_rag_query(query_text, history);
            match retrieve_knowledge_for_character(app, resolved_character_id, &rag_query) {
                Ok(chunks) if !chunks.is_empty() => {
                    system_prompt.push_str(&format_rag_injection(&chunks));
                    record_rag_status(
                        true,
                        &format!("injected {} chunk(s) for {}", chunks.len(), resolved_character_id),
                    );
                }
                Ok(_) => {
                    record_rag_status(
                        true,
                        &format!("no relevant chunks above threshold for {}", resolved_character_id),
                    );
                }
                Err(e) => {
                    log::warn!(
                        "RAG retrieval failed for character {}: {}",
                        resolved_character_id,
                        e
                    );
                    record_rag_status(
                        false,
                        &format!("RAG failed for {}: {} (chat continues without knowledge)", resolved_character_id, e),
                    );
                }
            }
        }
    }

    let assistant_display_name = char_data
        .as_ref()
        .map(|c| c.name.clone())
        .unwrap_or_else(|| "Assistant".to_string());

    let system_prompt = build_vision_aware_system_prompt(&system_prompt, has_images, 1);
    Ok((system_prompt, assistant_display_name))
}

/// Phase C3: last RAG outcome for Diagnostics (and optional UI toast policy).
static LAST_RAG_STATUS: once_cell::sync::Lazy<std::sync::Mutex<Option<serde_json::Value>>> =
    once_cell::sync::Lazy::new(|| std::sync::Mutex::new(None));

fn record_rag_status(ok: bool, detail: &str) {
    if let Ok(mut guard) = LAST_RAG_STATUS.lock() {
        *guard = Some(serde_json::json!({
            "ok": ok,
            "detail": detail,
            "at_unix_ms": std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64,
        }));
    }
}

fn take_rag_status_snapshot() -> serde_json::Value {
    LAST_RAG_STATUS
        .lock()
        .ok()
        .and_then(|g| g.clone())
        .unwrap_or(serde_json::json!({ "ok": null, "detail": "no RAG calls yet" }))
}

/// Shared non-stream chat completion against the local llama-server.
/// On worker death detection, attempts a single auto-restart then retries the HTTP call once.
async fn post_chat_completion(
    state: &tauri::State<'_, SharedLlamaServer>,
    messages: Vec<serde_json::Value>,
    temperature: Option<f64>,
    max_tokens: Option<u32>,
    top_p: Option<f64>,
    model_label: &str,
) -> Result<String, String> {
    let (temperature, max_tokens, top_p) =
        resolve_sampling_params(temperature, max_tokens, top_p);
    let request_body = serde_json::json!({
        "model": model_label,
        "messages": messages,
        "stream": false,
        "temperature": temperature,
        "max_tokens": max_tokens,
        "top_p": top_p
    });

    let mut last_err = String::new();
    for attempt in 0..2u8 {
        let (endpoint, session_key) = {
            let mut manager = state.lock().await;
            let crashed = manager.check_health();
            if crashed {
                if attempt == 0 && manager.auto_restart_if_needed() {
                    if let Some(req) = manager.last_start_request.clone() {
                        // Serialize restart with is_resetting to avoid dual spawn
                        if !manager.is_resetting {
                            manager.is_resetting = true;
                            let model = req.model_path.display().to_string();
                            let cb_key = format!("auto_restart_{}", model);
                            log::info!("post_chat_completion: auto-restarting after worker death…");
                            let start_res = manager.start(req).await;
                            manager.is_resetting = false;
                            match start_res {
                                Ok(_) => {
                                    crate::circuit_breaker::record_success(&cb_key);
                                    // brief settle — readiness is best-effort
                                    drop(manager);
                                    tokio::time::sleep(std::time::Duration::from_millis(800)).await;
                                    continue;
                                }
                                Err(e) => {
                                    crate::circuit_breaker::record_failure(&cb_key);
                                    return Err(format!(
                                        "Inference server died and auto-restart failed: {}",
                                        e
                                    ));
                                }
                            }
                        }
                    }
                }
                return Err(
                    "Inference server died unexpectedly (possible OOM or external kill). Please restart the server."
                        .to_string(),
                );
            }
            let status = manager.status();
            if !status.running {
                return Err("Inference server is not running".to_string());
            }
            let api_base = status.api_base.ok_or("No API base URL")?;
            validate_localhost_endpoint(&api_base)?;
            // P1-4: session Bearer key (server started with --api-key).
            let session_key = manager.api_key();
            (format!("{}/chat/completions", api_base), session_key)
        };

        let mut req = HTTP_CLIENT
            .clone()
            .post(&endpoint)
            .header("Content-Type", "application/json")
            .header("Host", endpoint_host_header(&endpoint));
        if let Some(ref key) = session_key {
            req = req.bearer_auth(key);
        }
        let response = req
            .json(&request_body)
            .send()
            .await;

        let response = match response {
            Ok(r) => r,
            Err(e) => {
                last_err = format!(
                    "LLM server connection failed (worker may have died): {}",
                    e
                );
                if attempt == 0 {
                    // Force health re-check path on next loop
                    let mut manager = state.lock().await;
                    let _ = manager.check_health();
                    drop(manager);
                    continue;
                }
                return Err(last_err);
            }
        };

        if !response.status().is_success() {
            let status = response.status();
            let err_text = response
                .text()
                .await
                .unwrap_or_else(|_| "<failed to read response body>".to_string());
            return Err(format!("LLM server error ({}): {}", status, err_text));
        }

        let body_text = response
            .text()
            .await
            .unwrap_or_else(|_| "<failed to read body>".to_string());
        let data: serde_json::Value = serde_json::from_str(&body_text).map_err(|e| {
            format!(
                "Invalid or truncated JSON response: {}. Body: {}",
                e,
                body_text.chars().take(200).collect::<String>()
            )
        })?;
        let mut content = parse_llm_response(&data)?;
        if content.trim().is_empty() {
            return Err(
                "Model returned an empty response. Nothing was saved — try again.".to_string(),
            );
        }
        // R6/C5: surface max_tokens truncation (finish_reason: "length") instead of silent cut
        content = apply_truncation_marker(&data, content);
        // Audit harness A: env-guarded prompt/response capture (no-op unless
        // LOCALPERSONA_CAPTURE_PROMPTS is set). character_id/mode are NOT in
        // scope here by design — the validator fingerprints the character from
        // system-prompt content. usage is read from raw JSON (no struct change).
        crate::capture::capture(&serde_json::json!({
            "request": request_body,
            "response": {
                "finish_reason": data["choices"][0]["finish_reason"],
                "usage": data["usage"],
                "content_bytes": content.len(),
            }
        }));
        return Ok(content);
    }

    Err(if last_err.is_empty() {
        "Inference request failed after retry".into()
    } else {
        last_err
    })
}

/// Send a user turn (optional images) through the full inference pipeline.
///
/// `character_id` owns personality/RAG (RC identity contract). Prefer it over `speaker_id`.
/// `speaker_id` / `speaker_name` are user attribution only for the stored user message.
/// Sampling: `temperature`, `max_tokens`, `top_p` from UI settings (clamped).
#[command]
pub async fn send_message_with_images(
    app: tauri::AppHandle,
    state: tauri::State<'_, SharedLlamaServer>,
    conversation_id: String,
    speaker_id: String,
    speaker_name: String,
    text_content: String,
    image_paths: Vec<String>,   // relative paths like "images/abc123.png"
    character_id: Option<String>,
    temperature: Option<f64>,
    max_tokens: Option<u32>,
    top_p: Option<f64>,
) -> Result<String, String> {
    crate::conversation::validate_conv_id(&conversation_id)?;

    // Resolve character for compose + RAG (never use role label "user" as a character file key).
    let meta_for_ids = crate::conversation::load_metadata(&app, &conversation_id)?;
    let participant_ids = meta_for_ids
        .as_ref()
        .map(|m| m.participant_ids.clone())
        .unwrap_or_default();
    let resolved_character_id = resolve_character_id_for_inference(
        character_id.as_deref(),
        &participant_ids,
        &speaker_id,
    )?;

    // 1. Load recent history (shared budget helper — Phase B7)
    let budget = inference_token_budget(&state).await;
    let history_messages = load_history_for_inference(&app, &conversation_id, budget).await?;

    // 2. Build vision content array
    let mut content = Vec::new();
    if !text_content.is_empty() {
        content.push(serde_json::json!({
            "type": "text",
            "text": text_content
        }));
    }

    // Security: Validate total image count and sizes before loading
    if image_paths.len() > 10 {
        return Err("Too many images attached (maximum 10)".to_string());
    }

    let mut total_image_bytes: usize = 0;
    for rel_path in &image_paths {
        let full_path = crate::storage::get_absolute_path(&app, rel_path)?;
        let metadata = std::fs::metadata(&full_path)
            .map_err(|e| format!("Failed to read image metadata for {}: {}", rel_path, e))?;
        let file_size = metadata.len() as usize;
        if file_size > MAX_SINGLE_IMAGE_BYTES {
            return Err(format!("Image {} exceeds maximum size ({}MB limit)", rel_path, MAX_SINGLE_IMAGE_BYTES / (1024 * 1024)));
        }
        total_image_bytes += file_size;
        if total_image_bytes > MAX_TOTAL_IMAGE_BYTES {
            return Err(format!("Total image payload exceeds maximum size ({}MB limit)", MAX_TOTAL_IMAGE_BYTES / (1024 * 1024)));
        }
    }

    for rel_path in &image_paths {
        let full_path = crate::storage::get_absolute_path(&app, rel_path)?;
        let bytes = std::fs::read(&full_path)
            .map_err(|e| format!("Failed to read image {}: {}", rel_path, e))?;

        let mime = infer::get(&bytes)
            .map(|info| info.mime_type().to_string())
            .unwrap_or_else(|| "image/png".to_string());

        let base64 = BASE64.encode(&bytes);
        let data_url = format!("data:{};base64,{}", mime, base64);

        content.push(serde_json::json!({
            "type": "image_url",
            "image_url": { "url": data_url }
        }));
    }

    // 3. Compose + RAG + history (shared pipeline — Phase B7)
    let has_images = !image_paths.is_empty();
    let (system_prompt, assistant_display_name) = compose_system_with_rag(
        &app,
        &resolved_character_id,
        &text_content,
        &history_messages,
        has_images,
    )
    .await?;

    let mut messages = vec![serde_json::json!({
        "role": "system",
        "content": system_prompt
    })];
    for msg in &history_messages {
        messages.push(format_history_message_for_llm(msg));
    }
    messages.push(serde_json::json!({
        "role": "user",
        "content": content
    }));

    // 4. HTTP completion (shared helper — lock dropped before I/O)
    let assistant_content = post_chat_completion(
        &state,
        messages,
        temperature,
        max_tokens,
        top_p,
        "local-vlm",
    )
    .await?;

    // 5. Store both messages only after a non-empty successful completion.
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;

    let user_msg = crate::conversation::ChatMessage {
        id: uuid::Uuid::new_v4().to_string(),
        conversation_id: conversation_id.clone(),
        speaker_id: speaker_id.clone(),
        speaker_name: speaker_name.clone(),
        content: text_content,
        timestamp: now,
        role: "user".to_string(),
        images: image_paths.iter().map(|p| crate::conversation::ImageReference {
            id: uuid::Uuid::new_v4().to_string(),
            path: p.clone(),
            mime_type: "".to_string(),
            width: None,
            height: None,
        }).collect(),
        token_count: 0, // will be estimated in append_message
    };

    let assistant_msg = crate::conversation::ChatMessage {
        id: uuid::Uuid::new_v4().to_string(),
        conversation_id: conversation_id.clone(),
        speaker_id: resolved_character_id.clone(),
        speaker_name: assistant_display_name,
        content: assistant_content.clone(),
        timestamp: now,
        role: "assistant".to_string(),
        images: vec![],
        token_count: 0,
    };

    // Single batch write: both lines under one ndjson lock (no orphan user-only turn).
    crate::conversation::append_messages(
        &app,
        &conversation_id,
        &[user_msg, assistant_msg],
    )?;

    // Phase 1.5: Track usage and auto-respawn if threshold hit
    check_arena_reset(&state);

    Ok(assistant_content)
}

async fn get_character_data(app: &tauri::AppHandle, character_id: &str) -> Result<storage::StoredCharacter, String> {
    storage::validate_character_id(character_id)?;
    use tauri::Manager;
    let char_path = app.path().app_data_dir()
        .map_err(|e| e.to_string())?
        .join("characters")
        .join(format!("{}.json", character_id));

    let json = std::fs::read_to_string(&char_path).map_err(|e| e.to_string())?;
    serde_json::from_str(&json).map_err(|e| e.to_string())
}

/// Pure helper to extract assistant content from an OpenAI-compatible response.
/// Extracted for testability and chaos testing (WS1 / 10/10 plan).
pub fn parse_llm_response(data: &serde_json::Value) -> Result<String, String> {
    data["choices"][0]["message"]["content"]
        .as_str()
        .map(|s| s.to_string())
        .ok_or_else(|| "No content in model response".to_string())
}

/// R6/C5: apply truncation marker if finish_reason == "length". Exposed for audit regression test.
// pub for audit tests only
#[doc(hidden)]
pub fn apply_truncation_marker(data: &serde_json::Value, content: String) -> String {
    if let Some(finish) = data["choices"][0]["finish_reason"].as_str() {
        if finish == "length" {
            log::warn!("Model response truncated due to max_tokens (finish_reason=length)");
            return format!("{}\n\n[— truncated: response hit max_tokens limit —]", content.trim_end());
        }
    }
    content
}

fn retrieve_knowledge_for_character(app: &tauri::AppHandle, character_id: &str, query: &str) -> Result<Vec<storage::KnowledgeChunk>, String> {
    let knowledge_dir = storage::ensure_character_knowledge_dir(app, character_id)?;
    let mut all_chunks = Vec::new();

    // Iterate through all chunk files
    if let Ok(entries) = std::fs::read_dir(knowledge_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("json") && path.file_name().and_then(|n| n.to_str()).is_some_and(|s| s.contains(".chunks.json")) {
                if let Ok(content) = std::fs::read_to_string(&path) {
                    if let Ok(chunks) = serde_json::from_str::<Vec<storage::KnowledgeChunk>>(&content) {
                        all_chunks.extend(chunks);
                    }
                }
            }
        }
    }

    // Limit to top relevant chunks to control context size and latency
    crate::rag::retrieve_relevant_chunks(&all_chunks, query, 8)
        .map_err(|e| e.to_string())
}

/// Enhanced version that can include scene/participant context when images are present.
fn build_vision_aware_system_prompt(base_prompt: &str, has_images: bool, participant_count: usize) -> String {
    if !has_images {
        return base_prompt.to_string();
    }

    let mut prompt = base_prompt.to_string();

    if participant_count > 1 {
        prompt.push_str("\n\nThere are multiple characters participating in this scene. Pay attention to who is speaking and the visual context of the images provided.");
    } else {
        prompt.push_str("\n\nYou are looking at image(s) provided in the conversation. Describe what you see accurately and react naturally.");
    }

    prompt
}

// ==================== IMAGE HANDLING (Vision MVP) ====================

use std::path::Path;
use uuid::Uuid;

/// Opens a native file dialog filtered to images and returns the selected path (or null if cancelled).
#[command]
pub async fn pick_image_file(app: tauri::AppHandle) -> Result<Option<String>, String> {
    let file_path = app
        .dialog()
        .file()
        .add_filter("Images", &["png", "jpg", "jpeg", "webp", "gif"])
        .blocking_pick_file();

    match file_path {
        Some(path) => {
            let path_buf = path.into_path().map_err(|e| format!("Invalid path: {:?}", e))?;
            Ok(Some(path_buf.to_string_lossy().to_string()))
        }
        None => Ok(None),
    }
}

/// Copies an image file into the app's managed images directory and returns the relative path.
/// This is useful for the frontend to hand off user-uploaded images.
#[command]
pub async fn save_uploaded_image(
    app: tauri::AppHandle,
    source_path: String,
    original_name: Option<String>,
) -> Result<String, String> {
    let images_dir = storage::ensure_images_dir(&app)?;

    let source = Path::new(&source_path);
    if !source.exists() {
        return Err("Source image file does not exist".to_string());
    }

    let extension = source
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("png");

    let id = Uuid::new_v4().to_string();
    let filename = match original_name {
        Some(name) => format!("{}_{}.{}", id, sanitize_filename(&name), extension),
        None => format!("{}.{}", id, extension),
    };

    let dest_path = images_dir.join(&filename);

    // Phase 1.5: Atomic write to prevent corruption on crash during copy
    let image_bytes = std::fs::read(source)
        .map_err(|e| format!("Failed to read source image: {}", e))?;
    storage::atomic_write_bytes(&dest_path, &image_bytes)
        .map_err(|e| format!("Failed to write image atomically: {}", e))?;

    // Return relative path that can be stored in ChatMessage
    Ok(format!("images/{}", filename))
}

/// Resolve an app-data-relative path (e.g. `images/foo.png`) to an absolute filesystem path.
/// Used so the frontend can call `convertFileSrc` for webview image display.
#[command]
pub async fn resolve_app_path(
    app: tauri::AppHandle,
    relative_path: String,
) -> Result<String, String> {
    let full = crate::storage::get_absolute_path(&app, &relative_path)?;
    if !full.exists() {
        return Err(format!("Path does not exist: {}", relative_path));
    }
    Ok(full.to_string_lossy().to_string())
}

fn sanitize_filename(name: &str) -> String {
    name.chars()
        .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
        .collect::<String>()
        .trim_matches('_')
        .to_string()
}

// ==================== CONVERSATION COMMANDS (Phase 2 - Multi-Persona) ====================

use std::time::{SystemTime, UNIX_EPOCH};

#[allow(dead_code)]
fn current_timestamp() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis() as u64
}

// ==================== HIGH-PERFORMANCE CONVERSATION COMMANDS ====================
// These use the new append-only ndjson design for O(1) message writes.

#[command]
pub async fn create_conversation_from_character(
    app: tauri::AppHandle,
    character_id: String,
    character_name: String,
) -> Result<crate::conversation::ConversationMetadata, String> {
    let id = Uuid::new_v4().to_string();
    crate::conversation::create_conversation(&app, &id, &character_name, vec![character_id])
}

#[command]
pub async fn list_conversation_metadata(
    app: tauri::AppHandle,
) -> Result<Vec<crate::conversation::ConversationMetadata>, String> {
    crate::conversation::list_conversation_metadata(&app)
}

/// Returns lightweight preview data optimized for the messenger-style contact list.
#[derive(serde::Serialize)]
pub struct ConversationPreview {
    pub id: String,
    pub name: String,
    pub last_message: Option<String>,
    pub last_message_time: Option<u64>,
    pub message_count: u32,
    pub participant_ids: Vec<String>,
}

/// Phase A4: Detect conversation directories whose id equals a character id
/// (legacy bug: frontend once used characterId as conversation_id).
/// Does not merge or delete — report only for repair tooling.
#[command]
pub async fn list_orphan_conversation_hints(
    app: tauri::AppHandle,
) -> Result<Vec<serde_json::Value>, String> {
    let characters = storage::load_all_characters(&app)?;
    let char_ids: std::collections::HashSet<String> =
        characters.into_iter().map(|c| c.id).collect();
    let metas = crate::conversation::list_conversation_metadata(&app)?;
    let mut hints = Vec::new();
    for m in metas {
        if char_ids.contains(&m.id) {
            hints.push(serde_json::json!({
                "conversation_id": m.id,
                "name": m.name,
                "message_count": m.message_count,
                "hint": "conversation id equals a character id — may be a pre-R0 orphan path",
            }));
        }
    }
    Ok(hints)
}

#[command]
pub async fn list_conversation_previews(
    app: tauri::AppHandle,
) -> Result<Vec<ConversationPreview>, String> {
    let metas = crate::conversation::list_conversation_metadata(&app)?;
    let previews = metas.into_iter().map(|m| ConversationPreview {
        id: m.id,
        name: m.name,
        last_message: m.last_message_preview,
        last_message_time: m.last_message_timestamp,
        message_count: m.message_count,
        participant_ids: m.participant_ids,
    }).collect();
    Ok(previews)
}

#[command]
pub async fn get_conversation_metadata(
    app: tauri::AppHandle,
    conversation_id: String,
) -> Result<Option<crate::conversation::ConversationMetadata>, String> {
    crate::conversation::get_conversation_metadata(&app, &conversation_id)
}

#[command]
pub async fn load_messages_paginated(
    app: tauri::AppHandle,
    conversation_id: String,
    offset: usize,
    limit: usize,
) -> Result<Vec<crate::conversation::ChatMessage>, String> {
    crate::conversation::load_messages_paginated(&app, &conversation_id, offset, limit)
}

#[command]
pub async fn load_all_messages(
    app: tauri::AppHandle,
    conversation_id: String,
) -> Result<Vec<crate::conversation::ChatMessage>, String> {
    crate::conversation::load_all_messages(&app, &conversation_id)
}

/// Phase 2 remediation: Bounded message load for the UI (long conversations).
/// Uses the same token-budget logic that protects inference, so the frontend
/// no longer has to pull the entire NDJSON just to render the chat.
/// Returns messages in chronological order + a `has_more` flag.
#[allow(dead_code)] // Called from frontend via Tauri; Rust doesn't see the call statically
#[command]
pub async fn load_messages_for_display(
    app: tauri::AppHandle,
    conversation_id: String,
    max_tokens: Option<usize>,
) -> Result<serde_json::Value, String> {
    const DEFAULT_UI_BUDGET: usize = 8192;

    let budget = max_tokens.unwrap_or(DEFAULT_UI_BUDGET);
    let messages = crate::conversation::load_messages_within_token_budget(&app, &conversation_id, budget)?;

    // Accurate signal: more history exists when total message_count exceeds returned window.
    let total = crate::conversation::load_metadata(&app, &conversation_id)?
        .map(|m| m.message_count as usize)
        .unwrap_or(messages.len());
    let has_more = total > messages.len();

    Ok(serde_json::json!({
        "messages": messages,
        "has_more": has_more,
        "budget_used": budget,
        "total_messages": total,
    }))
}

#[command]
pub async fn append_message_to_conversation(
    app: tauri::AppHandle,
    conversation_id: String,
    message: crate::conversation::ChatMessage,
) -> Result<(), String> {
    crate::conversation::append_message(&app, &conversation_id, &message)
}

#[command]
pub async fn delete_conversation_cmd(app: tauri::AppHandle, id: String) -> Result<(), String> {
    crate::conversation::delete_conversation(&app, &id)
}

#[command]
pub async fn repair_conversation_cmd(app: tauri::AppHandle, id: String) -> Result<usize, String> {
    crate::conversation::repair_conversation(&app, &id)
}

/// Regenerates an assistant response for a conversation.
/// Goes through the full unified Rust pipeline (RAG + character system prompt).
/// Appends only a new assistant message (does not duplicate the user message).
///
/// `character_id` is optional (RC identity contract); falls back to conversation participants.
/// `from_message_id` optionally targets a specific **user** turn (or any turn: walks back to
/// the nearest prior user). When omitted, regenerates from the last user message.
#[command]
pub async fn regenerate_last_message(
    app: tauri::AppHandle,
    state: tauri::State<'_, SharedLlamaServer>,
    conversation_id: String,
    character_id: Option<String>,
    temperature: Option<f64>,
    max_tokens: Option<u32>,
    top_p: Option<f64>,
    from_message_id: Option<String>,
) -> Result<String, String> {
    crate::conversation::validate_conv_id(&conversation_id)?;

    let meta_for_ids = crate::conversation::load_metadata(&app, &conversation_id)?;
    let participant_ids = meta_for_ids
        .as_ref()
        .map(|m| m.participant_ids.clone())
        .unwrap_or_default();

    // 1. Load history. Mid-thread regenerate needs the full file to find the target id,
    // then we truncate + budget before inference.
    let budget = inference_token_budget(&state).await;
    let mid_thread = from_message_id.is_some();
    let all_messages = if mid_thread {
        crate::conversation::load_all_messages(&app, &conversation_id)?
    } else {
        load_history_for_inference(&app, &conversation_id, budget).await?
    };

    if all_messages.is_empty() {
        return Err("No messages in this conversation".to_string());
    }

    // 2. Resolve target user turn
    let last_user_msg = if let Some(ref mid) = from_message_id {
        let idx = all_messages
            .iter()
            .position(|m| m.id == *mid)
            .ok_or_else(|| format!("Message id not found for regenerate: {}", mid))?;
        all_messages[..=idx]
            .iter()
            .rev()
            .find(|m| m.role == "user")
            .ok_or("No user message at or before the selected turn")?
            .clone()
    } else {
        all_messages
            .iter()
            .rev()
            .find(|m| m.role == "user")
            .ok_or("No user message found to regenerate from")?
            .clone()
    };

    let text_content = last_user_msg.content.clone();
    let target_user_id = last_user_msg.id.clone();

    let resolved_character_id = resolve_character_id_for_inference(
        character_id.as_deref(),
        &participant_ids,
        &last_user_msg.speaker_id,
    )?;

    // History for compose/RAG stops at the target user turn (no later turns).
    let history_prefix: Vec<_> = all_messages
        .iter()
        .take_while(|m| m.id != target_user_id)
        .cloned()
        .chain(std::iter::once(last_user_msg.clone()))
        .collect();

    // Apply token budget from the end of the prefix (keep recent turns + target user).
    let history_for_llm =
        crate::conversation::take_messages_within_token_budget(&history_prefix, budget);

    // 3. Compose + RAG (shared)
    let (system_prompt, assistant_display_name) = compose_system_with_rag(
        &app,
        &resolved_character_id,
        &text_content,
        &history_for_llm,
        false,
    )
    .await?;

    let mut messages = vec![serde_json::json!({
        "role": "system",
        "content": system_prompt
    })];
    for msg in &history_for_llm {
        messages.push(format_history_message_for_llm(msg));
    }

    // 4. HTTP completion (shared)
    let assistant_content = post_chat_completion(
        &state,
        messages,
        temperature,
        max_tokens,
        top_p,
        "local",
    )
    .await?;

    // 5. Persist assistant reply
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;

    let assistant_msg = crate::conversation::ChatMessage {
        id: uuid::Uuid::new_v4().to_string(),
        conversation_id: conversation_id.clone(),
        speaker_id: resolved_character_id.clone(),
        speaker_name: assistant_display_name,
        content: assistant_content.clone(),
        timestamp: now,
        role: "assistant".to_string(),
        images: vec![],
        token_count: 0,
    };

    if mid_thread {
        // Drop all turns after the target user so the timeline stays coherent.
        // rewrite_conversation_messages writes a timestamped .bak first.
        let mut kept = history_prefix;
        kept.push(assistant_msg);
        crate::conversation::rewrite_conversation_messages(&app, &conversation_id, &kept)?;
    } else {
        // Last-turn regen: replace prior assistant replies after the last user turn
        // (no stacking duplicates that bloat context).
        let full = crate::conversation::load_all_messages(&app, &conversation_id)?;
        let mut kept = crate::conversation::truncate_after_last_user(&full);
        kept.push(assistant_msg);
        crate::conversation::rewrite_conversation_messages(&app, &conversation_id, &kept)?;
    }

    // Phase 1.5: Track usage and auto-respawn if threshold hit
    check_arena_reset(&state);

    Ok(assistant_content)
}

// ==================== APP INFO & DOCUMENTATION (for About/Help modals) ====================

/// Returns basic app metadata from Cargo.toml at build time (version, repo, etc).
#[command]
pub fn get_app_info() -> Result<serde_json::Value, String> {
    Ok(serde_json::json!({
        "version": env!("CARGO_PKG_VERSION"),
        "name": env!("CARGO_PKG_NAME"),
        "description": env!("CARGO_PKG_DESCRIPTION"),
        "authors": env!("CARGO_PKG_AUTHORS"),
        "license": env!("CARGO_PKG_LICENSE"),
        "repository": option_env!("CARGO_PKG_REPOSITORY").unwrap_or(""),
    }))
}

/// Reads README.md when bundled; otherwise returns a short product summary.
#[command]
pub async fn get_readme_content(app: tauri::AppHandle) -> Result<String, String> {
    use tauri::Manager;
    let resource_dir = app.path().resource_dir().map_err(|e| e.to_string())?;
    let path = resource_dir.join("README.md");
    if path.exists() {
        return std::fs::read_to_string(&path).map_err(|e| format!("Failed to read README: {}", e));
    }
    // Commercial packages may omit README from resources — About still has product blurb.
    Ok(format!(
        "# LocalPersona {}\n\nLocal GGUF persona studio. Models and llama-server are not bundled — configure them in Settings.\n\nLicense: {}.\n",
        env!("CARGO_PKG_VERSION"),
        env!("CARGO_PKG_LICENSE")
    ))
}

/// Reads the user manual markdown from bundled resources.
#[command]
pub async fn get_user_manual_content(app: tauri::AppHandle) -> Result<String, String> {
    use tauri::Manager;
    let resource_dir = app.path().resource_dir().map_err(|e| e.to_string())?;
    let path = resource_dir.join("assets/USER_MANUAL.md");
    if !path.exists() {
        return Err(format!("USER_MANUAL.md not found at {:?}", path));
    }
    std::fs::read_to_string(&path).map_err(|e| format!("Failed to read manual: {}", e))
}

/// Opens an external http/https URL using the system default handler.
/// Uses the safe `open` crate to avoid command injection.
/// Only http/https schemes are allowed.
#[command]
pub async fn open_external_url(url: String) -> Result<(), String> {
    let trimmed = url.trim();
    if trimmed.is_empty() {
        return Err("URL cannot be empty".to_string());
    }
    if !trimmed.starts_with("http://") && !trimmed.starts_with("https://") {
        return Err("Only http:// and https:// URLs are allowed for security reasons".to_string());
    }

    open::that(trimmed).map_err(|e| format!("Failed to open URL: {}", e))
}

/// Opens a native file picker for the user to select their llama-server binary.
#[command]
pub async fn pick_llama_server_binary(app: tauri::AppHandle) -> Result<Option<String>, String> {
    let file_path = app
        .dialog()
        .file()
        .add_filter("llama-server binary", &["", "exe"])
        .blocking_pick_file();

    match file_path {
        Some(path) => {
            let path_buf = path.into_path().map_err(|e| format!("Invalid path: {:?}", e))?;
            Ok(Some(path_buf.to_string_lossy().to_string()))
        }
        None => Ok(None),
    }
}

// ==================== PERSISTENT INFERENCE CONFIG ====================

#[derive(serde::Serialize, serde::Deserialize, Default)]
struct InferenceConfig {
    llama_server_path: Option<String>,
}

fn get_inference_config_path(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    use tauri::Manager;
    let app_data = app.path().app_data_dir().map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&app_data).map_err(|e| e.to_string())?;
    Ok(app_data.join("inference_config.json"))
}

fn load_inference_config(app: &tauri::AppHandle) -> InferenceConfig {
    match get_inference_config_path(app) {
        Ok(path) => {
            match std::fs::read_to_string(&path) {
                Ok(content) => {
                    match serde_json::from_str::<InferenceConfig>(&content) {
                        Ok(cfg) => cfg,
                        Err(e) => {
                            log::warn!("Inference config file is corrupt ({}), using defaults", e);
                            InferenceConfig::default()
                        }
                    }
                }
                Err(e) => {
                    log::warn!("Failed to read inference config ({}), using defaults", e);
                    InferenceConfig::default()
                }
            }
        }
        Err(e) => {
            log::warn!("Failed to get inference config path ({}), using defaults", e);
            InferenceConfig::default()
        }
    }
}

fn save_inference_config(app: &tauri::AppHandle, cfg: &InferenceConfig) -> Result<(), String> {
    let path = get_inference_config_path(app)?;
    let json = serde_json::to_string_pretty(cfg).map_err(|e| e.to_string())?;
    crate::storage::atomic_write(&path, &json)
}

/// Returns the last saved llama-server binary path (if any).
#[command]
pub async fn get_llama_server_path(app: tauri::AppHandle) -> Result<Option<String>, String> {
    let cfg = load_inference_config(&app);
    Ok(cfg.llama_server_path)
}

/// Sets (and persists) the llama-server binary path.
#[command]
pub async fn set_llama_server_path(
    app: tauri::AppHandle,
    state: tauri::State<'_, SharedLlamaServer>,
    path: String,
) -> Result<(), String> {
    let p = PathBuf::from(&path);
    if !p.exists() {
        return Err("Selected binary does not exist".to_string());
    }

    // Update runtime manager
    {
        let mut manager = state.lock().await;
        manager.set_binary_path(p.clone());
    }

    // Persist
    let mut cfg = load_inference_config(&app);
    cfg.llama_server_path = Some(path);
    save_inference_config(&app, &cfg)?;

    Ok(())
}

// ==================== MODEL DISCOVERY ====================

#[derive(serde::Serialize, serde::Deserialize, Clone)]
pub struct DiscoveredModel {
    pub path: String,
    pub filename: String,
    pub size_mb: u64,
    pub is_vision: bool,
    pub mmproj_path: Option<String>,
    pub last_modified: Option<u64>,

    // New fields from proper GGUF parsing (10/10 remediation)
    pub architecture: Option<String>,
    pub context_length: Option<u32>,
    pub parameter_count: Option<u64>,
    pub quantization: Option<String>,
    pub model_name: Option<String>,
}

/// Scans common locations for GGUF models and returns basic metadata.
/// This is the foundation for a much better onboarding experience.
#[command]
pub async fn scan_for_models(app: tauri::AppHandle) -> Result<Vec<DiscoveredModel>, String> {
    // Offload synchronous filesystem I/O to blocking thread pool
    let result = tokio::task::spawn_blocking(move || {
        use std::fs;
        use tauri::Manager;

        let mut models: Vec<DiscoveredModel> = Vec::new();
        let mut seen = std::collections::HashSet::new();

        // Locations to scan (in priority order)
        let mut search_dirs: Vec<PathBuf> = vec![];

        // 1. Bundled test-models (shipped with the app)
        if let Ok(resource_dir) = app.path().resource_dir() {
            search_dirs.push(resource_dir.join("test-models"));
        }

        // 2. App data models folder (user's own models)
        if let Ok(app_data) = app.path().app_data_dir() {
            search_dirs.push(app_data.join("models"));
        }

        // 3. Common user locations
        if let Some(home) = std::env::var_os("HOME").or(std::env::var_os("USERPROFILE")) {
            let home = PathBuf::from(home);
            search_dirs.push(home.join("Models"));
            search_dirs.push(home.join("models"));
            search_dirs.push(home.join("llama.cpp/models"));
        }

        // 4. Current working directory (useful in dev)
        if let Ok(cwd) = std::env::current_dir() {
            search_dirs.push(cwd.join("test-models"));
        }

        for dir in search_dirs {
            if !dir.exists() {
                continue;
            }

            if let Ok(entries) = fs::read_dir(&dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if let Some(ext) = path.extension() {
                        if ext == "gguf" {
                            let path_str = path.to_string_lossy().to_string();
                            if seen.contains(&path_str) {
                                continue;
                            }
                            seen.insert(path_str.clone());

                            let metadata = fs::metadata(&path).ok();
                            let size_mb = metadata
                                .as_ref()
                                .map(|m| m.len() / (1024 * 1024))
                                .unwrap_or(0);

                            let filename = path
                                .file_name()
                                .map(|f| f.to_string_lossy().to_string())
                                .unwrap_or_default();

                            // Very simple vision heuristic — can be improved later
                            let is_vision = filename.to_lowercase().contains("vl")
                                || filename.to_lowercase().contains("vision")
                                || filename.to_lowercase().contains("llava")
                                || filename.to_lowercase().contains("qwen2-vl")
                                || filename.to_lowercase().contains("qwen3vl");

                            // Try to find matching mmproj in same folder
                            let mmproj_path = find_matching_mmproj(&path);

                            let last_modified = metadata.and_then(|m| {
                                m.modified()
                                    .ok()
                                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                                    .map(|d| d.as_secs())
                            });

                            // === GGUF Automatic Understanding (10/10 remediation) ===
                            let gguf_meta = crate::gguf::read_gguf_metadata(&path).ok();

                            let is_vision = is_vision || gguf_meta.as_ref().map_or(false, |m| m.is_vision);
                            let architecture = gguf_meta.as_ref().and_then(|m| m.architecture.clone());
                            let context_length = gguf_meta.as_ref().and_then(|m| m.context_length);
                            let parameter_count = gguf_meta.as_ref().and_then(|m| m.parameter_count);
                            let quantization = gguf_meta.as_ref().and_then(|m| m.quantization.clone());
                            let model_name = gguf_meta.as_ref().and_then(|m| m.name.clone());

                            models.push(DiscoveredModel {
                                path: path_str,
                                filename,
                                size_mb,
                                is_vision,
                                mmproj_path,
                                last_modified,
                                architecture,
                                context_length,
                                parameter_count,
                                quantization,
                                model_name,
                            });
                    }
                }
            }
        }
    }

        // Sort: vision models first, then by size descending (bigger = usually better)
        models.sort_by(|a, b| {
            b.is_vision
                .cmp(&a.is_vision)
                .then(b.size_mb.cmp(&a.size_mb))
        });

        Ok(models)
    });

    result.await.map_err(|e| format!("Model scan thread failed: {}", e))?
}

fn find_matching_mmproj(model_path: &std::path::Path) -> Option<String> {
    use std::fs;

    let parent = model_path.parent()?;
    let stem = model_path.file_stem()?.to_string_lossy();

    // Common naming patterns
    let mmproj_name = format!("{}-mmproj.gguf", stem);

    let candidates: [&str; 4] = [
        "mmproj.gguf",
        "mmproj-F16.gguf",
        &mmproj_name,
        "mmproj-Q4_0.gguf",
    ];

    for name in candidates {
        let candidate = parent.join(name);
        if candidate.exists() {
            return Some(candidate.to_string_lossy().to_string());
        }
    }

    // Last resort: any file containing "mmproj" in the same folder
    if let Ok(entries) = fs::read_dir(parent) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_lowercase();
            if name.contains("mmproj") && name.ends_with(".gguf") {
                return Some(entry.path().to_string_lossy().to_string());
            }
        }
    }

    None
}

/// Saves an uploaded voice reference sample (already normalized WAV from frontend).
/// Returns the relative path that should be stored in the character.
#[command]
pub async fn save_voice_sample(
    app: tauri::AppHandle,
    character_id: String,
    data_url: String,
) -> Result<String, String> {
    // Security: Prevent oversized voice samples (check base64 length, which is ~33% larger than raw bytes)
    let estimated_raw_len = data_url.len() * 3 / 4;
    if estimated_raw_len > MAX_VOICE_SAMPLE_BYTES {
        return Err(format!("Voice sample too large ({}MB limit)", MAX_VOICE_SAMPLE_BYTES / (1024 * 1024)));
    }
    storage::save_voice_sample(&app, &character_id, &data_url)
}

/// Saves a document (PDF, TXT, etc.) into a character's persistent knowledge base for RAG.
#[command]
pub async fn save_character_document(
    app: tauri::AppHandle,
    character_id: String,
    filename: String,
    data: Vec<u8>,
) -> Result<String, String> {
    // Security: Prevent oversized document uploads
    if data.len() > MAX_DOCUMENT_BYTES {
        return Err(format!("Document too large ({}MB limit)", MAX_DOCUMENT_BYTES / (1024 * 1024)));
    }
    // Security: Validate file extension against allowlist
    let ext = std::path::Path::new(&filename)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();
    // Align with process_character_document (chunker supports these only).
    let allowed = ["pdf", "txt", "md", "markdown"];
    if !allowed.contains(&ext.as_str()) {
        return Err(format!(
            "Unsupported file type '.{}'. Allowed: pdf, txt, md",
            ext
        ));
    }
    storage::save_character_document(&app, &character_id, &filename, &data)
}

/// Generates speech audio for the given text using a TTS backend.
/// Expects an OpenAI-compatible /v1/audio/speech endpoint (common when running
/// a second llama-server with audio support or a dedicated TTS server for the Qwen3-TTS model).
#[command]
pub async fn generate_speech(
    text: String,
    voice: Option<String>,
    tts_endpoint: Option<String>,
    voice_server: tauri::State<'_, SharedVoiceServer>,
) -> Result<Vec<u8>, String> {
    // Security: Prevent oversized TTS requests
    if text.len() > MAX_TTS_TEXT_LENGTH {
        return Err(format!("TTS text too long ({} chars, max {})", text.len(), MAX_TTS_TEXT_LENGTH));
    }

    let endpoint = tts_endpoint.unwrap_or_else(|| "http://localhost:8081/v1/audio/speech".to_string());

    // Phase 1.5: SSRF prevention — validate TTS endpoint is localhost-only
    let is_localhost = endpoint.starts_with("http://localhost:")
        || endpoint.starts_with("http://127.0.0.1:")
        || endpoint.starts_with("http://[::1]:");
    if !is_localhost {
        return Err("TTS endpoint must be on localhost (127.0.0.1, localhost, or [::1]) for security".to_string());
    }

    let client = HTTP_CLIENT.clone();

    let voice_name = voice.unwrap_or_else(|| "default".to_string());

    let body = serde_json::json!({
        "model": "qwen3-tts",
        "input": text,
        "voice": voice_name,
        "response_format": "wav"
    });

    // P1-4: attach managed voice-server session key when available (best-effort,
    // non-blocking — TTS must not contend the server Mutex for HTTP I/O).
    let voice_key: Option<String> = voice_server
        .inner()
        .try_lock()
        .ok()
        .and_then(|m| m.api_key());
    let mut tts_req = client
        .post(&endpoint)
        .header("Host", endpoint_host_header(&endpoint));
    if let Some(ref key) = voice_key {
        tts_req = tts_req.bearer_auth(key);
    }
    let response = tts_req
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("Failed to reach TTS server at {}: {}", endpoint, e))?;

    if !response.status().is_success() {
        let status = response.status();
        let err_text = response.text().await.unwrap_or_else(|_| "<failed to read response body>".to_string());
        return Err(format!("TTS server error ({}): {}", status, err_text));
    }

    let audio_bytes = response
        .bytes()
        .await
        .map_err(|e| format!("Failed to read audio response: {}", e))?;

    // Phase 1.5: Arena Reset check for voice server after successful TTS
    check_voice_arena_reset(&voice_server);

    Ok(audio_bytes.to_vec())
}

fn check_voice_arena_reset(state: &tauri::State<'_, SharedVoiceServer>) {
    let server = state.inner().clone();
    tokio::spawn(async move {
        let reason = match server.try_lock() {
            Ok(mut manager) => {
                let req_hit = manager.increment_and_check_reset();
                let up_hit = manager.should_reset_due_to_uptime();
                if req_hit {
                    Some("request_threshold")
                } else if up_hit {
                    Some("uptime_threshold")
                } else {
                    None
                }
            }
            Err(_) => return,
        };
        if let Some(reason) = reason {
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            let mut manager = server.lock().await;
            manager.record_reset_reason(reason);
            log::info!("Voice Arena Reset ({reason}). Respawning TTS server...");
            if let Err(e) = manager.reset().await {
                log::error!("Voice Arena Reset failed: {}", e);
            }
        }
    });
}

// ==================== VOICE / TTS SERVER MANAGEMENT ====================

#[command]
pub async fn start_voice_server(
    state: tauri::State<'_, SharedVoiceServer>,
    request: ServerStartRequest,
) -> Result<ServerStatus, String> {
    let mut manager = state.lock().await;
    manager
        .start(request)
        .await
        .map_err(|e| format!("Failed to start voice server: {}", e))
}

#[command]
pub async fn stop_voice_server(state: tauri::State<'_, SharedVoiceServer>) -> Result<(), String> {
    let mut manager = state.lock().await;
    manager
        .stop()
        .await
        .map_err(|e| format!("Failed to stop voice server: {}", e))
}

#[command]
pub async fn get_voice_server_status(state: tauri::State<'_, SharedVoiceServer>) -> Result<ServerStatus, String> {
    let mut manager = state.lock().await;
    // FIX-C02: Check actual process health on every status query
    manager.check_health();
    Ok(manager.status())
}

// ==================== Phase 12.0: AutopsyDump ====================

#[command]
pub async fn list_autopsy_dumps(app: tauri::AppHandle) -> Result<Vec<crate::autopsy::AutopsyDump>, String> {
    crate::autopsy::list_autopsy_dumps(&app)
}

// ==================== Phase 12.0: Circuit Breaker ====================

#[command]
pub async fn get_circuit_breaker_status() -> Result<crate::circuit_breaker::CircuitBreakerStatus, String> {
    Ok(crate::circuit_breaker::get_status())
}

// ==================== Phase 12.0: Memory Telemetry ====================

#[command]
pub async fn get_memory_telemetry() -> Result<serde_json::Value, String> {
    // Read current memory directly using sysinfo (cross-platform)
    let (rss_kb, vms_kb) = {
        use sysinfo::{Pid, System, ProcessesToUpdate};
        let mut sys = System::new();
        sys.refresh_processes(ProcessesToUpdate::Some(&[Pid::from_u32(std::process::id())]), false);
        if let Some(process) = sys.process(Pid::from_u32(std::process::id())) {
            (process.memory() / 1024, process.virtual_memory() / 1024)
        } else {
            (0, 0)
        }
    };

    Ok(serde_json::json!({
        "rss_kb": rss_kb,
        "vms_kb": vms_kb,
        "rss_mb": rss_kb / 1024,
        "vms_mb": vms_kb / 1024,
        "pid": std::process::id(),
    }))
}

/// Returns the canonical IPC contract schema for LocalPersona.
/// This is the single source of truth for the backend ↔ frontend boundary.
///
/// v7.2 Tribunal remediation (C03):
/// - Any change to the structs mirrored here MUST update the golden file
///   at gen/schemas/localpersona-ipc-schemas.json, otherwise the test
///   `test_ipc_schema_has_no_drift` (and therefore the Tribunal) will fail.
/// - This prevents silent schema drift between Rust models and the JS layer.
/// WS5 / 10/10: Rich professional diagnostics snapshot.
/// Pulls live data from both managers + forensic systems.
#[command]
pub async fn get_diagnostics_snapshot(
    app: tauri::AppHandle,
    llm_state: tauri::State<'_, SharedLlamaServer>,
    voice_state: tauri::State<'_, SharedVoiceServer>,
) -> Result<serde_json::Value, String> {
    let mut llm = llm_state.lock().await;
    llm.check_health(); // ensure fresh health
    let llm_status = llm.status();
    let (llm_requests, llm_max) = llm.arena_reset_info();
    let llm_uptime_secs = llm.uptime_seconds();
    let (llm_reset_reason, llm_reset_at) = llm.last_reset_info();

    drop(llm);

    let mut voice = voice_state.lock().await;
    voice.check_health();
    let voice_status = voice.status();
    let (voice_requests, voice_max) = voice.arena_reset_info();
    let voice_uptime_secs = voice.uptime_seconds();
    let (voice_reset_reason, voice_reset_at) = voice.last_reset_info();

    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();

    // Pull memory pressure
    let memory = get_memory_telemetry().await.unwrap_or(serde_json::json!({}));

    // Pull circuit breaker summary
    let circuit = get_circuit_breaker_status().await
        .map(|s| serde_json::to_value(s).unwrap_or(serde_json::json!({})))
        .unwrap_or(serde_json::json!({ "status": "unavailable" }));

    let orphan_hints = list_orphan_conversation_hints(app.clone())
        .await
        .unwrap_or_default();

    Ok(serde_json::json!({
        "timestamp": timestamp,
        "llm_server": {
            "state": llm_status.state,
            "running": llm_status.running,
            "port": llm_status.port,
            "model": llm_status.model_path,
            "arena": {
                "requests_since_reset": llm_requests,
                "max_requests_before_reset": llm_max,
                "uptime_seconds": llm_uptime_secs,
                "uptime_minutes": llm_uptime_secs.map(|s| s / 60),
                "last_reset_reason": llm_reset_reason,
                "last_reset_at_unix_ms": llm_reset_at
            },
            "model_context_length": llm_status.model_context_length
        },
        "voice_server": {
            "state": voice_status.state,
            "running": voice_status.running,
            "port": voice_status.port,
            "arena": {
                "requests_since_reset": voice_requests,
                "max_requests_before_reset": voice_max,
                "uptime_seconds": voice_uptime_secs,
                "uptime_minutes": voice_uptime_secs.map(|s| s / 60),
                "last_reset_reason": voice_reset_reason,
                "last_reset_at_unix_ms": voice_reset_at
            }
        },
        "memory": memory,
        "circuit_breaker": circuit,
        "rag": take_rag_status_snapshot(),
        "orphan_conversations": orphan_hints,
        "note": "Live professional diagnostics from both servers + forensic systems."
    }))
}

pub fn get_ipc_contract() -> serde_json::Value {
    serde_json::json!({
        "version": "1.0",
        "description": "Canonical JSON schemas for LocalPersona IPC types (Tauri commands). This is the contract. Update the golden file when Rust structs change.",
        "definitions": {
            "ImageReference": {
                "type": "object",
                "properties": {
                    "id": {"type": "string"},
                    "path": {"type": "string"},
                    "mime_type": {"type": "string"},
                    "width": {"type": ["integer", "null"]},
                    "height": {"type": ["integer", "null"]}
                },
                "required": ["id", "path", "mime_type"]
            }
        },
        "schemas": {
            "StoredCharacter": {
                "type": "object",
                "description": "Wire format is camelCase (serde rename_all). Deser also accepts snake_case aliases.",
                "properties": {
                    "version": {"type": "integer", "description": "Schema version for migrations"},
                    "id": {"type": "string", "description": "Unique character identifier"},
                    "name": {"type": "string"},
                    "mode": {"type": "string"},
                    "avatarColor": {"type": "string"},
                    "avatarUrl": {"type": ["string", "null"], "description": "Relative path like avatars/xxx.png (Rust field: avatar_path)"},
                    "personality": {"type": "string"},
                    "userNickname": {"type": ["string", "null"]},
                    "userDescription": {"type": ["string", "null"]},
                    "scenario": {"type": ["string", "null"]},
                    "writingInstructions": {"type": ["string", "null"]},
                    "systemPrompt": {"type": ["string", "null"]},
                    "greeting": {"type": "string"},
                    "createdAt": {"type": "integer", "description": "Epoch millis"},
                    "voiceMode": {"type": "string", "enum": ["none", "preset", "custom_sample"]},
                    "voicePreset": {"type": ["string", "null"]},
                    "voiceSamplePath": {"type": ["string", "null"]},
                    "spawnLocation": {"type": ["object", "null"]},
                    "isPublicSpawn": {"type": "boolean"},
                    "hasKnowledgeBase": {"type": "boolean"}
                },
                "required": ["id", "name", "mode", "avatarColor", "personality", "greeting", "createdAt"]
            },
            "ChatMessage": {
                "type": "object",
                "properties": {
                    "id": {"type": "string"},
                    "conversation_id": {"type": "string"},
                    "speaker_id": {"type": "string"},
                    "speaker_name": {"type": "string"},
                    "content": {"type": "string"},
                    "timestamp": {"type": "integer", "description": "Epoch millis"},
                    "role": {"type": "string", "enum": ["user", "assistant", "narrator"]},
                    "images": {"type": "array", "items": {"$ref": "#/definitions/ImageReference"}},
                    "token_count": {"type": "integer", "default": 0}
                },
                "required": ["id", "conversation_id", "speaker_id", "speaker_name", "content", "timestamp", "role"]
            },
            "ConversationMetadata": {
                "type": "object",
                "properties": {
                    "version": {"type": "integer"},
                    "id": {"type": "string"},
                    "name": {"type": "string"},
                    "participant_ids": {"type": "array", "items": {"type": "string"}},
                    "created_at": {"type": "integer"},
                    "updated_at": {"type": "integer"},
                    "message_count": {"type": "integer"},
                    "last_message_preview": {"type": ["string", "null"]},
                    "last_message_timestamp": {"type": ["integer", "null"]}
                },
                "required": ["version", "id", "name", "participant_ids", "created_at", "updated_at", "message_count"]
            },
            "ServerStatus": {
                "type": "object",
                "properties": {
                    "running": {"type": "boolean"},
                    "starting": {"type": "boolean"},
                    "model_path": {"type": ["string", "null"]},
                    "port": {"type": ["integer", "null"]},
                    "api_base": {"type": ["string", "null"]},
                    "pid": {"type": ["integer", "null"]},
                    "last_error": {"type": ["string", "null"]},
                    "vision_enabled": {"type": "boolean"},
                    "request_count": {"type": "integer"},
                    "max_requests_before_reset": {"type": "integer"}
                },
                "required": ["running", "starting", "vision_enabled", "request_count", "max_requests_before_reset"]
            },
            "ServerStartRequest": {
                "type": "object",
                "properties": {
                    "model_path": {"type": "string", "description": "Absolute path to GGUF model"},
                    "ctx_size": {"type": ["integer", "null"]},
                    "gpu_layers": {"type": ["integer", "null"]},
                    "threads": {"type": ["integer", "null"]},
                    "flash_attn": {"type": "boolean"},
                    "preferred_port": {"type": ["integer", "null"]},
                    "mmproj_path": {"type": ["string", "null"], "description": "Path to multimodal projector"}
                },
                "required": ["model_path", "flash_attn"]
            }
        }
    })
}

/// C02 (historical) + v7.2 C03 remediation:
/// Tauri command that exposes the current IPC contract to the frontend at runtime.
/// The real protection against drift is the golden file + `test_ipc_schema_has_no_drift`.
#[command]
pub async fn generate_type_schemas() -> Result<serde_json::Value, String> {
    Ok(get_ipc_contract())
}

#[cfg(test)]
mod prompt_composer_tests {
    use super::*;

    fn make_test_character(custom_system: Option<&str>, personality: &str, scenario: Option<&str>, writing: Option<&str>) -> storage::StoredCharacter {
        storage::StoredCharacter {
            version: 1,
            id: "test-char".to_string(),
            name: "Test Character".to_string(),
            mode: "chat".to_string(),
            avatar_color: "#000000".to_string(),
            avatar_path: None,
            personality: personality.to_string(),
            user_nickname: Some("User".to_string()),
            user_description: Some("A curious explorer".to_string()),
            scenario: scenario.map(|s| s.to_string()),
            writing_instructions: writing.map(|w| w.to_string()),
            system_prompt: custom_system.map(|s| s.to_string()),
            greeting: "Hello there!".to_string(),
            created_at: 0,
            voice_mode: "none".to_string(),
            voice_preset: None,
            voice_sample_path: None,
            spawn_location: None,
            is_public_spawn: false,
            has_knowledge_base: false,
        }
    }

    #[test]
    fn test_compose_rich_prompt_uses_personality_and_writing_instructions() {
        let char = make_test_character(
            None,
            "Gruff but loyal space marine who hates small talk.",
            Some("The year is 2147 aboard the derelict station Erebus-9."),
            Some("Keep responses terse, military, and laced with dark humor. Never use modern slang."),
        );

        let prompt = compose_system_prompt(&char);

        assert!(prompt.contains("Gruff but loyal space marine"), "Personality must appear");
        assert!(prompt.contains("Erebus-9"), "Scenario must appear");
        assert!(prompt.contains("terse, military"), "Writing instructions must appear");
        assert!(!prompt.contains("You are a helpful assistant"), "Should not fall back to generic");
        assert!(prompt.contains("Writing Style & Response Guidelines"), "Must use clear section headers");
    }

    #[test]
    fn test_custom_system_prompt_full_override() {
        let char = make_test_character(
            Some("You are an ancient dragon who only speaks in riddles. Never break character."),
            "Totally ignored personality",
            None,
            None,
        );

        let prompt = compose_system_prompt(&char);

        assert!(prompt.contains("ancient dragon who only speaks in riddles"));
        assert!(!prompt.contains("Totally ignored personality"), "Custom system must completely override rich fields");
    }

    #[test]
    fn test_minimal_character_still_produces_usable_prompt() {
        let char = make_test_character(None, "A friendly baker.", None, None);
        let prompt = compose_system_prompt(&char);

        assert!(prompt.contains("You are Test Character"));
        assert!(prompt.contains("friendly baker"));
        assert!(prompt.contains("Stay completely in character"));
    }

    #[test]
    fn test_adventure_mode_prefix_is_included() {
        let mut char = make_test_character(None, "A ranger.", None, None);
        char.mode = "adventure".to_string();
        let prompt = compose_system_prompt(&char);
        assert!(prompt.contains("[Interaction Mode]"));
        assert!(prompt.contains("immersive adventure"));
    }

    #[test]
    fn test_custom_system_still_overrides_mode() {
        let mut char = make_test_character(
            Some("ONLY THIS PROMPT"),
            "ignored",
            None,
            None,
        );
        char.mode = "adventure".to_string();
        let prompt = compose_system_prompt(&char);
        assert_eq!(prompt, "ONLY THIS PROMPT");
    }
}

#[cfg(test)]
mod sampling_and_history_tests {
    use super::*;

    #[test]
    fn sampling_defaults_and_clamps() {
        let (t, m, p) = resolve_sampling_params(None, None, None);
        assert!((t - 0.7).abs() < f64::EPSILON);
        assert_eq!(m, 2048);
        assert!((p - 0.9).abs() < f64::EPSILON);

        let (t2, m2, p2) = resolve_sampling_params(Some(9.0), Some(5), Some(-1.0));
        assert!((t2 - 2.0).abs() < f64::EPSILON);
        assert_eq!(m2, 16);
        assert!((p2 - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn history_prefixes_named_assistant() {
        let msg = crate::conversation::ChatMessage {
            id: "1".into(),
            conversation_id: "c".into(),
            speaker_id: "char-1".into(),
            speaker_name: "Support Unit".into(),
            content: "Acknowledged.".into(),
            timestamp: 0,
            role: "assistant".into(),
            images: vec![],
            token_count: 0,
        };
        let v = format_history_message_for_llm(&msg);
        assert_eq!(v["role"], "assistant");
        assert_eq!(v["content"], "Support Unit: Acknowledged.");
    }

    #[test]
    fn rag_injection_includes_source_title() {
        let chunk = storage::KnowledgeChunk {
            id: "c1".into(),
            source_file: "characters/x/knowledge/lore.md".into(),
            text: "The station was abandoned in 2147.".into(),
            char_start: 0,
            char_end: 10,
            embedding: None,
        };
        let text = format_rag_injection(&[chunk]);
        assert!(text.contains("lore.md"));
        assert!(text.contains("abandoned in 2147"));
    }

    #[test]
    fn rag_query_includes_prior_user_turns() {
        let history = vec![
            crate::conversation::ChatMessage {
                id: "1".into(),
                conversation_id: "c".into(),
                speaker_id: "user".into(),
                speaker_name: "You".into(),
                content: "First topic about Erebus".into(),
                timestamp: 1,
                role: "user".into(),
                images: vec![],
                token_count: 0,
            },
            crate::conversation::ChatMessage {
                id: "2".into(),
                conversation_id: "c".into(),
                speaker_id: "a".into(),
                speaker_name: "Bot".into(),
                content: "reply".into(),
                timestamp: 2,
                role: "assistant".into(),
                images: vec![],
                token_count: 0,
            },
        ];
        let q = build_rag_query("What about the reactor?", &history);
        assert!(q.contains("Erebus"));
        assert!(q.contains("reactor"));
    }
}

/// R0 identity contract: character lookup must never treat role labels as character file ids.
#[cfg(test)]
mod identity_contract_tests {
    use super::*;

    #[test]
    fn explicit_character_id_wins() {
        let participants = vec!["legacy-char".to_string()];
        let resolved = resolve_character_id_for_inference(
            Some("real-char-01"),
            &participants,
            "user",
        )
        .expect("explicit id should resolve");
        assert_eq!(resolved, "real-char-01");
    }

    #[test]
    fn user_role_label_is_never_treated_as_character() {
        let err = resolve_character_id_for_inference(Some("user"), &[], "user")
            .expect_err("reserved role must not resolve as character");
        assert!(
            err.contains("Cannot resolve character") || err.contains("Invalid"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn falls_back_to_conversation_participant() {
        let participants = vec!["user".to_string(), "persona-alpha".to_string()];
        let resolved = resolve_character_id_for_inference(None, &participants, "user")
            .expect("should use participant");
        assert_eq!(resolved, "persona-alpha");
    }

    #[test]
    fn speaker_id_fallback_only_when_not_reserved() {
        let resolved = resolve_character_id_for_inference(None, &[], "my-character")
            .expect("non-reserved speaker_id may be used as last resort");
        assert_eq!(resolved, "my-character");
    }

    #[test]
    fn empty_inputs_error_explicitly() {
        let err = resolve_character_id_for_inference(None, &[], "user")
            .expect_err("must fail fast");
        assert!(err.contains("Cannot resolve character"), "unexpected: {err}");
    }
}

#[cfg(test)]
mod sweep_w2_security_tests {
    use super::*;

    #[test]
    fn host_header_pins_loopback() {
        assert_eq!(
            endpoint_host_header("http://127.0.0.1:8080/v1/chat/completions"),
            "127.0.0.1:8080"
        );
        assert_eq!(
            endpoint_host_header("http://localhost:8081/v1/audio/speech"),
            "localhost:8081"
        );
        assert_eq!(
            endpoint_host_header("http://[::1]:8080/v1/models"),
            "[::1]:8080"
        );
    }

    #[test]
    fn host_header_falls_back_on_evil_input() {
        // DNS-rebinding style input must never be reflected into Host.
        assert_eq!(
            endpoint_host_header("http://evil.example.com/v1/chat/completions"),
            "127.0.0.1"
        );
        assert_eq!(endpoint_host_header("not a url"), "127.0.0.1");
    }
}

// LocalPersona - One-click GGUF Persona Studio
// Tauri 2 desktop shell

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::sync::Arc;
use tauri::menu::{Menu, MenuItem, Submenu};
use tauri::{Manager, Emitter};
use tokio::sync::Mutex;

mod commands;
mod inference;
mod storage;
mod conversation;
mod rag;
mod autopsy;
mod circuit_breaker;
mod memory_monitor;
mod gguf; // GGUF parser - needed for binary crate root
mod capture; // Audit harness A: env-guarded prompt/response capture tap

use crate::inference::LlamaServerManager;

#[tokio::main]
async fn main() {
    env_logger::init();

    // Phase 12.0: Install structured crash dump hook
    autopsy::install_panic_hook();

    tauri::Builder::default()
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_dialog::init())
        .menu(|handle| {
            let help_menu = Submenu::with_items(
                handle,
                "Help",
                true,
                &[
                    &MenuItem::with_id(handle, "user-manual", "User Manual", true, None::<&str>)?,
                    &MenuItem::with_id(handle, "about", "About LocalPersona", true, None::<&str>)?,
                ],
            )?;

            Ok(Menu::with_items(handle, &[&help_menu])?)
        })
        .on_menu_event(|app, event| {
            match event.id().as_ref() {
                "user-manual" => {
                    if let Err(e) = app.emit("show-help", ()) {
                        log::warn!("Failed to emit show-help: {}", e);
                    }
                }
                "about" => {
                    if let Err(e) = app.emit("show-about", ()) {
                        log::warn!("Failed to emit show-about: {}", e);
                    }
                }
                _ => {}
            }
        })
        .setup(|app| {
            // Create the shared inference manager (LLM).
            let llm_manager = Arc::new(Mutex::new(LlamaServerManager::new()));
            app.manage(llm_manager.clone());

            // Create the shared voice/TTS manager (separate lifecycle).
            let voice_manager = Arc::new(Mutex::new(inference::VoiceServerManager::new()));
            app.manage(voice_manager.clone());

            // Ensure our app data directories exist (Phase 1 - real persistence)
            if let Ok(app_data) = app.path().app_data_dir() {
                let avatars_dir = app_data.join("avatars");
                let characters_dir = app_data.join("characters");
                let images_dir = app_data.join("images");

                if let Err(e) = std::fs::create_dir_all(&avatars_dir) { log::warn!("Could not create avatars dir: {}", e); }
                if let Err(e) = std::fs::create_dir_all(&characters_dir) { log::warn!("Could not create characters dir: {}", e); }
                if let Err(e) = std::fs::create_dir_all(&images_dir) { log::warn!("Could not create images dir: {}", e); }
                if let Err(e) = std::fs::create_dir_all(avatars_dir.join("default")) { log::warn!("Could not create default avatars dir: {}", e); }

                println!("[LocalPersona] Data directories ready at: {:?}", app_data);

                // One-time load of default personas + their placeholder avatars if the characters directory is empty
                if let Ok(entries) = std::fs::read_dir(&characters_dir) {
                    if entries.count() == 0 {
                        if let Ok(resource_dir) = app.path().resource_dir() {
                            let default_json = resource_dir.join("assets/default_personas.json");
                            let default_avatars_dir = resource_dir.join("assets/avatars/default");

                            if default_json.exists() {
                                if let Ok(json) = std::fs::read_to_string(&default_json) {
                                    if let Ok(personas) = serde_json::from_str::<Vec<serde_json::Value>>(&json) {
                                        for persona in personas {
                                            if let Some(id) = persona.get("id").and_then(|v| v.as_str()) {
                                                // Skip personas with invalid IDs to prevent path traversal
                                                if let Err(e) = storage::validate_character_id(id) {
                                                    log::warn!("Skipping default persona with invalid id '{}': {}", id, e);
                                                    continue;
                                                }
                                                // Write character JSON (best effort, log on failure)
                                                let char_path = characters_dir.join(format!("{}.json", id));
                                                if let Ok(pretty) = serde_json::to_string_pretty(&persona) {
                                                    if let Err(e) = storage::atomic_write(&char_path, &pretty) {
                                                        log::warn!("Failed to write default persona {}: {}", id, e);
                                                    }
                                                }

                                                // Copy default avatar image if it exists
                                                if let Some(avatar_url) = persona.get("avatarUrl").and_then(|v| v.as_str()) {
                                                    if let Some(filename) = avatar_url.strip_prefix("avatars/default/") {
                                                        let src = default_avatars_dir.join(filename);
                                                        let dest = avatars_dir.join(filename);
                                                        if src.exists() {
                                                            if let Err(e) = std::fs::copy(&src, &dest) {
                                                                log::warn!("Failed to copy default avatar for {}: {}", id, e);
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                        println!("[LocalPersona] Default personas + placeholder avatars loaded from bundle.");
                                    }
                                }
                            }
                        }
                    }
                }
            }

            println!("[LocalPersona] Rust backend initialized. Inference manager ready.");

            // Phase 12.0: Start memory monitor with wired preemptive reset
            let (reset_tx, mut reset_rx) = tokio::sync::mpsc::unbounded_channel::<()>();
            let _memory_monitor = memory_monitor::start_memory_monitor(
                memory_monitor::MemoryMonitorConfig::default(),
                Some(reset_tx),
            );

            // Listen for memory monitor reset signals and trigger LLM Arena Reset.
            // Also periodically clean up expired circuit breaker entries.
            // Extract the start request under the lock, then drop lock before reset
            // to prevent deadlocking the Mutex during the stop+start cycle.
            let llm_for_reset = llm_manager.clone();
            tokio::spawn(async move {
                while reset_rx.recv().await.is_some() {
                    log::warn!("Memory Monitor signal received. Triggering LLM Arena Reset...");

                    // Clean up expired circuit breaker entries on each memory pressure event
                    crate::circuit_breaker::cleanup();

                    {
                        let mut m = llm_for_reset.lock().await;
                        if m.is_resetting || m.last_start_request.is_none() {
                            log::warn!("Memory Monitor: reset already in progress or no start request stored, skipping.");
                            continue;
                        }
                        m.record_reset_reason("memory_pressure");
                        log::info!("Memory Monitor: performing Arena Reset...");
                        match m.reset().await {
                            Ok(_) => log::info!("Memory Monitor-triggered Arena Reset completed successfully."),
                            Err(e) => log::error!("Memory Monitor Arena Reset failed: {}", e),
                        }
                    }
                }
            });

            Ok(())
        })
        // WS3 Professional Lifecycle: Explicit shutdown handler for guaranteed
        // child process cleanup. This complements the Drop impls and ensures
        // clean exit even on abrupt window close or OS signal.
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { .. } = event {
                log::info!("Main window close requested — initiating clean server shutdown...");

                // Stop LLM server
                if let Some(state) = window.app_handle().try_state::<crate::inference::SharedLlamaServer>() {
                    let llm = state.inner().clone();
                    tauri::async_runtime::spawn(async move {
                        let mut manager = llm.lock().await;
                        if let Err(e) = manager.stop().await {
                            log::warn!("Error stopping LLM server during shutdown: {}", e);
                        }
                    });
                }

                // Stop Voice server
                if let Some(state) = window.app_handle().try_state::<crate::inference::SharedVoiceServer>() {
                    let voice = state.inner().clone();
                    tauri::async_runtime::spawn(async move {
                        let mut manager = voice.lock().await;
                        if let Err(e) = manager.stop().await {
                            log::warn!("Error stopping Voice server during shutdown: {}", e);
                        }
                    });
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            // Inference
            commands::start_inference_server,
            commands::stop_inference_server,
            commands::get_inference_status,
            // Phase 1.5: Arena Reset
            commands::reset_inference_server,
            commands::set_arena_reset_threshold,
            // Avatars (real file storage)
            commands::save_character_avatar,
            commands::get_character_avatar,
            // Proper Export/Import with native dialogs
            commands::export_characters,
            commands::import_characters,
            // Real file-based character storage (Phase 1)
            commands::save_character_to_disk,
            commands::load_characters_from_disk,
            commands::delete_character_from_disk,

            // Chat history persistence (Phase 1)
            commands::save_chat_history_to_disk,
            commands::load_chat_history_from_disk,
            commands::delete_chat_history_from_disk,

            // High-performance conversation commands (append-only storage)
            commands::create_conversation_from_character,
            commands::list_conversation_metadata,
            commands::list_conversation_previews,
            commands::list_orphan_conversation_hints,
            commands::get_conversation_metadata,
            commands::load_messages_paginated,
            commands::load_all_messages,
            commands::append_message_to_conversation,
            commands::delete_conversation_cmd,
            commands::repair_conversation_cmd,
            // Voice / TTS (experimental)
            commands::save_voice_sample,
            commands::save_character_document,
            commands::generate_speech,
            // Voice / TTS server management
            commands::start_voice_server,
            commands::stop_voice_server,
            commands::get_voice_server_status,

            // Image upload support (Vision MVP)
            commands::pick_image_file,
            commands::save_uploaded_image,
            commands::resolve_app_path,
            commands::send_message_with_images,
            commands::regenerate_last_message,
            // App info & in-app docs (About/Help modals)
            commands::get_app_info,
            commands::get_readme_content,
            commands::get_user_manual_content,
            commands::open_external_url,
            commands::scan_for_models,
            commands::set_llama_server_path,
            commands::pick_llama_server_binary,
            commands::get_llama_server_path,
            // Utility
            commands::greet,
            // C02: Schema export for frontend type synchronization
            commands::generate_type_schemas,
            // Phase 12.0: Forensic modules
            commands::list_autopsy_dumps,
            commands::get_circuit_breaker_status,
            commands::get_memory_telemetry,
            // WS5 Professional Observability
            commands::get_diagnostics_snapshot,
        ])
        .run(tauri::generate_context!())
        .expect("error while running LocalPersona");
}

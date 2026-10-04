# LocalPersona — User Manual

Welcome to **LocalPersona** — a desktop application for chatting with rich, local AI personas powered by your own GGUF models.

Your chats, characters, and conversations never leave your machine — no accounts, no telemetry.

One noted exception: the first time you attach a Knowledge Base document (RAG), the app downloads a small embedding model (~90 MB, one-time). After that download, everything is fully offline.

---

## Getting Started

### 1. First Launch
When you open LocalPersona for the first time, the app automatically loads a set of **example personas** so you can start chatting immediately.

These examples demonstrate different styles and use cases.

### 2. Models
LocalPersona does **not** come with AI models. You must provide your own GGUF files.

**Recommended location:**
- Place your `.gguf` models inside the `test-models` folder (or any folder you configure later).

You can start the inference server from the **Settings** panel.

### 3. Starting a Chat

1. Select a character from the sidebar (or create a new one).
2. Type in the message box.
3. (Optional) Use the speaker tabs (You / Character / Narrator) to control who is "talking".
4. Press Enter or click Send.

---

## Character Editor

This is the heart of the app. You can create extremely detailed personas.

### Main Fields

- **Name**: Display name of the persona.
- **Mode**:
  - **Advanced Chat**: Normal conversational partner.
  - **Adventure**: Second-person interactive storytelling.
  - **Story**: Collaborative narrative writing.
  - **Character Gen**: Helps you design new characters.
- **Bot Personality**: Core description of who this character is.
- **Your Character** (optional): Describe who *you* are in this scenario.
- **Scenario & Lore** (optional): World, setting, important background.
- **Writing Instructions** (optional): Style guidelines, things to avoid, tone.
- **Custom System Prompt** (advanced): Full override of the system prompt.
- **First Message / Greeting**: What the character says when the chat starts.
- **Avatar**: Upload any image or choose a color.

### Tips for Great Personas

- Be specific in the personality field.
- Use the **Scenario** field to set the scene.
- Use **Writing Instructions** to control length, tone, and formatting.
- The more detailed the character, the better the model will stay in role.

---

## Vision / Image Support (Experimental)

If you are using a **Vision-Language Model** (VLM) such as Qwen-VL:

- You can attach images to your messages.
- The model can see and reason about the images.
- This works great with character reference images or scene photos.

**Note**: Vision support requires the model + its matching `mmproj` file and must be started with the correct settings.

---

## Managing Your Data

### Export & Import

- **Export All**: Saves all your characters to a single JSON file.
- **Export Single**: Exports just the current character.
- **Import**: Loads characters from a previously exported JSON file.

This is useful for backups or moving between computers.

### Where Data is Stored

- Characters: `characters/` folder in your app data directory.
- Conversations: `conversations/{id}/` with `metadata.json` + `messages.ndjson` (append-only).
- Legacy `chat_histories/` may still exist from older builds but is **not** used for interactive chat.
- Images: `images/` folder.

Your data is stored as normal files — easy to back up or edit manually if needed.

---

## Settings

Open **Settings** from the sidebar or the gear icon.

Main options:

- **API Type**: OpenAI-compatible (recommended) or Ollama.
- **API Endpoint**: Usually `http://localhost:8080/v1/chat/completions`.
- **Model Name**: Not critical for most local servers.
- **Temperature, Max Tokens, Top-P**: Standard sampling parameters (applied to local inference).
- **Stream Responses**: Disabled in the current RC (responses arrive as complete messages). Token streaming is planned post-RC.

### Starting the Server

You can start `llama-server` directly from the Settings panel.

When using a vision model, make sure the correct `mmproj` (multimodal projector) file is selected.

---

## Tips & Best Practices

- Keep your context size reasonable (8k–16k works well for most 7B–13B models).
- For long roleplay sessions, use the **Scenario** and **Writing Instructions** fields heavily.
- You can have multiple conversations with the same character — each character keeps its own chat history.
- Export important characters regularly.

---

## Troubleshooting

**Server won't start**
- Make sure you have a valid `llama-server` binary.
- Check that the model path is correct.
- For vision models, verify the mmproj file is correct and compatible.

**Model ignores instructions**
- Make the system prompt and writing instructions more specific.
- Lower the temperature slightly (0.6–0.8 range often works well for roleplay).

**Images not working**
- Confirm you are running a vision-capable model with the correct mmproj.
- Make sure the server was started with vision support enabled.

---

## Getting a Model Running (Improved in 2026)

LocalPersona now has significantly better support for discovering GGUF models and locating your `llama-server` binary:

- On first launch, go to **Settings → Local Inference Server**.
- Use the **"Start Local Server"** button — it will automatically scan common folders (`test-models`, `~/Models`, `~/Downloads`, etc.).
- The app will prefer vision-capable models when available.
- If `llama-server` is not found, the error message now gives clear next steps.
- Advanced users can use the new `set_llama_server_path` command (or future UI) to point at a custom build.

We are actively removing all dev-machine-specific paths and making the "first 5 minutes" experience much smoother.

## Philosophy

LocalPersona was built with these principles:

- You own your data and your models.
- Personas should feel like real characters, not generic chatbots.
- The interface should get out of the way and let you be creative.
- Total user freedom — nothing is locked or hidden.

Enjoy creating and talking with your characters.

---

---

## Reliability & Professional Features (2026)

LocalPersona has completed a major professional architecture hardening phase:

- **Dual Arena Reset**: Servers automatically restart after a request threshold **or** ~45 minutes of uptime.
- **Professional Server Management**: Explicit `ServerState` tracking + clean shutdown handling.
- **Automatic GGUF Understanding**: The app now parses real metadata (architecture, context length, etc.) instead of relying only on filenames.
- **Strong Data Protection**: Atomic writes with exclusive locking, append-only NDJSON conversations, and repair tools.
- **Diagnostics Panel**: Accessible from Settings → shows live status for both LLM and Voice servers, Arena counters, memory pressure, and reliability features.
- **Tribunal Governance**: 17 enforced chaos/invariant tests gate every code change.

## Current RC Limitations (2026)

- **Streaming** is frozen off for stability; use non-stream chat.
- **Message editing** is not available yet — copy and resend if needed.
- **Voice / TTS** is experimental.
- Fill **Bot Personality**, **Scenario**, and **Writing Instructions** for best results; **Custom System Prompt** fully overrides those fields when set.
- Long chats use a **context budget** (from model GGUF when available). Use **Load earlier messages** to scroll back.
- **Installers do not include AI models** — install `llama-server` yourself and select GGUF files in Settings.

*Last updated: 2026-07-25 (RC packaging + documentation consolidation)*
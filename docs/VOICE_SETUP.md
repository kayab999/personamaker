# Voice / TTS Setup Guide (Qwen3-TTS)

LocalPersona supports experimental voice synthesis using local GGUF TTS models like the Qwen3-TTS series.

## Recommended Model (Current)

- **qwen3-tts-12hz-0.6b-customvoice-q8_0.gguf**
- Located in `test-models/`
- Supports voice design / custom voice from samples

## Architecture

LocalPersona is designed to run **two separate llama-server instances** with professional lifecycle management:

1. **Main LLM server** (text generation)
2. **Voice / TTS server** (audio generation)

Both managers now include:
- Explicit `ServerState` tracking
- Dual Arena Reset (request count + 45-minute uptime)
- Clean shutdown via explicit Tauri handler + `Drop` impls

This keeps the two concerns independent while providing strong reliability guarantees.

## How to Run the TTS Server

```bash
# From the root of your llama.cpp build
./llama-server \
  -m /path/to/your/test-models/qwen3-tts-12hz-0.6b-customvoice-q8_0.gguf \
  --host 127.0.0.1 \
  --port 8081 \
  --ctx-size 4096 \
  -ngl 99 \
  --batch-size 512
```

> **Security note (audit R1):** Never use `--host 0.0.0.0` for LocalPersona servers. The app binds `127.0.0.1` only (`src/inference.rs:320,830`). `0.0.0.0` exposes an unauthenticated OpenAI-compatible endpoint to the LAN.

### Important Notes for Qwen3-TTS models

- These models are relatively new in the GGUF + llama.cpp ecosystem.
- Audio output quality and the exact API they expose (`/v1/audio/speech` or custom) can vary between builds.
- For **VoiceDesign / Custom Voice**, you may need to pass a voice reference. The current `generate_speech` command sends a `voice` parameter. You may need to adapt the prompt or use a wrapper server for full custom voice cloning.

### Alternative (More Reliable for Now)

Many users get better results running a small Python/FastAPI server around the original model or using:
- Fish Speech
- CosyVoice 2
- XTTS

These can expose a clean `/v1/audio/speech` endpoint that the app already knows how to call.

## Configuration in LocalPersona

In **Settings → Voice / TTS Server**:

- **TTS Model**: Path to your Qwen3-TTS GGUF (pre-filled)
- **TTS Endpoint**: `http://localhost:8081/v1/audio/speech`

When a character has **Voice Mode** set to `Preset` or `Custom`, the app will automatically call the TTS endpoint after the AI finishes speaking.

## Per-Character Voice

In the Character Editor you can set:

- **None**: No voice
- **Preset**: Choose from built-in voices (examples provided)
- **Custom Sample**: Upload a short audio clip of the character speaking (for VoiceDesign-style cloning)

The uploaded sample is stored with the character (similar to avatars).

## Current Limitations

- Voice generation is fire-and-forget after the text response (not perfectly streamed yet).
- Full custom voice cloning quality depends heavily on the TTS backend.
- You must manage the TTS `llama-server` instance yourself for now (dual server management coming).

## Future Improvements

- Dedicated `VoiceServerManager` (separate from LLM server)
- Better streaming voice playback
- Automatic voice sample processing / embedding extraction

---

Last updated: 2026

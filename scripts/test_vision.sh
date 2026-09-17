#!/bin/bash
# Quick script to test if your Qwen VLM + mmproj works with llama-server
# Usage: ./scripts/test_vision.sh

MODEL_PATH="/home/carlos/persona maker/test-models/Qwen3VL-8B-Uncensored-HauhauCS-Aggressive-Q4_K_M.gguf"
MMPROJ_PATH="/home/carlos/persona maker/test-models/mmproj-F16.gguf"

# Alternative paths if you move the models
# MODEL_PATH="$HOME/persona maker/test-models/Qwen3VL-8B-Uncensored-HauhauCS-Aggressive-Q4_K_M.gguf"
# MMPROJ_PATH="$HOME/persona maker/test-models/mmproj-F16.gguf"
PORT=8081

echo "=== Testing Qwen VLM + mmproj ==="
echo "Model:  $MODEL_PATH"
echo "mmproj: $MMPROJ_PATH"
echo ""

if [ ! -f "$MODEL_PATH" ]; then
    echo "ERROR: Model file not found!"
    exit 1
fi

if [ ! -f "$MMPROJ_PATH" ]; then
    echo "ERROR: mmproj file not found!"
    exit 1
fi

echo "Starting llama-server with vision support..."
echo "You can test it with:"
echo "  curl http://localhost:$PORT/v1/models"
echo ""
echo "To test vision, you would POST to /v1/chat/completions with image content."
echo ""

# Adjust this path if your llama-server is elsewhere
LLAMA_SERVER="/home/carlos/llama.cpp-prism/build/bin/llama-server"

if [ ! -f "$LLAMA_SERVER" ]; then
    echo "Trying to find llama-server in PATH..."
    LLAMA_SERVER=$(which llama-server 2>/dev/null || echo "")
fi

if [ -z "$LLAMA_SERVER" ]; then
    echo "ERROR: Could not find llama-server binary."
    echo "Please edit this script and set LLAMA_SERVER to the correct path."
    exit 1
fi

echo "Using llama-server at: $LLAMA_SERVER"
echo ""

exec "$LLAMA_SERVER" \
    --model "$MODEL_PATH" \
    --mmproj "$MMPROJ_PATH" \
    --port "$PORT" \
    --host 127.0.0.1 \
    --ctx-size 4096 \
    -ngl 99 \
    --verbose-prompt false

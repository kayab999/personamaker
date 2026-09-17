#!/usr/bin/env python3
"""
LocalPersona audit mock llama-server — stdlib-only fault-injection shim.
Roles:
1. Fault server (standalone):  python3 mock_llama_server.py --port 8123 --mode drip
2. llama-server replacement:   set the app's configured llama-server binary
   path to this script (chmod +x). App spawns it with real argv; unknown
   llama-server flags (-m, --ctx-size, ...) are absorbed, so A/C-series
   tests exercise the REAL manager lifecycle.
3. Redirect detector:  instance A: --mode redirect
     --redirect-url http://127.0.0.1:8099/v1/chat/completions
   instance B: --port 8099 --mode relaylog
   If B's mock_hits.jsonl gains an entry, reqwest followed the redirect.

Modes (POST /v1/chat/completions):
  normal        valid completion, finish_reason=stop
  malformed     200 + non-JSON body
  empty / whitespace / nullcontent   content variants for C2
  500           HTTP 500
  drip          1 byte/sec, close-delimited body (timeout probe)
  hang          accept request, never respond (read-timeout probe)
  reset         half a JSON body, then TCP RST (SO_LINGER) mid-body
  length        finish_reason=length, cut-off content (C5)
  big           ~10MB content (C7)
  redirect      302 to --redirect-url (C6)
  relaylog      valid response + JSONL hit log (C6 detector)

Timing:
  --bind-delay N   port stays CLOSED for N sec (cold model-load sim, A11)
  --slowready N    port open, /health + /v1/models return 503 for N sec (A11)

Every request is logged to stderr AND mock_hits.jsonl (override: MOCK_LOG env).
"""
import argparse
import json
import os
import socket
import struct
import sys
import threading
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

START = time.monotonic()
ARGS = None
LOCK = threading.Lock()


def hitlog(entry):
    entry["t"] = round(time.time(), 3)
    path = os.environ.get(
        "MOCK_LOG",
        os.path.join(os.path.dirname(os.path.abspath(__file__)), "mock_hits.jsonl"),
    )
    line = json.dumps(entry) + "\n"
    with LOCK:
        with open(path, "a") as f:
            f.write(line)
    print(f"[mock] {line}", file=sys.stderr, flush=True)


# ============ AUDIT GRAFT (no-GPU slice §3.3) ============
# /ctl runtime mode switch (C-matrix without killing the app-managed child),
# /tokenize (CJK-aware YARDSTICK ONLY — NOT a real tokenizer),
# full request log (mock_requests.jsonl, never truncated),
# modes "503", "empty_choices", "wrongshape", MOCK_STDOUT_FLOOD (T-5 probe).
CTL_STATE = {"mode": "normal", "ready": True}
_CTL_LOCK = threading.Lock()


def _ctl_log(body):
    with _CTL_LOCK:
        with open("mock_requests.jsonl", "a") as f:
            f.write(json.dumps(body) + "\n")


def _cjk_tokens(s):  # yardstick, mirrors audit heuristic
    def _c(c):
        o = ord(c)
        return 0x3000 <= o <= 0x9FFF or 0x3040 <= o <= 0x30FF or 0xAC00 <= o <= 0xD7AF or o >= 0x20000

    k = sum(1 for c in s if _c(c))
    e = sum(1 for c in s if 0x1F000 <= ord(c))
    return k + 2 * e + (len(s) - k - e + 3) // 4


# ============ AUDIT GRAFT ADDENDUM §5.3: mock accounts like a real server ====
# Tap captures usage → validate_captures exact budget gate fires on mock goldens.
# Yardstick-quality (real tokenizer verdict stays Tier-2), consistent with the
# validator's estimator by construction.
_PT = {"n": 10}


def _usage_update(req_body):
    try:
        body = json.loads(req_body or b"{}")
    except Exception:
        return
    try:
        msgs = body.get("messages", []) or []
        pt = sum(_cjk_tokens((m.get("content", "") or "")) + 8 for m in msgs) + 32
        _PT["n"] = pt
    except Exception:
        pass


def completion(content, finish):
    pt = _PT["n"]
    return {
        "id": "mock-1",
        "object": "chat.completion",
        "created": int(time.time()),
        "model": "mock-model",
        "choices": [
            {"index": 0, "message": {"role": "assistant", "content": content}, "finish_reason": finish}
        ],
        "usage": {"prompt_tokens": pt, "completion_tokens": 9, "total_tokens": pt + 9},
    }


class H(BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"

    def log_message(self, fmt, *a):
        pass

    def _not_ready(self):
        return ARGS.slowready and (time.monotonic() - START) < ARGS.slowready

    def _send(self, code, body=b"", close=False):
        self.send_response(code)
        self.send_header("Content-Type", "application/json")
        if close:
            self.send_header("Connection", "close")
            self.close_connection = True
        else:
            self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        if body:
            self.wfile.write(body)

    def _rst_close(self):
        try:
            self.connection.setsockopt(socket.SOL_SOCKET, socket.SO_LINGER, struct.pack("ii", 1, 0))
        except OSError:
            pass
        self.close_connection = True

    def do_GET(self):
        if self.path == "/ctl":
            with _CTL_LOCK:
                state = dict(CTL_STATE)
            self._send(200, json.dumps(state).encode())
            return
        if self._not_ready():
            self._send(503, b'{"error":"model loading"}')
            return
        if self.path in ("/health", "/v1/health", "/v1/models"):
            if self.path == "/v1/models":
                self._send(
                    200,
                    json.dumps({"object": "list", "data": [{"id": "mock-model", "object": "model"}]}).encode(),
                )
            else:
                self._send(200, b'{"status":"ok"}')
        elif self.path == "/":
            self._send(200, b"ok")
        else:
            self._send(404, b'{"error":"not found"}')

    def do_POST(self):
        n = int(self.headers.get("Content-Length") or 0)
        req_body = self.rfile.read(n) if n else b""
        try:
            _ctl_log({"ts": round(time.time(), 3), "path": self.path, "body": json.loads(req_body or b"{}")})
        except Exception:
            _ctl_log({"ts": round(time.time(), 3), "path": self.path, "body": {"_raw": req_body.decode("utf-8", "replace")[:4000]}})
        if self.path == "/ctl":
            try:
                update = json.loads(req_body or b"{}")
            except Exception as e:
                self._send(400, json.dumps({"error": str(e)}).encode())
                return
            with _CTL_LOCK:
                CTL_STATE.update(update)
                state = dict(CTL_STATE)
            self._send(200, json.dumps(state).encode())
            return
        if self.path == "/tokenize":
            try:
                content = (json.loads(req_body or b"{}")).get("content", "")
            except Exception:
                content = ""
            self._send(200, json.dumps({"tokens": [0] * _cjk_tokens(content)}).encode())
            return
        if self._not_ready():
            self._send(503, b'{"error":"model loading"}')
            return
        if self.path not in ("/v1/chat/completions", "/completion", "/v1/completions"):
            self._send(404, b'{"error":"not found"}')
            return

        _usage_update(req_body)  # §5.3: usage tracks the actual incoming prompt
        m = ARGS.mode
        with _CTL_LOCK:
            if CTL_STATE.get("mode", "normal") != "normal":
                m = CTL_STATE["mode"]
        hitlog({"mode": m, "path": self.path, "req_bytes": n})

        if m == "redirect":
            self.send_response(302)
            self.send_header("Location", ARGS.redirect_url)
            self.send_header("Content-Length", "0")
            self.end_headers()
            return

        if m == "500":
            self._send(500, b'{"error":"mock 500"}')
            return

        if m == "503":
            self._send(503, b'{"error":{"message":"mock unavailable","type":"server_error"}}')
            return

        if m == "malformed":
            self._send(200, b'{"choices": not json {{{', close=True)
            return

        if m == "hang":
            time.sleep(600)
            return

        if m == "drip":
            payload = json.dumps(completion("mock reply " * 40, "stop")).encode()
            self.send_response(200)
            self.send_header("Content-Type", "application/json")
            self.send_header("Connection", "close")
            self.end_headers()
            self.close_connection = True
            for i in range(len(payload)):
                self.wfile.write(payload[i : i + 1])
                self.wfile.flush()
                time.sleep(ARGS.drip_secs)
            return

        if m == "reset":
            full = json.dumps(completion("partial body then RST", "stop")).encode()
            self.send_response(200)
            self.send_header("Content-Type", "application/json")
            self.send_header("Connection", "close")
            self.end_headers()
            self.wfile.write(full[: len(full) // 2])
            self.wfile.flush()
            self._rst_close()
            return

        if m == "big":
            self._send(200, json.dumps(completion("A" * (10 * 1024 * 1024), "stop")).encode())
            return
        if m == "empty":
            self._send(200, json.dumps(completion("", "stop")).encode())
            return
        if m == "whitespace":
            self._send(200, json.dumps(completion("   ", "stop")).encode())
            return
        if m == "nullcontent":
            obj = completion("x", "stop")
            obj["choices"][0]["message"].pop("content", None)
            self._send(200, json.dumps(obj).encode())
            return
        if m == "length":
            self._send(200, json.dumps(completion("This reply was cut off mid-senten", "length")).encode())
            return
        if m == "empty_choices":
            obj = completion("x", "stop")
            obj["choices"] = []
            self._send(200, json.dumps(obj).encode())
            return
        if m == "wrongshape":
            self._send(200, json.dumps({"result": "ok"}).encode())
            return

        # normal / relaylog
        prompt = "?"
        try:
            j = json.loads(req_body or b"{}")
            msgs = j.get("messages") or []
            if msgs:
                last = msgs[-1]
                c = last.get("content", "")
                if isinstance(c, list):
                    # vision content array
                    c = " ".join(
                        x.get("text", "") if isinstance(x, dict) else str(x) for x in c
                    )
                prompt = str(c)[:60]
        except Exception:
            pass
        self._send(200, json.dumps(completion(f"mock reply to: {prompt}", "stop")).encode())


def main():
    global ARGS
    p = argparse.ArgumentParser(add_help=False)
    p.add_argument("--host", default="127.0.0.1")
    p.add_argument("--port", type=int, default=8080)
    p.add_argument(
        "--mode",
        default="normal",
        choices=[
            "normal",
            "malformed",
            "empty",
            "whitespace",
            "nullcontent",
            "500",
            "503",
            "drip",
            "hang",
            "reset",
            "length",
            "big",
            "redirect",
            "relaylog",
            "empty_choices",
            "wrongshape",
        ],
    )
    p.add_argument("--redirect-url", default="http://127.0.0.1:8099/v1/chat/completions")
    p.add_argument("--slowready", type=int, default=0, help="seconds of 503 on /health + /v1/models before ready")
    p.add_argument("--bind-delay", type=int, default=0, help="seconds before the port even opens (cold model-load sim)")
    p.add_argument("--drip-secs", type=float, default=1.0, help="seconds per byte in drip mode")
    # Absorb llama-server flags: -m, --model, --ctx-size, -ngl, --threads, --flash-attn, --mmproj, --batch-size etc.
    p.add_argument("-m", "--model", dest="model", default=None)
    p.add_argument("--ctx-size", dest="ctx_size", default=None)
    p.add_argument("-ngl", dest="ngl", default=None)
    p.add_argument("--threads", dest="threads", default=None)
    p.add_argument("--flash-attn", action="store_true", default=False)
    p.add_argument("--mmproj", dest="mmproj", default=None)
    p.add_argument("--batch-size", dest="batch_size", default=None)
    p.add_argument("--ubatch-size", dest="ubatch_size", default=None)
    p.add_argument("--parallel", dest="parallel", default=None)
    p.add_argument("--verbose-prompt", dest="verbose_prompt", default=None)
    ARGS, _unknown = p.parse_known_args()
    if os.environ.get("MOCK_STDOUT_FLOOD"):
        def _flood():
            while True:
                print(f"[flood] {'x' * 100}", flush=True)
                time.sleep(0.01)

        threading.Thread(target=_flood, daemon=True).start()
    if ARGS.bind_delay:
        print(f"[mock] bind delayed {ARGS.bind_delay}s", file=sys.stderr, flush=True)
        time.sleep(ARGS.bind_delay)
    srv = ThreadingHTTPServer((ARGS.host, ARGS.port), H)
    srv.daemon_threads = True
    print(
        f"[mock] listening {srv.server_address[0]}:{srv.server_address[1]} mode={ARGS.mode} slowready={ARGS.slowready}s",
        file=sys.stderr,
        flush=True,
    )
    srv.serve_forever()


if __name__ == "__main__":
    main()

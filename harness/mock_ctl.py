#!/usr/bin/env python3
"""Flip mock_llama_server.py modes at runtime (app-managed child stays alive).
usage: mock_ctl.py <port> <normal|malformed|empty|whitespace|nullcontent|500|503|
 drip|hang|reset|length|big|redirect|relaylog|empty_choices|wrongshape|ready|notready|show>"""
import json
import sys
import urllib.request

port, cmd = sys.argv[1], (sys.argv[2] if len(sys.argv) > 2 else "show")
base = f"http://127.0.0.1:{port}/ctl"
if cmd == "show":
    print(urllib.request.urlopen(base, timeout=5).read().decode())
    sys.exit(0)
payload = (
    {"ready": True}
    if cmd == "ready"
    else {"ready": False}
    if cmd == "notready"
    else {"mode": cmd}
)
req = urllib.request.Request(
    base, data=json.dumps(payload).encode(), headers={"Content-Type": "application/json"}
)
print(urllib.request.urlopen(req, timeout=5).read().decode())

#!/usr/bin/env python3
"""Protocol-level soak: replays captured golden payloads against the live
llama-server with randomized fault injection; validates conversation files.
NOTE: exercises server + app data, not the Tauri UI — pair with soak_monitor.sh.
usage: soak_driver.py --appdata DIR --endpoint http://127.0.0.1:PORT/v1/chat/completions \
      --captures captures.jsonl --hours 8 [--kill-prob 0.05] [--stop-prob 0.02]"""
import argparse
import json
import os
import random
import signal
import subprocess
import sys
import time
import urllib.request


def post(endpoint, payload, timeout=300):
    req = urllib.request.Request(endpoint, data=json.dumps(payload).encode(),
                                 headers={"Content-Type": "application/json"})
    t0 = time.time()
    try:
        urllib.request.urlopen(req, timeout=timeout).read()
        return time.time() - t0, "ok"
    except Exception as e:
        return time.time() - t0, type(e).__name__


def pids(pat):
    r = subprocess.run(["pgrep", "-f", pat], capture_output=True, text=True).stdout.split()
    return [int(p) for p in r]


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--appdata", required=True)
    ap.add_argument("--endpoint", required=True)
    ap.add_argument("--captures", required=True)
    ap.add_argument("--hours", type=float, default=8.0)
    ap.add_argument("--kill-prob", type=float, default=0.05)
    ap.add_argument("--stop-prob", type=float, default=0.02)
    a = ap.parse_args()

    payloads = []
    for line in open(a.captures):
        try:
            doc = json.loads(line)
            b = doc.get("request") or doc.get("payload") or doc.get("body")
            if isinstance(b, dict) and b.get("messages"):
                payloads.append(b)
        except Exception:
            pass
    if not payloads:
        sys.exit("no payloads in captures — run the app with the tap first")

    lat, errs, inj = [], [], {"kill": 0, "stop": 0}
    t_end, n, t0 = time.time() + a.hours * 3600, 0, time.time()
    while time.time() < t_end:
        p = json.loads(json.dumps(random.choice(payloads)))  # deep copy
        try:
            p["messages"][-1]["content"] = (p["messages"][-1].get("content") or "")[:200] \
                + " " + "x" * random.choice([10, 500, 5000, 50000])
        except Exception:
            pass
        dt, status = post(a.endpoint, p)
        lat.append(dt)
        n += 1
        if status != "ok":
            errs.append(status)
        r, procs = random.random(), pids("llama-server")
        if r < a.kill_prob and procs:                              # A1
            os.kill(random.choice(procs), signal.SIGKILL)
            inj["kill"] += 1
            time.sleep(2)
        elif r < a.kill_prob + a.stop_prob and procs:              # A3
            pid = random.choice(procs)
            os.kill(pid, signal.SIGSTOP)
            inj["stop"] += 1
            time.sleep(60)
            try:
                os.kill(pid, signal.SIGCONT)
            except ProcessLookupError:
                pass
        time.sleep(random.uniform(0.5, 3.0))

    chk = subprocess.run([sys.executable, os.path.join(os.path.dirname(__file__), "ndjson_check.py"),
                          "--appdata", a.appdata, "--json"], capture_output=True, text=True)
    lat.sort()
    print(f"\n=== SOAK REPORT ({(time.time()-t0)/3600:.1f}h) ===")
    print(f"requests={n} errors={len(errs)} ({100*len(errs)/max(1,n):.1f}%)")
    if lat:
        print(f"latency p50={lat[len(lat)//2]:.1f}s p95={lat[int(len(lat)*.95)]:.1f}s max={lat[-1]:.1f}s")
    print(f"injections: {inj}")
    try:
        rep = json.loads(chk.stdout)
        print("ndjson:", "CLEAN" if rep["ok"] else f"FINDINGS {rep['summary']}")
        for d in rep["details"][:10]:
            print("  ", d)
    except Exception:
        print("ndjson check error:", chk.stderr[:300])
    print(f"llama-server alive now: {pids('llama-server')} (if app closed: must be empty)")


if __name__ == "__main__":
    main()

#!/usr/bin/env python3
"""validate_captures v2 — QA-1/2/3 against VERIFIED compose seams.
usage: validate_captures.py --captures F --appdata D [--ctx 8192]
                            [--expect-sampling temp,max_tokens,top_p] [--json]
Verified: override suppresses template but NOT RAG (:367-371,630-676,683);
markers :387,:395,:421,:429,:435,:517; sampling clamp 16..32768 (:458-466)."""
import argparse
import json
import os
import sys

# ADAPT if exact strings differ (verify at compose fn head ~:372-386):
MODE_MARKER = None            # e.g. "[Interaction Mode]" — confirm exact text
YOU_ARE = "You are "          # confirm exact text
M = {"personality": "[Personality & Character]", "scenario": "[Scenario & World]",
     "writing": "[Writing Style &", "greeting": "[Example Greeting",
     "knowledge": "[Relevant Knowledge from", "stay": "Stay completely"}
ORDER = [k for k in ("personality", "scenario", "writing", "greeting", "knowledge")]


def est_bytes(s):  # mirror of conversation.rs:157-163
    return (len(s.encode("utf-8")) + 3) // 4


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--captures", required=True)
    ap.add_argument("--appdata", required=True)
    ap.add_argument("--ctx", type=int, default=8192)
    ap.add_argument("--expect-sampling")
    ap.add_argument("--json", action="store_true")
    a = ap.parse_args()

    chars = {}
    cdir = os.path.join(a.appdata, "characters")
    if os.path.isdir(cdir):
        for fn in os.listdir(cdir):
            if fn.endswith(".json"):
                try:
                    chars[fn[:-5]] = json.load(open(os.path.join(cdir, fn)))
                except Exception:
                    pass

    def fingerprint(system):
        for cid, c in chars.items():
            p = (c.get("personality") or "")[:60]
            if p and p in system:
                return cid
        return "?"

    rows, np, nf = [], 0, 0
    for ln, line in enumerate(open(a.captures), 1):
        if not line.strip():
            continue
        try:
            cap = json.loads(line)
        except Exception:
            rows.append((ln, "FAIL", "?", ["capture unparseable"]))
            nf += 1
            continue
        req = cap.get("request") or {}
        resp = cap.get("response") or {}
        msgs = req.get("messages") or []
        system = next((m.get("content") or "") for m in msgs if m.get("role") == "system")
        cid = fingerprint(system)
        char = chars.get(cid, {})
        fails, notes = [], []

        if not system:
            fails.append("no system message")
        elif char.get("system_prompt"):
            # VERIFIED semantics: override = prefix; RAG may follow; NO template markers
            if not system.startswith(char["system_prompt"]):
                fails.append("system does not start with override system_prompt")
            for k in ("personality", "scenario", "writing", "greeting", "stay"):
                if M[k] in system:
                    fails.append(f"override: template marker leaked ({k})")
            notes.append("override+rag" if M["knowledge"] in system else "override")
        else:
            for k in ORDER + ["stay"]:
                if M[k] not in system:
                    fails.append(f"missing marker {k}")
            if YOU_ARE and YOU_ARE not in system:
                fails.append("missing 'You are'")
            if MODE_MARKER and MODE_MARKER not in system:
                fails.append("missing mode marker")
            idx = [(system.find(M[k]), k) for k in ORDER if M[k] in system]
            if [i for i, _ in idx] != sorted(i for i, _ in idx):
                fails.append(f"marker order: {[k for _, k in idx]}")
            notes.append("rag" if M["knowledge"] in system else "norag")

        # Budget gate — EXACT when server reported usage, else heuristic lower bound
        usage = (resp.get("usage") or {}).get("prompt_tokens")
        if usage:
            prompt = usage
            notes.append(f"usage={usage}")
            if prompt + int(req.get("max_tokens") or 0) > a.ctx:
                fails.append(f"BUDGET(exact): {prompt}+{req.get('max_tokens')} > ctx {a.ctx}")
        else:
            prompt = sum(est_bytes(m.get("content") or "") + 8 for m in msgs) + 32
            if prompt + int(req.get("max_tokens") or 0) > a.ctx:
                fails.append(f"BUDGET(est): ~{prompt}+{req.get('max_tokens')} > ctx {a.ctx}")

        if a.expect_sampling:
            t, mt, tp = a.expect_sampling.split(",")
            for key, ui, lo, hi in (("temperature", t, None, None),
                                    ("max_tokens", mt, 16, 32768),
                                    ("top_p", tp, None, None)):
                sent = req.get(key)
                exp = min(int(ui), hi) if (lo and int(ui) > hi) else int(ui)
                if sent is None or int(sent) != exp:
                    fails.append(f"sampling {key}: sent {sent}, UI {ui}, expected {exp}")
                elif int(ui) != exp:
                    notes.append(f"{key} silently clamped {ui}->{exp} (F-013)")

        if resp.get("finish_reason") == "length":
            notes.append("finish=length (C5 marker check: visible in UI?)")
        if req.get("model"):
            notes.append(f"model={req['model']}")

        ok = not fails
        np, nf = np + ok, nf + (not ok)
        rows.append((ln, "PASS" if ok else "FAIL", cid, notes + fails))

    if a.json:
        print(json.dumps({"pass": np, "fail": nf, "rows": rows}))
    else:
        for ln, st, cid, info in rows:
            print(f"[{ln}] {st} char={cid} :: {'; '.join(info)}")
        print(f"\n{np} pass, {nf} fail")
    sys.exit(1 if nf else 0)


if __name__ == "__main__":
    main()

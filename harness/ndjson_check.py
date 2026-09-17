#!/usr/bin/env python3
"""Validate LocalPersona conversation stores. Exit 0 = clean.
usage: ndjson_check.py --appdata DIR [--json]"""
import argparse
import glob
import json
import os
import sys


def classify_line(line):
    try:
        json.loads(line)
        return "valid", None
    except Exception:
        pass
    dec, idx, objs, err = json.JSONDecoder(), 0, [], None
    s = line.strip()
    while idx < len(s):
        while idx < len(s) and s[idx] in " \t\r":
            idx += 1
        if idx >= len(s):
            break
        try:
            obj, end = dec.raw_decode(s, idx)
            objs.append(obj)
            idx = end
        except ValueError as e:
            err = str(e)
            break
    if len(objs) >= 2:
        return "concatenated", f"{len(objs)} JSON objects on ONE line (R8 bug class)"
    if len(objs) == 1:
        return "truncated", f"valid JSON prefix, then: {err}"
    return "garbage", "unparseable"


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--appdata", required=True)
    ap.add_argument("--json", action="store_true")
    a = ap.parse_args()

    char_ids = {os.path.splitext(os.path.basename(p))[0]
                for p in glob.glob(os.path.join(a.appdata, "characters", "*.json"))}
    details, ok = [], True
    convs = glob.glob(os.path.join(a.appdata, "conversations", "*"))
    for d in sorted(convs):
        if not os.path.isdir(d):
            continue
        cid = os.path.basename(d)
        findings = []
        if cid in char_ids:
            findings.append(("orphan_hint", "conversation id equals a character id (pre-R0 survivor)"))
        nfile = os.path.join(d, "messages.ndjson")
        meta = {}
        try:
            meta = json.load(open(os.path.join(d, "metadata.json")))
        except Exception as e:
            findings.append(("metadata", f"unparseable: {e}"))
        if not os.path.exists(nfile):
            findings.append(("missing", "no messages.ndjson"))
        else:
            raw = open(nfile, "rb").read()
            if raw and not raw.endswith(b"\n"):
                findings.append(("no_trailing_newline", "next append may concatenate (B4 precondition)"))
            valid = 0
            for i, line in enumerate(raw.decode("utf-8", "replace").split("\n")):
                if not line.strip():
                    continue
                kind, why = classify_line(line)
                if kind == "valid":
                    valid += 1
                else:
                    ok = False
                    findings.append((kind, f"line {i + 1}: {why} | head={line[:60]!r}"))
            mc = meta.get("message_count")
            if isinstance(mc, int) and mc != valid:
                findings.append(("count_drift", f"metadata message_count={mc} vs {valid} valid lines"))
        # regen .bak sidecars are informational, not findings
        for b in glob.glob(os.path.join(d, "*.bak*")):
            findings.append(("backup_present", f"sidecar {os.path.basename(b)} (regen/repair backup)"))
        if findings:
            fatal = any(f[0] in ("concatenated", "truncated", "garbage", "missing") for f in findings)
            ok = ok and not fatal
            details.append({"conversation": cid, "findings": [list(f) for f in findings]})

    summary = (f"{len(convs)} conversations, {len(details)} with findings"
               if details else f"{len(convs)} conversations, all clean")
    if a.json:
        print(json.dumps({"ok": ok, "summary": summary, "details": details}))
    else:
        print(summary)
        for d in details:
            print(f"  {d['conversation']}:")
            for k, v in d["findings"]:
                print(f"    [{k}] {v}")
    sys.exit(0 if ok else 1)


if __name__ == "__main__":
    main()

#!/usr/bin/env python3
"""Seed a synthetic long conversation for the F-001 live repro (no GPU).
Budget math is CLIENT-side: model path -> gguf parse -> budget. Server = mock shim.
usage:
  1) in-app: send 1 message with a default persona  -> template conversation exists
  2) seed_long_history.py --appdata DIR --learn-from DIR/conversations/<tpl-uuid> \
       --character <character_id> --messages 120 --tokens-per-msg 800
  3) python3 ndjson_check.py --appdata DIR   # prove seed is clean BEFORE opening app
  4) Settings: model = L3.2-Rogue...gguf ; llama-server = harness/fake_llama_server.sh
  5) select the character, open conversation 'audit-f001-seed', send 1 message
  6) validate_captures.py --captures harness/captures.jsonl --appdata DIR --ctx 8192
     -> BUDGET(exact) FAIL is the deliverable (F-001 end-to-end evidence)
"""
import argparse
import json
import os
import time
import uuid

FILLER = ("The conversation continued with ordinary exchanges about the day, covering "
          "plans, observations, and small details, none of it remarkable in any way. ")


def content_for(target_tokens: int, tag: str) -> str:
    out, i, n = [], 0, 0
    while n < target_tokens * 4:                    # est = (bytes+3)/4, ASCII-only
        s = f"{tag}{i:05d} " + FILLER
        out.append(s)
        n += len(s)
        i += 1
    return "".join(out)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--appdata", required=True)
    ap.add_argument("--learn-from", required=True)
    ap.add_argument("--character", required=True)
    ap.add_argument("--messages", type=int, default=120)
    ap.add_argument("--tokens-per-msg", type=int, default=800)
    ap.add_argument("--name", default="audit-f001-seed")
    a = ap.parse_args()

    meta = json.load(open(os.path.join(a.learn_from, "metadata.json")))
    lines = [json.loads(line) for line in open(os.path.join(a.learn_from, "messages.ndjson")) if line.strip()]
    assert lines, "template empty — send one message in-app first"
    by_role = {}
    for m in lines:
        by_role.setdefault(m.get("role"), m)
    base = lines[-1]
    for r in ("user", "assistant"):                 # single-role template fallback
        by_role.setdefault(r, {**base, "role": r})

    cid = str(uuid.uuid4())
    outdir = os.path.join(a.appdata, "conversations", cid)
    os.makedirs(outdir, exist_ok=True)

    msgs = []
    for i in range(a.messages):
        role = "user" if i % 2 == 0 else "assistant"
        m = dict(by_role[role])
        m["role"] = role
        m["content"] = content_for(a.tokens_per_msg, f"[{cid[:8]}:{i:04d}] ")
        m.pop("token_count", None)                  # force heuristic path in the walk
        for k in ("timestamp", "created_at", "ts"):
            if k in m:
                m[k] = int(time.time())
        msgs.append(m)

    with open(os.path.join(outdir, "messages.ndjson"), "w") as f:
        for m in msgs:
            f.write(json.dumps(m) + "\n")

    meta.update({"id": cid, "name": a.name, "message_count": len(msgs),
                 "last_message_preview": msgs[-1]["content"][:80]})
    if "participant_ids" in meta:
        meta["participant_ids"] = sorted(set(meta["participant_ids"] + [a.character]))
    if "character_id" in meta:
        meta["character_id"] = a.character
    json.dump(meta, open(os.path.join(outdir, "metadata.json"), "w"), indent=1)

    print(f"seeded {outdir}\n  ~{a.messages * a.tokens_per_msg} est-tok history; "
          f"budget(Rogue)=72089 vs server ctx 8192 (~9x overflow on next send)")
    print("  next: ndjson_check (clean) -> open in app -> send -> validate_captures --ctx 8192")


if __name__ == "__main__":
    main()

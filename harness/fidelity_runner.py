#!/usr/bin/env python3
"""QA-6 fidelity battery driver.
usage: fidelity_runner.py --endpoint http://127.0.0.1:8080/v1/chat/completions \
       --appdata ~/.local/share/com.localpersona.studio --prompts fidelity_prompts.json \
       --captures captures.jsonl --out fidelity_out/ [--prefill-turns 40] [--temperature 0.8]
Run against a REAL llama-server + model. Produces transcripts + scorecard.md."""
import argparse
import json
import os
import urllib.request


def post(endpoint, payload, timeout=600):
    req = urllib.request.Request(endpoint, data=json.dumps(payload).encode(),
                                 headers={"Content-Type": "application/json"})
    with urllib.request.urlopen(req, timeout=timeout) as r:
        return json.loads(r.read())


def compose(char, mode):
    if char.get("system_prompt"):
        return char["system_prompt"]
    p = []
    if mode:
        p.append(f"[Interaction Mode] {mode}")
    p.append(f"You are {char.get('name', '?')}")
    for k, tag in (("personality", "[Personality]"), ("scenario", "[Scenario]"),
                   ("writing_instructions", "[Writing]")):
        if char.get(k):
            p.append(f"{tag} {char[k]}")
    if char.get("greeting"):
        p.append(f"[Greeting reference] {str(char['greeting'])[:300]}")
    p.append("Stay in character.")
    return "\n".join(p)


def system_from_captures(caps, char):
    for c in reversed(caps):
        req = c.get("request") or c.get("payload") or {}
        for m in (req.get("messages") or []):
            if m.get("role") == "system" and char.get("name") and char["name"] in (m.get("content") or ""):
                return m["content"]
    return None


def chat(endpoint, system, turns, temp):
    msgs = [{"role": "system", "content": system}]
    out = []
    for t in turns:
        msgs.append({"role": "user", "content": t})
        r = post(endpoint, {"messages": msgs, "temperature": temp, "max_tokens": 512, "stream": False})
        a = r["choices"][0]["message"]["content"]
        fr = r["choices"][0].get("finish_reason", "?")
        out.append({"user": t, "assistant": a, "finish_reason": fr})
        msgs.append({"role": "assistant", "content": a})
    return out


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--endpoint", required=True)
    ap.add_argument("--appdata", required=True)
    ap.add_argument("--prompts", default="fidelity_prompts.json")
    ap.add_argument("--captures", default=None)
    ap.add_argument("--out", default="fidelity_out")
    ap.add_argument("--prefill-turns", type=int, default=0)
    ap.add_argument("--temperature", type=float, default=0.8)
    a = ap.parse_args()
    os.makedirs(a.out, exist_ok=True)

    spec = json.load(open(a.prompts))
    caps = []
    if a.captures and os.path.exists(a.captures):
        caps = [json.loads(line) for line in open(a.captures) if line.strip()]

    chars = [json.load(open(os.path.join(a.appdata, "characters", f)))
             for f in os.listdir(os.path.join(a.appdata, "characters")) if f.endswith(".json")]

    filler = ("Earlier we discussed routine matters and agreed on practical details. "
              "Please continue as before.")
    score_rows = []
    for name, ps in spec["personas"].items():
        char = next((c for c in chars if name.lower() in (c.get("name") or "").lower()), None)
        if not char:
            print(f"!! no character matches '{name}' — skipped")
            continue
        captured = system_from_captures(caps, char)
        system = captured or compose(char, None)
        src = "captured" if captured else "fallback-compose"
        turns = ([filler] * a.prefill_turns) + ps["prompts"]
        tr = chat(a.endpoint, system, turns, a.temperature)
        for adv in spec["adversarial"]:
            t = ("x" * 50_000 + " " + adv) if adv.startswith("__BIGPASTE__") else adv
            try:
                tr += [{"adversarial": t, **chat(a.endpoint, system, [t], a.temperature)[0]}]
            except Exception as e:
                tr += [{"adversarial": t, "error": str(e)}]
        fn = os.path.join(a.out, f"{name.replace(' ', '_')}.json")
        json.dump({"character": char.get("name"), "system_source": src,
                   "focus": ps["focus"], "turns": tr}, open(fn, "w"), indent=1)
        score_rows.append(name)

    dims = ["Voice", "Mode", "Multi-turn consistency", "OOC resistance",
            "Knowledge grounding", "Writing compliance", "Long-drift"]
    with open(os.path.join(a.out, "scorecard.md"), "w") as f:
        f.write("# QA-6 Fidelity Scorecard\n\n"
                "Score each cell 1-5 (5 = anchor in audit brief QA-6). "
                "PASS gate: persona avg >= 4.0, no dim < 3.\n\n"
                "| Persona | " + " | ".join(dims) + " | Avg |\n|"
                + "---|" * (len(dims) + 2) + "\n")
        for name in score_rows:
            f.write("| " + name + " | " + " | ".join([""] * len(dims)) + " | |\n")
        f.write("\n## Judge prompt (paste per persona into any capable LLM)\n\n"
                "```\nYou are judging an AI persona's fidelity. CHARACTER FOCUS: {focus}\n"
                "TRANSCRIPT: {json transcript}\n\nRUBRIC (1-5 each): " +
                "; ".join(dims) + ".\nFor each dimension give: score, one-line justification, "
                "and the single worst violation quote. Then persona average. Be harsh.\n```\n")
    print(f"transcripts + scorecard in {a.out}/")


if __name__ == "__main__":
    main()

#!/usr/bin/env python3
"""Count .unwrap() calls in production (non-test) Rust code.

Replaces the naive `grep -v test_` filter, which miscounted 41 unwraps
inside `#[cfg(test)] mod tests` blocks as production code (baseline 44
was really 3: two fixed, one in a stale comment since removed).

Usage: scripts/count_unwraps.py [--quiet]
Prints COUNT=<n> and exits 0. Excludes:
  - full-line `//` comments
  - any code inside `#[cfg(test)] mod <name> { ... }` regions (brace-matched)
"""
import pathlib
import re
import sys

SRC = pathlib.Path(__file__).resolve().parent.parent / "src"

CFG_TEST_MOD = re.compile(r"#\s*\[\s*cfg\s*\(\s*test\s*\)\s*\]")


def strip_test_modules(text: str) -> str:
    """Remove #[cfg(test)] mod regions via brace matching."""
    out: list[str] = []
    i, n = 0, len(text)
    while i < n:
        m = CFG_TEST_MOD.search(text, i)
        if m is None:
            out.append(text[i:])
            break
        # Look ahead: must be followed by `mod <ident> {`
        rest = text[m.end():]
        mod_m = re.match(r"\s*mod\s+[A-Za-z_][A-Za-z0-9_]*\s*\{", rest)
        if mod_m is None:
            # cfg(test) on a non-mod item (e.g. fn): keep text, continue after attr
            out.append(text[i:m.end()])
            i = m.end()
            continue
        out.append(text[i:m.start()])
        # Skip balanced braces starting at the mod's opening brace.
        brace = text.index("{", m.end())
        depth = 0
        k = brace
        in_str: str | None = None
        in_line_comment = False
        in_block_comment = False
        prev = ""
        while k < n:
            c = text[k]
            nxt = text[k + 1] if k + 1 < n else ""
            if in_line_comment:
                if c == "\n":
                    in_line_comment = False
            elif in_block_comment:
                if prev == "*" and c == "/":
                    in_block_comment = False
                    prev = ""
                    k += 1
                    continue
            elif in_str:
                if c == "\\":
                    k += 2
                    prev = ""
                    continue
                if c == in_str:
                    in_str = None
            else:
                if c == "/" and nxt == "/":
                    in_line_comment = True
                    k += 1
                elif c == "/" and nxt == "*":
                    in_block_comment = True
                    k += 1
                elif c in "\"'":
                    in_str = c
                elif c == "{":
                    depth += 1
                elif c == "}":
                    depth -= 1
                    if depth == 0:
                        k += 1
                        break
            prev = c
            k += 1
        i = k
    return "".join(out)


def count_file(path: pathlib.Path) -> int:
    text = path.read_text(encoding="utf-8", errors="replace")
    text = strip_test_modules(text)
    total = 0
    for line in text.splitlines():
        stripped = line.strip()
        if stripped.startswith("//"):
            continue
        # crude trailing-comment strip (good enough: no unwrap() in strings in src/)
        code = line.split("//", 1)[0]
        if "Regex::new" in code:
            # Static literal patterns: fail-fast at startup on programmer error
            # (matches the old gate's documented Regex::new exemption).
            continue
        total += code.count(".unwrap()")
    return total


def main() -> int:
    total = 0
    for path in sorted(SRC.rglob("*.rs")):
        total += count_file(path)
    if "--quiet" not in sys.argv:
        print(f"COUNT={total}")
    else:
        print(total)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

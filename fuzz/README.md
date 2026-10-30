# Fuzzing LocalPersona

Phase A (2026-10-30) establishes real corpus seeds and a documented, bounded
run procedure. Toolchain requirement: `cargo-fuzz` + a nightly toolchain
(both available on the dev host; verify with `cargo fuzz --version` and
`rustup toolchain list`).

## Targets

| Target | Calls (production path) | Watch for |
|---|---|---|
| `gguf_parse` | `localpersona::gguf::parse_gguf_bytes` | panics, degenerate loops, OOM on hostile headers (bounds: `MAX_KV_COUNT` / `MAX_KEY_LEN` / `MAX_STRING_LEN` / `MAX_ARRAY_LEN`) |
| `ndjson_parse` | `localpersona::conversation::parse_message_line` | panics/accepts-varying on crash-cut or hostile JSON lines |
| `pdf_ingest` | `localpersona::storage::pdf_fuzz_entry` | panics/hangs in the PDF parser (RUSTSEC-2026-0187 class; process-isolated in production) |

## Corpus policy

- Seeds live in `fuzz/corpus/<target>/`. Valid seeds are minimal, real-shaped
  documents; truncated seeds cover the graceful-failure paths.
- Seeds are NOT padding: each targets a distinct branch of the production
  parser. Behaviour changes to a parser must update its seeds in the same
  commit.
- `tests/fuzz_corpus_seeds.rs` is the **rot guard**: it calls the same parse
  functions the targets call and pins each seed's expected outcome. It runs
  in every Tribunal run, so a parser change that silently voids a seed fails
  the suite instead of rotting the corpus.
- New crashes raised by libFuzzer go to `localpersona/fuzz/artifacts/<target>`
  (with `--artifact-prefix` or the default `./crash-*`). Genuinely interesting
  crashers should be reduced, added as an extra seed, and accompanied by a
  regression test in the matching `tests/*.rs` suite.

## Running (bounded)

Recommended local cadence: before any release tag, or after touching
`src/gguf.rs`, `src/conversation.rs`, `src/storage.rs`.

```bash
# from the repository root (nightly is required by libFuzzer; the dev host
# has nightly installed but stable is the default, hence the +nightly proxy)
cargo +nightly fuzz run gguf_parse fuzz/corpus/gguf_parse -- -max_total_time=60 -rss_limit_mb=4096
cargo +nightly fuzz run ndjson_parse fuzz/corpus/ndjson_parse -- -max_total_time=60
cargo +nightly fuzz run pdf_ingest fuzz/corpus/pdf_ingest -- -max_total_time=120 -rss_limit_mb=4096
```

- `-max_total_time` bounds the wall-clock budget; corpus ingestion happens
  first, so even a 60 s run executes every seed.
- A crash prints the artifact path and reproduces with
  `cargo fuzz run <target> fuzz/artifacts/<crashfile>`.
- CI: the Tribunal (local + CI) intentionally stays on deterministic tests;
  libFuzzer runs are developer-triggered. If a crash is found, fix with a
  regression test, add the reduced input as a seed, and record it in the
  changelog.

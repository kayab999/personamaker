# test-models/ manifest — audit reproducibility (F-017)

Update in the same change as the roster. Cross-checked by
`tests/gguf_roster_probe.rs` (`roster_ctx_ground_truth` + `pinned_ab_verdict`).

| File | Size | SHA-256 (short) | general.architecture | ctx key | value | gguf.rs:249 reads? | budget today | Tier-2 role |
|---|---|---|---|---|---|---|---|---|
| Gemma-3-1B-it-GLM-4.7-Flash-Heretic-Uncensored-Thinking_Q5_k_m.gguf | 812M | ffd5366ab8ad | gemma3 | gemma3.context_length | 32768 | No | 4500 | QA-4 gemma family |
| L3.2-Rogue-Creative-Instruct-Uncensored-Abliterated-7B-D_AU-Q3_k_m.gguf | 3.5G | a1c2a7650512 | llama | llama.context_length | 131072 | **YES** | **72,089 (F-001 live-repro)** | F-001 live trigger (Scenario B confirmed) |
| Qwen3.8-2B-Uncensored-Q4_K_M.gguf | 1.2G | 61713f6d29b5 | qwen35 | qwen35.context_length | 262144 | No (`qwen35` ≠ `qwen3`; nonstandard arch string — verify conversion source before QA-4) | 4500 | post-F-008 worst case (144,179) |

Roster history: 2 GGUF + incomplete `.crdownload` (pre-audit) → 3 GGUF / 5.5G
(2026-09-17, this audit; `.crdownload` resolved into L3.2-Rogue-7B).

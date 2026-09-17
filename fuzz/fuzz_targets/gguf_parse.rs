#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // Use bytes parser (no file needed) — exercises array skip, string len, etc.
    let _ = localpersona::gguf::parse_gguf_bytes(data);
});

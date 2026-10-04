#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // Sweep W1: same parse the disposable --extract-pdf worker runs.
    // In production hostile bytes never reach this in-process; the fuzzer
    // hunts panics/hangs in the parser itself (RUSTSEC-2026-0187 class).
    localpersona::storage::pdf_fuzz_entry(data);
});

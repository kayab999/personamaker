#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // Try to parse a single NDJSON line as ChatMessage
    let _ = localpersona::conversation::parse_message_line(data);
    // Also try JSON Value parse for broader coverage
    let _ = serde_json::from_slice::<serde_json::Value>(data);
});

//! IPC contract gate — the Rust <-> JavaScript seam.
//!
//! Background: 0.9.0-rc.1 shipped with two defects that a fully green Tribunal
//! could not see, because every existing job tested the Rust crate in
//! isolation:
//!
//!   1. `tauri.conf.json` never set `app.withGlobalTauri`, so Tauri never
//!      injected `window.__TAURI__`. `isTauri()` returned false and every
//!      `callTauri()` was a no-op.
//!   2. `load_messages_for_display` existed in `commands.rs` and was called by
//!      the frontend, but was missing from `generate_handler!` — so the chat
//!      view could never load messages. It was masked by an
//!      `#[allow(dead_code)]` on the function, which silenced the one warning
//!      that would have caught it.
//!
//! These tests make both classes of defect fail CI. They are pure static
//! analysis over checked-in files: no Tauri runtime, no display, no build.

use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn read(rel: &str) -> String {
    let path = repo_root().join(rel);
    fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()))
}

/// Collect every `#[command]`-annotated function name in `commands.rs`.
fn declared_commands(src: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let mut cursor = 0usize;
    while let Some(rel) = src[cursor..].find("#[command") {
        let attr_start = cursor + rel;
        // The function signature follows the attribute; keep the search window
        // tight so a stray match can never swallow an unrelated function.
        let window_end = (attr_start + 600).min(src.len());
        let window = &src[attr_start..window_end];
        if let Some(fn_rel) = window.find("fn ") {
            let name_start = attr_start + fn_rel + 3;
            let name: String = src[name_start..]
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                .collect();
            if !name.is_empty() {
                cursor = name_start + name.len();
                out.insert(name);
            } else {
                cursor = name_start;
            }
        } else {
            cursor = attr_start + "#[command".len();
        }
    }
    out
}

/// Collect every `commands::<name>` reference inside `generate_handler!`.
fn registered_commands(main_src: &str) -> BTreeSet<String> {
    const MARKER: &str = "commands::";
    let mut out = BTreeSet::new();
    let mut rest = main_src;
    while let Some(pos) = rest.find(MARKER) {
        let after = &rest[pos + MARKER.len()..];
        let name: String = after
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
            .collect();
        if !name.is_empty() {
            out.insert(name);
        }
        rest = after;
    }
    out
}

/// Collect every `callTauri('name'` / `callTauri("name"` in the frontend.
///
/// The frontend routes 100% of IPC through `callTauri`, and every call site
/// passes a string literal — verified by inspection of script.js. If a future
/// refactor introduces a computed command name, this test will under-approximate
/// rather than fail spuriously, which is the safe direction.
fn invoked_commands(js: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for (marker, terminator) in [("callTauri('", '\''), ("callTauri(\"", '"')] {
        let mut rest = js;
        while let Some(pos) = rest.find(marker) {
            let after = &rest[pos + marker.len()..];
            let name: String = after.chars().take_while(|c| *c != terminator).collect();
            let looks_like_command = !name.is_empty()
                && name
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '_');
            if looks_like_command {
                out.insert(name);
            }
            rest = after;
        }
    }
    out
}

fn format_diff(missing: &BTreeSet<String>, context: &str) -> String {
    missing
        .iter()
        .map(|name| format!("  - {name}  ({context})"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// P0-2 regression gate: a command the frontend calls must be registered,
/// or the call silently fails at runtime with "command not found".
#[test]
fn every_frontend_call_is_registered() {
    let invoked = invoked_commands(&read("frontend/script.js"));
    let registered = registered_commands(&read("src/main.rs"));

    assert!(
        !invoked.is_empty(),
        "no callTauri('...') call sites found in frontend/script.js — \
         the parser or the frontend has changed shape; fix the gate, do not delete it"
    );

    let missing: BTreeSet<String> = invoked.difference(&registered).cloned().collect();
    assert!(
        missing.is_empty(),
        "frontend invokes commands that are NOT in generate_handler!:\n{}\n\
         Each of these is a dead call at runtime (Tauri returns 'command not found').",
        format_diff(&missing, "invoked by frontend, unregistered")
    );
}

/// Inverse gate: a `#[command]` that is never registered is dead weight — and
/// historically the exact shape of the bug that broke chat loading.
#[test]
fn every_command_is_registered() {
    let declared = declared_commands(&read("src/commands.rs"));
    let registered = registered_commands(&read("src/main.rs"));

    assert!(
        !declared.is_empty(),
        "no #[command] functions found in src/commands.rs — parser drift"
    );

    let unregistered: BTreeSet<String> = declared.difference(&registered).cloned().collect();
    assert!(
        unregistered.is_empty(),
        "commands are declared but never registered in generate_handler!:\n{}\n\
         Either register them or delete them; an unregistered #[command] is unreachable.",
        format_diff(&unregistered, "declared, unregistered")
    );
}

/// P0-1 regression gate: without this flag Tauri does not inject the
/// `window.__TAURI__` global API, and the entire frontend becomes inert.
#[test]
fn global_tauri_api_is_enabled() {
    let raw = read("tauri.conf.json");
    let cfg: serde_json::Value =
        serde_json::from_str(&raw).expect("tauri.conf.json must be valid JSON");

    let flag = cfg
        .get("app")
        .and_then(|app| app.get("withGlobalTauri"))
        .and_then(|v| v.as_bool());

    assert_eq!(
        flag,
        Some(true),
        "tauri.conf.json must set app.withGlobalTauri = true.\n\
         The frontend resolves IPC through `window.__TAURI__.core.invoke`; Tauri only\n\
         injects that global when this flag is set (default is false). Without it,\n\
         isTauri() is false and every callTauri() is a no-op."
    );
}

/// The bundled frontend must actually exist and carry the IPC entry point.
#[test]
fn bundled_frontend_is_present() {
    let root = repo_root();
    for rel in ["frontend/index.html", "frontend/script.js"] {
        let path = root.join(rel);
        assert!(path.is_file(), "missing bundled frontend asset: {}", path.display());
    }

    let html = read("frontend/index.html");
    assert!(
        html.contains("script.js"),
        "frontend/index.html does not load script.js — the app would boot to an empty shell"
    );
}

/// Guard against reintroducing the `#[allow(dead_code)]` that hid the
/// unregistered-command bug. If a command looks unused, the answer is to
/// register it (or delete it) — never to silence the warning.
#[test]
fn commands_are_not_dead_code_suppressed() {
    let src = read("src/commands.rs");
    let offenders: Vec<String> = src
        .lines()
        .enumerate()
        .filter(|(_, line)| line.contains("allow(dead_code)"))
        .map(|(idx, _)| format!("src/commands.rs:{}", idx + 1))
        .collect();

    assert!(
        offenders.is_empty(),
        "`#[allow(dead_code)]` found in commands.rs at: {:?}\n\
         This annotation previously masked an unregistered #[command] and broke\n\
         the chat view. Remove it and let the compiler warn, or register the command.",
        offenders
    );
}

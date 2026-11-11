//! `hopback-hook`: run by Claude Code on hook events (see `plugins/hopback`).
//! Reads the event JSON on stdin and updates `~/.claude/hopback/<session_id>.json`.
//! It never prints and always exits 0, whatever happens.

use std::io::{IsTerminal, Read};

const MAX_INPUT: u64 = 16 * 1024 * 1024;

fn main() {
    let _ = std::panic::catch_unwind(run);
    std::process::exit(0);
}

fn run() {
    let stdin = std::io::stdin();
    if stdin.is_terminal() {
        return;
    }
    let mut raw = Vec::new();
    if stdin.lock().take(MAX_INPUT).read_to_end(&mut raw).is_err() || raw.is_empty() {
        return;
    }
    let Some(dir) = hopback_hook::marker_dir() else { return };
    let _ = hopback_hook::handle(&raw, &dir, hopback_hook::claude_pid(), hopback_hook::now_ms());
}

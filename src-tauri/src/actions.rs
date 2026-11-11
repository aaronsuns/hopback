//! Going places: raising VS Code windows and opening sessions.

use crate::state::{lock, App};
use hopback_core::board;
use hopback_core::transcript::folder_name;
use hopback_core::windows;
use std::process::Command;
use std::time::{Duration, Instant};

pub const VSCODE_BUNDLE_ID: &str = "com.microsoft.VSCode";

/// Claude Code extension URI that opens a session in the focused VS Code window.
pub fn open_session_uri(session_id: &str) -> String {
    format!("vscode://anthropic.claude-code/open?session={session_id}")
}

fn raise_window(app: &App, menu_index: usize, title: &str) -> bool {
    let pid = *lock(&app.vscode_pid);
    pid.is_some_and(|pid| crate::ax::raise(pid, menu_index, title))
}

pub fn go_to_window_number(app: &App, number: usize) {
    let row = lock(&app.board).windows.iter().find(|w| w.number == number).cloned();
    if let Some(w) = row {
        raise_window(app, w.menu_index, &w.title);
    }
}

pub fn cycle_window(app: &App, forward: bool) {
    let (_, ws) = app.windows();
    let pid = *lock(&app.vscode_pid);
    let current = pid.and_then(crate::ax::focused_window_title);
    if let Some(i) = windows::cycle(&ws, current.as_deref(), forward) {
        raise_window(app, ws[i].menu_index, &ws[i].title);
    }
}

pub fn next_needs_you(app: &App) {
    let needs = lock(&app.board).needs_you.clone();
    let mut cursor = lock(&app.needs_cursor);
    if let Some(s) = board::next_needs_you(&needs, cursor.as_deref()) {
        *cursor = Some(s.id.clone());
        let id = s.id.clone();
        drop(cursor);
        go_to_session(app, &id);
    }
}

/// Raise the window that shows the session; otherwise open its folder in VS Code
/// and then the conversation.
pub fn go_to_session(app: &App, id: &str) {
    let row = lock(&app.board).windows.iter().find(|w| w.session.as_ref().is_some_and(|s| s.id == id)).cloned();
    if let Some(w) = row {
        if raise_window(app, w.menu_index, &w.title) {
            return;
        }
    }
    let Some(session) = app.find_session(id) else {
        log::warn!("go to session: unknown session");
        return;
    };
    let app = app.clone();
    let id = id.to_owned();
    std::thread::spawn(move || open_in_vscode(&app, &session.cwd, &id));
}

fn open_in_vscode(app: &App, cwd: &str, id: &str) {
    if !cwd.is_empty() && std::path::Path::new(cwd).is_dir() {
        let ok = Command::new("open").args(["-b", VSCODE_BUNDLE_ID, cwd]).status().is_ok_and(|s| s.success());
        if !ok {
            log::warn!("could not open the folder in VS Code");
            return;
        }
        wait_for_folder_window(app, folder_name(cwd));
    }
    if hopback_hook::valid_session_id(id) {
        let _ = Command::new("open").arg(open_session_uri(id)).status();
    }
}

/// Wait until VS Code's focused window is the folder's (needs Accessibility),
/// so the URI lands in the right window. Without it, wait a fixed moment.
fn wait_for_folder_window(app: &App, folder: &str) {
    if !crate::ax::is_trusted() {
        std::thread::sleep(Duration::from_millis(1500));
        return;
    }
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        let pid = crate::ax::vscode_pid();
        *lock(&app.vscode_pid) = pid;
        let focused = pid.and_then(crate::ax::focused_window_title);
        if focused.and_then(|t| windows::folder_from_title(&t)).as_deref() == Some(folder) {
            std::thread::sleep(Duration::from_millis(400));
            return;
        }
        std::thread::sleep(Duration::from_millis(250));
    }
}

pub fn open_accessibility_settings() {
    crate::ax::prompt_for_trust();
    let _ = Command::new("open")
        .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility")
        .status();
}

pub fn open_notification_settings() {
    let _ = Command::new("open").arg("x-apple.systempreferences:com.apple.preference.notifications").status();
}

#[cfg(test)]
mod tests {
    #[test]
    fn uri() {
        assert_eq!(
            super::open_session_uri("3f2a9c1e-5b7d-4e8f-9a0b-1c2d3e4f5a6b"),
            "vscode://anthropic.claude-code/open?session=3f2a9c1e-5b7d-4e8f-9a0b-1c2d3e4f5a6b"
        );
    }
}

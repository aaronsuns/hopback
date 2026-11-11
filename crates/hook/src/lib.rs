//! Shared model for Hopback's per-session marker files, plus the pure logic the
//! `hopback-hook` binary uses to turn a Claude Code hook event into a marker.
//!
//! The app reads the same [`Marker`] type, so the on-disk format lives here.

use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// Name of the file the hook touches so the app can tell the plugin works.
pub const HOOK_SEEN_FILE: &str = ".hook-seen";

/// Sessions started inside Hopback's own cache (the "Ask" helper) are ignored.
pub const OWN_CACHE_FRAGMENT: &str = "/Library/Caches/se.zexa.hopback";

/// Tools that mean "Claude is waiting for an answer", not "approve a tool call".
pub const QUESTION_TOOLS: &[&str] = &["AskUserQuestion", "ExitPlanMode"];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Hash, PartialOrd, Ord)]
#[serde(rename_all = "lowercase")]
pub enum State {
    Approve,
    Question,
    Working,
    Idle,
}

impl State {
    pub fn glyph(self) -> &'static str {
        match self {
            State::Approve => "⚠️",
            State::Question => "❓",
            State::Working => "⚙️",
            State::Idle => "💤",
        }
    }
    pub fn needs_you(self) -> bool {
        matches!(self, State::Approve | State::Question)
    }
}

/// One marker file per session: `<dir>/<session_id>.json`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Marker {
    pub session_id: String,
    pub state: State,
    #[serde(default)]
    pub cwd: String,
    #[serde(default)]
    pub transcript_path: String,
    /// Milliseconds since the Unix epoch.
    pub updated_at: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pid: Option<i32>,
    /// The tool the session is waiting on (approve/question), if known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool: Option<String>,
}

/// The subset of the hook input we care about. Everything is optional so that
/// unexpected input degrades to "do nothing" instead of an error.
#[derive(Debug, Default, Deserialize)]
pub struct HookInput {
    pub session_id: Option<String>,
    pub transcript_path: Option<String>,
    pub cwd: Option<String>,
    pub hook_event_name: Option<String>,
    pub tool_name: Option<String>,
    pub notification_type: Option<String>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Action {
    Set { state: State, tool: Option<String> },
    Remove,
    Ignore,
}

fn is_question_tool(tool: Option<&str>) -> bool {
    tool.is_some_and(|t| QUESTION_TOOLS.contains(&t))
}

/// Derive what to do with the marker given the event and the previous state.
pub fn derive(input: &HookInput, prev: Option<State>) -> Action {
    let tool = input.tool_name.as_deref();
    let waiting = |state| Action::Set { state, tool: tool.map(str::to_owned) };
    match input.hook_event_name.as_deref() {
        Some("PermissionRequest") => {
            if is_question_tool(tool) {
                waiting(State::Question)
            } else {
                waiting(State::Approve)
            }
        }
        Some("PreToolUse") => {
            if is_question_tool(tool) {
                waiting(State::Question)
            } else {
                Action::Set { state: State::Working, tool: None }
            }
        }
        Some("PostToolUse" | "PostToolUseFailure" | "UserPromptSubmit") => {
            Action::Set { state: State::Working, tool: None }
        }
        Some("Notification") => match input.notification_type.as_deref() {
            // A permission prompt for AskUserQuestion/ExitPlanMode is still a question.
            Some("permission_prompt") if prev == Some(State::Question) => Action::Ignore,
            Some("permission_prompt") if prev == Some(State::Approve) => Action::Ignore,
            Some("permission_prompt") => Action::Set { state: State::Approve, tool: None },
            Some("elicitation_dialog") => Action::Set { state: State::Question, tool: None },
            _ => Action::Ignore,
        },
        Some("Stop") => Action::Set { state: State::Idle, tool: None },
        Some("SessionStart") => match prev {
            Some(_) => Action::Ignore,
            None => Action::Set { state: State::Idle, tool: None },
        },
        Some("SessionEnd") => Action::Remove,
        _ => Action::Ignore,
    }
}

/// Session ids become file names, so only allow a safe character set.
pub fn valid_session_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 128 && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

pub fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

/// Default marker directory: `$HOPBACK_DIR`, else `~/.claude/hopback`.
pub fn marker_dir() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("HOPBACK_DIR").filter(|d| !d.is_empty()) {
        return Some(PathBuf::from(dir));
    }
    let home = std::env::var_os("HOME").filter(|h| !h.is_empty())?;
    Some(PathBuf::from(home).join(".claude").join("hopback"))
}

pub fn marker_path(dir: &Path, session_id: &str) -> PathBuf {
    dir.join(format!("{session_id}.json"))
}

pub fn read_marker(path: &Path) -> Option<Marker> {
    serde_json::from_slice(&fs::read(path).ok()?).ok()
}

/// Write via temp file + rename so readers never see a half-written marker.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let dir = path.parent().unwrap_or(Path::new("."));
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("marker");
    let tmp = dir.join(format!(".{name}.{}.tmp", std::process::id()));
    let result = (|| {
        let mut f = fs::File::create(&tmp)?;
        f.write_all(bytes)?;
        f.sync_data().ok();
        fs::rename(&tmp, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result
}

/// Process the raw stdin of one hook invocation. Never panics; errors are swallowed
/// by the caller. `pid` is the Claude process id, if it could be found.
pub fn handle(raw: &[u8], dir: &Path, pid: Option<i32>, now: u64) -> std::io::Result<()> {
    let Ok(input) = serde_json::from_slice::<HookInput>(raw) else { return Ok(()) };
    let Some(session_id) = input.session_id.as_deref().filter(|s| valid_session_id(s)) else {
        return Ok(());
    };
    let cwd = input.cwd.clone().unwrap_or_default();
    if cwd.contains(OWN_CACHE_FRAGMENT) {
        return Ok(());
    }
    fs::create_dir_all(dir)?;
    let _ = fs::write(dir.join(HOOK_SEEN_FILE), now.to_string());

    let path = marker_path(dir, session_id);
    let prev = read_marker(&path);
    match derive(&input, prev.as_ref().map(|m| m.state)) {
        Action::Ignore => Ok(()),
        Action::Remove => match fs::remove_file(&path) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e),
            _ => Ok(()),
        },
        Action::Set { state, tool } => {
            let keep = |new: Option<String>, old: Option<String>| new.filter(|s| !s.is_empty()).or(old);
            let prev = prev.unwrap_or(Marker {
                session_id: String::new(),
                state,
                cwd: String::new(),
                transcript_path: String::new(),
                updated_at: 0,
                pid: None,
                tool: None,
            });
            let marker = Marker {
                session_id: session_id.to_owned(),
                state,
                cwd: keep(input.cwd, Some(prev.cwd)).unwrap_or_default(),
                transcript_path: keep(input.transcript_path, Some(prev.transcript_path)).unwrap_or_default(),
                updated_at: now,
                pid: pid.or(prev.pid),
                tool,
            };
            let bytes = serde_json::to_vec(&marker).map_err(std::io::Error::other)?;
            write_atomic(&path, &bytes)
        }
    }
}

/// Find the Claude process that ran this hook: walk up from our parent, skipping
/// the shell that Claude Code uses to run hook commands.
#[cfg(target_os = "macos")]
pub fn claude_pid() -> Option<i32> {
    const SHELLS: &[&str] = &["sh", "bash", "zsh", "dash", "fish", "env", "test"];
    let mut pid = unsafe { libc::getppid() };
    for _ in 0..4 {
        if pid <= 1 {
            return None;
        }
        let (ppid, name) = proc_info(pid)?;
        if !SHELLS.contains(&name.as_str()) {
            return Some(pid);
        }
        pid = ppid;
    }
    None
}

#[cfg(not(target_os = "macos"))]
pub fn claude_pid() -> Option<i32> {
    None
}

#[cfg(target_os = "macos")]
fn proc_info(pid: i32) -> Option<(i32, String)> {
    let mut info: libc::proc_bsdinfo = unsafe { std::mem::zeroed() };
    let size = std::mem::size_of::<libc::proc_bsdinfo>() as i32;
    let n =
        unsafe { libc::proc_pidinfo(pid, libc::PROC_PIDTBSDINFO, 0, &mut info as *mut _ as *mut libc::c_void, size) };
    if n != size {
        return None;
    }
    let name: Vec<u8> = info.pbi_comm.iter().take_while(|&&c| c != 0).map(|&c| c as u8).collect();
    Some((info.pbi_ppid as i32, String::from_utf8_lossy(&name).into_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(event: &str, tool: Option<&str>) -> HookInput {
        HookInput {
            session_id: Some("s1".into()),
            hook_event_name: Some(event.into()),
            tool_name: tool.map(Into::into),
            ..Default::default()
        }
    }
    fn set(state: State) -> Action {
        Action::Set { state, tool: None }
    }

    #[test]
    fn permission_request_is_approve_unless_question_tool() {
        assert_eq!(
            derive(&input("PermissionRequest", Some("Bash")), None),
            Action::Set { state: State::Approve, tool: Some("Bash".into()) }
        );
        assert_eq!(
            derive(&input("PermissionRequest", Some("ExitPlanMode")), None),
            Action::Set { state: State::Question, tool: Some("ExitPlanMode".into()) }
        );
    }

    #[test]
    fn tool_use_and_prompt_are_working() {
        assert_eq!(derive(&input("PreToolUse", Some("Read")), None), set(State::Working));
        assert_eq!(derive(&input("PostToolUse", Some("Read")), Some(State::Approve)), set(State::Working));
        assert_eq!(derive(&input("PostToolUseFailure", Some("Bash")), None), set(State::Working));
        assert_eq!(derive(&input("UserPromptSubmit", None), Some(State::Idle)), set(State::Working));
    }

    #[test]
    fn asking_tools_are_questions() {
        for t in QUESTION_TOOLS {
            assert_eq!(
                derive(&input("PreToolUse", Some(t)), None),
                Action::Set { state: State::Question, tool: Some(t.to_string()) }
            );
        }
    }

    #[test]
    fn stop_end_and_start() {
        assert_eq!(derive(&input("Stop", None), Some(State::Working)), set(State::Idle));
        assert_eq!(derive(&input("SessionEnd", None), Some(State::Idle)), Action::Remove);
        assert_eq!(derive(&input("SessionStart", None), None), set(State::Idle));
        assert_eq!(derive(&input("SessionStart", None), Some(State::Working)), Action::Ignore);
    }

    #[test]
    fn notifications() {
        let mut n = input("Notification", None);
        n.notification_type = Some("permission_prompt".into());
        assert_eq!(derive(&n, Some(State::Working)), set(State::Approve));
        assert_eq!(derive(&n, Some(State::Question)), Action::Ignore);
        n.notification_type = Some("elicitation_dialog".into());
        assert_eq!(derive(&n, None), set(State::Question));
        n.notification_type = Some("idle_prompt".into());
        assert_eq!(derive(&n, Some(State::Idle)), Action::Ignore);
    }

    #[test]
    fn unknown_events_are_ignored() {
        assert_eq!(derive(&input("SubagentStop", None), None), Action::Ignore);
        assert_eq!(derive(&input("PreCompact", None), None), Action::Ignore);
        assert_eq!(derive(&HookInput::default(), None), Action::Ignore);
    }

    #[test]
    fn session_id_validation() {
        assert!(valid_session_id("0b7c2f9e-1111-4c4c-9a9a-123456789abc"));
        assert!(!valid_session_id(""));
        assert!(!valid_session_id("../etc/passwd"));
        assert!(!valid_session_id("a/b"));
        assert!(!valid_session_id(&"x".repeat(200)));
    }

    #[test]
    fn handle_writes_and_preserves_fields() {
        let dir = tempfile::tempdir().unwrap();
        let start = br#"{"session_id":"s1","hook_event_name":"UserPromptSubmit","cwd":"/Users/alex/web-app","transcript_path":"/t/s1.jsonl"}"#;
        handle(start, dir.path(), Some(42), 1000).unwrap();
        // A later event without cwd keeps the earlier cwd and pid.
        handle(br#"{"session_id":"s1","hook_event_name":"Stop"}"#, dir.path(), None, 2000).unwrap();
        let m = read_marker(&marker_path(dir.path(), "s1")).unwrap();
        assert_eq!(m.state, State::Idle);
        assert_eq!(m.cwd, "/Users/alex/web-app");
        assert_eq!(m.transcript_path, "/t/s1.jsonl");
        assert_eq!(m.pid, Some(42));
        assert_eq!(m.updated_at, 2000);
        assert!(dir.path().join(HOOK_SEEN_FILE).exists());
    }

    #[test]
    fn handle_ignores_own_cache_sessions() {
        let dir = tempfile::tempdir().unwrap();
        let raw = br#"{"session_id":"s9","hook_event_name":"UserPromptSubmit","cwd":"/Users/alex/Library/Caches/se.zexa.hopback/history"}"#;
        handle(raw, dir.path(), None, 1).unwrap();
        assert!(!marker_path(dir.path(), "s9").exists());
    }

    #[test]
    fn atomic_write_leaves_no_temp_files() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("a.json");
        write_atomic(&p, b"{}").unwrap();
        write_atomic(&p, b"{\"x\":1}").unwrap();
        assert_eq!(fs::read(&p).unwrap(), b"{\"x\":1}");
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
    }
}

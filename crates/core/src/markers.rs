//! Reading the hook's marker directory and deciding which markers are still live.

pub use hopback_hook::{Marker, State, HOOK_SEEN_FILE};
use std::path::Path;

pub const DAY_MS: u64 = 24 * 60 * 60 * 1000;

/// A marker is ended when its Claude process is gone, or it has been idle for a day.
pub fn is_ended(m: &Marker, now_ms: u64, pid_alive: impl Fn(i32) -> bool) -> bool {
    if let Some(pid) = m.pid {
        if !pid_alive(pid) {
            return true;
        }
    }
    m.state == State::Idle && now_ms.saturating_sub(m.updated_at) > DAY_MS
}

/// Whether a process exists (EPERM still means it exists).
pub fn pid_alive(pid: i32) -> bool {
    if pid <= 0 {
        return false;
    }
    let r = unsafe { libc::kill(pid, 0) };
    r == 0 || std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
}

/// All parseable markers in `dir`. Temp files and `.hook-seen` are skipped.
pub fn read_all(dir: &Path) -> Vec<Marker> {
    let Ok(entries) = std::fs::read_dir(dir) else { return Vec::new() };
    let mut out: Vec<Marker> = entries
        .flatten()
        .filter_map(|e| {
            let name = e.file_name();
            let name = name.to_str()?;
            if name.starts_with('.') || !name.ends_with(".json") {
                return None;
            }
            hopback_hook::read_marker(&e.path())
        })
        .collect();
    out.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    out
}

/// Live markers: not ended. Returns (live, ended-ids) so the caller may clean up.
pub fn live(markers: Vec<Marker>, now_ms: u64, alive: impl Fn(i32) -> bool) -> (Vec<Marker>, Vec<String>) {
    let (ended, live): (Vec<_>, Vec<_>) = markers.into_iter().partition(|m| is_ended(m, now_ms, &alive));
    (live, ended.into_iter().map(|m| m.session_id).collect())
}

pub fn hook_seen(dir: &Path) -> bool {
    dir.join(HOOK_SEEN_FILE).exists()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn marker(id: &str, state: State, at: u64, pid: Option<i32>) -> Marker {
        Marker {
            session_id: id.into(),
            state,
            cwd: "/Users/alex/web-app".into(),
            transcript_path: String::new(),
            updated_at: at,
            pid,
            tool: None,
        }
    }

    #[test]
    fn staleness_rules() {
        let now = 10 * DAY_MS;
        let alive = |p: i32| p == 1;
        assert!(!is_ended(&marker("a", State::Working, now - 5, Some(1)), now, alive));
        assert!(is_ended(&marker("b", State::Working, now - 5, Some(2)), now, alive));
        assert!(is_ended(&marker("c", State::Idle, now - DAY_MS - 1, None), now, alive));
        assert!(!is_ended(&marker("d", State::Idle, now - DAY_MS + 1, None), now, alive));
        // Only idle markers age out; a waiting session stays until its process dies.
        assert!(!is_ended(&marker("e", State::Approve, now - 3 * DAY_MS, None), now, alive));
    }

    #[test]
    fn live_partitions() {
        let now = 5 * DAY_MS;
        let (live, ended) = live(
            vec![marker("a", State::Working, now, Some(1)), marker("b", State::Working, now, Some(9))],
            now,
            |p| p == 1,
        );
        assert_eq!(live.len(), 1);
        assert_eq!(ended, vec!["b".to_string()]);
    }

    #[test]
    fn reads_directory_tolerantly() {
        let dir = tempfile::tempdir().unwrap();
        let d = dir.path();
        std::fs::write(d.join("a.json"), serde_json::to_vec(&marker("a", State::Idle, 1, None)).unwrap()).unwrap();
        std::fs::write(d.join("b.json"), serde_json::to_vec(&marker("b", State::Approve, 2, None)).unwrap()).unwrap();
        std::fs::write(d.join("broken.json"), b"{nope").unwrap();
        std::fs::write(d.join(".a.json.1.tmp"), b"{}").unwrap();
        std::fs::write(d.join(HOOK_SEEN_FILE), b"1").unwrap();
        let all = read_all(d);
        assert_eq!(all.iter().map(|m| m.session_id.as_str()).collect::<Vec<_>>(), vec!["b", "a"]);
        assert!(hook_seen(d));
        assert!(read_all(&d.join("missing")).is_empty());
    }

    #[test]
    fn own_pid_is_alive() {
        assert!(pid_alive(std::process::id() as i32));
        assert!(!pid_alive(0));
        assert!(!pid_alive(-5));
    }
}

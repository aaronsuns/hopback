//! Which Claude Code session a VS Code window shows, read from VS Code's
//! per-workspace state database (`workspaceStorage/<hash>/state.vscdb`).
//!
//! See MAINTAINING.md ("Mapping VS Code windows to sessions") for the findings
//! behind this. VS Code flushes this database periodically, so it can lag.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Keys in the `ItemTable` that can carry the Claude Code webview state.
pub const SIDEBAR_KEY_PREFIX: &str = "memento/webviewView.claudeVSCode";
pub const EDITOR_PARTS_KEY: &str = "memento/workbench.parts.editor";
pub const PANEL_VIEW_TYPE: &str = "claudeVSCodePanel";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Shown {
    pub session_id: String,
    pub updated_at: u64,
}

/// The workspace folder path from a `workspace.json` (`{"folder": "file:///…"}`).
/// Multi-root workspaces (`{"workspace": "…/x.code-workspace"}`) give the file's stem dir.
pub fn workspace_path(workspace_json: &str) -> Option<String> {
    let v: serde_json::Value = serde_json::from_str(workspace_json).ok()?;
    if let Some(folder) = v.get("folder").and_then(|f| f.as_str()) {
        return file_uri_to_path(folder);
    }
    let ws = v.get("workspace").and_then(|f| f.as_str())?;
    let p = file_uri_to_path(ws)?;
    let stem = p.strip_suffix(".code-workspace").unwrap_or(&p);
    Some(stem.to_owned())
}

pub fn file_uri_to_path(uri: &str) -> Option<String> {
    let rest = uri.strip_prefix("file://")?;
    Some(percent_decode(rest))
}

fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let Some(n) = std::str::from_utf8(&b[i + 1..i + 3]).ok().and_then(|h| u8::from_str_radix(h, 16).ok()) {
                out.push(n);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn is_uuid_char(c: u8) -> bool {
    c.is_ascii_hexdigit() || c == b'-'
}

/// Find every `sessionID` in a (possibly multiply-escaped) JSON blob, with the
/// nearest `sessionUpdatedAt`.
pub fn scan_session_ids(blob: &str) -> Vec<Shown> {
    let bytes = blob.as_bytes();
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(pos) = blob[from..].find("sessionID").map(|p| p + from) {
        from = pos + "sessionID".len();
        // Skip quotes, backslashes, colons and spaces up to the value.
        let mut i = from;
        while i < bytes.len() && matches!(bytes[i], b'"' | b'\\' | b':' | b' ') {
            i += 1;
        }
        let start = i;
        while i < bytes.len() && is_uuid_char(bytes[i]) {
            i += 1;
        }
        let id = &blob[start..i];
        if id.len() < 8 {
            continue;
        }
        // The webview state is a small object; look for its timestamp nearby.
        let lo = floor_char(blob, pos.saturating_sub(200));
        let hi = floor_char(blob, (i + 200).min(blob.len()));
        let updated_at = nearest_number_after(&blob[lo..hi], "sessionUpdatedAt").unwrap_or(0);
        out.push(Shown { session_id: id.to_owned(), updated_at });
    }
    out
}

fn floor_char(s: &str, mut i: usize) -> usize {
    while i > 0 && !s.is_char_boundary(i) {
        i -= 1;
    }
    i
}

fn nearest_number_after(s: &str, key: &str) -> Option<u64> {
    let p = s.find(key)? + key.len();
    let digits: String =
        s[p..].chars().skip_while(|c| !c.is_ascii_digit()).take_while(|c| c.is_ascii_digit()).collect();
    digits.parse().ok()
}

/// From the relevant `ItemTable` rows of one workspace, the session shown there:
/// the most recently updated of the sidebar view and any Claude editor tabs.
pub fn shown_session(rows: &[(String, String)]) -> Option<Shown> {
    rows.iter()
        .filter(|(k, v)| k.starts_with(SIDEBAR_KEY_PREFIX) || (k == EDITOR_PARTS_KEY && v.contains(PANEL_VIEW_TYPE)))
        .flat_map(|(k, v)| {
            if k == EDITOR_PARTS_KEY {
                // Only consider session ids that sit inside a Claude panel's serialized state.
                v.match_indices(PANEL_VIEW_TYPE)
                    .flat_map(|(p, _)| {
                        let end = floor_char(v, (p + 4000).min(v.len()));
                        scan_session_ids(&v[p..end]).into_iter().take(1)
                    })
                    .collect::<Vec<_>>()
            } else {
                scan_session_ids(v)
            }
        })
        .max_by_key(|s| s.updated_at)
}

/// Read the relevant rows from a `state.vscdb` without disturbing VS Code.
pub fn read_state_rows(db: &Path) -> Vec<(String, String)> {
    use rusqlite::{Connection, OpenFlags};
    let Ok(conn) = Connection::open_with_flags(db, OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX)
    else {
        return Vec::new();
    };
    let _ = conn.busy_timeout(std::time::Duration::from_millis(250));
    let sql =
        "SELECT key, CAST(value AS TEXT) FROM ItemTable WHERE key LIKE 'memento/webviewView.claudeVSCode%' OR key = ?1";
    let Ok(mut stmt) = conn.prepare(sql) else { return Vec::new() };
    stmt.query_map([EDITOR_PARTS_KEY], |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?.unwrap_or_default()))
    })
    .map(|rows| rows.flatten().collect())
    .unwrap_or_default()
}

#[derive(Debug, Clone)]
pub struct WorkspaceDir {
    pub path: String,
    pub storage: PathBuf,
    pub modified: std::time::SystemTime,
}

/// Index of workspace folder name → storage dirs (most recently used first).
pub fn scan_workspace_storage(root: &Path) -> HashMap<String, Vec<WorkspaceDir>> {
    let mut map: HashMap<String, Vec<WorkspaceDir>> = HashMap::new();
    let Ok(entries) = std::fs::read_dir(root) else { return map };
    for e in entries.flatten() {
        let dir = e.path();
        let Ok(json) = std::fs::read_to_string(dir.join("workspace.json")) else { continue };
        let Some(path) = workspace_path(&json) else { continue };
        let db = dir.join("state.vscdb");
        let modified = std::fs::metadata(&db).and_then(|m| m.modified()).unwrap_or(std::time::UNIX_EPOCH);
        let name = crate::transcript::folder_name(&path).to_owned();
        map.entry(name).or_default().push(WorkspaceDir { path, storage: dir, modified });
    }
    for v in map.values_mut() {
        v.sort_by_key(|d| std::cmp::Reverse(d.modified));
    }
    map
}

pub fn default_storage_root(home: &Path) -> PathBuf {
    home.join("Library/Application Support/Code/User/workspaceStorage")
}

#[cfg(test)]
mod tests {
    use super::*;

    const A: &str = "11111111-2222-4333-8444-555555555555";
    const B: &str = "aaaaaaaa-bbbb-4ccc-8ddd-eeeeeeeeeeee";

    #[test]
    fn workspace_json() {
        assert_eq!(
            workspace_path(r#"{"folder":"file:///Users/alex/web-app"}"#).as_deref(),
            Some("/Users/alex/web-app")
        );
        assert_eq!(
            workspace_path(r#"{"folder":"file:///Users/alex/my%20docs-site"}"#).as_deref(),
            Some("/Users/alex/my docs-site")
        );
        assert_eq!(
            workspace_path(r#"{"workspace":"file:///Users/alex/platform.code-workspace"}"#).as_deref(),
            Some("/Users/alex/platform")
        );
        assert_eq!(workspace_path(r#"{"folder":"vscode-remote://ssh/x"}"#), None);
        assert_eq!(workspace_path("garbage"), None);
    }

    #[test]
    fn sidebar_state() {
        let v = format!(
            r#"{{"webviewState":"{{\"isFullEditor\":false,\"sessionID\":\"{A}\",\"sessionUpdatedAt\":1700000000000}}"}}"#
        );
        let rows = vec![("memento/webviewView.claudeVSCodeSidebarSecondary".to_owned(), v)];
        assert_eq!(shown_session(&rows), Some(Shown { session_id: A.into(), updated_at: 1_700_000_000_000 }));
    }

    #[test]
    fn newest_of_sidebar_and_editor_tabs_wins() {
        let side = format!(r#"{{"webviewState":"{{\"sessionUpdatedAt\":100,\"sessionID\":\"{A}\"}}"}}"#);
        let editor = format!(
            r#"{{"editors":[{{"id":"WebviewEditor","value":"{{\"viewType\":\"mainThreadWebview-claudeVSCodePanel\",\"state\":\"{{\\\"sessionID\\\":\\\"{B}\\\",\\\"sessionUpdatedAt\\\":200}}\"}}"}}]}}"#
        );
        let rows = vec![
            ("memento/webviewView.claudeVSCodeSidebarSecondary".to_owned(), side),
            (EDITOR_PARTS_KEY.to_owned(), editor),
        ];
        assert_eq!(shown_session(&rows).unwrap().session_id, B);
    }

    #[test]
    fn editor_parts_without_claude_panel_are_ignored() {
        let editor = format!(r#"{{"other":"sessionID\":\"{B}\""}}"#);
        assert_eq!(shown_session(&[(EDITOR_PARTS_KEY.to_owned(), editor)]), None);
        assert_eq!(shown_session(&[]), None);
    }

    #[test]
    fn scan_tolerates_noise() {
        assert!(scan_session_ids("sessionID").is_empty());
        assert!(scan_session_ids(r#"{"sessionID":""}"#).is_empty());
        let s = scan_session_ids(&format!(r#"ééé{{"sessionID":"{A}"}}"#));
        assert_eq!(s, vec![Shown { session_id: A.into(), updated_at: 0 }]);
    }

    #[test]
    fn reads_a_real_sqlite_file() {
        let dir = tempfile::tempdir().unwrap();
        let ws = dir.path().join("abc123");
        std::fs::create_dir(&ws).unwrap();
        std::fs::write(ws.join("workspace.json"), r#"{"folder":"file:///Users/alex/infra"}"#).unwrap();
        let db = ws.join("state.vscdb");
        let conn = rusqlite::Connection::open(&db).unwrap();
        conn.execute("CREATE TABLE ItemTable (key TEXT UNIQUE ON CONFLICT REPLACE, value BLOB)", []).unwrap();
        let v = format!(r#"{{"webviewState":"{{\"sessionID\":\"{A}\",\"sessionUpdatedAt\":5}}"}}"#);
        conn.execute("INSERT INTO ItemTable VALUES ('memento/webviewView.claudeVSCodeSidebarSecondary', ?1)", [&v])
            .unwrap();
        conn.execute("INSERT INTO ItemTable VALUES ('unrelated', 'x')", []).unwrap();
        drop(conn);
        let map = scan_workspace_storage(dir.path());
        let entry = &map["infra"][0];
        assert_eq!(entry.path, "/Users/alex/infra");
        let rows = read_state_rows(&entry.storage.join("state.vscdb"));
        assert_eq!(rows.len(), 1);
        assert_eq!(shown_session(&rows).unwrap().session_id, A);
        assert!(read_state_rows(&dir.path().join("missing.vscdb")).is_empty());
    }
}

//! The model behind the menu: which window shows which session, what needs the
//! user, what works in the background, and the tray title.

use crate::markers::State;
use crate::transcript::folder_name;
use crate::vscode::Shown;
use crate::windows::Window;
use serde::Serialize;
use std::collections::HashMap;

/// Everything Hopback knows about one session.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Session {
    pub id: String,
    pub cwd: String,
    pub title: String,
    /// Live state from the hook; `None` for sessions only known from history.
    pub state: Option<State>,
    pub updated_at: u64,
    pub tool: Option<String>,
}

impl Session {
    pub fn folder(&self) -> &str {
        folder_name(&self.cwd)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct WindowRow {
    pub number: usize,
    pub title: String,
    pub folder: Option<String>,
    pub menu_index: usize,
    pub session: Option<Session>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Board {
    pub windows: Vec<WindowRow>,
    pub needs_you: Vec<Session>,
    pub background: Vec<Session>,
    pub working: usize,
    pub accessibility: bool,
}

/// Pick the session each window shows.
///
/// 1. The session VS Code's own state says the window shows (`shown`, keyed by
///    folder name), if Hopback knows it.
/// 2. Otherwise the most recently active session whose cwd is that folder,
///    preferring live ones.
pub fn map_windows(windows: &[Window], shown: &HashMap<String, Shown>, sessions: &[Session]) -> Vec<Option<String>> {
    windows
        .iter()
        .map(|w| {
            let folder = w.folder.as_deref()?;
            if let Some(s) = shown.get(folder) {
                if sessions.iter().any(|x| x.id == s.session_id) {
                    return Some(s.session_id.clone());
                }
            }
            sessions
                .iter()
                .filter(|s| s.folder() == folder)
                .max_by_key(|s| (s.state.is_some(), s.updated_at))
                .map(|s| s.id.clone())
        })
        .collect()
}

/// Build the board. `live` are sessions with a live marker; `known` adds history
/// sessions (used only to resolve what a window shows).
pub fn build(windows: &[Window], shown: &HashMap<String, Shown>, live: &[Session], known: &[Session], accessibility: bool) -> Board {
    let mut all: Vec<Session> = live.to_vec();
    for k in known {
        if !all.iter().any(|s| s.id == k.id) {
            all.push(k.clone());
        }
    }
    let mapped = map_windows(windows, shown, &all);
    let rows: Vec<WindowRow> = windows
        .iter()
        .zip(&mapped)
        .map(|(w, id)| WindowRow {
            number: w.number,
            title: w.title.clone(),
            folder: w.folder.clone(),
            menu_index: w.menu_index,
            session: id.as_ref().and_then(|id| all.iter().find(|s| &s.id == id).cloned()),
        })
        .collect();
    let shown_ids: Vec<&str> = mapped.iter().flatten().map(String::as_str).collect();
    let mut needs_you: Vec<Session> = live.iter().filter(|s| s.state.is_some_and(State::needs_you)).cloned().collect();
    needs_you.sort_by_key(|s| s.updated_at);
    let mut background: Vec<Session> = live
        .iter()
        .filter(|s| s.state == Some(State::Working) && !shown_ids.contains(&s.id.as_str()))
        .cloned()
        .collect();
    background.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    let working = live.iter().filter(|s| s.state == Some(State::Working)).count();
    Board { windows: rows, needs_you, background, working, accessibility }
}

/// `⚠1 ⚙2 · ▢8`: needs-you count (only when non-zero), working count, and with
/// Accessibility on, how many windows show a session.
pub fn tray_title(b: &Board) -> String {
    let mut s = String::new();
    if !b.needs_you.is_empty() {
        s.push_str(&format!("⚠{} ", b.needs_you.len()));
    }
    s.push_str(&format!("⚙{}", b.working));
    if b.accessibility {
        let showing = b.windows.iter().filter(|w| w.session.is_some()).count();
        s.push_str(&format!(" · ▢{showing}"));
    }
    s
}

/// "now", "3m ago", "5h ago", "2d ago".
pub fn ago(then_ms: u64, now_ms: u64) -> String {
    let secs = now_ms.saturating_sub(then_ms) / 1000;
    match secs {
        0..=59 => "now".into(),
        60..=3599 => format!("{}m ago", secs / 60),
        3600..=86_399 => format!("{}h ago", secs / 3600),
        _ => format!("{}d ago", secs / 86_400),
    }
}

/// One menu row: glyph, folder, title, age.
pub fn row_label(s: &Session, now_ms: u64) -> String {
    let glyph = s.state.map(State::glyph).unwrap_or("·");
    let title = crate::transcript::one_line(&s.title, 60);
    format!("{glyph} {} — {}  ({})", s.folder(), title, ago(s.updated_at, now_ms))
}

/// What a waiting session waits for, for banners.
pub fn waiting_for(s: &Session) -> String {
    match (s.state, s.tool.as_deref()) {
        (Some(State::Question), Some("ExitPlanMode")) => "Plan ready for review".into(),
        (Some(State::Question), _) => "Waiting for your answer".into(),
        (Some(State::Approve), Some(tool)) => format!("Approve {tool}"),
        (Some(State::Approve), None) => "Waiting for approval".into(),
        _ => String::new(),
    }
}

/// The next needs-you session after `last` (wrapping), for ⌃⌥0.
pub fn next_needs_you<'a>(needs: &'a [Session], last: Option<&str>) -> Option<&'a Session> {
    let i = last.and_then(|l| needs.iter().position(|s| s.id == l)).map_or(0, |i| (i + 1) % needs.len().max(1));
    needs.get(i)
}

/// Detect new entries into approve/question. Returns the ids to banner about.
pub fn transitions(prev: &HashMap<String, State>, now: &[Session]) -> Vec<String> {
    now.iter()
        .filter(|s| s.state.is_some_and(State::needs_you) && prev.get(&s.id) != s.state.as_ref())
        .map(|s| s.id.clone())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::windows::number_windows;

    fn s(id: &str, folder: &str, state: Option<State>, at: u64) -> Session {
        Session {
            id: id.into(),
            cwd: format!("/Users/alex/{folder}"),
            title: format!("title {id}"),
            state,
            updated_at: at,
            tool: None,
        }
    }

    fn wins() -> Vec<Window> {
        number_windows(&[(0, "web-app".into()), (1, "a.rs — api-server".into()), (2, "infra".into())])
    }

    #[test]
    fn vscode_state_wins_over_recency() {
        let sessions = vec![
            s("old", "web-app", None, 1),
            s("new", "web-app", Some(State::Working), 9),
            s("api", "api-server", Some(State::Idle), 5),
        ];
        let shown = HashMap::from([("web-app".to_owned(), Shown { session_id: "old".into(), updated_at: 1 })]);
        // A→Z: api-server, infra, web-app
        assert_eq!(map_windows(&wins(), &shown, &sessions), vec![Some("api".into()), None, Some("old".into())]);
    }

    #[test]
    fn fallback_prefers_live_then_recent() {
        let sessions =
            vec![s("hist", "web-app", None, 50), s("live1", "web-app", Some(State::Idle), 10), s("live2", "web-app", Some(State::Idle), 20)];
        let shown = HashMap::from([("web-app".to_owned(), Shown { session_id: "unknown".into(), updated_at: 1 })]);
        assert_eq!(map_windows(&wins(), &shown, &sessions)[2].as_deref(), Some("live2"));
    }

    #[test]
    fn board_sections_and_title() {
        let live = vec![
            s("a", "web-app", Some(State::Approve), 30),
            s("q", "infra", Some(State::Question), 10),
            s("w1", "api-server", Some(State::Working), 40),
            s("w2", "docs-site", Some(State::Working), 50),
            s("i", "web-app", Some(State::Idle), 1),
        ];
        let b = build(&wins(), &HashMap::new(), &live, &[], true);
        assert_eq!(b.needs_you.iter().map(|s| s.id.as_str()).collect::<Vec<_>>(), vec!["q", "a"]);
        // w1 is shown in the api-server window; w2 has no window.
        assert_eq!(b.background.iter().map(|s| s.id.as_str()).collect::<Vec<_>>(), vec!["w2"]);
        assert_eq!(b.working, 2);
        assert_eq!(tray_title(&b), "⚠2 ⚙2 · ▢3");
        let quiet = build(&[], &HashMap::new(), &[s("i", "infra", Some(State::Idle), 1)], &[], false);
        assert_eq!(tray_title(&quiet), "⚙0");
    }

    #[test]
    fn history_sessions_fill_windows_but_not_sections() {
        let known = vec![s("h", "infra", None, 3)];
        let b = build(&wins(), &HashMap::new(), &[], &known, true);
        assert_eq!(b.windows[1].session.as_ref().unwrap().id, "h");
        assert!(b.needs_you.is_empty() && b.background.is_empty());
    }

    #[test]
    fn relative_times() {
        let now = 10_000_000_000;
        assert_eq!(ago(now - 5_000, now), "now");
        assert_eq!(ago(now - 3 * 60_000, now), "3m ago");
        assert_eq!(ago(now - 5 * 3_600_000, now), "5h ago");
        assert_eq!(ago(now - 2 * 86_400_000, now), "2d ago");
        assert_eq!(ago(now + 5, now), "now");
    }

    #[test]
    fn next_needs_you_cycles() {
        let n = vec![s("a", "x", Some(State::Approve), 1), s("b", "y", Some(State::Question), 2)];
        assert_eq!(next_needs_you(&n, None).unwrap().id, "a");
        assert_eq!(next_needs_you(&n, Some("a")).unwrap().id, "b");
        assert_eq!(next_needs_you(&n, Some("b")).unwrap().id, "a");
        assert_eq!(next_needs_you(&n, Some("gone")).unwrap().id, "a");
        assert!(next_needs_you(&[], Some("a")).is_none());
    }

    #[test]
    fn transitions_fire_once() {
        let now = vec![s("a", "x", Some(State::Approve), 1), s("w", "y", Some(State::Working), 1)];
        assert_eq!(transitions(&HashMap::new(), &now), vec!["a".to_string()]);
        let prev = HashMap::from([("a".to_owned(), State::Approve)]);
        assert!(transitions(&prev, &now).is_empty());
        let prev = HashMap::from([("a".to_owned(), State::Question)]);
        assert_eq!(transitions(&prev, &now), vec!["a".to_string()]);
    }

    #[test]
    fn labels() {
        let mut x = s("a", "web-app", Some(State::Approve), 0);
        x.tool = Some("Bash".into());
        assert_eq!(waiting_for(&x), "Approve Bash");
        x.state = Some(State::Question);
        x.tool = Some("ExitPlanMode".into());
        assert_eq!(waiting_for(&x), "Plan ready for review");
        assert_eq!(row_label(&x, 120_000), "❓ web-app — title a  (2m ago)");
    }
}

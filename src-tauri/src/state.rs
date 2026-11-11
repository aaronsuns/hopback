//! Shared application state and the refresh loop that keeps the board current.

use hopback_core::board::{self, Board, Session};
use hopback_core::history::{Index, RefreshStats};
use hopback_core::markers::{self, State};
use hopback_core::settings::Settings;
use hopback_core::vscode::{self, Shown, WorkspaceDir};
use hopback_core::windows;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Child;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

pub fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

pub struct Paths {
    pub home: PathBuf,
    pub markers: PathBuf,
    pub projects: PathBuf,
    pub cache: PathBuf,
    pub settings_file: PathBuf,
    pub log_file: PathBuf,
    pub workspace_storage: PathBuf,
    pub installed_plugins: PathBuf,
}

impl Paths {
    pub fn from_home(home: PathBuf) -> Paths {
        Paths {
            markers: home.join(".claude/hopback"),
            projects: home.join(".claude/projects"),
            cache: home.join("Library/Caches/se.zexa.hopback"),
            settings_file: home.join("Library/Application Support/se.zexa.hopback/settings.json"),
            log_file: home.join("Library/Logs/Hopback/hopback.log"),
            workspace_storage: vscode::default_storage_root(&home),
            installed_plugins: home.join(".claude/plugins/installed_plugins.json"),
            home,
        }
    }
}

#[derive(Default, Clone, serde::Serialize)]
pub struct IndexStatus {
    pub building: bool,
    pub sessions: usize,
    pub last_refresh_ms: Option<u64>,
    pub last_error: Option<String>,
}

pub struct Shared {
    pub paths: Paths,
    pub settings: Mutex<Settings>,
    pub board: Mutex<Board>,
    pub live: Mutex<Vec<Session>>,
    prev_states: Mutex<HashMap<String, State>>,
    pub index: Mutex<Index>,
    pub index_status: Mutex<IndexStatus>,
    index_refreshing: AtomicBool,
    shown: Mutex<(Option<Instant>, HashMap<String, Shown>)>,
    ws_dirs: Mutex<(Option<Instant>, WorkspaceDirs)>,
    titles: Mutex<HashMap<String, TranscriptMeta>>,
    pub needs_cursor: Mutex<Option<String>>,
    pub update_available: Mutex<Option<String>>,
    pub update_status: Mutex<String>,
    pub ask_child: Mutex<Option<Child>>,
    pub ask_cancelled: AtomicBool,
    pub shortcuts_paused: AtomicBool,
    pub vscode_pid: Mutex<Option<i32>>,
    pub hook_seen: AtomicBool,
}

struct TranscriptMeta {
    mtime: u64,
    read_at: Instant,
    title: String,
    cwd: String,
}

pub type App = Arc<Shared>;
type WorkspaceDirs = HashMap<String, Vec<WorkspaceDir>>;

impl Shared {
    pub fn new(paths: Paths, settings: Settings) -> Shared {
        let index = Index::open(&paths.cache);
        let status = IndexStatus { sessions: index.entries.len(), ..Default::default() };
        Shared {
            settings: Mutex::new(settings),
            board: Mutex::new(Board::default()),
            live: Mutex::new(Vec::new()),
            prev_states: Mutex::new(HashMap::new()),
            index: Mutex::new(index),
            index_status: Mutex::new(status),
            index_refreshing: AtomicBool::new(false),
            shown: Mutex::new((None, HashMap::new())),
            ws_dirs: Mutex::new((None, HashMap::new())),
            titles: Mutex::new(HashMap::new()),
            needs_cursor: Mutex::new(None),
            update_available: Mutex::new(None),
            update_status: Mutex::new("not checked yet".into()),
            ask_child: Mutex::new(None),
            ask_cancelled: AtomicBool::new(false),
            shortcuts_paused: AtomicBool::new(false),
            vscode_pid: Mutex::new(None),
            hook_seen: AtomicBool::new(false),
            paths,
        }
    }

    /// Rebuild the history index in the background (incremental). No-op if one
    /// is already running.
    pub fn refresh_index(self: &Arc<Self>, on_done: impl FnOnce(RefreshStats) + Send + 'static) {
        if self.index_refreshing.swap(true, Ordering::SeqCst) {
            return;
        }
        lock(&self.index_status).building = true;
        let me = self.clone();
        std::thread::spawn(move || {
            let t = Instant::now();
            let mut fresh = Index::open(&me.paths.cache);
            let result = fresh.refresh(&me.paths.projects);
            let mut st = lock(&me.index_status);
            st.building = false;
            match result {
                Ok(stats) => {
                    log::info!(
                        "history index: {} sessions, {} updated, {} removed in {:?}",
                        stats.total,
                        stats.updated,
                        stats.removed,
                        t.elapsed()
                    );
                    st.sessions = stats.total;
                    st.last_refresh_ms = Some(hopback_hook::now_ms());
                    st.last_error = None;
                    drop(st);
                    *lock(&me.index) = fresh;
                    me.index_refreshing.store(false, Ordering::SeqCst);
                    on_done(stats);
                }
                Err(e) => {
                    log::warn!("history index refresh failed: {e}");
                    st.last_error = Some(e.to_string());
                    drop(st);
                    me.index_refreshing.store(false, Ordering::SeqCst);
                }
            }
        });
    }

    fn title_for(&self, m: &markers::Marker) -> String {
        if let Some(e) = lock(&self.index).get(&m.session_id) {
            if !e.title.is_empty() {
                return e.title.clone();
            }
        }
        self.transcript_meta(Path::new(&m.transcript_path)).0
    }

    /// (title, cwd, mtime) of a transcript, re-read at most once a minute while it changes.
    fn transcript_meta(&self, path: &Path) -> (String, String, u64) {
        let key = path.to_string_lossy().into_owned();
        let mtime = std::fs::metadata(path)
            .and_then(|x| x.modified())
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map_or(0, |d| d.as_millis() as u64);
        let mut titles = lock(&self.titles);
        if let Some(c) = titles.get(&key) {
            if c.mtime == mtime || c.read_at.elapsed() < Duration::from_secs(60) {
                return (c.title.clone(), c.cwd.clone(), mtime);
            }
        }
        let t = std::fs::File::open(path)
            .ok()
            .map(|f| hopback_core::transcript::parse(std::io::BufReader::new(f), false))
            .unwrap_or_default();
        let title = t.title.unwrap_or_else(|| "New session".to_owned());
        let cwd = t.cwd.unwrap_or_default();
        titles.insert(key, TranscriptMeta { mtime, read_at: Instant::now(), title: title.clone(), cwd: cwd.clone() });
        (title, cwd, mtime)
    }

    /// A session VS Code shows but Hopback has not indexed yet: read its transcript.
    fn session_from_transcript(&self, id: &str) -> Option<Session> {
        if !hopback_hook::valid_session_id(id) {
            return None;
        }
        let file = format!("{id}.jsonl");
        let path = std::fs::read_dir(&self.paths.projects)
            .ok()?
            .flatten()
            .map(|d| d.path().join(&file))
            .find(|p| p.is_file())?;
        let (title, cwd, mtime) = self.transcript_meta(&path);
        Some(Session { id: id.to_owned(), cwd, title, state: None, updated_at: mtime, tool: None })
    }

    fn read_live(&self) -> Vec<Session> {
        let now = hopback_hook::now_ms();
        let (live, ended) = markers::live(markers::read_all(&self.paths.markers), now, markers::pid_alive);
        for id in ended {
            let _ = std::fs::remove_file(hopback_hook::marker_path(&self.paths.markers, &id));
        }
        live.iter()
            .map(|m| Session {
                id: m.session_id.clone(),
                cwd: m.cwd.clone(),
                title: self.title_for(m),
                state: Some(m.state),
                updated_at: m.updated_at,
                tool: m.tool.clone(),
            })
            .collect()
    }

    /// Which session each window's folder shows, per VS Code's saved state.
    fn read_shown(&self, ws: &[windows::Window]) -> HashMap<String, Shown> {
        {
            let s = lock(&self.shown);
            if s.0.is_some_and(|t| t.elapsed() < Duration::from_secs(3)) {
                return s.1.clone();
            }
        }
        let dirs = {
            let mut d = lock(&self.ws_dirs);
            if d.0.is_none_or(|t| t.elapsed() > Duration::from_secs(60)) {
                *d = (Some(Instant::now()), vscode::scan_workspace_storage(&self.paths.workspace_storage));
            }
            d.1.clone()
        };
        let mut out = HashMap::new();
        for w in ws {
            let Some(folder) = &w.folder else { continue };
            if let Some(dir) = dirs.get(folder).and_then(|v| v.first()) {
                let rows = vscode::read_state_rows(&dir.storage.join("state.vscdb"));
                if let Some(s) = vscode::shown_session(&rows) {
                    out.insert(folder.clone(), s);
                }
            }
        }
        *lock(&self.shown) = (Some(Instant::now()), out.clone());
        out
    }

    pub fn windows(&self) -> (bool, Vec<windows::Window>) {
        let trusted = crate::ax::is_trusted();
        let pid = crate::ax::vscode_pid();
        *lock(&self.vscode_pid) = pid;
        let ws = match (trusted, pid) {
            (true, Some(pid)) => crate::ax::list_windows(pid).map(|t| windows::number_windows(&t)).unwrap_or_default(),
            _ => Vec::new(),
        };
        (trusted, ws)
    }

    /// Recompute everything. Returns the sessions that just started waiting.
    pub fn refresh(&self) -> Vec<Session> {
        self.hook_seen.store(markers::hook_seen(&self.paths.markers), Ordering::Relaxed);
        let live = self.read_live();
        let (trusted, ws) = self.windows();
        let shown = if ws.is_empty() { HashMap::new() } else { self.read_shown(&ws) };
        let mut known: Vec<Session> = lock(&self.index)
            .entries
            .iter()
            .map(|e| Session {
                id: e.session_id.clone(),
                cwd: e.cwd.clone(),
                title: e.title.clone(),
                state: None,
                updated_at: e.updated_at,
                tool: None,
            })
            .collect();
        for sh in shown.values() {
            let id = &sh.session_id;
            if !live.iter().chain(known.iter()).any(|s| &s.id == id) {
                known.extend(self.session_from_transcript(id));
            }
        }
        let b = board::build(&ws, &shown, &live, &known, trusted);
        let fresh_ids = {
            let mut prev = lock(&self.prev_states);
            let t = board::transitions(&prev, &live);
            *prev = live.iter().filter_map(|s| s.state.map(|st| (s.id.clone(), st))).collect();
            t
        };
        let fresh = live.iter().filter(|s| fresh_ids.contains(&s.id)).cloned().collect();
        *lock(&self.board) = b;
        *lock(&self.live) = live;
        fresh
    }

    /// Look a session up among live sessions, window rows and history.
    pub fn find_session(&self, id: &str) -> Option<Session> {
        if let Some(s) = lock(&self.live).iter().find(|s| s.id == id) {
            return Some(s.clone());
        }
        if let Some(s) = lock(&self.board).windows.iter().filter_map(|w| w.session.as_ref()).find(|s| s.id == id) {
            return Some(s.clone());
        }
        lock(&self.index).get(id).map(|e| Session {
            id: e.session_id.clone(),
            cwd: e.cwd.clone(),
            title: e.title.clone(),
            state: None,
            updated_at: e.updated_at,
            tool: None,
        })
    }

    pub fn claude_path(&self) -> Option<PathBuf> {
        let setting = lock(&self.settings).claude_path.clone();
        let path = login_shell_path();
        hopback_core::ask::find_claude(&setting, path.as_deref(), &self.paths.home, |p: &Path| p.is_file())
    }
}

/// The PATH a login shell would have (GUI apps get a minimal PATH). Cached.
pub fn login_shell_path() -> Option<String> {
    static CACHE: std::sync::OnceLock<Option<String>> = std::sync::OnceLock::new();
    CACHE
        .get_or_init(|| {
            let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".into());
            let mut child = std::process::Command::new(shell)
                .args(["-l", "-c", "printf %s \"$PATH\""])
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::null())
                .spawn()
                .ok()?;
            let deadline = Instant::now() + Duration::from_secs(5);
            loop {
                match child.try_wait() {
                    Ok(Some(_)) => break,
                    Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(50)),
                    _ => {
                        let _ = child.kill();
                        return None;
                    }
                }
            }
            let mut out = String::new();
            use std::io::Read;
            child.stdout.take()?.read_to_string(&mut out).ok()?;
            Some(out).filter(|s| !s.is_empty())
        })
        .clone()
}

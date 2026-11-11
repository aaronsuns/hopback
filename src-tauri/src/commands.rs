//! Commands the frontend calls.

use crate::state::{lock, App};
use hopback_core::board::{self, Session};
use hopback_core::diagnostics::{self, Check};
use hopback_core::markers::State;
use hopback_core::plugin::{self, PluginStep};
use hopback_core::search::{self, MatchKind};
use hopback_core::settings::Settings;
use hopback_core::{ask, shortcuts};
use serde::Serialize;
use std::io::Read;
use std::process::{Command, Stdio};
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};
use tauri::{AppHandle, State as TState};
use tauri_plugin_autostart::ManagerExt;
use tauri_plugin_clipboard_manager::ClipboardExt;

type Shared<'a> = TState<'a, App>;

#[derive(Serialize)]
pub struct Row {
    session_id: String,
    folder: String,
    title: String,
    state: Option<State>,
    glyph: String,
    ago: String,
    window: Option<usize>,
    snippet: Option<String>,
    content_match: bool,
    reason: Option<String>,
}

fn to_row(s: &Session, window: Option<usize>, now: u64) -> Row {
    Row {
        session_id: s.id.clone(),
        folder: s.folder().to_owned(),
        title: s.title.clone(),
        state: s.state,
        glyph: s.state.map(State::glyph).unwrap_or("·").to_owned(),
        ago: board::ago(s.updated_at, now),
        window,
        snippet: None,
        content_match: false,
        reason: None,
    }
}

/// Live sessions plus the sessions windows show (open-sessions mode).
fn open_sessions(app: &App) -> Vec<(Session, Option<usize>)> {
    let b = lock(&app.board).clone();
    let mut out: Vec<(Session, Option<usize>)> = Vec::new();
    for w in &b.windows {
        if let Some(s) = &w.session {
            let live = lock(&app.live).iter().find(|l| l.id == s.id).cloned();
            out.push((live.unwrap_or_else(|| s.clone()), Some(w.number)));
        }
    }
    for s in lock(&app.live).iter() {
        if !out.iter().any(|(o, _)| o.id == s.id) {
            out.push((s.clone(), None));
        }
    }
    out
}

#[tauri::command]
pub fn search(app: Shared, mode: String, query: String) -> Vec<Row> {
    let now = hopback_hook::now_ms();
    if mode == "history" {
        let mut idx = lock(&app.index);
        if query.trim().chars().count() >= search::MIN_CONTENT_QUERY {
            idx.load_texts();
        }
        let entries: Vec<search::Entry> = idx
            .entries
            .iter()
            .map(|e| search::Entry {
                title: &e.title,
                folder: &e.folder,
                updated_at: e.updated_at,
                text: idx.cached_text(&e.session_id),
            })
            .collect();
        let hits = search::search(&entries, &query);
        let live = lock(&app.live).clone();
        return hits
            .into_iter()
            .take(200)
            .map(|h| {
                let e = &idx.entries[h.index];
                let state = live.iter().find(|l| l.id == e.session_id).and_then(|l| l.state);
                let s = Session {
                    id: e.session_id.clone(),
                    cwd: e.cwd.clone(),
                    title: e.title.clone(),
                    state,
                    updated_at: e.updated_at,
                    tool: None,
                };
                Row { snippet: h.snippet, content_match: h.kind == MatchKind::Content, ..to_row(&s, None, now) }
            })
            .collect();
    }
    let sessions = open_sessions(&app);
    let entries: Vec<search::Entry> = sessions
        .iter()
        .map(|(s, _)| search::Entry { title: &s.title, folder: s.folder(), updated_at: s.updated_at, text: None })
        .collect();
    search::search(&entries, &query)
        .into_iter()
        .map(|h| to_row(&sessions[h.index].0, sessions[h.index].1, now))
        .collect()
}

#[tauri::command]
pub fn index_status(app: Shared) -> crate::state::IndexStatus {
    lock(&app.index_status).clone()
}

#[tauri::command]
pub fn refresh_history(app: Shared) {
    app.inner().refresh_index(|_| {});
}

#[tauri::command]
pub fn go_to(app: Shared, session_id: String) {
    crate::actions::go_to_session(app.inner(), &session_id);
}

#[tauri::command]
pub async fn ask(app: Shared<'_>, question: String) -> Result<Vec<Row>, String> {
    let app = app.inner().clone();
    tauri::async_runtime::spawn_blocking(move || run_ask(&app, &question)).await.map_err(|e| e.to_string())?
}

fn run_ask(app: &App, question: &str) -> Result<Vec<Row>, String> {
    let claude = app.claude_path().ok_or("Could not find `claude`. Set its path in Settings → General.")?;
    let root = lock(&app.index).root.clone();
    if !root.join(hopback_core::history::INDEX_FILE).exists() {
        return Err("The history index is not ready yet.".into());
    }
    let mut cmd = Command::new(&claude);
    cmd.args(ask::args()).current_dir(&root).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null());
    if let Some(p) = crate::state::login_shell_path() {
        cmd.env("PATH", p);
    }
    let mut child = cmd.spawn().map_err(|e| format!("Could not start claude: {e}"))?;
    if let Some(mut stdin) = child.stdin.take() {
        use std::io::Write;
        let _ = stdin.write_all(ask::prompt(question).as_bytes());
    }
    let mut stdout = child.stdout.take().ok_or("no output")?;
    let reader = std::thread::spawn(move || {
        let mut s = String::new();
        let _ = stdout.read_to_string(&mut s);
        s
    });
    app.ask_cancelled.store(false, Ordering::SeqCst);
    *lock(&app.ask_child) = Some(child);
    let deadline = Instant::now() + Duration::from_secs(ask::TIMEOUT_SECS);
    let outcome = loop {
        if app.ask_cancelled.load(Ordering::SeqCst) {
            break Err("Cancelled.".to_string());
        }
        let mut guard = lock(&app.ask_child);
        let Some(child) = guard.as_mut() else { break Err("Cancelled.".to_string()) };
        match child.try_wait() {
            Ok(Some(status)) if status.success() => break Ok(()),
            Ok(Some(_)) => break Err("claude exited with an error. Is it logged in?".to_string()),
            Ok(None) if Instant::now() > deadline => break Err("Claude took too long (90 s).".to_string()),
            Ok(None) => {}
            Err(e) => break Err(e.to_string()),
        }
        drop(guard);
        std::thread::sleep(Duration::from_millis(100));
    };
    if let Some(mut c) = lock(&app.ask_child).take() {
        let _ = c.kill();
        let _ = c.wait();
    }
    outcome?;
    let out = reader.join().unwrap_or_default();
    let now = hopback_hook::now_ms();
    let hits = {
        let idx = lock(&app.index);
        ask::parse_answer(&out, |id| idx.get(id).is_some())?
    };
    Ok(hits
        .into_iter()
        .filter_map(|h| {
            let s = app.find_session(&h.session_id)?;
            Some(Row { reason: Some(h.reason), ..to_row(&s, None, now) })
        })
        .collect())
}

#[tauri::command]
pub fn cancel_ask(app: Shared) {
    app.ask_cancelled.store(true, Ordering::SeqCst);
    if let Some(mut c) = lock(&app.ask_child).take() {
        let _ = c.kill();
        let _ = c.wait();
    }
}

#[derive(Serialize)]
pub struct ShortcutRow {
    id: String,
    label: String,
    accel: Option<String>,
    display: String,
}

#[derive(Serialize)]
pub struct SettingsView {
    settings: Settings,
    shortcuts: Vec<ShortcutRow>,
    version: String,
}

fn settings_view(app: &App, handle: &AppHandle) -> SettingsView {
    let settings = lock(&app.settings).clone();
    let shortcuts = shortcuts::effective(&settings.shortcuts)
        .into_iter()
        .map(|(id, a)| ShortcutRow {
            id: id.into(),
            label: shortcuts::ACTIONS.iter().find(|x| x.0 == id).map_or(id, |x| x.1).into(),
            display: a.as_ref().map(|a| a.display()).unwrap_or_default(),
            accel: a.map(|a| a.canonical()),
        })
        .collect();
    SettingsView { settings, shortcuts, version: handle.package_info().version.to_string() }
}

fn save(app: &App) {
    let s = lock(&app.settings).clone();
    if let Err(e) = s.save(&app.paths.settings_file) {
        log::warn!("could not save settings: {e}");
    }
}

#[tauri::command]
pub fn get_settings(app: Shared, handle: AppHandle) -> SettingsView {
    settings_view(app.inner(), &handle)
}

/// Save the General tab. Shortcuts are kept as they are.
#[tauri::command]
pub fn set_general(app: Shared, handle: AppHandle, settings: Settings) -> SettingsView {
    {
        let mut cur = lock(&app.settings);
        let shortcuts = cur.shortcuts.clone();
        *cur = Settings { shortcuts, ..settings };
    }
    save(app.inner());
    crate::apply_autostart(&handle, app.inner());
    settings_view(app.inner(), &handle)
}

#[tauri::command]
pub fn set_shortcut(app: Shared, handle: AppHandle, id: String, accel: Option<String>) -> Result<SettingsView, String> {
    if !shortcuts::ACTIONS.iter().any(|a| a.0 == id) {
        return Err("Unknown action.".into());
    }
    let user = lock(&app.settings).shortcuts.clone();
    let value = match accel {
        Some(a) => Some(shortcuts::validate(&id, &a, &user).map_err(|r| r.0)?.canonical()),
        None => None,
    };
    lock(&app.settings).shortcuts.insert(id, value);
    save(app.inner());
    crate::keys::register_all(&handle, app.inner());
    crate::tray::update(&handle, app.inner());
    Ok(settings_view(app.inner(), &handle))
}

#[tauri::command]
pub fn reset_shortcuts(app: Shared, handle: AppHandle) -> SettingsView {
    lock(&app.settings).shortcuts.clear();
    save(app.inner());
    crate::keys::register_all(&handle, app.inner());
    settings_view(app.inner(), &handle)
}

/// Pause all shortcuts while the recorder listens, so it can see the keys.
#[tauri::command]
pub fn pause_shortcuts(app: Shared, handle: AppHandle, paused: bool) {
    app.shortcuts_paused.store(paused, Ordering::SeqCst);
    crate::keys::register_all(&handle, app.inner());
}

#[derive(Serialize)]
pub struct WelcomeStatus {
    accessibility: bool,
    plugin: PluginStep,
    start_at_login: bool,
    plugin_commands: Vec<String>,
}

fn plugin_installed(app: &App) -> bool {
    std::fs::read_to_string(&app.paths.installed_plugins).is_ok_and(|s| diagnostics::plugin_installed(&s))
}

#[tauri::command]
pub fn welcome_status(app: Shared, handle: AppHandle) -> WelcomeStatus {
    WelcomeStatus {
        accessibility: crate::ax::is_trusted(),
        plugin: plugin::step(plugin_installed(&app), hopback_core::markers::hook_seen(&app.paths.markers)),
        start_at_login: handle.autolaunch().is_enabled().unwrap_or(false),
        plugin_commands: plugin::manual_commands(),
    }
}

/// `Some(true)` allowed, `Some(false)` off, `None` macOS has not been asked yet.
#[tauri::command]
pub fn notification_permission() -> Option<bool> {
    crate::banners::allowed()
}

#[derive(Serialize)]
pub struct InstallResult {
    ok: bool,
    /// False when `claude` could not be found; the UI then points at the manual way.
    claude_found: bool,
    /// The last few lines of output when it failed.
    output: String,
}

const INSTALL_TIMEOUT: Duration = Duration::from_secs(120);
const INSTALL_TAIL_LINES: usize = 8;

/// Run `claude plugin marketplace add …` then `claude plugin install …`.
#[tauri::command]
pub async fn install_plugin(app: Shared<'_>) -> Result<InstallResult, String> {
    let app = app.inner().clone();
    tauri::async_runtime::spawn_blocking(move || run_install(&app)).await.map_err(|e| e.to_string())
}

fn run_install(app: &App) -> InstallResult {
    let Some(claude) = app.claude_path() else {
        return InstallResult { ok: false, claude_found: false, output: String::new() };
    };
    let mut out = String::new();
    let mut ok = false;
    // The marketplace step fails harmlessly when it was added before, so the
    // install step decides the outcome.
    for args in plugin::INSTALL_ARGS {
        out.push_str(&format!("$ claude {}\n", args.join(" ")));
        match run_captured(&claude, args, &app.paths.home) {
            Ok((success, text)) => {
                out.push_str(&text);
                ok = success;
            }
            Err(e) => {
                out.push_str(&e);
                ok = false;
            }
        }
        out.push('\n');
    }
    if ok {
        log::info!("plugin installed");
    } else {
        log::warn!("plugin install failed: {}", plugin::tail(&out, INSTALL_TAIL_LINES));
    }
    InstallResult {
        ok,
        claude_found: true,
        output: if ok { String::new() } else { plugin::tail(&out, INSTALL_TAIL_LINES) },
    }
}

/// Run `claude args…`, returning success and stdout+stderr.
fn run_captured(claude: &std::path::Path, args: &[&str], cwd: &std::path::Path) -> Result<(bool, String), String> {
    let mut cmd = Command::new(claude);
    cmd.args(args).current_dir(cwd).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    if let Some(p) = crate::state::login_shell_path() {
        cmd.env("PATH", p);
    }
    let mut child = cmd.spawn().map_err(|e| format!("Could not start claude: {e}"))?;
    let pipe = |r: Option<Box<dyn Read + Send>>| {
        std::thread::spawn(move || {
            let mut s = String::new();
            if let Some(mut r) = r {
                let _ = r.read_to_string(&mut s);
            }
            s
        })
    };
    let out = pipe(child.stdout.take().map(|r| Box::new(r) as Box<dyn Read + Send>));
    let err = pipe(child.stderr.take().map(|r| Box::new(r) as Box<dyn Read + Send>));
    let deadline = Instant::now() + INSTALL_TIMEOUT;
    let status = loop {
        match child.try_wait() {
            Ok(Some(s)) => break Ok(s.success()),
            Ok(None) if Instant::now() > deadline => {
                let _ = child.kill();
                let _ = child.wait();
                break Err("claude took too long (2 min).".to_string());
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(100)),
            Err(e) => break Err(e.to_string()),
        }
    };
    let text = format!("{}{}", out.join().unwrap_or_default(), err.join().unwrap_or_default());
    match status {
        Ok(success) => Ok((success, text)),
        Err(e) => Err(format!("{text}{e}")),
    }
}

#[tauri::command]
pub fn open_accessibility() {
    crate::actions::open_accessibility_settings();
}

#[tauri::command]
pub fn test_banner(handle: AppHandle) {
    crate::banners::show(
        "Hopback".into(),
        "Banners are on".into(),
        "You'll see one when a session needs you.".into(),
        || {},
    );
    let _ = handle;
}

#[tauri::command]
pub fn open_notification_settings() {
    crate::actions::open_notification_settings();
}

#[tauri::command]
pub fn set_start_at_login(app: Shared, handle: AppHandle, on: bool) -> bool {
    lock(&app.settings).start_at_login = on;
    save(app.inner());
    crate::apply_autostart(&handle, app.inner());
    handle.autolaunch().is_enabled().unwrap_or(false)
}

#[tauri::command]
pub fn finish_welcome(app: Shared, handle: AppHandle) {
    lock(&app.settings).welcome_done = true;
    save(app.inner());
    if let Some(w) = tauri::Manager::get_webview_window(&handle, "welcome") {
        let _ = w.close();
    }
}

#[tauri::command]
pub fn copy_text(handle: AppHandle, text: String) {
    let _ = handle.clipboard().write_text(text);
}

#[derive(Serialize)]
pub struct DiagnosticsView {
    checks: Vec<Check>,
    report: String,
}

fn os_version() -> String {
    Command::new("sw_vers")
        .arg("-productVersion")
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
        .unwrap_or_default()
}

pub fn collect_diagnostics(app: &App, handle: &AppHandle) -> DiagnosticsView {
    let version = handle.package_info().version.to_string();
    let p = &app.paths;
    let trusted = crate::ax::is_trusted();
    let pid = crate::ax::vscode_pid();
    let wins = lock(&app.board).windows.len();
    let plugin = plugin_installed(app);
    let hook = hopback_core::markers::hook_seen(&p.markers);
    let markers = hopback_core::markers::read_all(&p.markers).len();
    let transcripts = hopback_core::history::list_transcripts(&p.projects).len();
    let st = lock(&app.index_status).clone();
    let claude = app.claude_path();
    let update = lock(&app.update_status).clone();
    let checks = vec![
        Check::new("Hopback", Some(true), format!("version {version}"), ""),
        Check::new(
            "Accessibility",
            Some(trusted),
            if trusted { "on" } else { "off" },
            "Turn it on in System Settings → Privacy & Security → Accessibility.",
        ),
        Check::new(
            "VS Code",
            Some(pid.is_some() && (wins > 0 || !trusted)),
            match pid {
                Some(_) if trusted => format!("running, {wins} window(s) found"),
                Some(_) => "running (windows need Accessibility)".into(),
                None => "not running".into(),
            },
            "Start Visual Studio Code. Other editors are not supported yet.",
        ),
        Check::new(
            "Claude Code plugin",
            Some(plugin),
            if plugin { "installed" } else { "not installed" },
            "In a terminal: claude plugin marketplace add aaronsuns/hopback, then claude plugin install hopback@hopback.",
        ),
        Check::new(
            "Hook seen",
            Some(hook),
            if hook { "yes" } else { "not yet" },
            "Open a new Claude Code session (or reload the VS Code window); sessions that were already open don't load new plugins.",
        ),
        Check::new(
            "Marker directory",
            Some(p.markers.is_dir()),
            format!("{} live marker(s)", markers),
            "Created by the plugin's hook on first use.",
        ),
        Check::new(
            "Transcripts",
            Some(transcripts > 0),
            format!("{transcripts} found"),
            "Claude Code keeps them under ~/.claude/projects.",
        ),
        Check::new(
            "History index",
            Some(st.last_error.is_none()),
            if st.building {
                format!("building… ({} sessions so far)", st.sessions)
            } else {
                format!("{} sessions", st.sessions)
            },
            st.last_error.clone().unwrap_or_default(),
        ),
        Check::new(
            "claude for Ask",
            Some(claude.is_some()),
            if claude.is_some() { "found" } else { "not found" },
            "Set the path to claude in Settings → General.",
        ),
        match crate::banners::allowed() {
            Some(on) => Check::new(
                "Notifications",
                Some(on),
                if on { "allowed" } else { "off" },
                "Allow Hopback in System Settings → Notifications.",
            ),
            None => Check::new("Notifications", None, "not asked yet; use Send test banner in the welcome window", ""),
        },
        Check::new("Update check", None, update, ""),
    ];
    let report = diagnostics::report(&version, &os_version(), &checks);
    DiagnosticsView { checks, report }
}

#[tauri::command]
pub fn get_diagnostics(app: Shared, handle: AppHandle) -> DiagnosticsView {
    collect_diagnostics(app.inner(), &handle)
}

#[tauri::command]
pub fn app_version(handle: AppHandle) -> String {
    handle.package_info().version.to_string()
}

#[tauri::command]
pub fn open_url(url: String) {
    if url.starts_with("https://") {
        let _ = Command::new("open").arg(url).status();
    }
}

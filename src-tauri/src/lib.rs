//! Hopback: a menu bar app that tracks Claude Code sessions in VS Code.

mod actions;
mod ax;
mod banners;
mod commands;
mod keys;
mod state;
mod tray;
mod updater;
mod views;

use state::{lock, App, Paths, Shared};
use std::sync::mpsc;
use std::sync::Arc;
use std::time::Duration;
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Emitter, RunEvent};
use tauri_plugin_autostart::{MacosLauncher, ManagerExt};
use tauri_plugin_global_shortcut::ShortcutState;

const POLL: Duration = Duration::from_secs(2);
const HISTORY_EVERY: Duration = Duration::from_secs(10 * 60);

fn home() -> std::path::PathBuf {
    std::env::var_os("HOME").map(Into::into).unwrap_or_else(|| "/".into())
}

fn init_logging(paths: &Paths) {
    let logger = hopback_core::logging::FileLogger::new(
        paths.log_file.clone(),
        hopback_core::logging::MAX_BYTES,
        hopback_core::logging::KEEP,
    );
    if log::set_boxed_logger(Box::new(logger)).is_ok() {
        log::set_max_level(log::LevelFilter::Info);
    }
}

/// Keep the login item in line with the setting. Only for an installed app,
/// so development builds never register themselves as login items.
pub(crate) fn apply_autostart(app: &AppHandle, shared: &App) {
    let installed = std::env::current_exe().is_ok_and(|p| p.starts_with("/Applications"));
    if !installed {
        return;
    }
    let want = lock(&shared.settings).start_at_login;
    let al = app.autolaunch();
    let is = al.is_enabled().unwrap_or(false);
    let r = match (want, is) {
        (true, false) => al.enable(),
        (false, true) => al.disable(),
        _ => Ok(()),
    };
    if let Err(e) = r {
        log::warn!("start at login: {e}");
    }
}

fn on_menu(app: &AppHandle, shared: &App, id: &str) {
    match id {
        "quit" => app.exit(0),
        "settings" => views::open_settings(app),
        "diagnostics" => views::open_diagnostics(app),
        "about" => views::open_about(app),
        "search_open" => views::open_search(app, "open"),
        "search_history" => views::open_search(app, "history"),
        "status:ax" => actions::open_accessibility_settings(),
        "status:plugin" => views::open_welcome(app),
        "status:update" => views::open_settings(app),
        other => {
            if let Some(sid) = other.strip_prefix("go:") {
                actions::go_to_session(shared, sid);
            } else if let Some(n) = other.strip_prefix("win:").and_then(|n| n.parse().ok()) {
                let row = lock(&shared.board).windows.iter().find(|w| w.number == n).cloned();
                match row.and_then(|w| w.session) {
                    Some(s) => actions::go_to_session(shared, &s.id),
                    None => actions::go_to_window_number(shared, n),
                }
            }
        }
    }
}

/// The refresh loop: on marker changes (debounced) or every couple of seconds.
fn start_refresh_loop(app: AppHandle, shared: App) {
    let (tx, rx) = mpsc::channel::<()>();
    let watcher = hopback_core::watch::watch_dir(&shared.paths.markers, move || {
        let _ = tx.send(());
    });
    if let Err(e) = &watcher {
        log::warn!("could not watch the marker directory: {e}");
    }
    std::thread::spawn(move || {
        let _keep = watcher;
        let mut last_history = std::time::Instant::now();
        loop {
            let fresh = shared.refresh();
            banners::announce(&shared, fresh);
            tray::update(&app, &shared);
            let _ = app.emit("board-changed", ());
            if last_history.elapsed() > HISTORY_EVERY {
                last_history = std::time::Instant::now();
                shared.refresh_index(|_| {});
            }
            if rx.recv_timeout(POLL).is_ok() {
                // Debounce bursts of hook writes.
                std::thread::sleep(Duration::from_millis(150));
                while rx.try_recv().is_ok() {}
            }
        }
    });
}

pub fn run() {
    let paths = Paths::from_home(home());
    init_logging(&paths);
    let (settings, corrupt) = hopback_core::settings::Settings::load(&paths.settings_file);
    if corrupt {
        log::warn!("settings file was unreadable; using defaults");
    }
    let shared: App = Arc::new(Shared::new(paths, settings));
    if std::env::args().any(|a| a == "--print-board") {
        // Maintainer aid: print what Hopback sees (windows, mapping, sessions) and exit.
        shared.refresh();
        let b = lock(&shared.board).clone();
        println!("{}", serde_json::to_string_pretty(&b).unwrap_or_default());
        return;
    }
    log::info!("Hopback {} starting", env!("CARGO_PKG_VERSION"));

    let for_keys = shared.clone();
    let for_setup = shared.clone();
    let result = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _, _| views::open_search(app, "open")))
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_autostart::init(MacosLauncher::LaunchAgent, None))
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(move |app, shortcut, event| {
                    if event.state() == ShortcutState::Pressed {
                        if let Some(action) = keys::action_for(shortcut) {
                            keys::run(app, &for_keys, action);
                        }
                    }
                })
                .build(),
        )
        .manage(shared.clone())
        .invoke_handler(tauri::generate_handler![
            commands::search,
            commands::index_status,
            commands::refresh_history,
            commands::go_to,
            commands::ask,
            commands::cancel_ask,
            commands::get_settings,
            commands::set_general,
            commands::set_shortcut,
            commands::reset_shortcuts,
            commands::pause_shortcuts,
            commands::welcome_status,
            commands::open_accessibility,
            commands::test_banner,
            commands::open_notification_settings,
            commands::set_start_at_login,
            commands::finish_welcome,
            commands::copy_text,
            commands::get_diagnostics,
            commands::app_version,
            commands::open_url,
        ])
        .setup(move |app| {
            let shared = for_setup;
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);
            banners::init();
            let menu_shared = shared.clone();
            TrayIconBuilder::with_id(tray::TRAY_ID)
                .title("⚙0")
                .tooltip("Hopback")
                .show_menu_on_left_click(true)
                .on_menu_event(move |app, ev| on_menu(app, &menu_shared, ev.id().as_ref()))
                .build(app)?;
            let handle = app.handle().clone();
            keys::register_all(&handle, &shared);
            apply_autostart(&handle, &shared);
            let h2 = handle.clone();
            shared.refresh_index(move |_| {
                let _ = h2.emit("index-changed", ());
            });
            start_refresh_loop(handle.clone(), shared.clone());
            updater::start(handle.clone(), shared.clone());
            if !lock(&shared.settings).welcome_done {
                views::open_welcome(&handle);
            }
            Ok(())
        })
        .build(tauri::generate_context!());
    let app = match result {
        Ok(a) => a,
        Err(e) => {
            log::error!("could not start: {e}");
            return;
        }
    };
    app.run(|_, event| {
        if let RunEvent::ExitRequested { code: None, api, .. } = event {
            api.prevent_exit(); // closing the last window must not quit a menu bar app
        }
    });
}

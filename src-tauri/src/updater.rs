//! Update checks via the Tauri updater plugin: on start and every 6 hours.
//!
//! The signing public key and the endpoint live in `tauri.conf.json`
//! (`plugins.updater`). Until a key is configured there, updates are disabled.

use crate::state::{lock, App};
use std::time::Duration;
use tauri::AppHandle;
use tauri_plugin_updater::UpdaterExt;

const EVERY: Duration = Duration::from_secs(6 * 60 * 60);
const RETRY_WHEN_BUSY: Duration = Duration::from_secs(10 * 60);

pub fn configured(app: &AppHandle) -> bool {
    app.config()
        .plugins
        .0
        .get("updater")
        .and_then(|u| u.get("pubkey"))
        .and_then(|k| k.as_str())
        .is_some_and(|k| !k.trim().is_empty())
}

pub fn start(app: AppHandle, shared: App) {
    if !configured(&app) {
        *lock(&shared.update_status) = "updates not configured in this build".into();
        return;
    }
    if let Err(e) = app.plugin(tauri_plugin_updater::Builder::new().build()) {
        *lock(&shared.update_status) = format!("updater unavailable: {e}");
        return;
    }
    tauri::async_runtime::spawn(async move {
        loop {
            let wait = if lock(&shared.settings).check_updates { check(&app, &shared).await } else { EVERY };
            tokio_sleep(wait).await;
        }
    });
}

async fn tokio_sleep(d: Duration) {
    let _ = tauri::async_runtime::spawn_blocking(move || std::thread::sleep(d)).await;
}

/// Returns how long to wait before the next check.
async fn check(app: &AppHandle, shared: &App) -> Duration {
    let update = match app.updater() {
        Ok(u) => u.check().await,
        Err(e) => Err(e),
    };
    let update = match update {
        Ok(Some(u)) => u,
        Ok(None) => {
            *lock(&shared.update_status) = "up to date".into();
            return EVERY;
        }
        Err(e) => {
            log::warn!("update check failed: {e}");
            *lock(&shared.update_status) = "check failed".into();
            return EVERY;
        }
    };
    let version = update.version.clone();
    *lock(&shared.update_status) = format!("{version} available");
    let auto = lock(&shared.settings).install_updates_automatically;
    if !auto {
        *lock(&shared.update_available) = Some(version);
        crate::tray::update(app, shared);
        return EVERY;
    }
    if crate::views::any_open(app) {
        return RETRY_WHEN_BUSY; // restart only when no Hopback window is open
    }
    match update.download_and_install(|_, _| {}, || {}).await {
        Ok(()) => {
            log::info!("installed update {version}");
            crate::banners::show("Hopback".into(), format!("Hopback updated to {version}"), String::new(), || {});
            std::thread::sleep(Duration::from_secs(2));
            app.restart();
        }
        Err(e) => {
            log::warn!("update install failed: {e}");
            *lock(&shared.update_available) = Some(version);
            crate::tray::update(app, shared);
            EVERY
        }
    }
}

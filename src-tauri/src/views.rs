//! Hopback's windows. They all load the same frontend with a route in the hash.

use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};

pub fn open(app: &AppHandle, label: &str, route: &str, title: &str, size: (f64, f64), on_top: bool) {
    if let Some(w) = app.get_webview_window(label) {
        let _ = w.eval(format!("window.location.hash = {}", serde_json::to_string(route).unwrap_or_default()));
        let _ = w.show();
        let _ = w.set_focus();
        return;
    }
    let built = WebviewWindowBuilder::new(app, label, WebviewUrl::App(format!("index.html#{route}").into()))
        .title(title)
        .inner_size(size.0, size.1)
        .resizable(true)
        .always_on_top(on_top)
        .center()
        .focused(true)
        .build();
    match built {
        Ok(w) => {
            let _ = w.set_focus();
        }
        Err(e) => log::warn!("could not open the {label} window: {e}"),
    }
}

pub fn open_search(app: &AppHandle, mode: &str) {
    open(app, "search", &format!("search/{mode}"), "Hopback", (680.0, 460.0), true);
}

pub fn open_settings(app: &AppHandle) {
    open(app, "settings", "settings", "Hopback Settings", (600.0, 600.0), false);
}

pub fn open_welcome(app: &AppHandle) {
    open(app, "welcome", "welcome", "Welcome to Hopback", (580.0, 600.0), false);
}

pub fn open_diagnostics(app: &AppHandle) {
    open(app, "diagnostics", "diagnostics", "Hopback Diagnostics", (680.0, 520.0), false);
}

pub fn open_about(app: &AppHandle) {
    open(app, "about", "about", "About Hopback", (380.0, 300.0), false);
}

pub fn any_open(app: &AppHandle) -> bool {
    app.webview_windows().values().any(|w| w.is_visible().unwrap_or(false))
}

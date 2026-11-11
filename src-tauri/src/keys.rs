//! Registering the global shortcuts from settings.

use crate::state::{lock, App};
use hopback_core::shortcuts;
use std::collections::HashMap;
use std::sync::Mutex;
use tauri::AppHandle;
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut};

static ACTIONS: Mutex<Option<HashMap<u32, &'static str>>> = Mutex::new(None);

pub fn action_for(shortcut: &Shortcut) -> Option<&'static str> {
    lock(&ACTIONS).as_ref()?.get(&shortcut.id()).copied()
}

/// (Re-)register every shortcut. Failures are logged, not fatal.
pub fn register_all(app: &AppHandle, shared: &App) {
    let gs = app.global_shortcut();
    let _ = gs.unregister_all();
    let mut map = HashMap::new();
    if !shared.shortcuts_paused.load(std::sync::atomic::Ordering::SeqCst) {
        let user = lock(&shared.settings).shortcuts.clone();
        for (id, accel) in shortcuts::effective(&user) {
            let Some(accel) = accel else { continue };
            match accel.to_plugin_string().parse::<Shortcut>() {
                Ok(sc) => match gs.register(sc) {
                    Ok(()) => {
                        map.insert(sc.id(), id);
                    }
                    Err(e) => log::warn!("shortcut {} for {id} not registered: {e}", accel.display()),
                },
                Err(e) => log::warn!("shortcut {} for {id} not understood: {e}", accel.display()),
            }
        }
    }
    *lock(&ACTIONS) = Some(map);
}

pub fn run(app: &AppHandle, shared: &App, action: &str) {
    use crate::actions::*;
    match action {
        "next_needs_you" => next_needs_you(shared),
        "next_window" => cycle_window(shared, true),
        "prev_window" => cycle_window(shared, false),
        "search_open" => crate::views::open_search(app, "open"),
        "search_history" => crate::views::open_search(app, "history"),
        a => {
            if let Some(n) = a.strip_prefix("window").and_then(|n| n.parse().ok()) {
                go_to_window_number(shared, n);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use hopback_core::shortcuts::{effective, ACTIONS};
    use std::collections::BTreeMap;
    use tauri_plugin_global_shortcut::Shortcut;

    #[test]
    fn every_default_parses_for_the_plugin() {
        for (id, accel) in effective(&BTreeMap::new()) {
            let s = accel.unwrap().to_plugin_string();
            assert!(s.parse::<Shortcut>().is_ok(), "{id}: {s}");
        }
        assert_eq!(ACTIONS.len(), 14);
    }
}

//! User settings, stored as JSON with atomic writes. A missing or corrupt file
//! means defaults.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Per action: `Some(accel)` to override, `None` to clear. Missing = default.
    pub shortcuts: BTreeMap<String, Option<String>>,
    pub banners_approve: bool,
    pub banners_question: bool,
    /// Path to `claude`; empty = find it automatically.
    pub claude_path: String,
    pub start_at_login: bool,
    pub check_updates: bool,
    pub install_updates_automatically: bool,
    /// Set once the welcome window has been finished.
    pub welcome_done: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            shortcuts: BTreeMap::new(),
            banners_approve: true,
            banners_question: true,
            claude_path: String::new(),
            start_at_login: true,
            check_updates: true,
            install_updates_automatically: true,
            welcome_done: false,
        }
    }
}

impl Settings {
    /// Load, falling back to defaults on any problem. The bool says whether the
    /// file existed but could not be read (so the caller can log it).
    pub fn load(path: &Path) -> (Settings, bool) {
        match std::fs::read(path) {
            Ok(bytes) => match serde_json::from_slice(&bytes) {
                Ok(s) => (s, false),
                Err(_) => (Settings::default(), true),
            },
            Err(_) => (Settings::default(), false),
        }
    }

    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let bytes = serde_json::to_vec_pretty(self).map_err(std::io::Error::other)?;
        hopback_hook::write_atomic(path, &bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_and_corrupt_files_give_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("settings.json");
        assert_eq!(Settings::load(&p), (Settings::default(), false));
        std::fs::write(&p, b"{ not json").unwrap();
        assert_eq!(Settings::load(&p), (Settings::default(), true));
    }

    #[test]
    fn round_trip_and_partial_files() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("nested/settings.json");
        let mut s = Settings { banners_question: false, claude_path: "/opt/claude".into(), ..Default::default() };
        s.shortcuts.insert("window1".into(), None);
        s.shortcuts.insert("search_open".into(), Some("Ctrl+Alt+KeyS".into()));
        s.save(&p).unwrap();
        assert_eq!(Settings::load(&p).0, s);
        // Older files without newer fields still load, with defaults for the rest.
        std::fs::write(&p, br#"{"banners_approve":false,"unknown":1}"#).unwrap();
        let loaded = Settings::load(&p).0;
        assert!(!loaded.banners_approve && loaded.banners_question && loaded.start_at_login);
    }
}

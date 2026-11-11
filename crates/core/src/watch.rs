//! Watching the marker directory for changes (FSEvents via `notify`).

use notify::{RecursiveMode, Watcher};
use std::path::Path;

/// Call `on_change` whenever anything in `dir` changes. Keep the returned
/// watcher alive for as long as you want events. Creates `dir` if missing.
pub fn watch_dir(dir: &Path, on_change: impl Fn() + Send + 'static) -> notify::Result<notify::RecommendedWatcher> {
    std::fs::create_dir_all(dir).map_err(notify::Error::io)?;
    let mut w = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
        if res.is_ok() {
            on_change();
        }
    })?;
    w.watch(dir, RecursiveMode::NonRecursive)?;
    Ok(w)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;
    use std::time::Duration;

    #[test]
    fn reports_new_marker_files() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("hopback");
        let (tx, rx) = mpsc::channel();
        let _w = watch_dir(&dir, move || {
            let _ = tx.send(());
        })
        .unwrap();
        std::thread::sleep(Duration::from_millis(200));
        hopback_hook::write_atomic(&dir.join("s.json"), b"{}").unwrap();
        assert!(rx.recv_timeout(Duration::from_secs(5)).is_ok());
    }
}

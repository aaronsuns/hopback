//! A tiny file logger with size-based rotation (`hopback.log`, `.1` … `.N`).
//! Callers must never log conversation text.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

pub const MAX_BYTES: u64 = 1024 * 1024;
pub const KEEP: usize = 3;

pub struct FileLogger {
    path: PathBuf,
    max_bytes: u64,
    keep: usize,
    lock: Mutex<()>,
}

impl FileLogger {
    pub fn new(path: PathBuf, max_bytes: u64, keep: usize) -> FileLogger {
        if let Some(d) = path.parent() {
            let _ = fs::create_dir_all(d);
        }
        FileLogger { path, max_bytes, keep, lock: Mutex::new(()) }
    }

    fn rotated(&self, n: usize) -> PathBuf {
        let mut s = self.path.as_os_str().to_owned();
        s.push(format!(".{n}"));
        PathBuf::from(s)
    }

    fn rotate_if_needed(&self) {
        let size = fs::metadata(&self.path).map(|m| m.len()).unwrap_or(0);
        if size < self.max_bytes {
            return;
        }
        let _ = fs::remove_file(self.rotated(self.keep));
        for n in (1..self.keep).rev() {
            let _ = fs::rename(self.rotated(n), self.rotated(n + 1));
        }
        let _ = fs::rename(&self.path, self.rotated(1));
    }

    pub fn write_line(&self, line: &str) {
        let _g = self.lock.lock();
        self.rotate_if_needed();
        if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(&self.path) {
            let _ = writeln!(f, "{line}");
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl log::Log for FileLogger {
    fn enabled(&self, m: &log::Metadata) -> bool {
        m.level() <= log::Level::Info || m.target().starts_with("hopback")
    }
    fn log(&self, r: &log::Record) {
        if self.enabled(r.metadata()) {
            let ts = crate::history::iso_date(hopback_hook::now_ms());
            self.write_line(&format!("{ts} {} {}: {}", r.level(), r.target(), r.args()));
        }
    }
    fn flush(&self) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rotates_and_keeps_n() {
        let tmp = tempfile::tempdir().unwrap();
        let log = FileLogger::new(tmp.path().join("Logs/hopback.log"), 100, 3);
        for i in 0..40 {
            log.write_line(&format!("line {i:02} ......................"));
        }
        let mut names: Vec<String> =
            fs::read_dir(tmp.path().join("Logs")).unwrap().map(|e| e.unwrap().file_name().into_string().unwrap()).collect();
        names.sort();
        assert_eq!(names, vec!["hopback.log", "hopback.log.1", "hopback.log.2", "hopback.log.3"]);
        let current = fs::read_to_string(log.path()).unwrap();
        assert!(current.contains("line 39"));
        assert!(fs::metadata(log.path()).unwrap().len() <= 100 + 40);
    }
}

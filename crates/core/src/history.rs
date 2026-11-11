//! The history index: one plain-text file per transcript plus a manifest, kept
//! under the cache directory and refreshed incrementally (only changed files).
//!
//! Layout (also explained to Claude by the Ask prompt):
//!
//! ```text
//! <cache>/history/index.json          manifest: one entry per session
//! <cache>/history/sessions/<id>.txt   title, folder, cwd, date, then the conversation
//! ```

use crate::transcript::{self, folder_name, Role};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

pub const INDEX_VERSION: u32 = 1;
pub const INDEX_FILE: &str = "index.json";
pub const SESSIONS_DIR: &str = "sessions";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Entry {
    pub session_id: String,
    pub transcript: String,
    pub mtime_ms: u64,
    pub size: u64,
    pub cwd: String,
    pub folder: String,
    pub title: String,
    /// Same as `mtime_ms`; the last time the session changed.
    pub updated_at: u64,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct Manifest {
    version: u32,
    entries: Vec<Entry>,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct RefreshStats {
    pub total: usize,
    pub updated: usize,
    pub removed: usize,
}

pub struct Index {
    pub root: PathBuf,
    pub entries: Vec<Entry>,
    /// Conversation text per session id, loaded lazily for content search.
    texts: HashMap<String, String>,
}

/// All `*.jsonl` files one level below each project dir.
pub fn list_transcripts(projects: &Path) -> Vec<PathBuf> {
    let Ok(dirs) = std::fs::read_dir(projects) else { return Vec::new() };
    let mut out = Vec::new();
    for d in dirs.flatten() {
        let Ok(files) = std::fs::read_dir(d.path()) else { continue };
        for f in files.flatten() {
            let p = f.path();
            if p.extension().is_some_and(|e| e == "jsonl") && p.is_file() {
                out.push(p);
            }
        }
    }
    out
}

fn mtime_ms(m: &std::fs::Metadata) -> u64 {
    m.modified().ok().and_then(|t| t.duration_since(UNIX_EPOCH).ok()).map_or(0, |d| d.as_millis() as u64)
}

/// Render a session's text file.
pub fn render_text(e: &Entry, messages: &[(Role, String)]) -> String {
    let mut s = format!(
        "# {}\nsession: {}\nfolder: {}\ncwd: {}\nupdated: {}\n",
        e.title,
        e.session_id,
        e.folder,
        e.cwd,
        iso_date(e.updated_at)
    );
    for (role, text) in messages {
        s.push_str(match role {
            Role::User => "\n## User\n",
            Role::Assistant => "\n## Claude\n",
        });
        s.push_str(text);
        s.push('\n');
    }
    s
}

/// `YYYY-MM-DD HH:MM` (UTC) from epoch milliseconds.
pub fn iso_date(ms: u64) -> String {
    let secs = ms / 1000;
    let days = (secs / 86_400) as i64;
    let (h, m) = ((secs % 86_400) / 3600, (secs % 3600) / 60);
    // Civil-from-days (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let mo = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(mo <= 2);
    format!("{y:04}-{mo:02}-{d:02} {h:02}:{m:02} UTC")
}

impl Index {
    pub fn open(cache_dir: &Path) -> Index {
        let root = cache_dir.join("history");
        let entries = std::fs::read(root.join(INDEX_FILE))
            .ok()
            .and_then(|b| serde_json::from_slice::<Manifest>(&b).ok())
            .filter(|m| m.version == INDEX_VERSION)
            .map(|m| m.entries)
            .unwrap_or_default();
        Index { root, entries, texts: HashMap::new() }
    }

    pub fn text_path(&self, id: &str) -> PathBuf {
        self.root.join(SESSIONS_DIR).join(format!("{id}.txt"))
    }

    /// Bring the index up to date with `projects`. Only changed transcripts are
    /// re-read. Sessions started inside the cache itself (Ask runs) are skipped.
    pub fn refresh(&mut self, projects: &Path) -> std::io::Result<RefreshStats> {
        std::fs::create_dir_all(self.root.join(SESSIONS_DIR))?;
        let old: HashMap<String, Entry> = self.entries.drain(..).map(|e| (e.transcript.clone(), e)).collect();
        let mut stats = RefreshStats::default();
        let mut seen_ids = std::collections::HashSet::new();
        let own = self.root.to_string_lossy().into_owned();
        for path in list_transcripts(projects) {
            let Ok(meta) = std::fs::metadata(&path) else { continue };
            let key = path.to_string_lossy().into_owned();
            let (mt, size) = (mtime_ms(&meta), meta.len());
            if let Some(e) = old.get(&key).filter(|e| e.mtime_ms == mt && e.size == size) {
                if self.text_path(&e.session_id).exists() {
                    seen_ids.insert(e.session_id.clone());
                    self.entries.push(e.clone());
                    continue;
                }
            }
            let Ok(file) = std::fs::File::open(&path) else { continue };
            let t = transcript::parse(std::io::BufReader::new(file), true);
            let id = path.file_stem().and_then(|s| s.to_str()).unwrap_or_default().to_owned();
            if !hopback_hook::valid_session_id(&id) {
                continue;
            }
            let cwd = t.cwd.unwrap_or_default();
            if t.title.is_none() || cwd.starts_with(&own) || cwd.contains(hopback_hook::OWN_CACHE_FRAGMENT) {
                continue; // empty sessions and our own Ask runs
            }
            let entry = Entry {
                session_id: id.clone(),
                transcript: key,
                mtime_ms: mt,
                size,
                folder: folder_name(&cwd).to_owned(),
                cwd,
                title: t.title.unwrap_or_default(),
                updated_at: mt,
            };
            hopback_hook::write_atomic(&self.text_path(&id), render_text(&entry, &t.messages).as_bytes())?;
            self.texts.remove(&id);
            seen_ids.insert(id);
            self.entries.push(entry);
            stats.updated += 1;
        }
        // Remove text files of transcripts that are gone.
        if let Ok(files) = std::fs::read_dir(self.root.join(SESSIONS_DIR)) {
            for f in files.flatten() {
                let name = f.file_name().to_string_lossy().into_owned();
                if let Some(id) = name.strip_suffix(".txt") {
                    if !seen_ids.contains(id) {
                        let _ = std::fs::remove_file(f.path());
                        self.texts.remove(id);
                        stats.removed += 1;
                    }
                }
            }
        }
        self.entries.sort_by_key(|e| std::cmp::Reverse(e.updated_at));
        stats.total = self.entries.len();
        let manifest = Manifest { version: INDEX_VERSION, entries: self.entries.clone() };
        hopback_hook::write_atomic(&self.root.join(INDEX_FILE), &serde_json::to_vec(&manifest)?)?;
        Ok(stats)
    }

    /// Conversation text for content search (cached in memory after first use).
    pub fn text(&mut self, id: &str) -> Option<&str> {
        if !self.texts.contains_key(id) {
            let raw = std::fs::read_to_string(self.text_path(id)).ok()?;
            // Skip the header block; search the conversation only.
            let body = raw.split_once("\n\n").map_or(String::new(), |(_, b)| b.to_owned());
            self.texts.insert(id.to_owned(), body);
        }
        self.texts.get(id).map(String::as_str)
    }

    /// Load all texts (call off the UI thread before content searches).
    pub fn load_texts(&mut self) {
        let ids: Vec<String> = self.entries.iter().map(|e| e.session_id.clone()).collect();
        for id in ids {
            let _ = self.text(&id);
        }
    }

    pub fn cached_text(&self, id: &str) -> Option<&str> {
        self.texts.get(id).map(String::as_str)
    }

    pub fn get(&self, id: &str) -> Option<&Entry> {
        self.entries.iter().find(|e| e.session_id == id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_transcript(projects: &Path, folder: &str, id: &str, prompt: &str, reply: &str) -> PathBuf {
        let dir = projects.join(format!("-Users-alex-{folder}"));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join(format!("{id}.jsonl"));
        let lines = [
            format!(
                r#"{{"type":"user","sessionId":"{id}","cwd":"/Users/alex/{folder}","message":{{"content":"{prompt}"}}}}"#
            ),
            format!(
                r#"{{"type":"assistant","sessionId":"{id}","cwd":"/Users/alex/{folder}","message":{{"content":[{{"type":"text","text":"{reply}"}}]}}}}"#
            ),
        ];
        std::fs::write(&p, lines.join("\n") + "\n").unwrap();
        p
    }

    #[test]
    fn builds_incrementally_and_removes_deleted() {
        let tmp = tempfile::tempdir().unwrap();
        let projects = tmp.path().join("projects");
        let cache = tmp.path().join("cache");
        let a = write_transcript(&projects, "web-app", "aaaa-1", "Add login page", "Done, added it.");
        write_transcript(&projects, "infra", "bbbb-2", "Rotate the TLS certs", "Rotated.");
        std::fs::write(projects.join("-Users-alex-infra/empty.jsonl"), "").unwrap();

        let mut idx = Index::open(&cache);
        let s = idx.refresh(&projects).unwrap();
        assert_eq!((s.total, s.updated, s.removed), (2, 2, 0));
        let text = std::fs::read_to_string(idx.text_path("aaaa-1")).unwrap();
        assert!(text.starts_with("# Add login page\nsession: aaaa-1\nfolder: web-app\n"));
        assert!(text.contains("## User\nAdd login page\n\n## Claude\nDone, added it.\n"));

        // Nothing changed: nothing re-read.
        let mut idx = Index::open(&cache);
        assert_eq!(idx.entries.len(), 2);
        assert_eq!(idx.refresh(&projects).unwrap().updated, 0);

        // Change one, delete the other.
        std::thread::sleep(std::time::Duration::from_millis(20));
        let mut f = std::fs::OpenOptions::new().append(true).open(&a).unwrap();
        use std::io::Write;
        writeln!(f, r#"{{"type":"custom-title","sessionId":"aaaa-1","customTitle":"Login work"}}"#).unwrap();
        std::fs::remove_file(projects.join("-Users-alex-infra/bbbb-2.jsonl")).unwrap();
        let s = idx.refresh(&projects).unwrap();
        assert_eq!((s.total, s.updated, s.removed), (1, 1, 1));
        assert_eq!(idx.get("aaaa-1").unwrap().title, "Login work");
        assert!(!idx.text_path("bbbb-2").exists());
        assert_eq!(idx.text("aaaa-1").unwrap(), "## User\nAdd login page\n\n## Claude\nDone, added it.\n");
    }

    #[test]
    fn corrupt_or_old_manifest_rebuilds() {
        let tmp = tempfile::tempdir().unwrap();
        let cache = tmp.path().join("cache");
        std::fs::create_dir_all(cache.join("history")).unwrap();
        std::fs::write(cache.join("history").join(INDEX_FILE), b"{oops").unwrap();
        assert!(Index::open(&cache).entries.is_empty());
        let mut idx = Index::open(&cache);
        assert_eq!(idx.refresh(&tmp.path().join("no-projects")).unwrap().total, 0);
    }

    #[test]
    fn dates() {
        assert_eq!(iso_date(0), "1970-01-01 00:00 UTC");
        assert_eq!(iso_date(1_709_210_096_000), "2024-02-29 12:34 UTC");
    }
}

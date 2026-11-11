//! "Ask Claude": find a past session by description using `claude -p` over the
//! history cache, restricted to read-only tools.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const TIMEOUT_SECS: u64 = 90;
pub const TOOLS: &str = "Read,Grep,Glob";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AskHit {
    pub session_id: String,
    #[serde(default)]
    pub reason: String,
}

pub fn prompt(question: &str) -> String {
    format!(
        "You are helping someone find a past Claude Code conversation.\n\
         The current directory is a search cache:\n\
         - index.json: {{\"entries\": [{{\"session_id\", \"folder\", \"cwd\", \"title\", \"updated_at\" (ms since epoch)}}]}}\n\
         - sessions/<session_id>.txt: a header (title, session, folder, cwd, updated) and then the conversation,\n\
           with \"## User\" and \"## Claude\" headings.\n\
         Use Grep and Glob over sessions/ (and Read when needed) to find the conversations that best match\n\
         this description:\n\n{question}\n\n\
         Answer with only a JSON array, best match first, at most 8 items, no other text:\n\
         [{{\"session_id\": \"...\", \"reason\": \"one short line saying why it matches\"}}]\n\
         Answer [] if nothing matches."
    )
}

/// Arguments for `claude` (the prompt goes on stdin).
pub fn args() -> Vec<String> {
    [
        "-p",
        "--output-format",
        "text",
        "--tools",
        TOOLS,
        "--allowedTools",
        TOOLS,
        "--permission-mode",
        "dontAsk",
        "--no-session-persistence",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect()
}

/// Pull the JSON array out of Claude's answer, tolerating prose or code fences
/// around it. Unknown ids are dropped by `known`.
pub fn parse_answer(out: &str, known: impl Fn(&str) -> bool) -> Result<Vec<AskHit>, String> {
    let start = out.find('[').ok_or("Claude did not return a list.")?;
    let end = out.rfind(']').filter(|&e| e > start).ok_or("Claude did not return a list.")?;
    let hits: Vec<AskHit> =
        serde_json::from_str(&out[start..=end]).map_err(|_| "Could not read Claude's answer.".to_string())?;
    let mut seen = std::collections::HashSet::new();
    Ok(hits.into_iter().filter(|h| known(&h.session_id) && seen.insert(h.session_id.clone())).collect())
}

/// Find `claude`: the setting, then the login shell's PATH, then common places.
pub fn find_claude(
    setting: &str,
    login_path: Option<&str>,
    home: &Path,
    exists: impl Fn(&Path) -> bool,
) -> Option<PathBuf> {
    let setting = setting.trim();
    if !setting.is_empty() {
        let p = PathBuf::from(setting.replacen('~', &home.to_string_lossy(), 1));
        return exists(&p).then_some(p);
    }
    let from_path = login_path.into_iter().flat_map(|p| p.split(':')).map(|d| Path::new(d).join("claude"));
    let fallbacks = [
        PathBuf::from("/opt/homebrew/bin/claude"),
        PathBuf::from("/usr/local/bin/claude"),
        home.join(".local/bin/claude"),
    ];
    from_path.chain(fallbacks).find(|p| exists(p))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prompt_mentions_layout_and_question() {
        let p = prompt("the one where we fixed the flaky deploy");
        assert!(p.contains("sessions/<session_id>.txt"));
        assert!(p.contains("index.json"));
        assert!(p.contains("fixed the flaky deploy"));
        let a = args();
        assert!(a.windows(2).any(|w| w[0] == "--tools" && w[1] == "Read,Grep,Glob"));
        assert!(a.contains(&"-p".to_string()));
    }

    #[test]
    fn parses_answers() {
        let known = |id: &str| id != "ghost";
        let out = "Here you go:\n```json\n[{\"session_id\":\"a1\",\"reason\":\"talks about deploys\"},{\"session_id\":\"ghost\",\"reason\":\"x\"},{\"session_id\":\"a1\",\"reason\":\"dup\"},{\"session_id\":\"b2\"}]\n```";
        let hits = parse_answer(out, known).unwrap();
        assert_eq!(
            hits,
            vec![
                AskHit { session_id: "a1".into(), reason: "talks about deploys".into() },
                AskHit { session_id: "b2".into(), reason: String::new() }
            ]
        );
        assert_eq!(parse_answer("[]", known).unwrap(), vec![]);
        assert!(parse_answer("no idea", known).is_err());
        assert!(parse_answer("[not json]", known).is_err());
    }

    #[test]
    fn finds_claude_in_order() {
        let home = Path::new("/Users/alex");
        let exists = |p: &Path| p == Path::new("/Users/alex/.local/bin/claude") || p == Path::new("/custom/bin/claude");
        assert_eq!(find_claude("", Some("/usr/bin:/custom/bin"), home, exists), Some("/custom/bin/claude".into()));
        assert_eq!(find_claude("", None, home, exists), Some("/Users/alex/.local/bin/claude".into()));
        assert_eq!(
            find_claude("~/.local/bin/claude", None, home, exists),
            Some("/Users/alex/.local/bin/claude".into())
        );
        assert_eq!(find_claude("/nope/claude", None, home, exists), None);
        assert_eq!(find_claude("", None, home, |_| false), None);
    }
}

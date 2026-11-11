//! Reading Claude Code JSONL transcripts: working directory, title, and the plain
//! conversation text used for search. Tolerates partial lines and unknown entries.

use serde_json::Value;
use std::io::BufRead;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    User,
    Assistant,
}

#[derive(Debug, Default, Clone, PartialEq)]
pub struct Transcript {
    pub session_id: Option<String>,
    pub cwd: Option<String>,
    /// Best title: custom title, then summary, then AI title, then first prompt.
    pub title: Option<String>,
    pub first_prompt: Option<String>,
    pub messages: Vec<(Role, String)>,
}

#[derive(Default)]
struct Titles {
    custom: Option<String>,
    summary: Option<String>,
    ai: Option<String>,
}

const MAX_TITLE: usize = 120;

/// Parse a whole transcript. With `with_text = false`, message text is not kept.
pub fn parse<R: BufRead>(reader: R, with_text: bool) -> Transcript {
    let mut t = Transcript::default();
    let mut titles = Titles::default();
    for line in reader.split(b'\n') {
        let Ok(line) = line else { break };
        let Ok(v) = serde_json::from_slice::<Value>(&line) else { continue };
        apply(&mut t, &mut titles, &v, with_text);
    }
    t.title = [titles.custom, titles.summary, titles.ai, t.first_prompt.clone()]
        .into_iter()
        .flatten()
        .map(|s| one_line(&s, MAX_TITLE))
        .find(|s| !s.is_empty());
    t
}

pub fn parse_str(s: &str, with_text: bool) -> Transcript {
    parse(s.as_bytes(), with_text)
}

fn str_field<'a>(v: &'a Value, key: &str) -> Option<&'a str> {
    v.get(key).and_then(Value::as_str).filter(|s| !s.trim().is_empty())
}

fn apply(t: &mut Transcript, titles: &mut Titles, v: &Value, with_text: bool) {
    if t.session_id.is_none() {
        t.session_id = str_field(v, "sessionId").map(str::to_owned);
    }
    match v.get("type").and_then(Value::as_str).unwrap_or("") {
        "custom-title" => titles.custom = str_field(v, "customTitle").map(str::to_owned),
        "summary" => titles.summary = str_field(v, "summary").map(str::to_owned),
        "ai-title" => titles.ai = str_field(v, "aiTitle").map(str::to_owned),
        kind @ ("user" | "assistant") => {
            if t.cwd.is_none() {
                t.cwd = str_field(v, "cwd").map(str::to_owned);
            }
            let noise = v.get("isMeta").and_then(Value::as_bool).unwrap_or(false)
                || v.get("isCompactSummary").and_then(Value::as_bool).unwrap_or(false)
                || v.get("isSidechain").and_then(Value::as_bool).unwrap_or(false)
                || v.get("isVisibleInTranscriptOnly").and_then(Value::as_bool).unwrap_or(false);
            if noise {
                return;
            }
            let role = if kind == "user" { Role::User } else { Role::Assistant };
            let Some(content) = v.get("message").and_then(|m| m.get("content")) else { return };
            let mut texts: Vec<&str> = Vec::new();
            match content {
                Value::String(s) => texts.push(s),
                Value::Array(blocks) => {
                    for b in blocks {
                        if b.get("type").and_then(Value::as_str) == Some("text") {
                            if let Some(s) = b.get("text").and_then(Value::as_str) {
                                texts.push(s);
                            }
                        }
                    }
                }
                _ => {}
            }
            for raw in texts {
                let text = if role == Role::User { clean_user_text(raw) } else { Some(raw.trim().to_owned()) };
                let Some(text) = text.filter(|s| !s.is_empty()) else { continue };
                if role == Role::User && t.first_prompt.is_none() {
                    t.first_prompt = Some(text.clone());
                }
                if with_text {
                    t.messages.push((role, text));
                }
            }
        }
        _ => {}
    }
}

/// Tags that wrap machine-generated content inside user messages.
const NOISE_TAGS: &[&str] = &[
    "system-reminder",
    "ide_opened_file",
    "ide_selection",
    "ide_diagnostics",
    "command-name",
    "command-message",
    "command-args",
    "command-stdout",
    "local-command-stdout",
    "local-command-stderr",
    "local-command-caveat",
    "task-notification",
    "user-prompt-submit-hook",
    "bash-input",
    "bash-stdout",
    "bash-stderr",
];

/// Strip machine-generated wrappers from a user message. Returns `None` when
/// nothing a human typed is left.
pub fn clean_user_text(raw: &str) -> Option<String> {
    let mut s = raw.to_owned();
    for tag in NOISE_TAGS {
        let open = format!("<{tag}>");
        let close = format!("</{tag}>");
        while let Some(start) = s.find(&open) {
            let end = s[start..].find(&close).map(|e| start + e + close.len()).unwrap_or(s.len());
            s.replace_range(start..end, "");
        }
    }
    let s = s.trim();
    let lower = s.to_ascii_lowercase();
    if s.is_empty()
        || lower.starts_with("caveat: the messages below were generated")
        || lower.starts_with("[request interrupted")
        || s.starts_with("[Image")
    {
        return None;
    }
    Some(s.to_owned())
}

/// First non-empty line, whitespace collapsed, cut to `max` chars with an ellipsis.
pub fn one_line(s: &str, max: usize) -> String {
    let line = s.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or("");
    let collapsed = line.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.chars().count() <= max {
        collapsed
    } else {
        let cut: String = collapsed.chars().take(max.saturating_sub(1)).collect();
        format!("{}…", cut.trim_end())
    }
}

/// Last path component of a directory, used as the "folder" everywhere.
pub fn folder_name(cwd: &str) -> &str {
    cwd.trim_end_matches('/').rsplit('/').next().unwrap_or(cwd)
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = r#"{"type":"queue-operation","operation":"enqueue","sessionId":"s-1"}
{"type":"user","sessionId":"s-1","cwd":"/Users/alex/web-app","message":{"role":"user","content":"<command-name>/clear</command-name>"}}
{"type":"user","sessionId":"s-1","cwd":"/Users/alex/web-app","isMeta":true,"message":{"role":"user","content":"Caveat: internal"}}
{"type":"user","sessionId":"s-1","cwd":"/Users/alex/web-app","message":{"role":"user","content":[{"type":"text","text":"<ide_opened_file>The user opened src/app.tsx</ide_opened_file>"},{"type":"text","text":"Add a dark mode toggle\nto the header"}]}}
{"type":"assistant","sessionId":"s-1","cwd":"/Users/alex/web-app","message":{"role":"assistant","content":[{"type":"thinking","thinking":"hmm"},{"type":"text","text":"I'll add the toggle."},{"type":"tool_use","name":"Read","input":{}}]}}
{"type":"user","sessionId":"s-1","cwd":"/Users/alex/web-app","message":{"role":"user","content":[{"type":"tool_result","content":"file contents"}]}}
{"type":"some-future-entry","sessionId":"s-1","whatever":1}
{"type":"user","sessionId":"s-1","cwd":"/Users/alex/web-app","message":{"role":"user","content":"<system-reminder>be nice</system-reminder>Also persist the choice"}}
{"type":"assistant","sessionId":"s-1","message":{"role":"assist"#;

    #[test]
    fn parses_cwd_first_prompt_and_text() {
        let t = parse_str(FIXTURE, true);
        assert_eq!(t.session_id.as_deref(), Some("s-1"));
        assert_eq!(t.cwd.as_deref(), Some("/Users/alex/web-app"));
        assert_eq!(t.title.as_deref(), Some("Add a dark mode toggle"));
        assert_eq!(
            t.messages,
            vec![
                (Role::User, "Add a dark mode toggle\nto the header".to_owned()),
                (Role::Assistant, "I'll add the toggle.".to_owned()),
                (Role::User, "Also persist the choice".to_owned()),
            ]
        );
    }

    #[test]
    fn without_text_keeps_no_messages() {
        let t = parse_str(FIXTURE, false);
        assert!(t.messages.is_empty());
        assert_eq!(t.title.as_deref(), Some("Add a dark mode toggle"));
    }

    #[test]
    fn title_priority() {
        let base = r#"{"type":"user","sessionId":"s","cwd":"/x","message":{"content":"first prompt"}}"#;
        let ai = r#"{"type":"ai-title","sessionId":"s","aiTitle":"AI title"}"#;
        let summary = r#"{"type":"summary","summary":"Summary title","leafUuid":"u"}"#;
        let custom = r#"{"type":"custom-title","sessionId":"s","customTitle":"Custom"}"#;
        let custom2 = r#"{"type":"custom-title","sessionId":"s","customTitle":"Renamed"}"#;
        assert_eq!(parse_str(base, false).title.as_deref(), Some("first prompt"));
        assert_eq!(parse_str(&[base, ai].join("\n"), false).title.as_deref(), Some("AI title"));
        assert_eq!(parse_str(&[ai, base, summary].join("\n"), false).title.as_deref(), Some("Summary title"));
        let all = [custom, ai, base, summary, custom2].join("\n");
        assert_eq!(parse_str(&all, false).title.as_deref(), Some("Renamed"));
    }

    #[test]
    fn empty_and_garbage() {
        assert_eq!(parse_str("", true), Transcript::default());
        let t = parse_str("\n\n{not json\n[1,2]\n\"str\"\n", true);
        assert_eq!(t.title, None);
        assert_eq!(t.cwd, None);
    }

    #[test]
    fn noise_only_user_messages_are_skipped() {
        assert_eq!(clean_user_text("<task-notification>done</task-notification>"), None);
        assert_eq!(clean_user_text("[Request interrupted by user]"), None);
        assert_eq!(clean_user_text("  <system-reminder>x</system-reminder>  hi "), Some("hi".into()));
        assert_eq!(clean_user_text("<system-reminder>unterminated"), None);
        assert_eq!(clean_user_text("compare <b> tags"), Some("compare <b> tags".into()));
    }

    #[test]
    fn one_line_trims_and_truncates() {
        assert_eq!(one_line("\n  hello   world \nsecond", 50), "hello world");
        assert_eq!(one_line("abcdefghij", 5), "abcd…");
        assert_eq!(one_line("åäöåäöåäö", 4), "åäö…");
        assert_eq!(one_line("", 5), "");
    }

    #[test]
    fn folder_names() {
        assert_eq!(folder_name("/Users/alex/api-server"), "api-server");
        assert_eq!(folder_name("/Users/alex/infra/"), "infra");
        assert_eq!(folder_name("docs-site"), "docs-site");
    }
}

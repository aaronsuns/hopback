//! Diagnostics: a list of checks and a plain-text report. Never includes
//! conversation text.

use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Check {
    pub name: String,
    /// `None` = informational / unknown.
    pub ok: Option<bool>,
    pub detail: String,
    pub hint: String,
}

impl Check {
    pub fn new(name: &str, ok: Option<bool>, detail: impl Into<String>, hint: impl Into<String>) -> Check {
        Check { name: name.into(), ok, detail: detail.into(), hint: hint.into() }
    }
}

pub fn report(version: &str, os: &str, checks: &[Check]) -> String {
    let mut s = format!("Hopback {version} diagnostics\nmacOS {os}\n\n");
    for c in checks {
        let mark = match c.ok {
            Some(true) => "✓",
            Some(false) => "✗",
            None => "•",
        };
        s.push_str(&format!("{mark} {}: {}", c.name, c.detail));
        if c.ok == Some(false) && !c.hint.is_empty() {
            s.push_str(&format!(" — {}", c.hint));
        }
        s.push('\n');
    }
    s
}

/// Whether `installed_plugins.json` lists a `hopback@…` plugin.
pub fn plugin_installed(installed_plugins_json: &str) -> bool {
    serde_json::from_str::<serde_json::Value>(installed_plugins_json)
        .ok()
        .and_then(|v| v.get("plugins").and_then(|p| p.as_object()).map(|o| o.keys().any(|k| k.starts_with("hopback@"))))
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn report_format() {
        let r = report(
            "0.1.0",
            "14.5",
            &[
                Check::new("Accessibility", Some(true), "granted", ""),
                Check::new("Plugin", Some(false), "not installed", "run /plugin install hopback@hopback"),
                Check::new("Updates", None, "not checked yet", "ignored"),
            ],
        );
        assert_eq!(
            r,
            "Hopback 0.1.0 diagnostics\nmacOS 14.5\n\n✓ Accessibility: granted\n✗ Plugin: not installed — run /plugin install hopback@hopback\n• Updates: not checked yet\n"
        );
    }

    #[test]
    fn plugin_detection() {
        assert!(plugin_installed(r#"{"version":2,"plugins":{"hopback@hopback":[{"scope":"user"}]}}"#));
        assert!(!plugin_installed(r#"{"version":2,"plugins":{"other@market":[]}}"#));
        assert!(!plugin_installed("nope"));
    }
}

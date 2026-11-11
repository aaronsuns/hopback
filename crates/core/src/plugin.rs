//! The Claude Code plugin step of the welcome window: the install commands,
//! which state the step is in, and how much of a failed install's output to show.

use serde::Serialize;

/// The marketplace this repo publishes.
pub const MARKETPLACE: &str = "aaronsuns/hopback";
/// The plugin, as `name@marketplace`.
pub const PLUGIN: &str = "hopback@hopback";

/// Arguments to `claude`, in order: add the marketplace, then install the plugin.
pub const INSTALL_ARGS: [&[&str]; 2] = [&["plugin", "marketplace", "add", MARKETPLACE], &["plugin", "install", PLUGIN]];

/// The same steps as terminal commands, for the manual way.
pub fn manual_commands() -> Vec<String> {
    INSTALL_ARGS.iter().map(|a| format!("claude {}", a.join(" "))).collect()
}

/// Where the plugin step is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PluginStep {
    /// Not listed in `installed_plugins.json`: offer the Install button.
    NotInstalled,
    /// Installed, but no session has run the hook yet: ask for a new session.
    Installed,
    /// The hook has run (`.hook-seen` exists): done.
    Ready,
}

/// The hook having run wins: it proves the plugin works even if the
/// install list could not be read.
pub fn step(installed: bool, hook_seen: bool) -> PluginStep {
    match (installed, hook_seen) {
        (_, true) => PluginStep::Ready,
        (true, false) => PluginStep::Installed,
        (false, false) => PluginStep::NotInstalled,
    }
}

/// The last `n` non-empty lines of `output`, trimmed; what a failed install shows.
pub fn tail(output: &str, n: usize) -> String {
    let lines: Vec<&str> = output.lines().map(str::trim_end).filter(|l| !l.trim().is_empty()).collect();
    lines[lines.len().saturating_sub(n)..].join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manual_commands_match_install_args() {
        assert_eq!(
            manual_commands(),
            vec!["claude plugin marketplace add aaronsuns/hopback", "claude plugin install hopback@hopback"]
        );
    }

    #[test]
    fn step_states() {
        assert_eq!(step(false, false), PluginStep::NotInstalled);
        assert_eq!(step(true, false), PluginStep::Installed);
        assert_eq!(step(true, true), PluginStep::Ready);
        assert_eq!(step(false, true), PluginStep::Ready);
    }

    #[test]
    fn step_serializes_snake_case() {
        assert_eq!(serde_json::to_string(&PluginStep::NotInstalled).unwrap(), "\"not_installed\"");
    }

    #[test]
    fn tail_keeps_last_non_empty_lines() {
        let out = "Cloning example-market…\n\nwarning: slow network\nerror: plugin not found\n  in example-market\n\n";
        assert_eq!(tail(out, 2), "error: plugin not found\n  in example-market");
        assert_eq!(
            tail(out, 10),
            "Cloning example-market…\nwarning: slow network\nerror: plugin not found\n  in example-market"
        );
        assert_eq!(tail("", 3), "");
        assert_eq!(tail("one\r\ntwo\r\n", 1), "two");
    }
}

//! Global shortcuts: the actions, their defaults, parsing, display and validation.
//!
//! An accelerator is stored canonically as `Ctrl+Alt+Shift+Cmd+<Code>` (modifiers
//! in that order, only those present) where `<Code>` is a `KeyboardEvent.code`
//! value such as `KeyC`, `Digit1`, `Tab`, `F5` or `ArrowLeft`.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, PartialOrd, Ord)]
pub struct Mods {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub cmd: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Accel {
    pub mods: Mods,
    pub code: String,
}

pub const ACTIONS: &[(&str, &str, &str)] = &[
    ("window1", "Go to window 1", "Ctrl+Alt+Digit1"),
    ("window2", "Go to window 2", "Ctrl+Alt+Digit2"),
    ("window3", "Go to window 3", "Ctrl+Alt+Digit3"),
    ("window4", "Go to window 4", "Ctrl+Alt+Digit4"),
    ("window5", "Go to window 5", "Ctrl+Alt+Digit5"),
    ("window6", "Go to window 6", "Ctrl+Alt+Digit6"),
    ("window7", "Go to window 7", "Ctrl+Alt+Digit7"),
    ("window8", "Go to window 8", "Ctrl+Alt+Digit8"),
    ("window9", "Go to window 9", "Ctrl+Alt+Digit9"),
    ("next_needs_you", "Next session that needs you", "Ctrl+Alt+Digit0"),
    ("next_window", "Next window", "Ctrl+Alt+Tab"),
    ("prev_window", "Previous window", "Ctrl+Alt+Shift+Tab"),
    ("search_open", "Search open sessions", "Ctrl+Cmd+KeyC"),
    ("search_history", "Search history", "Ctrl+Shift+Cmd+KeyC"),
];

/// Combinations macOS (or nearly every Mac) keeps for itself.
const RESERVED: &[(&str, &str)] = &[
    ("Cmd+Space", "Spotlight"),
    ("Ctrl+Space", "input source switching"),
    ("Alt+Cmd+Space", "Finder search"),
    ("Ctrl+Cmd+Space", "the emoji picker"),
    ("Cmd+Tab", "the app switcher"),
    ("Shift+Cmd+Tab", "the app switcher"),
    ("Cmd+Backquote", "window cycling"),
    ("Shift+Cmd+Backquote", "window cycling"),
    ("Shift+Cmd+Digit3", "screenshots"),
    ("Shift+Cmd+Digit4", "screenshots"),
    ("Shift+Cmd+Digit5", "screenshots"),
    ("Shift+Cmd+Digit6", "screenshots"),
    ("Ctrl+Shift+Cmd+Digit3", "screenshots"),
    ("Ctrl+Shift+Cmd+Digit4", "screenshots"),
    ("Cmd+KeyQ", "quitting apps"),
    ("Cmd+KeyW", "closing windows"),
    ("Cmd+KeyH", "hiding apps"),
    ("Cmd+KeyM", "minimising windows"),
    ("Alt+Cmd+KeyH", "hiding other apps"),
    ("Alt+Cmd+KeyD", "showing the Dock"),
    ("Alt+Cmd+Escape", "Force Quit"),
    ("Ctrl+Cmd+KeyQ", "locking the screen"),
    ("Ctrl+Cmd+KeyF", "full screen"),
    ("Ctrl+ArrowUp", "Mission Control"),
    ("Ctrl+ArrowDown", "App Exposé"),
    ("Ctrl+ArrowLeft", "switching Spaces"),
    ("Ctrl+ArrowRight", "switching Spaces"),
    ("Cmd+KeyC", "Copy"),
    ("Cmd+KeyV", "Paste"),
    ("Cmd+KeyX", "Cut"),
    ("Cmd+KeyZ", "Undo"),
    ("Cmd+KeyA", "Select All"),
    ("Cmd+KeyS", "Save"),
];

pub fn parse(s: &str) -> Option<Accel> {
    let mut mods = Mods::default();
    let mut code = None;
    for part in s.split('+').map(str::trim).filter(|p| !p.is_empty()) {
        match part.to_ascii_lowercase().as_str() {
            "ctrl" | "control" => mods.ctrl = true,
            "alt" | "option" | "opt" => mods.alt = true,
            "shift" => mods.shift = true,
            "cmd" | "command" | "super" | "meta" => mods.cmd = true,
            _ if code.is_none() => code = Some(normalize_code(part)?),
            _ => return None,
        }
    }
    Some(Accel { mods, code: code? })
}

fn normalize_code(k: &str) -> Option<String> {
    let c = if k.len() == 1 {
        let ch = k.chars().next()?;
        if ch.is_ascii_alphabetic() {
            format!("Key{}", ch.to_ascii_uppercase())
        } else if ch.is_ascii_digit() {
            format!("Digit{ch}")
        } else {
            return None;
        }
    } else {
        k.to_owned()
    };
    is_known_code(&c).then_some(c)
}

fn is_known_code(c: &str) -> bool {
    const NAMED: &[&str] = &[
        "Tab",
        "Space",
        "Enter",
        "Escape",
        "Backspace",
        "Delete",
        "Home",
        "End",
        "PageUp",
        "PageDown",
        "ArrowUp",
        "ArrowDown",
        "ArrowLeft",
        "ArrowRight",
        "Minus",
        "Equal",
        "BracketLeft",
        "BracketRight",
        "Backslash",
        "Semicolon",
        "Quote",
        "Comma",
        "Period",
        "Slash",
        "Backquote",
    ];
    let keyed = |p: &str, ok: fn(&str) -> bool| c.strip_prefix(p).is_some_and(ok);
    NAMED.contains(&c)
        || keyed("Key", |r| r.len() == 1 && r.as_bytes()[0].is_ascii_uppercase())
        || keyed("Digit", |r| r.len() == 1 && r.as_bytes()[0].is_ascii_digit())
        || keyed("F", |r| r.parse::<u8>().is_ok_and(|n| (1..=20).contains(&n)))
}

impl Accel {
    pub fn canonical(&self) -> String {
        let m = &self.mods;
        let mut parts: Vec<&str> = Vec::new();
        for (on, name) in [(m.ctrl, "Ctrl"), (m.alt, "Alt"), (m.shift, "Shift"), (m.cmd, "Cmd")] {
            if on {
                parts.push(name);
            }
        }
        parts.push(&self.code);
        parts.join("+")
    }

    /// The form the global-shortcut plugin understands.
    pub fn to_plugin_string(&self) -> String {
        self.canonical().replace("Ctrl", "Control").replace("Cmd", "Super")
    }

    /// Mac glyphs in Apple's order: ⌃⌥⇧⌘ then the key.
    pub fn display(&self) -> String {
        let m = &self.mods;
        let mut s = String::new();
        for (on, g) in [(m.ctrl, "⌃"), (m.alt, "⌥"), (m.shift, "⇧"), (m.cmd, "⌘")] {
            if on {
                s.push_str(g);
            }
        }
        s.push_str(&key_label(&self.code));
        s
    }
}

pub fn key_label(code: &str) -> String {
    if let Some(r) = code.strip_prefix("Key").or_else(|| code.strip_prefix("Digit")) {
        return r.to_owned();
    }
    let s = match code {
        "Tab" => "⇥",
        "Space" => "Space",
        "Enter" => "↩",
        "Escape" => "⎋",
        "Backspace" => "⌫",
        "Delete" => "⌦",
        "ArrowUp" => "↑",
        "ArrowDown" => "↓",
        "ArrowLeft" => "←",
        "ArrowRight" => "→",
        "Minus" => "-",
        "Equal" => "=",
        "BracketLeft" => "[",
        "BracketRight" => "]",
        "Backslash" => "\\",
        "Semicolon" => ";",
        "Quote" => "'",
        "Comma" => ",",
        "Period" => ".",
        "Slash" => "/",
        "Backquote" => "`",
        other => other,
    };
    s.to_owned()
}

/// Effective binding per action: the user's choice (`None` = cleared) or the default.
pub fn effective(user: &BTreeMap<String, Option<String>>) -> Vec<(&'static str, Option<Accel>)> {
    ACTIONS
        .iter()
        .map(|(id, _, default)| {
            let chosen = match user.get(*id) {
                Some(v) => v.clone(),
                None => Some((*default).to_owned()),
            };
            (*id, chosen.as_deref().and_then(parse))
        })
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Rejection(pub String);

/// Check a new binding for `action` against the rules and the other bindings.
pub fn validate(action: &str, accel: &str, user: &BTreeMap<String, Option<String>>) -> Result<Accel, Rejection> {
    let a = parse(accel).ok_or_else(|| Rejection("Not a key combination Hopback understands.".into()))?;
    if !(a.mods.ctrl || a.mods.alt || a.mods.cmd) {
        return Err(Rejection("Use at least one of ⌃ Control, ⌥ Option or ⌘ Command.".into()));
    }
    let canon = a.canonical();
    if let Some((_, why)) = RESERVED.iter().find(|(r, _)| parse(r).is_some_and(|r| r.canonical() == canon)) {
        return Err(Rejection(format!("{} is reserved by macOS for {why}.", a.display())));
    }
    for (id, other) in effective(user) {
        if id != action && other.as_ref().is_some_and(|o| o.canonical() == canon) {
            let label = ACTIONS.iter().find(|x| x.0 == id).map_or(id, |x| x.1);
            return Err(Rejection(format!("{} is already used for “{label}”.", a.display())));
        }
    }
    Ok(a)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_and_canonicalise() {
        let a = parse("cmd+shift+ctrl+c").unwrap();
        assert_eq!(a.canonical(), "Ctrl+Shift+Cmd+KeyC");
        assert_eq!(a.display(), "⌃⇧⌘C");
        assert_eq!(a.to_plugin_string(), "Control+Shift+Super+KeyC");
        assert_eq!(parse("Ctrl+Alt+1").unwrap().canonical(), "Ctrl+Alt+Digit1");
        assert_eq!(parse("Control+Option+Tab").unwrap().display(), "⌃⌥⇥");
        assert_eq!(parse("Ctrl+Alt+F12").unwrap().display(), "⌃⌥F12");
        assert!(parse("Ctrl+Alt").is_none());
        assert!(parse("Ctrl+A+B").is_none());
        assert!(parse("Ctrl+Hyper").is_none());
        assert!(parse("Ctrl+F99").is_none());
        assert!(parse("").is_none());
    }

    #[test]
    fn defaults_are_valid_and_distinct() {
        let none = BTreeMap::new();
        let eff = effective(&none);
        assert_eq!(eff.len(), ACTIONS.len());
        for (id, a) in &eff {
            let a = a.as_ref().unwrap();
            assert_eq!(validate(id, &a.canonical(), &none).map(|x| x.canonical()), Ok(a.canonical()), "{id}");
        }
        assert_eq!(eff[9].1.as_ref().unwrap().display(), "⌃⌥0");
        assert_eq!(eff[13].1.as_ref().unwrap().display(), "⌃⇧⌘C");
    }

    #[test]
    fn rejections() {
        let none = BTreeMap::new();
        let reason = |acc: &str| validate("search_open", acc, &none).unwrap_err().0;
        assert!(reason("Shift+KeyK").contains("at least one"));
        assert!(reason("KeyK").contains("at least one"));
        assert!(reason("Cmd+Space").contains("Spotlight"));
        assert!(reason("Cmd+Tab").contains("reserved"));
        assert!(reason("Shift+Cmd+4").contains("screenshots"));
        assert!(reason("Cmd+Q").contains("quitting"));
        assert!(reason("Ctrl+Alt+1").contains("Go to window 1"));
        assert!(reason("nonsense").contains("understands"));
        assert!(validate("search_open", "Ctrl+Alt+KeyK", &none).is_ok());
        // Re-assigning an action to its own current binding is fine.
        assert!(validate("window1", "Ctrl+Alt+1", &none).is_ok());
    }

    #[test]
    fn cleared_bindings_free_the_combination() {
        let mut user = BTreeMap::new();
        user.insert("window1".to_owned(), None);
        assert!(validate("search_open", "Ctrl+Alt+1", &user).is_ok());
        assert!(effective(&user)[0].1.is_none());
        user.insert("window2".to_owned(), Some("Ctrl+Alt+KeyK".to_owned()));
        assert!(validate("search_open", "Ctrl+Alt+K", &user).unwrap_err().0.contains("window 2"));
    }
}

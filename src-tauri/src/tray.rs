//! The menu bar item: title and menu, rebuilt only when what it shows changes.

use crate::state::{lock, App};
use hopback_core::board::{self, Board};
use hopback_core::shortcuts;
use std::sync::Mutex;
use tauri::menu::{Menu, MenuBuilder, MenuItemBuilder};
use tauri::{AppHandle, Wry};

pub const TRAY_ID: &str = "hopback";

#[derive(Debug, Clone, PartialEq)]
pub struct Row {
    pub id: String,
    pub text: String,
    pub enabled: bool,
    pub accel: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Item {
    Row(Row),
    Separator,
}

fn row(id: &str, text: impl Into<String>, enabled: bool, accel: Option<String>) -> Item {
    Item::Row(Row { id: id.into(), text: text.into(), enabled, accel })
}

pub struct Status {
    pub accessibility: bool,
    pub hook_seen: bool,
    pub update: Option<String>,
}

/// The menu as data (so it can be compared and tested).
pub fn model(b: &Board, st: &Status, accels: &[(&str, Option<String>)], now: u64) -> Vec<Item> {
    let accel = |id: &str| accels.iter().find(|(a, _)| *a == id).and_then(|(_, x)| x.clone());
    let mut items = Vec::new();
    if !st.accessibility {
        items.push(row("status:ax", "Accessibility off — Turn on…", true, None));
    }
    if !st.hook_seen {
        items.push(row("status:plugin", "Needs-you alerts off — Install plugin…", true, None));
    }
    if let Some(v) = &st.update {
        items.push(row("status:update", format!("Update available ({v})"), true, None));
    }
    if !items.is_empty() {
        items.push(Item::Separator);
    }
    if !b.needs_you.is_empty() {
        items.push(row("hdr:needs", "Needs you", false, accel("next_needs_you")));
        for s in &b.needs_you {
            items.push(row(&format!("go:{}", s.id), format!("    {}", board::row_label(s, now)), true, None));
        }
        items.push(Item::Separator);
    }
    if !b.windows.is_empty() {
        items.push(row("hdr:windows", "Windows", false, None));
        for w in &b.windows {
            let text = match &w.session {
                Some(s) => format!("{}  {}", w.number, board::row_label(s, now)),
                None => format!("{}  {} — no Claude session", w.number, w.folder.as_deref().unwrap_or(&w.title)),
            };
            let a = if w.number <= 9 { accel(&format!("window{}", w.number)) } else { None };
            items.push(row(&format!("win:{}", w.number), text, true, a));
        }
        items.push(Item::Separator);
    }
    if !b.background.is_empty() {
        items.push(row("hdr:bg", "Working in background", false, None));
        for s in &b.background {
            items.push(row(&format!("go:{}", s.id), format!("    {}", board::row_label(s, now)), true, None));
        }
        items.push(Item::Separator);
    }
    items.push(row("search_open", "Search open sessions…", true, accel("search_open")));
    items.push(row("search_history", "Search history…", true, accel("search_history")));
    items.push(Item::Separator);
    items.push(row("settings", "Settings…", true, None));
    items.push(row("diagnostics", "Diagnostics…", true, None));
    items.push(row("about", "About Hopback", true, None));
    items.push(Item::Separator);
    items.push(row("quit", "Quit Hopback", true, None));
    items
}

fn build_menu(app: &AppHandle, items: &[Item]) -> tauri::Result<Menu<Wry>> {
    let mut mb = MenuBuilder::new(app);
    for it in items {
        mb = match it {
            Item::Separator => mb.separator(),
            Item::Row(r) => {
                let mut b = MenuItemBuilder::with_id(&r.id, &r.text).enabled(r.enabled);
                if let Some(a) = &r.accel {
                    b = b.accelerator(a);
                }
                match b.build(app) {
                    Ok(i) => mb.item(&i),
                    // An accelerator the menu can't show is not worth losing the row.
                    Err(_) => mb.item(&MenuItemBuilder::with_id(&r.id, &r.text).enabled(r.enabled).build(app)?),
                }
            }
        };
    }
    mb.build()
}

static LAST: Mutex<Option<(String, Vec<Item>)>> = Mutex::new(None);

pub fn update(app: &AppHandle, shared: &App) {
    let b = lock(&shared.board).clone();
    let st = Status {
        accessibility: b.accessibility,
        hook_seen: shared.hook_seen.load(std::sync::atomic::Ordering::Relaxed),
        update: lock(&shared.update_available).clone(),
    };
    let user = lock(&shared.settings).shortcuts.clone();
    let accels: Vec<(&str, Option<String>)> =
        shortcuts::effective(&user).into_iter().map(|(id, a)| (id, a.map(|a| a.canonical()))).collect();
    let title = board::tray_title(&b);
    let items = model(&b, &st, &accels, hopback_hook::now_ms());
    let mut last = lock(&LAST);
    if last.as_ref().is_some_and(|(t, i)| *t == title && *i == items) {
        return;
    }
    let Some(tray) = app.tray_by_id(TRAY_ID) else { return };
    let _ = tray.set_title(Some(&title));
    match build_menu(app, &items) {
        Ok(menu) => {
            let _ = tray.set_menu(Some(menu));
        }
        Err(e) => log::warn!("could not build the menu: {e}"),
    }
    *last = Some((title, items));
}

#[cfg(test)]
mod tests {
    use super::*;
    use hopback_core::board::{Session, WindowRow};
    use hopback_core::markers::State;

    fn s(id: &str, state: State) -> Session {
        Session {
            id: id.into(),
            cwd: "/Users/alex/web-app".into(),
            title: "Fix tests".into(),
            state: Some(state),
            updated_at: 0,
            tool: None,
        }
    }

    fn ids(items: &[Item]) -> Vec<String> {
        items.iter().map(|i| if let Item::Row(r) = i { r.id.clone() } else { "-".into() }).collect()
    }

    #[test]
    fn full_menu_order() {
        let b = Board {
            windows: vec![WindowRow {
                number: 1,
                title: "web-app".into(),
                folder: Some("web-app".into()),
                menu_index: 7,
                session: Some(s("w", State::Working)),
            }],
            needs_you: vec![s("n", State::Approve)],
            background: vec![s("b", State::Working)],
            working: 2,
            accessibility: false,
        };
        let st = Status { accessibility: false, hook_seen: false, update: Some("0.2.0".into()) };
        let accels = vec![
            ("next_needs_you", Some("Ctrl+Alt+Digit0".to_string())),
            ("window1", Some("Ctrl+Alt+Digit1".to_string())),
        ];
        let m = model(&b, &st, &accels, 0);
        assert_eq!(
            ids(&m),
            vec![
                "status:ax",
                "status:plugin",
                "status:update",
                "-",
                "hdr:needs",
                "go:n",
                "-",
                "hdr:windows",
                "win:1",
                "-",
                "hdr:bg",
                "go:b",
                "-",
                "search_open",
                "search_history",
                "-",
                "settings",
                "diagnostics",
                "about",
                "-",
                "quit"
            ]
        );
        let Item::Row(hdr) = &m[4] else { panic!() };
        assert_eq!(hdr.accel.as_deref(), Some("Ctrl+Alt+Digit0"));
        assert!(!hdr.enabled);
        let Item::Row(win) = &m[8] else { panic!() };
        assert_eq!(win.text, "1  ⚙️ web-app — Fix tests  (now)");
        assert_eq!(win.accel.as_deref(), Some("Ctrl+Alt+Digit1"));
    }

    #[test]
    fn quiet_menu() {
        let st = Status { accessibility: true, hook_seen: true, update: None };
        let m = model(&Board::default(), &st, &[], 0);
        assert_eq!(ids(&m)[0], "search_open");
    }
}

//! VS Code window titles: finding the window entries in the app's Window menu,
//! extracting the workspace folder, and numbering windows A→Z.

/// VS Code's default title separator.
const SEP: &str = " — ";
const APP_NAMES: &[&str] = &["Visual Studio Code", "Code"];
const FIXED_TAIL_ITEMS: &[&str] = &["Bring All to Front", "Arrange in Front"];

/// Given the Window menu's item titles (`None` = separator), return the indices
/// of the items that are windows. Those follow the standard items, after the
/// last of "Bring All to Front"/"Arrange in Front", or after the last separator.
pub fn window_item_indices(items: &[Option<String>]) -> Vec<usize> {
    let anchor = items
        .iter()
        .rposition(|i| i.as_deref().is_some_and(|t| FIXED_TAIL_ITEMS.contains(&t)))
        .or_else(|| items.iter().rposition(Option::is_none));
    let start = anchor.map_or(0, |a| a + 1);
    (start..items.len()).filter(|&i| items[i].as_deref().is_some_and(|t| !t.trim().is_empty())).collect()
}

/// Workspace folder name from a VS Code window title, e.g.
/// `"● main.rs — api-server"` → `api-server`, `"infra"` → `infra`.
pub fn folder_from_title(title: &str) -> Option<String> {
    let mut parts: Vec<&str> = title.split(SEP).map(str::trim).filter(|p| !p.is_empty()).collect();
    while parts.last().is_some_and(|p| APP_NAMES.contains(p)) {
        parts.pop();
    }
    let last = parts.last()?;
    let mut s = last.trim_start_matches('●').trim();
    // Remote / dev-host suffixes such as "web-app [SSH: box]".
    if let Some(i) = s.find(" [") {
        s = s[..i].trim();
    }
    if let Some(stripped) = s.strip_prefix('[').and_then(|r| r.split_once("] ")).map(|(_, rest)| rest) {
        s = stripped.trim();
    }
    let s = s.strip_suffix(" (Workspace)").unwrap_or(s).trim();
    if s.is_empty() || APP_NAMES.contains(&s) {
        None
    } else {
        Some(s.to_owned())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Window {
    /// 1-based position in A→Z order.
    pub number: usize,
    pub title: String,
    pub folder: Option<String>,
    /// Index of the item in the Window menu, used to raise it.
    pub menu_index: usize,
}

/// Number windows by folder name A→Z (case-insensitive); ties keep menu order.
/// Windows without a folder sort last.
pub fn number_windows(titles: &[(usize, String)]) -> Vec<Window> {
    let mut ws: Vec<Window> = titles
        .iter()
        .map(|(idx, t)| Window { number: 0, title: t.clone(), folder: folder_from_title(t), menu_index: *idx })
        .collect();
    ws.sort_by(|a, b| {
        let key = |w: &Window| (w.folder.is_none(), w.folder.as_deref().unwrap_or("").to_lowercase());
        key(a).cmp(&key(b)).then(a.menu_index.cmp(&b.menu_index))
    });
    for (i, w) in ws.iter_mut().enumerate() {
        w.number = i + 1;
    }
    ws
}

/// Index (into `windows`) of the window after/before the one titled `current`.
/// Starts at the first/last window when nothing matches.
pub fn cycle(windows: &[Window], current: Option<&str>, forward: bool) -> Option<usize> {
    if windows.is_empty() {
        return None;
    }
    let n = windows.len();
    match current.and_then(|c| windows.iter().position(|w| w.title == c)) {
        Some(i) if forward => Some((i + 1) % n),
        Some(i) => Some((i + n - 1) % n),
        None if forward => Some(0),
        None => Some(n - 1),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn items(v: &[Option<&str>]) -> Vec<Option<String>> {
        v.iter().map(|o| o.map(str::to_owned)).collect()
    }

    #[test]
    fn finds_window_items_after_standard_items() {
        let menu = items(&[
            Some("Minimize"),
            Some("Zoom"),
            None,
            Some("Bring All to Front"),
            Some("Arrange in Front"),
            None,
            Some("app.tsx — web-app"),
            Some("infra"),
        ]);
        assert_eq!(window_item_indices(&menu), vec![6, 7]);
        // No windows open.
        let empty = items(&[Some("Minimize"), None, Some("Bring All to Front"), Some("Arrange in Front")]);
        assert!(window_item_indices(&empty).is_empty());
        // Localised menu without the English anchors: fall back to the last separator.
        let other = items(&[Some("Réduire"), None, Some("Tout ramener au premier plan"), None, Some("docs-site")]);
        assert_eq!(window_item_indices(&other), vec![4]);
    }

    #[test]
    fn folder_from_titles() {
        assert_eq!(folder_from_title("main.rs — api-server").as_deref(), Some("api-server"));
        assert_eq!(folder_from_title("● main.rs — api-server").as_deref(), Some("api-server"));
        assert_eq!(folder_from_title("infra").as_deref(), Some("infra"));
        assert_eq!(folder_from_title("Welcome — docs-site").as_deref(), Some("docs-site"));
        assert_eq!(folder_from_title("a.ts — web-app — Visual Studio Code").as_deref(), Some("web-app"));
        assert_eq!(folder_from_title("a.ts — web-app [SSH: devbox]").as_deref(), Some("web-app"));
        assert_eq!(folder_from_title("[Extension Development Host] a.ts — web-app").as_deref(), Some("web-app"));
        assert_eq!(folder_from_title("platform (Workspace)").as_deref(), Some("platform"));
        assert_eq!(folder_from_title("Visual Studio Code"), None);
        assert_eq!(folder_from_title(""), None);
    }

    #[test]
    fn numbering_is_alphabetical_by_folder() {
        let ws = number_windows(&[
            (10, "x.ts — web-app".into()),
            (11, "infra".into()),
            (12, "Visual Studio Code".into()),
            (13, "README.md — API-server".into()),
            (14, "docs-site".into()),
        ]);
        let order: Vec<_> = ws.iter().map(|w| (w.number, w.folder.clone().unwrap_or_default(), w.menu_index)).collect();
        assert_eq!(
            order,
            vec![
                (1, "API-server".into(), 13),
                (2, "docs-site".into(), 14),
                (3, "infra".into(), 11),
                (4, "web-app".into(), 10),
                (5, String::new(), 12),
            ]
        );
    }

    #[test]
    fn cycling() {
        let ws = number_windows(&[(0, "a.ts — infra".into()), (1, "web-app".into()), (2, "api-server".into())]);
        // A→Z: api-server, infra, web-app
        assert_eq!(cycle(&ws, Some("a.ts — infra"), true), Some(2));
        assert_eq!(cycle(&ws, Some("web-app"), true), Some(0));
        assert_eq!(cycle(&ws, Some("api-server"), false), Some(2));
        assert_eq!(cycle(&ws, None, true), Some(0));
        assert_eq!(cycle(&ws, None, false), Some(2));
        assert_eq!(cycle(&[], None, true), None);
    }
}

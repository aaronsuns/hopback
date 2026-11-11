//! Search ranking: title/folder matches first, then (history only) conversation
//! text matches with a short snippet around the hit.

use serde::Serialize;

#[derive(Debug, Clone)]
pub struct Entry<'a> {
    pub title: &'a str,
    pub folder: &'a str,
    pub updated_at: u64,
    /// Conversation text, when content search is allowed for this entry.
    pub text: Option<&'a str>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum MatchKind {
    Title,
    Content,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Hit {
    pub index: usize,
    pub kind: MatchKind,
    pub snippet: Option<String>,
}

pub const MIN_CONTENT_QUERY: usize = 3;
const SNIPPET_BEFORE: usize = 30;
const SNIPPET_AFTER: usize = 60;

fn score_token(token: &str, title: &str, folder: &str) -> Option<u32> {
    let t = title.to_lowercase();
    let f = folder.to_lowercase();
    let word_start = |hay: &str| {
        hay.match_indices(token)
            .any(|(i, _)| i == 0 || !hay[..i].chars().next_back().is_some_and(char::is_alphanumeric))
    };
    if f == token {
        Some(5)
    } else if t.starts_with(token) || f.starts_with(token) {
        Some(4)
    } else if word_start(&t) || word_start(&f) {
        Some(2)
    } else if t.contains(token) || f.contains(token) {
        Some(1)
    } else {
        None
    }
}

/// Rank entries for `query`. An empty query returns everything, newest first.
pub fn search(entries: &[Entry], query: &str) -> Vec<Hit> {
    let q = query.trim().to_lowercase();
    let tokens: Vec<&str> = q.split_whitespace().collect();
    let mut title_hits: Vec<(u32, u64, usize)> = entries
        .iter()
        .enumerate()
        .filter_map(|(i, e)| {
            let score = tokens.iter().map(|t| score_token(t, e.title, e.folder)).sum::<Option<u32>>()?;
            Some((score, e.updated_at, i))
        })
        .collect();
    title_hits.sort_by(|a, b| b.0.cmp(&a.0).then(b.1.cmp(&a.1)));
    let mut hits: Vec<Hit> =
        title_hits.iter().map(|&(_, _, index)| Hit { index, kind: MatchKind::Title, snippet: None }).collect();

    if q.chars().count() >= MIN_CONTENT_QUERY {
        let mut content: Vec<(u64, Hit)> = entries
            .iter()
            .enumerate()
            .filter(|(i, _)| !title_hits.iter().any(|h| h.2 == *i))
            .filter_map(|(i, e)| {
                let text = e.text?;
                let (start, end) = find_ci(text, &q)?;
                Some((
                    e.updated_at,
                    Hit { index: i, kind: MatchKind::Content, snippet: Some(snippet(text, start, end)) },
                ))
            })
            .collect();
        content.sort_by_key(|c| std::cmp::Reverse(c.0));
        hits.extend(content.into_iter().map(|(_, h)| h));
    }
    hits
}

/// Case-insensitive find of an already-lowercased needle. Returns the byte range
/// in `hay`. ASCII needles use a fast byte scan; others lowercase the haystack.
pub fn find_ci(hay: &str, needle_lower: &str) -> Option<(usize, usize)> {
    if needle_lower.is_empty() {
        return None;
    }
    if needle_lower.is_ascii() {
        let n = needle_lower.as_bytes();
        let h = hay.as_bytes();
        let (first_lo, first_up) = (n[0], n[0].to_ascii_uppercase());
        let mut i = 0;
        while i + n.len() <= h.len() {
            if (h[i] == first_lo || h[i] == first_up) && h[i..i + n.len()].eq_ignore_ascii_case(n) {
                return Some((i, i + n.len()));
            }
            i += 1;
        }
        return None;
    }
    // Map char positions of the lowercased text back to the original.
    let lowered: Vec<(usize, char)> =
        hay.char_indices().flat_map(|(i, c)| c.to_lowercase().map(move |l| (i, l))).collect();
    let needle: Vec<char> = needle_lower.chars().collect();
    let pos = lowered.windows(needle.len()).position(|w| w.iter().map(|x| x.1).eq(needle.iter().copied()))?;
    let start = lowered[pos].0;
    let last = lowered[pos + needle.len() - 1].0;
    let end = last + hay[last..].chars().next().map_or(0, char::len_utf8);
    Some((start, end))
}

/// A one-line snippet around `hay[start..end]` with ellipses where cut.
pub fn snippet(hay: &str, start: usize, end: usize) -> String {
    let before: String = {
        let chars: Vec<char> = hay[..start].chars().rev().take(SNIPPET_BEFORE + 1).collect();
        let cut = chars.len() > SNIPPET_BEFORE;
        let s: String = chars.into_iter().take(SNIPPET_BEFORE).collect::<Vec<_>>().into_iter().rev().collect();
        if cut {
            format!("…{}", s.trim_start())
        } else {
            s
        }
    };
    let after: String = {
        let mut it = hay[end..].chars();
        let s: String = it.by_ref().take(SNIPPET_AFTER).collect();
        if it.next().is_some() {
            format!("{}…", s.trim_end())
        } else {
            s
        }
    };
    let joined = format!("{before}{}{after}", &hay[start..end]);
    joined.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn e<'a>(title: &'a str, folder: &'a str, at: u64, text: Option<&'a str>) -> Entry<'a> {
        Entry { title, folder, updated_at: at, text }
    }

    #[test]
    fn empty_query_lists_newest_first() {
        let es = [e("a", "web-app", 1, None), e("b", "infra", 3, None), e("c", "api-server", 2, None)];
        let idx: Vec<_> = search(&es, "  ").iter().map(|h| h.index).collect();
        assert_eq!(idx, vec![1, 2, 0]);
    }

    #[test]
    fn ranks_prefix_and_word_matches_above_substrings() {
        let es = [
            e("Refactor the login form", "web-app", 9, None),
            e("Login rate limiting", "api-server", 1, None),
            e("Fix bloginator", "docs-site", 5, None),
            e("Unrelated", "infra", 7, None),
        ];
        let idx: Vec<_> = search(&es, "login").iter().map(|h| h.index).collect();
        assert_eq!(idx, vec![1, 0, 2]);
    }

    #[test]
    fn all_tokens_must_match_across_title_and_folder() {
        let es = [e("Terraform state lock", "infra", 1, None), e("State machine", "web-app", 2, None)];
        let idx: Vec<_> = search(&es, "infra state").iter().map(|h| h.index).collect();
        assert_eq!(idx, vec![0]);
        // Exact folder name ranks highest.
        let idx: Vec<_> = search(&es, "web-app").iter().map(|h| h.index).collect();
        assert_eq!(idx, vec![1]);
    }

    #[test]
    fn content_matches_follow_title_matches_with_snippets() {
        let long = format!("{} the flaky Websocket reconnect test {}", "x ".repeat(40), "y ".repeat(60));
        let es = [
            e("Websocket client", "web-app", 1, Some("nothing")),
            e("Deploy pipeline", "infra", 9, Some(&long)),
            e("Other", "docs-site", 5, Some("no hit here")),
        ];
        let hits = search(&es, "websocket");
        assert_eq!(hits.len(), 2);
        assert_eq!(hits[0], Hit { index: 0, kind: MatchKind::Title, snippet: None });
        assert_eq!(hits[1].kind, MatchKind::Content);
        let snip = hits[1].snippet.as_deref().unwrap();
        assert!(snip.starts_with('…') && snip.ends_with('…'), "{snip}");
        assert!(snip.contains("Websocket reconnect"));
    }

    #[test]
    fn short_queries_do_not_search_content() {
        let es = [e("A", "web-app", 1, Some("go go"))];
        assert!(search(&es, "go").is_empty());
    }

    #[test]
    fn case_insensitive_find_handles_unicode() {
        assert_eq!(find_ci("Hello World", "world"), Some((6, 11)));
        assert_eq!(find_ci("abc", "abcd"), None);
        let s = "Läs ÅTERSTÅENDE fel";
        let (a, b) = find_ci(s, "återstående").unwrap();
        assert_eq!(&s[a..b], "ÅTERSTÅENDE");
        assert_eq!(find_ci("x", ""), None);
    }

    #[test]
    fn snippets_are_one_line() {
        let s = "line one\n\nline   two has the word here\nand more";
        let (a, b) = find_ci(s, "word").unwrap();
        assert_eq!(snippet(s, a, b), "line one line two has the word here and more");
    }
}

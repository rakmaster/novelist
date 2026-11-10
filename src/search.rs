use crate::model::{Book, NodeId};
use std::ops::Range;

#[derive(Clone, Debug, Default)]
pub struct Query {
    pub needle: String,
    pub case_sensitive: bool,
    pub whole_word: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Match {
    pub node: NodeId,
    /// Byte range within the node body.
    pub range: Range<usize>,
}

fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || c == '\''
}

fn is_apostrophe(c: char) -> bool {
    c == '\'' || c == '\u{2019}'
}

/// Boundary before `i`: not a word char, or an apostrophe that opens a quote.
fn boundary_before(hay: &[(usize, char)], i: usize) -> bool {
    if i == 0 {
        return true;
    }
    let p = hay[i - 1].1;
    !is_word_char(p) || (is_apostrophe(p) && (i == 1 || !hay[i - 2].1.is_alphabetic()))
}

/// Boundary after `end`: not a word char, or a closing quote / possessive 's.
fn boundary_after(hay: &[(usize, char)], end: usize) -> bool {
    let Some(&(_, c)) = hay.get(end) else { return true };
    if !is_word_char(c) {
        return true;
    }
    if !is_apostrophe(c) {
        return false;
    }
    match hay.get(end + 1) {
        None => true,
        Some(&(_, n)) if !n.is_alphabetic() => true,
        Some(&(_, 's' | 'S')) => hay.get(end + 2).is_none_or(|&(_, n)| !n.is_alphabetic()),
        _ => false,
    }
}

fn chars_eq(a: char, b: char, case_sensitive: bool) -> bool {
    a == b || (!case_sensitive && a.to_lowercase().eq(b.to_lowercase()))
}

/// All non-overlapping matches of `q` in `text`, as byte ranges.
pub fn find_in_text(text: &str, q: &Query) -> Vec<Range<usize>> {
    let needle: Vec<char> = q.needle.chars().collect();
    if needle.is_empty() {
        return Vec::new();
    }
    let hay: Vec<(usize, char)> = text.char_indices().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i + needle.len() <= hay.len() {
        let hit = needle
            .iter()
            .enumerate()
            .all(|(k, &n)| chars_eq(hay[i + k].1, n, q.case_sensitive));
        let ok = hit && {
            let end = i + needle.len();
            !q.whole_word || (boundary_before(&hay, i) && boundary_after(&hay, end))
        };
        if ok {
            let start = hay[i].0;
            let end = hay.get(i + needle.len()).map_or(text.len(), |h| h.0);
            out.push(start..end);
            i += needle.len();
        } else {
            i += 1;
        }
    }
    out
}

/// All matches across the whole manuscript in reading order.
pub fn find_all(book: &Book, q: &Query) -> Vec<Match> {
    let mut out = Vec::new();
    for (id, _, _) in book.flatten() {
        if let Some(n) = book.find(id) {
            for range in find_in_text(&n.body, q) {
                out.push(Match { node: id, range });
            }
        }
    }
    out
}

/// Every mention of any of `terms` (exact case, whole word) in reading order.
/// Overlapping hits (e.g. "Dan" inside "Dan Dan") keep only the longest.
pub fn mentions(book: &Book, terms: &[String]) -> Vec<Match> {
    let queries: Vec<Query> = terms
        .iter()
        .map(|t| Query { needle: t.clone(), case_sensitive: true, whole_word: true })
        .collect();
    let mut out = Vec::new();
    for (id, _, _) in book.flatten() {
        let Some(n) = book.find(id) else { continue };
        let mut ranges: Vec<Range<usize>> =
            queries.iter().flat_map(|q| find_in_text(&n.body, q)).collect();
        ranges.sort_by_key(|r| (r.start, std::cmp::Reverse(r.end)));
        let mut last_end = 0;
        for range in ranges {
            if range.start >= last_end {
                last_end = range.end;
                out.push(Match { node: id, range });
            }
        }
    }
    out
}

pub fn replace_in_text(text: &str, q: &Query, with: &str) -> (String, usize) {
    let ranges = find_in_text(text, q);
    let mut out = String::with_capacity(text.len());
    let mut last = 0;
    for r in &ranges {
        out.push_str(&text[last..r.start]);
        out.push_str(with);
        last = r.end;
    }
    out.push_str(&text[last..]);
    (out, ranges.len())
}

pub fn replace_all(book: &mut Book, q: &Query, with: &str) -> usize {
    let ids: Vec<NodeId> = book.flatten().into_iter().map(|(id, _, _)| id).collect();
    let mut total = 0;
    for id in ids {
        if let Some(n) = book.find_mut(id) {
            let (s, c) = replace_in_text(&n.body, q, with);
            if c > 0 {
                n.body = s;
                total += c;
            }
        }
    }
    total
}

pub fn replace_one(book: &mut Book, m: &Match, with: &str) {
    if let Some(n) = book.find_mut(m.node) {
        if m.range.end <= n.body.len() {
            n.body.replace_range(m.range.clone(), with);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn q(s: &str, cs: bool, ww: bool) -> Query {
        Query { needle: s.into(), case_sensitive: cs, whole_word: ww }
    }

    #[test]
    fn case_insensitive_and_whole_word() {
        let t = "Cat cat concat";
        assert_eq!(find_in_text(t, &q("cat", false, false)).len(), 3);
        assert_eq!(find_in_text(t, &q("cat", true, false)).len(), 2);
        assert_eq!(find_in_text(t, &q("cat", false, true)).len(), 2);
    }

    #[test]
    fn possessives_and_quotes_count_as_word_boundaries() {
        let t = "Paris's hat. 'Paris' said Paris. Parisian";
        assert_eq!(find_in_text(t, &q("Paris", true, true)).len(), 3);
    }

    #[test]
    fn mentions_use_longest_overlap() {
        let mut b = Book::default();
        let id = b.roots[0].id;
        b.find_mut(id).unwrap().body = "Dan Dan met Dan. Dan Dan left.".into();
        let m = mentions(&b, &["Dan Dan".into(), "Dan".into()]);
        assert_eq!(m.len(), 3);
    }

    #[test]
    fn replace_handles_unicode() {
        let (s, n) = replace_in_text("é cat é", &q("cat", false, false), "dog");
        assert_eq!((s.as_str(), n), ("é dog é", 1));
    }
}

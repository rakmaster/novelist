//! Lightweight Markdown-style inline formatting stored directly in chapter text:
//! `**bold**`, `*italic*`, `__underline__`, and `#`/`##`/`###` heading lines.

use std::ops::Range;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Style {
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub heading: u8,
    /// True for the formatting characters themselves (shown dimmed).
    pub marker: bool,
}

/// Split `text` into consecutive styled byte ranges covering the whole string.
pub fn spans(text: &str) -> Vec<(Range<usize>, Style)> {
    let mut out: Vec<(Range<usize>, Style)> = Vec::new();
    let mut push = |range: Range<usize>, style: Style| {
        if range.is_empty() {
            return;
        }
        if let Some((r, s)) = out.last_mut() {
            if *s == style && r.end == range.start {
                r.end = range.end;
                return;
            }
        }
        out.push((range, style));
    };

    let mut line_start = 0;
    for line in text.split_inclusive('\n') {
        let heading = heading_level(line);
        let base = Style { heading, ..Style::default() };
        let mut cur = base;
        let mut i = 0;
        if heading > 0 {
            let n = heading as usize + 1;
            push(line_start..line_start + n, Style { marker: true, ..base });
            i = n;
        }
        let bytes = line.as_bytes();
        let mut seg_start = i;
        while i < bytes.len() {
            let rest = &bytes[i..];
            let (len, toggle): (usize, fn(&mut Style)) = if rest.starts_with(b"**") {
                (2, |s| s.bold = !s.bold)
            } else if rest.starts_with(b"__") {
                (2, |s| s.underline = !s.underline)
            } else if rest[0] == b'*' {
                (1, |s| s.italic = !s.italic)
            } else {
                i += 1;
                continue;
            };
            push(line_start + seg_start..line_start + i, cur);
            let mut m = cur;
            m.marker = true;
            push(line_start + i..line_start + i + len, m);
            toggle(&mut cur);
            i += len;
            seg_start = i;
        }
        push(line_start + seg_start..line_start + line.len(), cur);
        line_start += line.len();
    }
    out
}

fn heading_level(line: &str) -> u8 {
    for (n, p) in [(3u8, "### "), (2, "## "), (1, "# ")] {
        if line.starts_with(p) {
            return n;
        }
    }
    0
}

fn char_to_byte(s: &str, c: usize) -> usize {
    s.char_indices().nth(c).map_or(s.len(), |(b, _)| b)
}

fn byte_to_char(s: &str, b: usize) -> usize {
    s[..b].chars().count()
}

/// Normalise body text to standard manuscript conventions: no tabs or leading
/// spaces (indentation is applied by the editor), single spaces between
/// words and sentences, no trailing spaces, and no blank lines between
/// paragraphs. Scene breaks should be a lone `#` line.
pub fn clean_manuscript(text: &str) -> String {
    let lines: Vec<String> = text
        .lines()
        .map(|l| l.replace('\t', " ").split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|l| !l.is_empty())
        .collect();
    lines.join("\n")
}

/// Toggle an inline marker (`**`, `*`, `__`) around the char range `sel`.
/// Returns the new char selection.
pub fn toggle_inline(text: &mut String, sel: Range<usize>, marker: &str) -> Range<usize> {
    let mlen = marker.chars().count();
    let (a, b) = (char_to_byte(text, sel.start), char_to_byte(text, sel.end));
    let wrapped = a >= marker.len()
        && text[..a].ends_with(marker)
        && text[b..].starts_with(marker)
        && !(marker == "*" && (text[..a].ends_with("**") && text[b..].starts_with("**")));
    if wrapped {
        text.replace_range(b..b + marker.len(), "");
        text.replace_range(a - marker.len()..a, "");
        sel.start - mlen..sel.end - mlen
    } else {
        text.insert_str(b, marker);
        text.insert_str(a, marker);
        sel.start + mlen..sel.end + mlen
    }
}

/// Set the heading level (0 = plain) of every line touched by `sel`; toggles
/// off when the first line already has that level.
pub fn toggle_heading(text: &mut String, sel: Range<usize>, level: u8) -> Range<usize> {
    let a = char_to_byte(text, sel.start);
    let b = char_to_byte(text, sel.end);
    let start = text[..a].rfind('\n').map_or(0, |p| p + 1);
    let end = text[b..].find('\n').map_or(text.len(), |p| b + p);
    let region = text[start..end].to_string();
    let first_level = heading_level(&region);
    let target = if first_level == level { 0 } else { level };

    let new_region: Vec<String> = region
        .split('\n')
        .map(|l| {
            let lv = heading_level(l);
            let body = &l[if lv > 0 { lv as usize + 1 } else { 0 }..];
            if target > 0 {
                format!("{} {}", "#".repeat(target as usize), body)
            } else {
                body.to_string()
            }
        })
        .collect();
    let new_region = new_region.join("\n");
    let new_end_char = byte_to_char(text, start) + new_region.chars().count();
    text.replace_range(start..end, &new_region);
    let s = sel.start.min(new_end_char);
    s..new_end_char.max(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_inline_styles() {
        let sp = spans("a **b** *c*");
        let bold: Vec<_> = sp.iter().filter(|(_, s)| s.bold && !s.marker).collect();
        assert_eq!(bold.len(), 1);
        assert_eq!(bold[0].0, 4..5);
        assert!(sp.iter().any(|(r, s)| s.italic && !s.marker && r == &(9..10)));
    }

    #[test]
    fn spans_cover_everything() {
        let t = "# H\nsome **x** and\n*y*";
        let sp = spans(t);
        assert_eq!(sp.first().unwrap().0.start, 0);
        assert_eq!(sp.last().unwrap().0.end, t.len());
        for w in sp.windows(2) {
            assert_eq!(w[0].0.end, w[1].0.start);
        }
    }

    #[test]
    fn toggle_wraps_and_unwraps() {
        let mut t = String::from("hello world");
        let sel = toggle_inline(&mut t, 0..5, "**");
        assert_eq!(t, "**hello** world");
        assert_eq!(sel, 2..7);
        let sel = toggle_inline(&mut t, sel, "**");
        assert_eq!(t, "hello world");
        assert_eq!(sel, 0..5);
    }

    #[test]
    fn cleans_manuscript_spacing() {
        let t = "\tIndented  line.  Two spaces. \n\n\n    Next paragraph\t\n#\n\nAfter break.\n";
        assert_eq!(
            clean_manuscript(t),
            "Indented line. Two spaces.\nNext paragraph\n#\nAfter break."
        );
        assert_eq!(clean_manuscript(""), "");
    }

    #[test]
    fn heading_toggle() {
        let mut t = String::from("Title\nbody");
        toggle_heading(&mut t, 0..0, 1);
        assert_eq!(t, "# Title\nbody");
        toggle_heading(&mut t, 0..0, 2);
        assert_eq!(t, "## Title\nbody");
        toggle_heading(&mut t, 0..0, 2);
        assert_eq!(t, "Title\nbody");
    }
}

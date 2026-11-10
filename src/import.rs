//! Plain-text import of characters (and a combined outline + characters document).
//!
//! Characters are recognised in any of these layouts (they can be mixed within
//! one document as long as it has no `#` headings, or use headings throughout):
//!
//! ```text
//! # Gavan                         Dan Dan: Smuggler boss of the docks.
//! Aliases: The Wizard
//! Role: war wizard                Gavan (war wizard)
//! Description: Tall, scarred.     Tall, scarred, patient.
//! ```
//!
//! Field labels: Aliases/AKA/Nicknames, Role/Occupation, Description/Appearance,
//! Synopsis/Arc/Summary, Notes/Background/Personality/Traits.

use crate::model::{Book, Character, WorldEntry};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Field {
    Aliases,
    Role,
    Description,
    Synopsis,
    Notes,
}

fn strip_bullet(line: &str) -> &str {
    line.trim_start_matches(['-', '*', '•', ' ', '\t'])
}

fn label(line: &str) -> Option<(Field, &str)> {
    let (l, rest) = line.split_once(':')?;
    let f = match l.trim().trim_matches('*').to_lowercase().as_str() {
        "aliases" | "alias" | "aka" | "also known as" | "nicknames" | "nickname" | "other names" => {
            Field::Aliases
        }
        "role" | "occupation" | "type" => Field::Role,
        "description" | "desc" | "appearance" | "bio" | "about" => Field::Description,
        "synopsis" | "arc" | "summary" | "character arc" | "story" => Field::Synopsis,
        "notes" | "note" | "background" | "traits" | "personality" => Field::Notes,
        _ => return None,
    };
    Some((f, rest.trim().trim_matches('*').trim()))
}

/// Split "Name (extra)" into (name, aliases, role).
fn split_name(raw: &str) -> (String, String, String) {
    let s = raw
        .trim_start_matches('#')
        .trim()
        .trim_matches('*')
        .trim_end_matches(':')
        .trim_matches('*')
        .trim();
    if let (Some(open), true) = (s.rfind('('), s.ends_with(')')) {
        let inner = s[open + 1..s.len() - 1].trim();
        let name = s[..open].trim().to_string();
        let lower = inner.to_lowercase();
        for p in ["aka ", "a.k.a. ", "also known as ", "aka: "] {
            if lower.starts_with(p) {
                return (name, inner[p.len()..].trim().to_string(), String::new());
            }
        }
        return (name, String::new(), inner.to_string());
    }
    (s.to_string(), String::new(), String::new())
}

fn starts_upper(s: &str) -> bool {
    s.trim_start_matches('*').chars().next().is_some_and(char::is_uppercase)
}

/// "Name: text", "Name - text" ...; the name part must be short and capitalised.
fn split_entry(line: &str) -> Option<(&str, &str)> {
    let line = strip_bullet(line);
    for sep in [": ", " – ", " — ", " - "] {
        if let Some((a, b)) = line.split_once(sep) {
            if a.split_whitespace().count() <= 5 && starts_upper(a) && !b.trim().is_empty() {
                return Some((a, b.trim()));
            }
        }
    }
    None
}

fn looks_like_name(line: &str) -> bool {
    let l = strip_bullet(line);
    l.split_whitespace().count() <= 5 && !l.ends_with(['.', '!', '?', ',']) && starts_upper(l)
}

fn append(c: &mut Character, f: Field, text: &str) {
    let (target, sep) = match f {
        Field::Aliases => (&mut c.aliases, ", "),
        Field::Role => (&mut c.role, " "),
        Field::Description => (&mut c.description, " "),
        Field::Synopsis => (&mut c.synopsis, " "),
        Field::Notes => (&mut c.notes, " "),
    };
    if text.is_empty() {
        return;
    }
    if !target.is_empty() {
        target.push_str(sep);
    }
    target.push_str(text);
}

fn parse_characters(text: &str) -> Vec<Character> {
    let heading_mode = text.lines().any(|l| l.trim_start().starts_with('#'));
    let mut out: Vec<Character> = Vec::new();
    let mut last = Field::Description;
    let mut prev_blank = true;
    let new_char = |out: &mut Vec<Character>, raw: &str| {
        let (name, aliases, role) = split_name(raw);
        out.push(Character { name, aliases, role, ..Character::default() });
    };
    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() {
            prev_blank = true;
            continue;
        }
        let was_blank = std::mem::replace(&mut prev_blank, false);
        if line.starts_with('#') {
            new_char(&mut out, line);
            last = Field::Description;
            continue;
        }
        if let Some((f, rest)) = label(strip_bullet(line)) {
            if let Some(c) = out.last_mut() {
                append(c, f, rest);
                last = f;
            }
            continue;
        }
        if !heading_mode {
            if let Some((name, rest)) = split_entry(line) {
                new_char(&mut out, name);
                if let Some(c) = out.last_mut() {
                    append(c, Field::Description, rest);
                }
                last = Field::Description;
                continue;
            }
            if was_blank && looks_like_name(line) {
                new_char(&mut out, strip_bullet(line));
                last = Field::Description;
                continue;
            }
        }
        if let Some(c) = out.last_mut() {
            let f = if matches!(last, Field::Role | Field::Aliases) { Field::Description } else { last };
            append(c, f, strip_bullet(line));
        }
    }
    out.retain(|c| !c.name.is_empty());
    out
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Section {
    Outline,
    Characters,
    World,
}

fn section_header(line: &str) -> Option<Section> {
    let t = line.trim().trim_start_matches('#').trim().trim_matches('*').trim_end_matches(':');
    match t.trim().to_lowercase().as_str() {
        "characters" | "character list" | "cast" | "cast of characters" | "dramatis personae" => {
            Some(Section::Characters)
        }
        "outline" | "chapters" | "chapter outline" | "plot outline" => Some(Section::Outline),
        "background" | "story bible" | "world" | "worldbuilding" | "world building" | "world-building" => {
            Some(Section::World)
        }
        _ => None,
    }
}

impl Book {
    /// Returns (added, updated). Characters whose name already exists
    /// (case-insensitive) only get their empty fields filled in.
    pub fn import_characters(&mut self, text: &str) -> (usize, usize) {
        let (mut added, mut updated) = (0, 0);
        for p in parse_characters(text) {
            let existing = self
                .characters
                .iter()
                .position(|c| c.name.to_lowercase() == p.name.to_lowercase());
            match existing {
                Some(i) => {
                    let c = &mut self.characters[i];
                    for (dst, src) in [
                        (&mut c.role, &p.role),
                        (&mut c.description, &p.description),
                        (&mut c.synopsis, &p.synopsis),
                        (&mut c.notes, &p.notes),
                    ] {
                        if dst.is_empty() {
                            *dst = src.clone();
                        }
                    }
                    if c.aliases.is_empty() {
                        c.aliases = p.aliases.clone();
                    }
                    updated += 1;
                }
                None => {
                    let id = self.add_character(&p.name);
                    if let Some(c) = self.find_character_mut(id) {
                        c.aliases = p.aliases;
                        c.role = p.role;
                        c.description = p.description;
                        c.synopsis = p.synopsis;
                        c.notes = p.notes;
                    }
                    added += 1;
                }
            }
        }
        (added, updated)
    }

    /// Import one document holding both parts, split by lines such as
    /// "Characters" / "Outline" (optionally `#` headings). Text before the first
    /// header is treated as the outline. Returns (sections, chars added,
    /// chars updated, world entries added or updated).
    pub fn import_combined(&mut self, text: &str) -> (usize, usize, usize, usize) {
        let mut outline = String::new();
        let mut chars = String::new();
        let mut world = String::new();
        let mut cur = Section::Outline;
        for line in text.lines() {
            if let Some(s) = section_header(line) {
                cur = s;
                continue;
            }
            let buf = match cur {
                Section::Outline => &mut outline,
                Section::Characters => &mut chars,
                Section::World => &mut world,
            };
            buf.push_str(line);
            buf.push('\n');
        }
        let sections = self.import_outline(&outline);
        let (a, u) = self.import_characters(&chars);
        let (wa, wu) = self.import_world(&world);
        (sections, a, u, wa + wu)
    }

    /// Returns (added, updated). Entries are matched by category + name
    /// (case-insensitive); existing entries only get empty fields filled.
    pub fn import_world(&mut self, text: &str) -> (usize, usize) {
        let (mut added, mut updated) = (0, 0);
        for p in parse_world(text) {
            let same = |e: &WorldEntry| {
                e.category.to_lowercase() == p.category.to_lowercase()
                    && e.name.to_lowercase() == p.name.to_lowercase()
            };
            match self.world.iter().position(same) {
                Some(i) => {
                    let e = &mut self.world[i];
                    if e.description.is_empty() {
                        e.description = p.description.clone();
                    }
                    if e.aliases.is_empty() {
                        e.aliases = p.aliases.clone();
                    }
                    updated += 1;
                }
                None => {
                    let id = self.add_world_entry(&p.category, &p.name);
                    if let Some(e) = self.find_world_mut(id) {
                        e.aliases = p.aliases;
                        e.description = p.description;
                    }
                    added += 1;
                }
            }
        }
        (added, updated)
    }
}

fn word_count(s: &str) -> usize {
    s.split_whitespace().count()
}

fn ends_sentence(s: &str) -> bool {
    s.ends_with(['.', '!', '?', ',', ';'])
}

/// "Balon - Ba - Si'Bal" -> ("Balon", "Ba, Si'Bal"). Plain hyphens inside a
/// word ("Jean-Luc") are left alone.
fn split_alt_names(raw: &str) -> (String, String) {
    let raw = raw.trim();
    if raw.contains(" -") || raw.contains("- ") {
        let mut parts = raw.split('-').map(str::trim).filter(|s| !s.is_empty());
        let name = parts.next().unwrap_or_default().to_string();
        return (name, parts.collect::<Vec<_>>().join(", "));
    }
    (raw.to_string(), String::new())
}

fn entry_title_ok(line: &str) -> bool {
    word_count(line) <= 6 && !ends_sentence(line) && !line.contains(':') && starts_upper(line)
}

/// A short line ending in ':' or starting with 'Appendix:' is always a category.
fn forced_heading(line: &str) -> Option<String> {
    let n = word_count(line);
    if let Some(rest) = line.strip_prefix("Appendix:") {
        return (n <= 8 && !rest.trim().is_empty()).then(|| rest.trim().to_string());
    }
    (line.ends_with(':') && n <= 6).then(|| line.trim_end_matches(':').trim().to_string())
}

enum Item {
    Heading(String),
    Block(Vec<String>),
}

fn append_body(e: &mut WorldEntry, text: &str) {
    if !e.description.is_empty() {
        e.description.push_str("\n\n");
    }
    e.description.push_str(text);
}

/// Parse world-building notes into categorised entries.
///
/// With `#` / `##` lines: `# Category`, `## Entry`, anything else is the entry text.
/// Otherwise (plain text pasted from a document): a short line standing alone
/// (or ending in ':', or starting 'Appendix:') is a category; a paragraph whose
/// first line is a short title and whose second line is ordinary prose is an
/// entry; other text is added to the previous entry, or to an entry named after
/// the category.
fn parse_world(text: &str) -> Vec<WorldEntry> {
    let mut out: Vec<WorldEntry> = Vec::new();
    let new_entry = |out: &mut Vec<WorldEntry>, cat: &str, raw_name: &str| {
        let (name, aliases) = split_alt_names(raw_name);
        out.push(WorldEntry {
            category: cat.to_string(),
            name,
            aliases,
            ..WorldEntry::default()
        });
    };

    if text.lines().any(|l| l.trim_start().starts_with('#')) {
        let mut cat = "General".to_string();
        for raw in text.lines() {
            let line = raw.trim();
            if let Some(h) = line.strip_prefix("##") {
                new_entry(&mut out, &cat, h.trim().trim_start_matches('#').trim());
            } else if let Some(h) = line.strip_prefix('#') {
                cat = h.trim().to_string();
            } else if let Some(e) = out.last_mut() {
                if line.is_empty() {
                    if !e.description.is_empty() && !e.description.ends_with("\n\n") {
                        e.description.push_str("\n\n");
                    }
                } else {
                    if !e.description.is_empty() && !e.description.ends_with("\n\n") {
                        e.description.push('\n');
                    }
                    e.description.push_str(line);
                }
            }
        }
        for e in &mut out {
            e.description = e.description.trim().to_string();
        }
        out.retain(|e| !e.name.is_empty());
        return out;
    }

    let mut items: Vec<Item> = Vec::new();
    let mut cur: Vec<String> = Vec::new();
    let flush = |items: &mut Vec<Item>, cur: &mut Vec<String>| {
        if !cur.is_empty() {
            items.push(Item::Block(std::mem::take(cur)));
        }
    };
    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() {
            flush(&mut items, &mut cur);
        } else if let Some(h) = forced_heading(line) {
            flush(&mut items, &mut cur);
            items.push(Item::Heading(h));
        } else {
            cur.push(line.to_string());
        }
    }
    flush(&mut items, &mut cur);

    let mut cat = "General".to_string();
    for item in items {
        let lines = match item {
            Item::Heading(h) => {
                cat = h;
                continue;
            }
            Item::Block(l) => l,
        };
        if lines.len() == 1 && entry_title_ok(&lines[0]) && !lines[0].contains(['\u{2014}', '\u{2013}']) {
            cat = lines[0].clone();
            continue;
        }
        let is_list = lines.len() >= 3 && lines.iter().all(|l| word_count(l) <= 4);
        let titled = lines.len() >= 2 && entry_title_ok(&lines[0]) && !lines[1].contains(':') && !is_list;
        if titled {
            new_entry(&mut out, &cat, &lines[0]);
            if let Some(e) = out.last_mut() {
                e.description = lines[1..].join("\n");
            }
            continue;
        }
        let body = lines.join("\n");
        match out.last_mut().filter(|e| e.category == cat) {
            Some(e) => append_body(e, &body),
            None => {
                out.push(WorldEntry { category: cat.clone(), name: cat.clone(), description: body, ..WorldEntry::default() });
            }
        }
    }
    out.retain(|e| !e.name.is_empty());
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn empty() -> Book {
        let mut b = Book::default();
        b.roots.clear();
        b
    }

    #[test]
    fn heading_layout() {
        let mut b = empty();
        let (a, _) = b.import_characters(
            "# Gavan (war wizard)\nAliases: The Wizard, Gav\nDescription: Tall.\nScarred.\n\n# Paris\nA boy.\nSynopsis: Grows up.",
        );
        assert_eq!(a, 2);
        let g = &b.characters[0];
        assert_eq!((g.name.as_str(), g.role.as_str()), ("Gavan", "war wizard"));
        assert_eq!(g.aliases, "The Wizard, Gav");
        assert_eq!(g.description, "Tall. Scarred.");
        assert_eq!(b.characters[1].description, "A boy.");
        assert_eq!(b.characters[1].synopsis, "Grows up.");
        assert_eq!(g.terms(), ["Gavan", "The Wizard", "Gav"]);
    }

    #[test]
    fn line_and_block_layouts() {
        let mut b = empty();
        b.import_characters(
            "Dan Dan: Smuggler boss.\n- Marvan - Archmage of the council\n\nCherine\nRole: courtesan\nShe is clever.",
        );
        let names: Vec<_> = b.characters.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, ["Dan Dan", "Marvan", "Cherine"]);
        assert_eq!(b.characters[1].description, "Archmage of the council");
        assert_eq!(b.characters[2].role, "courtesan");
        assert_eq!(b.characters[2].description, "She is clever.");
    }

    #[test]
    fn merge_fills_empty_fields_only() {
        let mut b = empty();
        b.import_characters("Paris: A boy.");
        let (a, u) = b.import_characters("Paris: Different.\nRole: courier");
        assert_eq!((a, u), (0, 1));
        assert_eq!(b.characters[0].description, "A boy.");
        assert_eq!(b.characters[0].role, "courier");
    }

    const WORLD: &str = "Magic System\n\nAll magic is evil\nEvil: selfishness exemplified by detriment to others\nMagic: the desire to affect results\n\nTime Frame\n\nSome things are very advanced, some things very backwards. The world is stuck.\n\nAnother paragraph about time and the age of the world and so on.\n\nNations\n\nGreat Andrews\nThe largest nation. Royalty with parliament. Sea trade, farms and military.\n\nCastle Mount \u{2014} capital city\n\nThe Outer\nBedouin tribes\n\nMerchant Guilds\n\nMachinists\nMasons\nGoldsmiths\nCriminal Families & Hierarchy: \nCriminal syndicates operate structured hierarchies.\nFather / Patriarch: The absolute authority.\n\nReligions\n\nAll religions are related to the same pantheon of many gods.\n\nBalon - Ba - Si\u{2019}Bal\nLord of the gods, master of sun and weather.\n\nThibul - Thrus- Trib\nGod of frightening weather.";

    #[test]
    fn world_plain_text_layout() {
        let mut b = empty();
        let (added, _) = b.import_world(WORLD);
        let got: Vec<(String, String)> =
            b.world.iter().map(|e| (e.category.clone(), e.name.clone())).collect();
        let want: Vec<(&str, &str)> = vec![
            ("Magic System", "Magic System"),
            ("Time Frame", "Time Frame"),
            ("Nations", "Great Andrews"),
            ("Nations", "The Outer"),
            ("Merchant Guilds", "Merchant Guilds"),
            ("Criminal Families & Hierarchy", "Criminal Families & Hierarchy"),
            ("Religions", "Religions"),
            ("Religions", "Balon"),
            ("Religions", "Thibul"),
        ];
        assert_eq!(added, want.len());
        let want: Vec<(String, String)> =
            want.into_iter().map(|(a, b)| (a.to_string(), b.to_string())).collect();
        assert_eq!(got, want);
        assert!(b.world[0].description.contains("All magic is evil"));
        assert!(b.world[1].description.contains("Another paragraph"));
        assert!(b.world[2].description.contains("Castle Mount"));
        assert_eq!(b.world[7].aliases, "Ba, Si\u{2019}Bal");
        assert_eq!(b.world[8].aliases, "Thrus, Trib");
    }

    #[test]
    fn world_markdown_layout_and_merge() {
        let mut b = empty();
        b.import_world("# Guilds\n## The Black Hand\nBlack market.\n\nMore.\n## Makers\nBuilders.");
        assert_eq!(b.world.len(), 2);
        assert_eq!(b.world[0].category, "Guilds");
        assert_eq!(b.world[0].description, "Black market.\n\nMore.");
        let (a, u) = b.import_world("# Guilds\n## makers\nOther.");
        assert_eq!((a, u), (0, 1));
        assert_eq!(b.world[1].description, "Builders.");
    }

    #[test]
    fn combined_document() {
        let mut b = Book::default();
        let r = b.import_combined(
            "Outline\nChapter 1: Market Street: Paris visits.\nCharacters\nParis: A boy.\nCherine: A courtesan.",
        );
        assert_eq!(r, (1, 2, 0, 0));
        assert_eq!(b.roots.len(), 1);
        assert_eq!(b.roots[0].title, "Market Street");
    }
}

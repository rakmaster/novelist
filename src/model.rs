use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub type NodeId = u64;

/// What an outline item is. A `Part` only divides the book (title and
/// synopsis, no text); `Matter` is an unnumbered front/back section such as a
/// Prelude or Epilogue; everything else is a `Chapter`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum NodeKind {
    #[default]
    Chapter,
    Part,
    Matter,
}

fn is_number_word(w: &str) -> bool {
    let lw = w.to_lowercase();
    !w.is_empty()
        && (w.chars().all(|c| c.is_ascii_digit())
            || w.chars().all(|c| "IVXLCivxlc".contains(c))
            || ["one", "two", "three", "four", "five", "six", "seven", "eight", "nine", "ten"]
                .contains(&lw.as_str()))
}

/// Guess the kind from the title, for books saved before kinds existed.
fn infer_kind(title: &str) -> NodeKind {
    let mut words = title.split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty());
    let first = words.next().unwrap_or("").to_lowercase();
    match first.as_str() {
        "part" if words.next().is_some_and(is_number_word) => NodeKind::Part,
        "prelude" | "prologue" | "epilogue" | "interlude" | "foreword" | "preface" | "afterword"
        | "introduction" | "acknowledgments" | "acknowledgements" | "dedication" => NodeKind::Matter,
        _ => NodeKind::Chapter,
    }
}

/// One outline item. Its `title` and `synopsis` are the outline entry; its
/// `body` is the manuscript text of that section. Because the manuscript is
/// derived from the outline tree, the two can never diverge.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Node {
    pub id: NodeId,
    pub title: String,
    #[serde(default)]
    pub synopsis: String,
    #[serde(default)]
    pub body: String,
    #[serde(default)]
    pub children: Vec<Node>,
    #[serde(default)]
    pub collapsed: bool,
    /// `None` (older files) means: infer from the title.
    #[serde(default)]
    pub kind: Option<NodeKind>,
}

/// A person in the story. `aliases` is a comma-separated list of other names
/// or nicknames that also count as mentions of this character.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Character {
    pub id: NodeId,
    pub name: String,
    #[serde(default)]
    pub aliases: String,
    #[serde(default)]
    pub role: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub synopsis: String,
    #[serde(default)]
    pub notes: String,
    /// Portrait as base64-encoded JPEG (see `portrait::encode`).
    #[serde(default)]
    pub image: Option<String>,
}

fn default_genre() -> String {
    crate::templates::DEFAULT_GENRE.into()
}

fn name_terms(name: &str, aliases: &str) -> Vec<String> {
    std::iter::once(name)
        .chain(aliases.split(','))
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(String::from)
        .collect()
}

impl Character {
    /// The name followed by every alias, trimmed and non-empty.
    pub fn terms(&self) -> Vec<String> {
        name_terms(&self.name, &self.aliases)
    }
}

/// A world-building entry (a nation, guild, god, magic rule, ...) filed under a
/// free-form `category`. `aliases` works like `Character::aliases`, e.g. the
/// regional names of a god.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct WorldEntry {
    pub id: NodeId,
    pub category: String,
    pub name: String,
    #[serde(default)]
    pub aliases: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub notes: String,
}

impl WorldEntry {
    pub fn terms(&self) -> Vec<String> {
        name_terms(&self.name, &self.aliases)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DropPos {
    Before,
    After,
    Inside,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Book {
    pub title: String,
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub roots: Vec<Node>,
    #[serde(default)]
    pub custom_words: Vec<String>,
    #[serde(default)]
    pub characters: Vec<Character>,
    #[serde(default)]
    pub world: Vec<WorldEntry>,
    /// Novel type; selects the suggested background categories.
    #[serde(default = "default_genre")]
    pub genre: String,
    next_id: NodeId,
}

impl Default for Book {
    fn default() -> Self {
        let mut book = Book {
            title: "Untitled Novel".into(),
            author: String::new(),
            roots: Vec::new(),
            custom_words: Vec::new(),
            characters: Vec::new(),
            world: Vec::new(),
            genre: default_genre(),
            next_id: 1,
        };
        book.add_child(None, "Chapter 1");
        book
    }
}

impl Node {
    pub fn kind(&self) -> NodeKind {
        self.kind.unwrap_or_else(|| infer_kind(&self.title))
    }

    pub fn word_count(&self) -> usize {
        count_words(&self.body)
    }

    pub fn total_word_count(&self) -> usize {
        self.word_count() + self.children.iter().map(Node::total_word_count).sum::<usize>()
    }

    fn contains(&self, id: NodeId) -> bool {
        self.id == id || self.children.iter().any(|c| c.contains(id))
    }
}

/// If `line` starts with `word` followed by a number/roman numeral, return the
/// text after the word (e.g. "Chapter 12: X" -> ": X" after skipping the number).
fn strip_numbered<'a>(line: &'a str, word: &str) -> Option<&'a str> {
    let rest = line.strip_prefix(word)?;
    if !rest.starts_with(' ') {
        return None;
    }
    let rest = rest.trim_start();
    let end = rest.find(|c: char| !c.is_alphanumeric()).unwrap_or(rest.len());
    if end == 0 {
        return None;
    }
    let numeral = &rest[..end];
    let ok = numeral.chars().all(|c| c.is_ascii_digit())
        || numeral.chars().all(|c| matches!(c, 'I' | 'V' | 'X' | 'L' | 'C'));
    ok.then_some(if word == "Part" { rest } else { &rest[end..] })
}

pub fn count_words(text: &str) -> usize {
    text.split_whitespace().count()
}

fn find_in(nodes: &[Node], id: NodeId) -> Option<&Node> {
    for n in nodes {
        if n.id == id {
            return Some(n);
        }
        if let Some(f) = find_in(&n.children, id) {
            return Some(f);
        }
    }
    None
}

fn path_in(nodes: &[Node], id: NodeId, path: &mut Vec<usize>) -> bool {
    for (i, n) in nodes.iter().enumerate() {
        path.push(i);
        if n.id == id || path_in(&n.children, id, path) {
            return true;
        }
        path.pop();
    }
    false
}

impl Book {
    pub fn find(&self, id: NodeId) -> Option<&Node> {
        find_in(&self.roots, id)
    }

    pub fn add_character(&mut self, name: &str) -> NodeId {
        let id = self.next_id;
        self.next_id += 1;
        self.characters.push(Character { id, name: name.into(), ..Character::default() });
        id
    }

    pub fn add_world_entry(&mut self, category: &str, name: &str) -> NodeId {
        let id = self.next_id;
        self.next_id += 1;
        self.world.push(WorldEntry {
            id,
            category: category.into(),
            name: name.into(),
            ..WorldEntry::default()
        });
        id
    }

    /// Add one empty entry per suggested category of the book's genre that has
    /// no entries yet. Returns how many were added.
    pub fn add_starter_entries(&mut self) -> usize {
        let mut added = 0;
        for (cat, _) in crate::templates::genre(&self.genre).categories {
            let exists = self.world.iter().any(|e| e.category.eq_ignore_ascii_case(cat));
            if !exists {
                self.add_world_entry(cat, cat);
                added += 1;
            }
        }
        added
    }

    pub fn find_world_mut(&mut self, id: NodeId) -> Option<&mut WorldEntry> {
        self.world.iter_mut().find(|e| e.id == id)
    }

    pub fn find_character_mut(&mut self, id: NodeId) -> Option<&mut Character> {
        self.characters.iter_mut().find(|c| c.id == id)
    }

    pub fn find_mut(&mut self, id: NodeId) -> Option<&mut Node> {
        let path = self.path_of(id)?;
        let (last, parents) = path.split_last()?;
        Some(&mut self.list_mut(parents)[*last])
    }

    fn path_of(&self, id: NodeId) -> Option<Vec<usize>> {
        let mut p = Vec::new();
        path_in(&self.roots, id, &mut p).then_some(p)
    }

    /// The sibling list that contains the node at `parent_path` + index; for an
    /// empty path this is the root list.
    fn list_mut(&mut self, parent_path: &[usize]) -> &mut Vec<Node> {
        let mut list = &mut self.roots;
        for &i in parent_path {
            list = &mut list[i].children;
        }
        list
    }

    fn new_node(&mut self, title: &str) -> Node {
        let id = self.next_id;
        self.next_id += 1;
        Node {
            id,
            title: title.into(),
            synopsis: String::new(),
            body: String::new(),
            children: Vec::new(),
            collapsed: false,
            kind: None,
        }
    }

    /// Returns true if the kind changed.
    pub fn set_kind(&mut self, id: NodeId, kind: NodeKind) -> bool {
        match self.find_mut(id) {
            Some(n) if n.kind() != kind || n.kind.is_none() => {
                n.kind = Some(kind);
                true
            }
            _ => false,
        }
    }

    /// Display numbers for chapters only: top-level chapters (including those
    /// inside a Part) count 1, 2, 3... through the whole book; chapters nested
    /// under a chapter are "N.M". Parts and front/back matter get no entry.
    pub fn chapter_numbers(&self) -> HashMap<NodeId, String> {
        fn walk(nodes: &[Node], parent: Option<&str>, counter: &mut usize, out: &mut HashMap<NodeId, String>) {
            let mut sub = 0;
            for n in nodes {
                if n.kind() == NodeKind::Chapter {
                    let label = match parent {
                        Some(p) => {
                            sub += 1;
                            format!("{p}.{sub}")
                        }
                        None => {
                            *counter += 1;
                            counter.to_string()
                        }
                    };
                    out.insert(n.id, label.clone());
                    walk(&n.children, Some(&label), counter, out);
                } else {
                    walk(&n.children, None, counter, out);
                }
            }
        }
        let mut out = HashMap::new();
        walk(&self.roots, None, &mut 0, &mut out);
        out
    }

    /// Append a new item as the last child of `parent` (or as a top-level item).
    pub fn add_child(&mut self, parent: Option<NodeId>, title: &str) -> Option<NodeId> {
        let node = self.new_node(title);
        let id = node.id;
        match parent {
            None => self.roots.push(node),
            Some(p) => {
                let n = self.find_mut(p)?;
                n.collapsed = false;
                n.children.push(node);
            }
        }
        Some(id)
    }

    /// Insert a new item directly after `id`, at the same level.
    pub fn add_sibling_after(&mut self, id: NodeId, title: &str) -> Option<NodeId> {
        let path = self.path_of(id)?;
        let node = self.new_node(title);
        let new_id = node.id;
        let (last, parents) = path.split_last()?;
        self.list_mut(parents).insert(last + 1, node);
        Some(new_id)
    }

    /// Remove an item together with all of its sub-items.
    pub fn remove(&mut self, id: NodeId) -> Option<Node> {
        let path = self.path_of(id)?;
        let (last, parents) = path.split_last()?;
        Some(self.list_mut(parents).remove(*last))
    }

    pub fn move_up(&mut self, id: NodeId) -> bool {
        let Some(path) = self.path_of(id) else { return false };
        let (&i, parents) = path.split_last().unwrap();
        if i == 0 {
            return false;
        }
        self.list_mut(parents).swap(i, i - 1);
        true
    }

    pub fn move_down(&mut self, id: NodeId) -> bool {
        let Some(path) = self.path_of(id) else { return false };
        let (&i, parents) = path.split_last().unwrap();
        let list = self.list_mut(parents);
        if i + 1 >= list.len() {
            return false;
        }
        list.swap(i, i + 1);
        true
    }

    /// Make the item the last child of its previous sibling.
    pub fn indent(&mut self, id: NodeId) -> bool {
        let Some(path) = self.path_of(id) else { return false };
        let (&i, parents) = path.split_last().unwrap();
        if i == 0 {
            return false;
        }
        let list = self.list_mut(parents);
        let node = list.remove(i);
        let prev = &mut list[i - 1];
        prev.collapsed = false;
        prev.children.push(node);
        true
    }

    /// Make the item a sibling placed right after its parent.
    pub fn outdent(&mut self, id: NodeId) -> bool {
        let Some(path) = self.path_of(id) else { return false };
        if path.len() < 2 {
            return false;
        }
        let (&i, parents) = path.split_last().unwrap();
        let node = self.list_mut(parents).remove(i);
        let (&pi, grand) = parents.split_last().unwrap();
        self.list_mut(grand).insert(pi + 1, node);
        true
    }

    /// Move an entire section (item + descendants) relative to `target`.
    /// Returns false if the move is invalid (onto itself or its own subtree).
    pub fn move_to(&mut self, id: NodeId, target: NodeId, pos: DropPos) -> bool {
        if id == target {
            return false;
        }
        match self.find(id) {
            Some(n) if !n.children.iter().any(|c| c.contains(target)) => {}
            _ => return false,
        }
        if self.find(target).is_none() {
            return false;
        }
        let Some(node) = self.remove(id) else { return false };
        let tpath = self.path_of(target).expect("target outside moved subtree");
        match pos {
            DropPos::Inside => {
                let (last, parents) = tpath.split_last().unwrap();
                let t = &mut self.list_mut(parents)[*last];
                t.collapsed = false;
                t.children.push(node);
            }
            DropPos::Before | DropPos::After => {
                let (last, parents) = tpath.split_last().unwrap();
                let at = if pos == DropPos::Before { *last } else { last + 1 };
                self.list_mut(parents).insert(at, node);
            }
        }
        true
    }

    /// Depth-first manuscript order: (id, depth, outline number like "2.1").
    pub fn flatten(&self) -> Vec<(NodeId, usize, String)> {
        fn walk(nodes: &[Node], depth: usize, prefix: &str, out: &mut Vec<(NodeId, usize, String)>) {
            for (i, n) in nodes.iter().enumerate() {
                let num = if prefix.is_empty() {
                    format!("{}", i + 1)
                } else {
                    format!("{prefix}.{}", i + 1)
                };
                out.push((n.id, depth, num.clone()));
                walk(&n.children, depth + 1, &num, out);
            }
        }
        let mut out = Vec::new();
        walk(&self.roots, 0, "", &mut out);
        out
    }

    pub fn total_word_count(&self) -> usize {
        self.roots.iter().map(Node::total_word_count).sum()
    }

    /// Append sections parsed from a plain-text outline. Recognised lines:
    /// `Prelude ...: synopsis`, `Part I: Title (Chapters 1-7)` (becomes a parent
    /// of the chapters that follow) and `Chapter N: Title: synopsis` (or
    /// `Chapter N: Title. synopsis`). Other lines extend the previous synopsis.
    /// Returns the number of sections added.
    pub fn import_outline(&mut self, text: &str) -> usize {
        let untouched = self.roots.len() == 1
            && self.roots[0].body.is_empty()
            && self.roots[0].synopsis.is_empty()
            && self.roots[0].children.is_empty()
            && self.roots[0].title == "Chapter 1";
        if untouched {
            self.roots.clear();
        }

        let mut added = 0;
        let mut part: Option<NodeId> = None;
        let mut last: Option<NodeId> = None;
        for line in text.lines().map(str::trim).filter(|l| !l.is_empty()) {
            let (parent, title, synopsis, kind) = if let Some(rest) = strip_numbered(line, "Part") {
                let head = rest.split_once(':').map_or(rest, |(_, t)| t).trim();
                let head = match head.rfind('(') {
                    Some(i) if head.ends_with(')') => head[..i].trim(),
                    _ => head,
                };
                let roman = line["Part".len()..].trim_start().split(':').next().unwrap_or("").trim();
                (None, format!("Part {roman}: {head}"), String::new(), NodeKind::Part)
            } else if let Some(rest) = strip_numbered(line, "Chapter") {
                let rest = rest.trim_start_matches([':', '.', ' ']);
                let cut = [rest.find(": "), rest.find(". ")].into_iter().flatten().min();
                let (t, s) = match cut {
                    Some(i) => (rest[..i].trim(), rest[i + 2..].trim()),
                    None => (rest.trim_end_matches('.'), ""),
                };
                (part, t.to_string(), s.to_string(), NodeKind::Chapter)
            } else if line.starts_with("Prelude") {
                let (t, s) = line.split_once(':').unwrap_or((line, ""));
                (None, t.trim().to_string(), s.trim().to_string(), NodeKind::Matter)
            } else {
                if let Some(n) = last.and_then(|id| self.find_mut(id)) {
                    if !n.synopsis.is_empty() {
                        n.synopsis.push(' ');
                    }
                    n.synopsis.push_str(line);
                }
                continue;
            };
            let Some(id) = self.add_child(parent, &title) else { continue };
            if let Some(n) = self.find_mut(id) {
                n.synopsis = synopsis;
                n.kind = Some(kind);
            }
            if kind == NodeKind::Part {
                part = Some(id);
            } else if parent.is_none() {
                part = None;
            }
            last = Some(id);
            added += 1;
        }
        added
    }

    pub fn to_markdown(&self) -> String {
        fn walk(nodes: &[Node], depth: usize, out: &mut String) {
            for n in nodes {
                out.push_str(&"#".repeat((depth + 1).min(6)));
                out.push(' ');
                out.push_str(&n.title);
                out.push_str("\n\n");
                if !n.body.trim().is_empty() {
                    out.push_str(n.body.trim_end());
                    out.push_str("\n\n");
                }
                walk(&n.children, depth + 1, out);
            }
        }
        let mut out = format!("# {}\n\n", self.title);
        if !self.author.is_empty() {
            out.push_str(&format!("*by {}*\n\n", self.author));
        }
        walk(&self.roots, 1, &mut out);
        out
    }

    pub fn to_plain_text(&self) -> String {
        fn strip(s: &str) -> String {
            s.lines()
                .map(|l| l.trim_start_matches('#').trim_start())
                .collect::<Vec<_>>()
                .join("\n")
                .replace("**", "")
                .replace("__", "")
                .replace('*', "")
        }
        fn walk(nodes: &[Node], out: &mut String) {
            for n in nodes {
                out.push_str(&n.title);
                out.push_str("\n\n");
                if !n.body.trim().is_empty() {
                    out.push_str(&strip(n.body.trim_end()));
                    out.push_str("\n\n");
                }
                walk(&n.children, out);
            }
        }
        let mut out = format!("{}\n", self.title);
        if !self.author.is_empty() {
            out.push_str(&format!("by {}\n", self.author));
        }
        out.push('\n');
        walk(&self.roots, &mut out);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn titles(b: &Book) -> Vec<String> {
        b.flatten()
            .iter()
            .map(|(id, d, _)| format!("{}{}", "-".repeat(*d), b.find(*id).unwrap().title))
            .collect()
    }

    fn sample() -> (Book, [NodeId; 5]) {
        let mut b = Book { roots: vec![], ..Book::default() };
        let a = b.add_child(None, "A").unwrap();
        let a1 = b.add_child(Some(a), "A1").unwrap();
        let a2 = b.add_child(Some(a), "A2").unwrap();
        let bb = b.add_child(None, "B").unwrap();
        let c = b.add_child(None, "C").unwrap();
        let _ = (a1, a2);
        (b, [a, a1, a2, bb, c])
    }

    #[test]
    fn move_down_moves_whole_section() {
        let (mut b, [a, ..]) = sample();
        assert!(b.move_down(a));
        assert_eq!(titles(&b), ["B", "A", "-A1", "-A2", "C"]);
    }

    #[test]
    fn indent_outdent() {
        let (mut b, [_, _, _, bb, _]) = sample();
        assert!(b.indent(bb));
        assert_eq!(titles(&b), ["A", "-A1", "-A2", "-B", "C"]);
        assert!(b.outdent(bb));
        assert_eq!(titles(&b), ["A", "-A1", "-A2", "B", "C"]);
    }

    #[test]
    fn move_to_positions() {
        let (mut b, [a, a1, _, _, c]) = sample();
        assert!(b.move_to(c, a, DropPos::Before));
        assert_eq!(titles(&b), ["C", "A", "-A1", "-A2", "B"]);
        assert!(b.move_to(a, c, DropPos::Inside));
        assert_eq!(titles(&b), ["C", "-A", "--A1", "--A2", "B"]);
        assert!(b.move_to(a1, c, DropPos::After));
        assert_eq!(titles(&b), ["C", "-A", "--A2", "A1", "B"]);
    }

    #[test]
    fn cannot_move_into_own_subtree() {
        let (mut b, [a, a1, ..]) = sample();
        assert!(!b.move_to(a, a1, DropPos::Inside));
        assert!(!b.move_to(a, a, DropPos::After));
        assert_eq!(titles(&b), ["A", "-A1", "-A2", "B", "C"]);
    }

    #[test]
    fn kinds_and_chapter_numbers() {
        let mut b = Book::default();
        b.roots.clear();
        b.import_outline(
            "Prelude (The Docks): Dan Dan.\nPart I: Seeds (Chapters 1-2)\nChapter 1: A. x\nChapter 2: B. y\nPart II: Shadows\nChapter 3: C. z",
        );
        let kinds: Vec<_> = b.flatten().iter().map(|(id, _, _)| b.find(*id).unwrap().kind()).collect();
        use NodeKind::*;
        assert_eq!(kinds, [Matter, Part, Chapter, Chapter, Part, Chapter]);
        let nums = b.chapter_numbers();
        let ids: Vec<_> = b.flatten().iter().map(|(id, _, _)| *id).collect();
        assert!(!nums.contains_key(&ids[0]) && !nums.contains_key(&ids[1]));
        assert_eq!(nums[&ids[2]], "1");
        assert_eq!(nums[&ids[3]], "2");
        assert_eq!(nums[&ids[5]], "3");
    }

    #[test]
    fn legacy_titles_infer_kind() {
        let mut b = Book::default();
        let id = b.add_child(None, "Part II: Shadows").unwrap();
        let id2 = b.add_child(None, "Epilogue").unwrap();
        let id3 = b.add_child(None, "Part of the Plan").unwrap();
        assert_eq!(b.find(id).unwrap().kind(), NodeKind::Part);
        assert_eq!(b.find(id2).unwrap().kind(), NodeKind::Matter);
        assert_eq!(b.find(id3).unwrap().kind(), NodeKind::Chapter);
        assert!(b.set_kind(id3, NodeKind::Part));
        assert!(!b.set_kind(id3, NodeKind::Part));
    }

    #[test]
    fn manuscript_follows_outline() {
        let (mut b, [a, _, _, bb, _]) = sample();
        b.find_mut(a).unwrap().body = "alpha".into();
        b.find_mut(bb).unwrap().body = "beta".into();
        b.move_to(bb, a, DropPos::Before);
        let md = b.to_markdown();
        assert!(md.find("beta").unwrap() < md.find("alpha").unwrap());
    }

    #[test]
    fn imports_outline_text() {
        let mut b = Book::default();
        let n = b.import_outline(
            "Prelude (The Docks, at Night): Dan Dan inspects barrels.\n\
             Part I: Seeds of Power (Chapters 1–7)\n\
             Chapter 1: Market Street: Paris visits the market.\n\
             Chapter 2: Whispers of War. Marvan and Langmar talk.\n\
             Part II: Shadows & Syndicate (Chapters 11–17)\n\
             Chapter 12: Into the Underbelly. Paris gravitates.",
        );
        assert_eq!(n, 6);
        assert_eq!(
            titles(&b),
            [
                "Prelude (The Docks, at Night)",
                "Part I: Seeds of Power",
                "-Market Street",
                "-Whispers of War",
                "Part II: Shadows & Syndicate",
                "-Into the Underbelly"
            ]
        );
        let id = b.roots[1].children[1].id;
        assert_eq!(b.find(id).unwrap().synopsis, "Marvan and Langmar talk.");
    }

    #[test]
    fn remove_and_counts() {
        let (mut b, [a, _, _, bb, _]) = sample();
        b.find_mut(bb).unwrap().body = "one two three".into();
        assert_eq!(b.total_word_count(), 3);
        b.remove(a);
        assert_eq!(titles(&b), ["B", "C"]);
    }
}

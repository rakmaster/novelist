use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::ops::Range;

const AFF: &str = include_str!("../assets/en_US.aff");
const DIC: &str = include_str!("../assets/en_US.dic");

pub struct SpellChecker {
    dict: spellbook::Dictionary,
    cache: RefCell<HashMap<String, bool>>,
    /// Words that are always correct (character names), matched exactly.
    extra: HashSet<String>,
}

fn edit_distance(a: &str, b: &str) -> usize {
    let b: Vec<char> = b.chars().collect();
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.chars().enumerate() {
        let mut cur = vec![i + 1];
        for (j, &cb) in b.iter().enumerate() {
            let cost = usize::from(ca != cb);
            cur.push((prev[j] + cost).min(prev[j + 1] + 1).min(cur[j] + 1));
        }
        prev = cur;
    }
    prev[b.len()]
}

fn is_word_char(c: char) -> bool {
    c.is_alphabetic() || c == '\'' || c == '\u{2019}'
}

/// Byte ranges of words (letters and inner apostrophes) in `text`.
pub fn words(text: &str) -> Vec<Range<usize>> {
    let mut out = Vec::new();
    let mut start: Option<usize> = None;
    let mut push = |s: usize, e: usize| {
        let w = &text[s..e];
        let lead = w.len() - w.trim_start_matches(['\'', '\u{2019}']).len();
        let trail = w.len() - w.trim_end_matches(['\'', '\u{2019}']).len();
        if s + lead < e - trail {
            out.push(s + lead..e - trail);
        }
    };
    for (i, c) in text.char_indices() {
        if is_word_char(c) {
            start.get_or_insert(i);
        } else if let Some(s) = start.take() {
            push(s, i);
        }
    }
    if let Some(s) = start {
        push(s, text.len());
    }
    out
}

impl SpellChecker {
    pub fn new(custom: &[String]) -> Self {
        let mut dict = spellbook::Dictionary::new(AFF, DIC).expect("bundled dictionary is valid");
        for w in custom {
            let _ = dict.add(w);
        }
        Self { dict, cache: RefCell::new(HashMap::new()), extra: HashSet::new() }
    }

    /// Replace the set of always-correct words (the words of every character
    /// name and alias).
    pub fn set_extra(&mut self, words: impl IntoIterator<Item = String>) {
        self.extra = words.into_iter().map(|w| w.replace('\u{2019}', "'")).collect();
        self.cache.borrow_mut().clear();
    }

    fn check_exact(&self, norm: &str) -> bool {
        self.extra.contains(norm) || self.dict.check(norm)
    }

    /// Check `'s` possessives and `s'` plural possessives against their stems.
    fn check_possessive(&self, norm: &str) -> bool {
        if let Some(base) = norm
            .strip_suffix("'s")
            .or_else(|| norm.strip_suffix("'S"))
        {
            return self.check_exact(base);
        }
        if let Some(plural) = norm.strip_suffix('\'')
            && plural.ends_with(['s', 'S'])
        {
            return self.check_exact(plural) || self.check_exact(&plural[..plural.len() - 1]);
        }
        false
    }

    pub fn is_correct(&self, word: &str) -> bool {
        if let Some(&v) = self.cache.borrow().get(word) {
            return v;
        }
        let norm = word.replace('\u{2019}', "'");
        let ok = self.check_exact(&norm) || self.check_possessive(&norm);
        self.cache.borrow_mut().insert(word.to_string(), ok);
        ok
    }

    /// `words` trims trailing apostrophes, so `names'` arrives as `names`;
    /// restore the apostrophe so `s'` possessives check against their stem.
    fn is_correct_at(&self, text: &str, r: Range<usize>) -> bool {
        let w = &text[r.clone()];
        if self.is_correct(w) {
            return true;
        }
        let s_apostrophe = w.ends_with(['s', 'S'])
            && text[r.end..]
                .chars()
                .next()
                .is_some_and(|c| c == '\'' || c == '\u{2019}');
        s_apostrophe && self.is_correct(&format!("{w}'"))
    }

    pub fn suggest(&self, word: &str) -> Vec<String> {
        let mut out = Vec::new();
        let norm = word.replace('\u{2019}', "'");
        self.dict.suggest(&norm, &mut out);
        out.truncate(6);
        let lower = norm.to_lowercase();
        let mut names: Vec<&String> = self
            .extra
            .iter()
            .filter(|n| n.chars().count() > 2 && edit_distance(&n.to_lowercase(), &lower) <= 2)
            .collect();
        names.sort();
        for n in names.into_iter().rev() {
            out.retain(|s| s != n);
            out.insert(0, n.clone());
        }
        out
    }

    pub fn add_word(&mut self, word: &str) {
        let _ = self.dict.add(word);
        self.cache.borrow_mut().clear();
    }

    /// Byte ranges of misspelled words in `text`.
    pub fn misspellings(&self, text: &str) -> Vec<Range<usize>> {
        words(text)
            .into_iter()
            .filter(|r| !self.is_correct_at(text, r.clone()))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_misspellings() {
        let sc = SpellChecker::new(&[]);
        let t = "The quick brwn fox don't";
        let bad = sc.misspellings(t);
        assert_eq!(bad.len(), 1);
        assert_eq!(&t[bad[0].clone()], "brwn");
        assert!(sc.suggest("brwn").iter().any(|s| s == "brown"));
    }

    #[test]
    fn character_names_are_correct_and_suggested() {
        let mut sc = SpellChecker::new(&[]);
        assert!(!sc.is_correct("Marvan"));
        sc.set_extra(["Marvan".to_string()]);
        assert!(sc.is_correct("Marvan"));
        assert_eq!(sc.suggest("Marvon")[0], "Marvan");
        sc.set_extra(Vec::new());
        assert!(!sc.is_correct("Marvan"));
    }

    #[test]
    fn possessive_forms_of_known_words() {
        let mut sc = SpellChecker::new(&[]);
        sc.set_extra(["Ragman".to_string()]);
        let text = "Ragman's cloak, Ragman’s blade, Ragmans' daggers, Ragmans’ knives.";
        assert!(sc.misspellings(text).is_empty());
        assert_eq!(sc.misspellings("Ragmans").len(), 1);
    }

    #[test]
    fn custom_words() {
        let mut sc = SpellChecker::new(&[]);
        assert!(!sc.is_correct("Zorblax"));
        sc.add_word("Zorblax");
        assert!(sc.is_correct("Zorblax"));
    }
}

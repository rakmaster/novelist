use crate::editor::{self, Env, FormatCmd, SpellAction};
use crate::format;
#[cfg(target_os = "macos")]
use crate::macos_menu::{MenuAction, NativeMenu};
use crate::model::{Book, DropPos, NodeId, NodeKind};
use crate::search::{self, Match, Query};
use crate::spell::SpellChecker;
use eframe::egui::{self, Align, Align2, Color32, FontFamily, FontId, Key, Sense, Stroke, vec2};
use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::path::PathBuf;

const MAX_UNDO: usize = 50;

fn load_splash_texture(ctx: &egui::Context) -> Option<egui::TextureHandle> {
    let bytes = include_bytes!("../assets/splash.png");
    let img = image::load_from_memory(bytes).ok()?.to_rgba8();
    let (w, h) = (img.width(), img.height());
    let max_w = 1920u32;
    let max_h = 1080u32;
    let (tw, th) = if w > max_w || h > max_h {
        let scale = (max_w as f32 / w as f32).min(max_h as f32 / h as f32);
        ((w as f32 * scale) as u32, (h as f32 * scale) as u32)
    } else {
        (w, h)
    };
    let resized = if tw != w || th != h {
        image::imageops::resize(&img, tw, th, image::imageops::FilterType::Triangle)
    } else {
        img
    };
    let color_image = egui::ColorImage::from_rgba_unmultiplied(
        [resized.width() as usize, resized.height() as usize],
        resized.as_raw(),
    );
    Some(ctx.load_texture("splash", color_image, egui::TextureOptions::LINEAR))
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum View {
    Chapter,
    Manuscript,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Side {
    Editor,
    Characters,
    World,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ImportMode {
    Outline,
    Characters,
    World,
    Combined,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Pending {
    New,
    Open,
    Quit,
}

enum OutlineAction {
    Select(NodeId),
    Toggle(NodeId),
    AddRoot,
    AddChild(NodeId),
    AddAfter(NodeId),
    Up(NodeId),
    Down(NodeId),
    Indent(NodeId),
    Outdent(NodeId),
    SetKind(NodeId, NodeKind),
    Delete(NodeId),
    Drop(NodeId, NodeId, DropPos),
}

struct Row {
    id: NodeId,
    depth: usize,
    title: String,
    synopsis: String,
    has_children: bool,
    collapsed: bool,
    kind: NodeKind,
    /// Chapter number, empty for parts and front/back matter.
    label: String,
}

#[derive(Default)]
struct FindState {
    open: bool,
    replace: bool,
    query: Query,
    with: String,
    current: usize,
    focus: bool,
    status: String,
}

pub struct App {
    book: Book,
    path: Option<PathBuf>,
    dirty: bool,
    selected: Option<NodeId>,
    view: View,
    font_size: f32,
    manuscript_format: bool,
    show_synopsis_in_manuscript: bool,
    spell: SpellChecker,
    spell_on: bool,
    spell_open: bool,
    spell_suggestions: Option<(String, Vec<String>)>,
    import_open: bool,
    import_text: String,
    import_mode: ImportMode,
    settings_open: bool,
    side: Side,
    selected_char: Option<NodeId>,
    selected_world: Option<NodeId>,
    spell_terms: Vec<String>,
    mention_cache: Option<(u64, HashMap<NodeId, Vec<Match>>)>,
    portraits: HashMap<NodeId, (u64, egui::TextureHandle)>,
    find: FindState,
    undo: Vec<Book>,
    scroll_request: Option<NodeId>,
    goto: Option<(NodeId, usize)>,
    last_focus: Option<NodeId>,
    /// Text edit that receives toolbar/native-menu formatting commands.
    fmt_focus: Option<egui::Id>,
    pending: Option<Pending>,
    allow_close: bool,
    message: String,
    last_title: String,
    launcher: bool,
    recents: Vec<PathBuf>,
    launcher_error: String,
    splash_texture: Option<egui::TextureHandle>,
    #[cfg(target_os = "macos")]
    native_menu: Option<NativeMenu>,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        editor::install_fonts(&cc.egui_ctx);
        crate::theme::apply(&cc.egui_ctx);
        let book = Book::default();
        let selected = book.roots.first().map(|n| n.id);
        Self {
            spell: SpellChecker::new(&book.custom_words),
            book,
            path: None,
            dirty: false,
            selected,
            view: View::Chapter,
            font_size: 20.0,
            manuscript_format: true,
            show_synopsis_in_manuscript: false,
            spell_on: true,
            spell_open: false,
            spell_suggestions: None,
            import_open: false,
            import_text: String::new(),
            import_mode: ImportMode::Combined,
            settings_open: false,
            side: Side::Editor,
            selected_char: None,
            selected_world: None,
            spell_terms: Vec::new(),
            mention_cache: None,
            portraits: HashMap::new(),
            find: FindState::default(),
            undo: Vec::new(),
            scroll_request: None,
            goto: None,
            last_focus: None,
            fmt_focus: None,
            pending: None,
            allow_close: false,
            message: String::new(),
            last_title: String::new(),
            launcher: true,
            recents: crate::recents::load(),
            launcher_error: String::new(),
            splash_texture: load_splash_texture(&cc.egui_ctx),
            #[cfg(target_os = "macos")]
            native_menu: None,
        }
    }

    // ---------- document / file handling ----------

    fn load_book(&mut self, book: Book, path: Option<PathBuf>) {
        self.spell = SpellChecker::new(&book.custom_words);
        self.spell_terms.clear();
        self.mention_cache = None;
        self.selected_char = book.characters.first().map(|c| c.id);
        self.selected_world = book.world.first().map(|e| e.id);
        self.selected = book.roots.first().map(|n| n.id);
        self.book = book;
        self.path = path;
        self.dirty = false;
        self.undo.clear();
        self.last_focus = None;
        self.fmt_focus = None;
        self.launcher = false;
        self.launcher_error.clear();
    }

    fn save(&mut self, force_dialog: bool) -> bool {
        let path = match (&self.path, force_dialog) {
            (Some(p), false) => p.clone(),
            _ => match rfd::FileDialog::new()
                .add_filter("Novel", &["novel"])
                .set_file_name(format!("{}.novel", self.book.title))
                .save_file()
            {
                Some(p) => p,
                None => return false,
            },
        };
        match serde_json::to_string_pretty(&self.book)
            .map_err(|e| e.to_string())
            .and_then(|s| std::fs::write(&path, s).map_err(|e| e.to_string()))
        {
            Ok(()) => {
                self.path = Some(path.clone());
                self.dirty = false;
                self.message = "Saved".into();
                crate::recents::push(&mut self.recents, path);
                true
            }
            Err(e) => {
                self.message = format!("Save failed: {e}");
                false
            }
        }
    }

    fn open_dialog(&mut self) {
        let Some(path) = rfd::FileDialog::new().add_filter("Novel", &["novel"]).pick_file() else {
            return;
        };
        match std::fs::read_to_string(&path)
            .map_err(|e| e.to_string())
            .and_then(|s| serde_json::from_str::<Book>(&s).map_err(|e| e.to_string()))
        {
            Ok(book) => {
                let p = path.clone();
                self.load_book(book, Some(path));
                self.message = "Opened".into();
                crate::recents::push(&mut self.recents, p);
            }
            Err(e) => self.message = format!("Open failed: {e}"),
        }
    }

    fn open_recent(&mut self, path: PathBuf) {
        match std::fs::read_to_string(&path)
            .map_err(|e| e.to_string())
            .and_then(|s| serde_json::from_str::<Book>(&s).map_err(|e| e.to_string()))
        {
            Ok(book) => {
                self.load_book(book, Some(path.clone()));
                self.message = "Opened".into();
                crate::recents::push(&mut self.recents, path);
            }
            Err(e) => {
                self.launcher_error = format!("Could not open {}: {}", path.display(), e);
                crate::recents::remove(&mut self.recents, &path);
            }
        }
    }

    fn export(&mut self, markdown: bool) {
        let (ext, text) = if markdown {
            ("md", self.book.to_markdown())
        } else {
            ("txt", self.book.to_plain_text())
        };
        if let Some(p) = rfd::FileDialog::new()
            .set_file_name(format!("{}.{ext}", self.book.title))
            .save_file()
        {
            self.message = match std::fs::write(&p, text) {
                Ok(()) => "Exported".into(),
                Err(e) => format!("Export failed: {e}"),
            };
        }
    }

    fn request(&mut self, ctx: &egui::Context, action: Pending) {
        if self.dirty {
            self.pending = Some(action);
        } else {
            self.perform(ctx, action);
        }
    }

    fn perform(&mut self, ctx: &egui::Context, action: Pending) {
        match action {
            Pending::New => self.load_book(Book::default(), None),
            Pending::Open => self.open_dialog(),
            Pending::Quit => {
                self.allow_close = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
    }

    // ---------- outline operations ----------

    fn snapshot(&mut self) {
        self.undo.push(self.book.clone());
        if self.undo.len() > MAX_UNDO {
            self.undo.remove(0);
        }
    }

    fn undo_outline(&mut self) {
        if let Some(b) = self.undo.pop() {
            self.book = b;
            if self.selected.is_some_and(|s| self.book.find(s).is_none()) {
                self.selected = self.book.roots.first().map(|n| n.id);
            }
            self.dirty = true;
        }
    }

    fn apply(&mut self, a: OutlineAction) {
        match a {
            OutlineAction::Select(id) => {
                self.selected = Some(id);
                self.scroll_request = Some(id);
            }
            OutlineAction::Toggle(id) => {
                if let Some(n) = self.book.find_mut(id) {
                    n.collapsed = !n.collapsed;
                }
            }
            OutlineAction::AddRoot => {
                self.snapshot();
                let n = self.book.flatten().len() + 1;
                let new = self.book.add_child(None, &format!("Section {n}"));
                self.select_new(new);
            }
            OutlineAction::AddChild(id) => {
                self.snapshot();
                let new = self.book.add_child(Some(id), "New section");
                self.select_new(new);
            }
            OutlineAction::AddAfter(id) => {
                self.snapshot();
                let new = self.book.add_sibling_after(id, "New section");
                self.select_new(new);
            }
            OutlineAction::Up(id) => self.structural(|b| b.move_up(id)),
            OutlineAction::Down(id) => self.structural(|b| b.move_down(id)),
            OutlineAction::Indent(id) => self.structural(|b| b.indent(id)),
            OutlineAction::Outdent(id) => self.structural(|b| b.outdent(id)),
            OutlineAction::SetKind(id, kind) => self.structural(|b| b.set_kind(id, kind)),
            OutlineAction::Delete(id) => {
                let order = self.book.flatten();
                let idx = order.iter().position(|(i, _, _)| *i == id);
                self.snapshot();
                self.book.remove(id);
                self.dirty = true;
                if self.selected.is_some_and(|s| self.book.find(s).is_none()) {
                    let order = self.book.flatten();
                    self.selected = idx
                        .and_then(|i| order.get(i.min(order.len().saturating_sub(1))))
                        .map(|(i, _, _)| *i);
                }
            }
            OutlineAction::Drop(id, target, pos) => self.structural(|b| b.move_to(id, target, pos)),
        }
    }

    fn select_new(&mut self, id: Option<NodeId>) {
        if let Some(id) = id {
            self.selected = Some(id);
            self.scroll_request = Some(id);
            self.dirty = true;
        }
    }

    fn structural(&mut self, f: impl FnOnce(&mut Book) -> bool) {
        self.snapshot();
        if f(&mut self.book) {
            self.dirty = true;
            self.scroll_request = self.selected;
        } else {
            self.undo.pop();
        }
    }

    // ---------- find / replace ----------

    fn goto_match(&mut self, m: &Match) {
        self.side = Side::Editor;
        self.selected = Some(m.node);
        self.scroll_request = Some(m.node);
        self.goto = Some((m.node, m.range.start));
    }

    fn step_match(&mut self, matches: &[Match], forward: bool) {
        if matches.is_empty() {
            return;
        }
        let n = matches.len();
        self.find.current = if forward {
            (self.find.current + 1) % n
        } else {
            (self.find.current + n - 1) % n
        };
        let m = matches[self.find.current].clone();
        self.goto_match(&m);
    }

    fn find_bar(&mut self, ui: &mut egui::Ui, matches: &[Match]) {
        let mut next = false;
        let mut prev = false;
        let mut replace_one = false;
        let mut replace_all = false;
        let mut close = false;
        ui.horizontal(|ui| {
            ui.label("Find:");
            let r = ui.add(egui::TextEdit::singleline(&mut self.find.query.needle).desired_width(220.0));
            if self.find.focus {
                r.request_focus();
                self.find.focus = false;
            }
            if r.changed() {
                self.find.current = 0;
                self.find.status.clear();
            }
            if r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                if ui.input(|i| i.modifiers.shift) {
                    prev = true;
                } else {
                    next = true;
                }
                r.request_focus();
            }
            prev |= ui.button("◀").on_hover_text("Previous (Shift+Enter)").clicked();
            next |= ui.button("▶").on_hover_text("Next (Enter)").clicked();
            ui.checkbox(&mut self.find.query.case_sensitive, "Match case");
            ui.checkbox(&mut self.find.query.whole_word, "Whole word");
            ui.toggle_value(&mut self.find.replace, "Replace");
            let label = if self.find.query.needle.is_empty() {
                String::new()
            } else if matches.is_empty() {
                "No matches".into()
            } else {
                format!("{} of {}", self.find.current.min(matches.len() - 1) + 1, matches.len())
            };
            ui.weak(label);
            if ui.button("✕").clicked() {
                close = true;
            }
        });
        if self.find.replace {
            ui.horizontal(|ui| {
                ui.label("With:");
                ui.add(egui::TextEdit::singleline(&mut self.find.with).desired_width(220.0));
                replace_one = ui.button("Replace").clicked();
                replace_all = ui.button("Replace all").clicked();
                ui.weak(&self.find.status);
            });
        }
        if ui.input(|i| i.key_pressed(Key::Escape)) {
            close = true;
        }

        if close {
            self.find.open = false;
        }
        if next {
            self.step_match(matches, true);
        }
        if prev {
            self.step_match(matches, false);
        }
        if replace_one && !matches.is_empty() {
            let i = self.find.current.min(matches.len() - 1);
            self.snapshot();
            search::replace_one(&mut self.book, &matches[i], &self.find.with.clone());
            self.dirty = true;
            self.find.status = "Replaced 1".into();
            // Matches shift after replacement; stay on the same index.
            let new = search::find_all(&self.book, &self.find.query);
            if !new.is_empty() {
                self.find.current = i.min(new.len() - 1);
                let m = new[self.find.current].clone();
                self.goto_match(&m);
            }
        }
        if replace_all && !matches.is_empty() {
            self.snapshot();
            let n = search::replace_all(&mut self.book, &self.find.query, &self.find.with.clone());
            self.dirty = true;
            self.find.status = format!("Replaced {n}");
            self.find.current = 0;
        }
    }

    // ---------- characters ----------

    /// (id, searchable names) for every character and world entry.
    fn tracked_terms(&self) -> Vec<(NodeId, Vec<String>)> {
        self.book
            .characters
            .iter()
            .map(|c| (c.id, c.terms()))
            .chain(self.book.world.iter().map(|e| (e.id, e.terms())))
            .collect()
    }

    fn sync_spell_names(&mut self) {
        let mut terms = Vec::new();
        for (_, names) in self.tracked_terms() {
            for t in names {
                for r in crate::spell::words(&t) {
                    terms.push(t[r].to_string());
                }
            }
        }
        terms.sort();
        terms.dedup();
        if terms != self.spell_terms {
            self.spell.set_extra(terms.clone());
            self.spell_terms = terms;
        }
    }

    fn refresh_mentions(&mut self) {
        let mut h = DefaultHasher::new();
        for (id, _, _) in self.book.flatten() {
            if let Some(n) = self.book.find(id) {
                n.body.hash(&mut h);
            }
        }
        let tracked = self.tracked_terms();
        tracked.hash(&mut h);
        let sig = h.finish();
        if self.mention_cache.as_ref().is_some_and(|(s, _)| *s == sig) {
            return;
        }
        let map = tracked
            .iter()
            .map(|(id, terms)| (*id, search::mentions(&self.book, terms)))
            .collect();
        self.mention_cache = Some((sig, map));
    }

    fn clean_manuscript_spacing(&mut self) {
        self.snapshot();
        let mut changed = 0;
        for (id, _, _) in self.book.flatten() {
            if let Some(n) = self.book.find_mut(id) {
                let cleaned = format::clean_manuscript(&n.body);
                if cleaned != n.body {
                    n.body = cleaned;
                    changed += 1;
                }
            }
        }
        if changed > 0 {
            self.dirty = true;
            self.message = format!("Cleaned spacing in {changed} sections");
        } else {
            self.undo.pop();
            self.message = "Spacing already clean".into();
        }
    }

    fn set_portrait(&mut self, id: NodeId, bytes: &[u8]) {
        match crate::portrait::encode(bytes) {
            Ok(b64) => {
                self.snapshot();
                if let Some(c) = self.book.find_character_mut(id) {
                    c.image = Some(b64);
                    self.dirty = true;
                    self.message = "Portrait updated".into();
                }
            }
            Err(e) => self.message = e,
        }
    }

    fn portrait_texture(&mut self, ctx: &egui::Context, id: NodeId) -> Option<egui::TextureHandle> {
        let Some(b64) = self.book.characters.iter().find(|c| c.id == id).and_then(|c| c.image.as_ref()) else {
            self.portraits.remove(&id);
            return None;
        };
        let mut h = DefaultHasher::new();
        b64.hash(&mut h);
        let sig = h.finish();
        if let Some((s, tex)) = self.portraits.get(&id) {
            if *s == sig {
                return Some(tex.clone());
            }
        }
        let img = crate::portrait::decode(b64)?;
        let tex = ctx.load_texture(format!("portrait-{id}"), img, egui::TextureOptions::LINEAR);
        self.portraits.insert(id, (sig, tex.clone()));
        Some(tex)
    }

    fn world_list_ui(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label("Novel type:");
            let before = self.book.genre.clone();
            egui::ComboBox::from_id_salt("genre")
                .selected_text(self.book.genre.as_str())
                .show_ui(ui, |ui| {
                    for g in crate::templates::GENRES {
                        ui.selectable_value(&mut self.book.genre, g.name.to_string(), g.name);
                    }
                });
            if self.book.genre != before {
                self.dirty = true;
            }
        });
        if ui
            .button("Add starter categories")
            .on_hover_text("Adds an empty entry for each suggested category of this novel type")
            .clicked()
        {
            self.snapshot();
            let n = self.book.add_starter_entries();
            self.message = format!("Added {n} starter entries");
            self.dirty |= n > 0;
            if self.selected_world.is_none() {
                self.selected_world = self.book.world.first().map(|e| e.id);
            }
        }
        if ui.button("+ Entry").on_hover_text("Add an entry in the selected category").clicked() {
            let cat = self
                .selected_world
                .and_then(|id| self.book.world.iter().find(|e| e.id == id))
                .map(|e| e.category.clone())
                .unwrap_or_else(|| "General".into());
            self.snapshot();
            self.selected_world = Some(self.book.add_world_entry(&cat, "New entry"));
            self.dirty = true;
        }
        ui.separator();
        let counts: HashMap<NodeId, usize> = self
            .mention_cache
            .as_ref()
            .map(|(_, m)| m.iter().map(|(k, v)| (*k, v.len())).collect())
            .unwrap_or_default();
        let mut cats: Vec<String> = Vec::new();
        for e in &self.book.world {
            if !cats.contains(&e.category) {
                cats.push(e.category.clone());
            }
        }
        let mut clicked = None;
        egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
            for cat in &cats {
                let title = if cat.trim().is_empty() { "(no category)" } else { cat.as_str() };
                egui::CollapsingHeader::new(title).id_salt(("world-cat", cat)).default_open(true).show(ui, |ui| {
                    for e in self.book.world.iter().filter(|e| &e.category == cat) {
                        let n = counts.get(&e.id).copied().unwrap_or(0);
                        let name = if e.name.trim().is_empty() { "(unnamed)" } else { e.name.as_str() };
                        if ui
                            .selectable_label(self.selected_world == Some(e.id), format!("{name}   \u{b7}   {n}"))
                            .clicked()
                        {
                            clicked = Some(e.id);
                        }
                    }
                });
            }
        });
        if clicked.is_some() {
            self.selected_world = clicked;
        }
    }

    fn world_ui(&mut self, ui: &mut egui::Ui, mut fmt: Option<(egui::Id, FormatCmd)>) {
        let valid = self.selected_world.filter(|id| self.book.world.iter().any(|e| e.id == *id));
        let Some(id) = valid.or_else(|| self.book.world.first().map(|e| e.id)) else {
            ui.centered_and_justified(|ui| {
                ui.weak(
                    "No background notes yet. Choose a novel type and add starter categories, \
                     add an entry, or use File \u{2192} Import\u{2026}",
                )
            });
            return;
        };
        self.selected_world = Some(id);

        let matches = self
            .mention_cache
            .as_ref()
            .and_then(|(_, m)| m.get(&id))
            .cloned()
            .unwrap_or_default();
        let numbers: HashMap<NodeId, String> =
            self.book.flatten().into_iter().map(|(i, _, n)| (i, n)).collect();
        let mut groups: Vec<(usize, Match)> = Vec::new();
        for m in &matches {
            match groups.last_mut() {
                Some((c, first)) if first.node == m.node => *c += 1,
                _ => groups.push((1, m.clone())),
            }
        }
        let titles: Vec<String> = groups
            .iter()
            .map(|(_, m)| self.book.find(m.node).map(|n| n.title.clone()).unwrap_or_default())
            .collect();

        let mut suggestions: Vec<String> = crate::templates::genre(&self.book.genre)
            .categories
            .iter()
            .map(|(c, _)| c.to_string())
            .collect();
        for e in &self.book.world {
            if !e.category.trim().is_empty() && !suggestions.contains(&e.category) {
                suggestions.push(e.category.clone());
            }
        }

        let mut jump: Option<Match> = None;
        let mut delete = false;
        let mut find_it: Option<String> = None;
        let App { book, dirty, fmt_focus, font_size, .. } = self;
        let Some(e) = book.find_world_mut(id) else { return };

        egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
            ui.push_id(id, |ui| {
                ui.set_max_width(ui.available_width().min(780.0));
                let mut changed = false;
                ui.label(egui::RichText::new("Category").weak().small());
                ui.horizontal(|ui| {
                    let r = ui.add(egui::TextEdit::singleline(&mut e.category).desired_width(ui.available_width() - 90.0));
                    if r.has_focus() {
                        *fmt_focus = None;
                    }
                    changed |= r.changed();
                    ui.menu_button("Pick\u{2026} \u{25be}", |ui| {
                        for s in &suggestions {
                            if ui.button(s).clicked() {
                                e.category = s.clone();
                                changed = true;
                                ui.close();
                            }
                        }
                    });
                });
                for (label, text, hint) in [
                    ("Name", &mut e.name, ""),
                    ("Other names (comma-separated; also counted as mentions)", &mut e.aliases, "e.g. regional names, nicknames"),
                ] {
                    ui.label(egui::RichText::new(label).weak().small());
                    let r = ui.add(egui::TextEdit::singleline(text).hint_text(hint).desired_width(f32::INFINITY));
                    if r.has_focus() {
                        *fmt_focus = None;
                    }
                    changed |= r.changed();
                }
                let prompt = crate::templates::hint_for(&e.category).unwrap_or("");
                ui.add_space(4.0);
                ui.label(egui::RichText::new("Description").weak().small());
                let description_id = egui::Id::unique(("world-description", id));
                if let Some((_, cmd)) = fmt.take_if(|(target, _)| *target == description_id) {
                    changed |= editor::apply_format(ui, description_id, &mut e.description, cmd);
                }
                let description = editor::styled_multiline(
                    ui,
                    description_id,
                    &mut e.description,
                    *font_size,
                    12,
                    prompt,
                );
                if description.has_focus() {
                    *fmt_focus = Some(description_id);
                }
                changed |= description.changed();

                ui.add_space(4.0);
                ui.label(egui::RichText::new("Notes").weak().small());
                let notes = ui.add(
                    egui::TextEdit::multiline(&mut e.notes)
                        .lock_focus(true)
                        .desired_rows(5)
                        .desired_width(f32::INFINITY),
                );
                if notes.has_focus() {
                    *fmt_focus = None;
                }
                changed |= notes.changed();
                *dirty |= changed;

                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.button("Find in text").clicked() {
                        find_it = Some(e.name.clone());
                    }
                    if ui.button("Delete entry").clicked() {
                        delete = true;
                    }
                });
                ui.separator();
                ui.heading("Appearances");
                if matches.is_empty() {
                    ui.weak("Not mentioned in the manuscript.");
                } else {
                    ui.label(format!("{} mention(s) in {} section(s)", matches.len(), groups.len()));
                }
                for ((count, first), title) in groups.iter().zip(&titles) {
                    ui.horizontal(|ui| {
                        let num = numbers.get(&first.node).cloned().unwrap_or_default();
                        let t = if title.trim().is_empty() { "(untitled)" } else { title.as_str() };
                        if ui.link(format!("{num}. {t}")).clicked() {
                            jump = Some(first.clone());
                        }
                        ui.weak(format!("\u{d7}{count}"));
                    });
                }
            });
        });

        if let Some(m) = jump {
            self.selected = Some(m.node);
            self.goto_match(&m);
        }
        if let Some(name) = find_it {
            self.find.query.needle = name;
            self.find.query.case_sensitive = true;
            self.find.query.whole_word = true;
            self.find.current = 0;
            self.find.open = true;
            self.find.focus = true;
            self.side = Side::Editor;
        }
        if delete {
            self.snapshot();
            self.book.world.retain(|e| e.id != id);
            self.selected_world = None;
            self.dirty = true;
        }
    }

    fn character_list_ui(&mut self, ui: &mut egui::Ui) {
        if ui.button("+ Character").clicked() {
            self.snapshot();
            let id = self.book.add_character("New character");
            self.selected_char = Some(id);
            self.dirty = true;
        }
        ui.separator();
        let counts: HashMap<NodeId, usize> = self
            .mention_cache
            .as_ref()
            .map(|(_, m)| m.iter().map(|(k, v)| (*k, v.len())).collect())
            .unwrap_or_default();
        let mut clicked = None;
        egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
            for c in &self.book.characters {
                let n = counts.get(&c.id).copied().unwrap_or(0);
                let name = if c.name.trim().is_empty() { "(unnamed)" } else { c.name.as_str() };
                let label = format!("{name}   \u{b7}   {n}");
                if ui.selectable_label(self.selected_char == Some(c.id), label).clicked() {
                    clicked = Some(c.id);
                }
            }
        });
        if clicked.is_some() {
            self.selected_char = clicked;
        }
    }

    fn characters_ui(&mut self, ui: &mut egui::Ui) {
        let valid = self.selected_char.filter(|id| self.book.characters.iter().any(|c| c.id == *id));
        let Some(id) = valid.or_else(|| self.book.characters.first().map(|c| c.id)) else {
            ui.centered_and_justified(|ui| ui.weak("Add a character, or use File \u{2192} Import\u{2026}"));
            return;
        };
        self.selected_char = Some(id);

        let hovering_file = ui.ctx().input(|i| !i.raw.hovered_files.is_empty());
        let dropped = ui.ctx().input(|i| i.raw.dropped_files.clone());
        if let Some(file) = dropped.first() {
            match file.bytes() {
                Ok(bytes) => self.set_portrait(id, &bytes),
                Err(e) => self.message = format!("Read failed: {e}"),
            }
        }
        let tex = self.portrait_texture(ui.ctx(), id);
        let mut choose_image = false;
        let mut remove_image = false;

        let matches = self
            .mention_cache
            .as_ref()
            .and_then(|(_, m)| m.get(&id))
            .cloned()
            .unwrap_or_default();
        let numbers: HashMap<NodeId, String> =
            self.book.flatten().into_iter().map(|(i, _, n)| (i, n)).collect();
        let mut groups: Vec<(usize, Match)> = Vec::new();
        for m in &matches {
            match groups.last_mut() {
                Some((c, first)) if first.node == m.node => *c += 1,
                _ => groups.push((1, m.clone())),
            }
        }
        let titles: Vec<String> = groups
            .iter()
            .map(|(_, m)| self.book.find(m.node).map(|n| n.title.clone()).unwrap_or_default())
            .collect();

        let mut jump: Option<Match> = None;
        let mut delete = false;
        let mut find_it: Option<String> = None;
        let App { book, dirty, .. } = self;
        let Some(c) = book.find_character_mut(id) else { return };

        egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
            ui.push_id(id, |ui| {
                ui.set_max_width(ui.available_width().min(780.0));
                let mut changed = false;
                ui.horizontal(|ui| {
                    match &tex {
                        Some(t) => {
                            ui.add(egui::Image::new(t).max_size(vec2(220.0, 260.0)).corner_radius(6.0));
                        }
                        None => {
                            let (rect, _) = ui.allocate_exact_size(vec2(160.0, 200.0), Sense::hover());
                            let color = if hovering_file {
                                Color32::from_rgb(60, 140, 255)
                            } else {
                                ui.visuals().weak_text_color()
                            };
                            ui.painter().rect_stroke(rect, 6.0, Stroke::new(1.5, color), egui::StrokeKind::Inside);
                            ui.painter().text(
                                rect.center(),
                                Align2::CENTER_CENTER,
                                "Drop a .jpg or .png\nportrait here",
                                FontId::proportional(14.0),
                                color,
                            );
                        }
                    }
                    ui.vertical(|ui| {
                        if ui.button("Choose image\u{2026}").clicked() {
                            choose_image = true;
                        }
                        if tex.is_some() && ui.button("Remove image").clicked() {
                            remove_image = true;
                        }
                        ui.weak("Or drag an image file onto this window.");
                    });
                });
                ui.add_space(6.0);
                ui.label(egui::RichText::new("Name").weak().small());
                changed |= ui
                    .add(egui::TextEdit::singleline(&mut c.name).desired_width(f32::INFINITY))
                    .changed();
                ui.label(egui::RichText::new("Aliases / nicknames (comma-separated; also counted as mentions)").weak().small());
                changed |= ui
                    .add(egui::TextEdit::singleline(&mut c.aliases).desired_width(f32::INFINITY))
                    .changed();
                ui.label(egui::RichText::new("Role").weak().small());
                changed |= ui
                    .add(egui::TextEdit::singleline(&mut c.role).desired_width(f32::INFINITY))
                    .changed();
                for (label, text, rows) in [
                    ("Description", &mut c.description, 5),
                    ("Synopsis / arc", &mut c.synopsis, 5),
                    ("Notes", &mut c.notes, 4),
                ] {
                    ui.add_space(4.0);
                    ui.label(egui::RichText::new(label).weak().small());
                    changed |= ui
                        .add(
                            egui::TextEdit::multiline(text)
                                .lock_focus(true)
                                .desired_rows(rows)
                                .desired_width(f32::INFINITY),
                        )
                        .changed();
                }
                *dirty |= changed;

                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.button("Find in text").clicked() {
                        find_it = Some(c.name.clone());
                    }
                    if ui.button("Delete character").clicked() {
                        delete = true;
                    }
                });
                ui.separator();
                ui.heading("Appearances");
                if matches.is_empty() {
                    ui.weak("Not mentioned yet. Add nicknames under Aliases if the text uses them.");
                } else {
                    ui.label(format!(
                        "{} mention{} in {} section{}",
                        matches.len(),
                        if matches.len() == 1 { "" } else { "s" },
                        groups.len(),
                        if groups.len() == 1 { "" } else { "s" },
                    ));
                }
                for ((count, first), title) in groups.iter().zip(&titles) {
                    ui.horizontal(|ui| {
                        let num = numbers.get(&first.node).cloned().unwrap_or_default();
                        let t = if title.trim().is_empty() { "(untitled)" } else { title.as_str() };
                        if ui.link(format!("{num}. {t}")).clicked() {
                            jump = Some(first.clone());
                        }
                        ui.weak(format!("\u{d7}{count}"));
                    });
                }
            });
        });

        if choose_image {
            if let Some(p) = rfd::FileDialog::new().add_filter("Image", &["jpg", "jpeg", "png"]).pick_file() {
                match std::fs::read(&p) {
                    Ok(bytes) => self.set_portrait(id, &bytes),
                    Err(e) => self.message = format!("Read failed: {e}"),
                }
            }
        }
        if remove_image {
            self.snapshot();
            if let Some(c) = self.book.find_character_mut(id) {
                c.image = None;
            }
            self.dirty = true;
        }
        if let Some(m) = jump {
            self.selected = Some(m.node);
            self.goto_match(&m);
        }
        if let Some(name) = find_it {
            self.find.query.needle = name;
            self.find.query.case_sensitive = true;
            self.find.query.whole_word = true;
            self.find.current = 0;
            self.find.open = true;
            self.find.focus = true;
            self.side = Side::Editor;
        }
        if delete {
            self.snapshot();
            self.book.characters.retain(|c| c.id != id);
            self.selected_char = None;
            self.dirty = true;
        }
    }

    // ---------- outline panel ----------

    fn visible_rows(&self) -> Vec<Row> {
        fn walk(
            nodes: &[crate::model::Node],
            depth: usize,
            prefix: &str,
            nums: &HashMap<NodeId, String>,
            out: &mut Vec<Row>,
        ) {
            for (i, n) in nodes.iter().enumerate() {
                let number = if prefix.is_empty() {
                    format!("{}", i + 1)
                } else {
                    format!("{prefix}.{}", i + 1)
                };
                out.push(Row {
                    id: n.id,
                    depth,
                    title: n.title.clone(),
                    synopsis: n.synopsis.clone(),
                    has_children: !n.children.is_empty(),
                    collapsed: n.collapsed,
                    kind: n.kind(),
                    label: nums.get(&n.id).cloned().unwrap_or_default(),
                });
                if !n.collapsed {
                    walk(&n.children, depth + 1, &number, nums, out);
                }
            }
        }
        let nums = self.book.chapter_numbers();
        let mut out = Vec::new();
        walk(&self.book.roots, 0, "", &nums, &mut out);
        out
    }

    fn outline_ui(&self, ui: &mut egui::Ui) -> Vec<OutlineAction> {
        let mut acts = Vec::new();
        let sel = self.selected;
        ui.horizontal(|ui| {
            if ui.button("+ Section").on_hover_text("Add top-level section").clicked() {
                acts.push(OutlineAction::AddRoot);
            }
            ui.add_enabled_ui(sel.is_some(), |ui| {
                let id = sel.unwrap_or(0);
                if ui.button("+ Sub").on_hover_text("Add sub-section").clicked() {
                    acts.push(OutlineAction::AddChild(id));
                }
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add_enabled_ui(sel.is_some(), |ui| {
                    let id = sel.unwrap_or(0);
                    if ui.button("🗑").on_hover_text("Delete section").clicked() {
                        acts.push(OutlineAction::Delete(id));
                    }
                });
            });
        });
        ui.separator();

        let dragged: Option<NodeId> = egui::DragAndDrop::payload::<NodeId>(ui.ctx()).map(|p| *p);
        let released = ui.input(|i| i.pointer.any_released());
        let rows = self.visible_rows();
        egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
            for row in &rows {
                let (rect, resp) = ui.allocate_exact_size(
                    vec2(ui.available_width(), 26.0),
                    Sense::click_and_drag(),
                );
                let is_sel = sel == Some(row.id);
                let visuals = ui.visuals();
                if is_sel {
                    ui.painter().rect_filled(rect, 3.0, visuals.selection.bg_fill);
                } else if resp.hovered() {
                    ui.painter().rect_filled(rect, 3.0, visuals.widgets.hovered.weak_bg_fill);
                }
                let text_color = if is_sel { visuals.selection.stroke.color } else { visuals.text_color() };
                let weak = visuals.weak_text_color();
                let x0 = rect.left() + 4.0 + row.depth as f32 * 16.0;
                let arrow = egui::Rect::from_min_size(egui::pos2(x0, rect.top()), vec2(16.0, rect.height()));
                let p = ui.painter().with_clip_rect(rect);
                if row.has_children {
                    p.text(
                        arrow.center(),
                        Align2::CENTER_CENTER,
                        if row.collapsed { "▶" } else { "▼" },
                        FontId::proportional(10.0),
                        weak,
                    );
                }
                let title = if row.title.trim().is_empty() { "(untitled)" } else { row.title.as_str() };
                let num_w = if row.label.is_empty() {
                    0.0
                } else {
                    p.text(
                        egui::pos2(x0 + 18.0, rect.center().y),
                        Align2::LEFT_CENTER,
                        format!("{}.", row.label),
                        FontId::proportional(12.0),
                        weak,
                    )
                    .width()
                };
                let title_font = if row.kind == NodeKind::Part {
                    FontId::new(15.0, FontFamily::Name(editor::SERIF_BOLD.into()))
                } else {
                    FontId::proportional(14.5)
                };
                p.text(
                    egui::pos2(x0 + 22.0 + num_w, rect.center().y),
                    Align2::LEFT_CENTER,
                    title,
                    title_font,
                    text_color,
                );

                if resp.clicked() {
                    let on_arrow = resp
                        .interact_pointer_pos()
                        .is_some_and(|pos| arrow.contains(pos));
                    if on_arrow && row.has_children {
                        acts.push(OutlineAction::Toggle(row.id));
                    } else {
                        acts.push(OutlineAction::Select(row.id));
                    }
                }
                if resp.drag_started() {
                    egui::DragAndDrop::set_payload(ui.ctx(), row.id);
                    acts.push(OutlineAction::Select(row.id));
                }
                if let Some(src) = dragged {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
                    if let Some(pos) = ui.ctx().pointer_interact_pos().filter(|p| rect.contains(*p)) {
                        let t = (pos.y - rect.top()) / rect.height();
                        let drop = if t < 0.28 {
                            DropPos::Before
                        } else if t > 0.72 {
                            DropPos::After
                        } else {
                            DropPos::Inside
                        };
                        let stroke = Stroke::new(2.0, Color32::from_rgb(60, 140, 255));
                        match drop {
                            DropPos::Before => {
                                ui.painter().hline(rect.x_range(), rect.top(), stroke);
                            }
                            DropPos::After => {
                                ui.painter().hline(rect.x_range(), rect.bottom(), stroke);
                            }
                            DropPos::Inside => {
                                ui.painter().rect_stroke(rect, 3.0, stroke, egui::StrokeKind::Inside);
                            }
                        }
                        if released && src != row.id {
                            acts.push(OutlineAction::Drop(src, row.id, drop));
                        }
                    }
                }
                if !row.synopsis.trim().is_empty() {
                    resp.clone().on_hover_text(&row.synopsis);
                }
                resp.context_menu(|ui| {
                    if ui.button("Add sub-section").clicked() {
                        acts.push(OutlineAction::AddChild(row.id));
                        ui.close();
                    }
                    if ui.button("Add section after").clicked() {
                        acts.push(OutlineAction::AddAfter(row.id));
                        ui.close();
                    }
                    ui.separator();
                    if ui.button("Move up").clicked() {
                        acts.push(OutlineAction::Up(row.id));
                        ui.close();
                    }
                    if ui.button("Move down").clicked() {
                        acts.push(OutlineAction::Down(row.id));
                        ui.close();
                    }
                    if ui.button("Indent").clicked() {
                        acts.push(OutlineAction::Indent(row.id));
                        ui.close();
                    }
                    if ui.button("Outdent").clicked() {
                        acts.push(OutlineAction::Outdent(row.id));
                        ui.close();
                    }
                    ui.separator();
                    ui.menu_button("Section type", |ui| {
                        for (label, kind) in [
                            ("Chapter", NodeKind::Chapter),
                            ("Part (divider, no text)", NodeKind::Part),
                            ("Front/back matter (unnumbered)", NodeKind::Matter),
                        ] {
                            if ui.radio(row.kind == kind, label).clicked() {
                                acts.push(OutlineAction::SetKind(row.id, kind));
                                ui.close();
                            }
                        }
                    });
                    ui.separator();
                    if ui.button("Delete").clicked() {
                        acts.push(OutlineAction::Delete(row.id));
                        ui.close();
                    }
                });
            }
        });
        if released && dragged.is_some() {
            egui::DragAndDrop::clear_payload(ui.ctx());
        }
        acts
    }

    // ---------- document area ----------

    fn document_ui(&mut self, ui: &mut egui::Ui, matches: &[Match], fmt: Option<(egui::Id, FormatCmd)>) {
        let App {
            book,
            spell,
            spell_on,
            find,
            selected,
            view,
            font_size,
            manuscript_format,
            show_synopsis_in_manuscript,
            scroll_request,
            goto,
            last_focus,
            fmt_focus,
            dirty,
            ..
        } = self;

        let ids: Vec<(NodeId, usize, String)> = match view {
            View::Manuscript => book.flatten(),
            View::Chapter => match selected {
                Some(s) if book.find(*s).is_some() => vec![(*s, 0, String::new())],
                _ => Vec::new(),
            },
        };
        if ids.is_empty() {
            ui.centered_and_justified(|ui| ui.weak("Add a section in the outline to start writing."));
            return;
        }

        let current = if find.open {
            matches.get(find.current).map(|m| (m.node, m.range.clone()))
        } else {
            None
        };
        let query = (find.open && !find.query.needle.is_empty()).then_some(&find.query);
        let mut spell_actions = Vec::new();
        let mut env = Env {
            spell: spell_on.then_some(&*spell),
            spell_actions: &mut spell_actions,
            query,
            current,
            fmt,
            goto: goto.take(),
            font_size: *font_size,
            manuscript: *manuscript_format,
            last_focus,
        };
        let single = *view == View::Chapter;
        let chapter_nums = book.chapter_numbers();
        let mut new_selected = None;
        let serif_bold = FontFamily::Name(editor::SERIF_BOLD.into());

        egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
            ui.vertical_centered(|ui| {
                ui.set_max_width(ui.available_width().min(780.0));
                for (id, depth, number) in &ids {
                    let id = *id;
                    let Some(node) = book.find_mut(id) else { continue };
                    let top = ui.cursor().top();
                    let kind = node.kind();
                    if !single {
                        ui.add_space(14.0);
                    }
                    ui.horizontal(|ui| {
                        let label = match kind {
                            NodeKind::Chapter => format!("Chapter {}", chapter_nums.get(&id).unwrap_or(number)),
                            NodeKind::Part => "Part divider".to_string(),
                            NodeKind::Matter => "Front/back matter".to_string(),
                        };
                        ui.weak(label);
                        ui.menu_button(egui::RichText::new("change type").small(), |ui| {
                            for (label, k) in [
                                ("Chapter", NodeKind::Chapter),
                                ("Part (divider, no text)", NodeKind::Part),
                                ("Front/back matter (unnumbered)", NodeKind::Matter),
                            ] {
                                if ui.radio(kind == k, label).clicked() {
                                    node.kind = Some(k);
                                    *dirty = true;
                                    ui.close();
                                }
                            }
                        });
                    });
                    let size = (env.font_size * (1.9 - 0.2 * *depth as f32)).max(env.font_size * 1.2);
                    let t = ui.add(
                        egui::TextEdit::singleline(&mut node.title)
                            .id(egui::Id::unique(("title", id)))
                            .font(FontId::new(size, serif_bold.clone()))
                            .hint_text("Title")
                            .frame(egui::Frame::NONE)
                            .desired_width(f32::INFINITY),
                    );
                    if t.has_focus() {
                        new_selected = Some(id);
                        *fmt_focus = None;
                    }
                    *dirty |= t.changed();

                    if single || *show_synopsis_in_manuscript {
                        ui.label(egui::RichText::new("Synopsis").weak().small());
                        let s = ui.add(
                            egui::TextEdit::multiline(&mut node.synopsis)
                                .id(egui::Id::unique(("syn", id)))
                                .hint_text("What happens in this section?")
                                .lock_focus(true)
                                .desired_rows(3)
                                .desired_width(f32::INFINITY),
                        );
                        if s.has_focus() {
                            new_selected = Some(id);
                            *fmt_focus = None;
                        }
                        *dirty |= s.changed();
                    }
                    ui.add_space(6.0);
                    if scroll_request.take_if(|r| *r == id).is_some() {
                        let rect = egui::Rect::from_min_size(
                            egui::pos2(ui.max_rect().left(), top),
                            vec2(10.0, 20.0),
                        );
                        ui.scroll_to_rect(rect, Some(Align::TOP));
                    }
                    if kind == NodeKind::Part && node.body.trim().is_empty() {
                        ui.weak("A part only groups the chapters beneath it, so it has no text of its own.");
                    } else {
                        if kind == NodeKind::Part {
                            ui.colored_label(
                                Color32::from_rgb(200, 140, 40),
                                "This part contains text. Move it into a chapter, or change the type.",
                            );
                        }
                        let rows = if single { 24 } else { 6 };
                        let before = env.last_focus.is_some_and(|f| f == id);
                        *dirty |= editor::body_editor(ui, node, &mut env, rows, fmt_focus);
                        if !before && env.last_focus.is_some_and(|f| f == id) {
                            new_selected = Some(id);
                        }
                    }
                    if !single {
                        ui.add_space(10.0);
                        ui.separator();
                    }
                }
            });
        });
        for action in spell_actions {
            match action {
                SpellAction::Ignore(word) => spell.add_word(&word),
                SpellAction::Add(word) => {
                    spell.add_word(&word);
                    if !book.custom_words.contains(&word) {
                        book.custom_words.push(word);
                        *dirty = true;
                    }
                }
            }
        }
        if let Some(s) = new_selected {
            *selected = Some(s);
        }
    }

    // ---------- dialogs ----------

    fn spelling_window(&mut self, ctx: &egui::Context) {
        let mut open = self.spell_open;
        let mut first: Option<(NodeId, std::ops::Range<usize>, String)> = None;
        for (id, _, _) in self.book.flatten() {
            if let Some(n) = self.book.find(id) {
                if let Some(r) = self.spell.misspellings(&n.body).into_iter().next() {
                    first = Some((id, r.clone(), n.body[r].to_string()));
                    break;
                }
            }
        }
        if self.spell_suggestions.as_ref().map(|s| &s.0) != first.as_ref().map(|f| &f.2) {
            self.spell_suggestions = first.as_ref().map(|f| (f.2.clone(), self.spell.suggest(&f.2)));
        }
        let mut replace_with: Option<String> = None;
        let mut ignore = false;
        let mut add = false;
        let mut show = false;
        egui::Window::new("Spelling").open(&mut open).collapsible(false).show(ctx, |ui| {
            match &first {
                None => {
                    ui.label("No spelling errors found.");
                }
                Some((id, range, word)) => {
                    let ctx_text = self
                        .book
                        .find(*id)
                        .map(|n| {
                            let a = n.body[..range.start].char_indices().rev().nth(30).map_or(0, |c| c.0);
                            let b = n.body[range.end..]
                                .char_indices()
                                .nth(30)
                                .map_or(n.body.len(), |c| range.end + c.0);
                            n.body[a..b].replace('\n', " ")
                        })
                        .unwrap_or_default();
                    ui.label(egui::RichText::new(word).strong().color(Color32::from_rgb(220, 50, 50)));
                    ui.weak(format!("…{ctx_text}…"));
                    ui.separator();
                    if let Some((_, sugg)) = &self.spell_suggestions {
                        if sugg.is_empty() {
                            ui.weak("No suggestions");
                        }
                        ui.horizontal_wrapped(|ui| {
                            for s in sugg {
                                if ui.button(s).clicked() {
                                    replace_with = Some(s.clone());
                                }
                            }
                        });
                    }
                    ui.separator();
                    ui.horizontal(|ui| {
                        ignore = ui.button("Ignore (session)").clicked();
                        add = ui.button("Add to dictionary").clicked();
                        show = ui.button("Show in text").clicked();
                    });
                }
            }
        });
        self.spell_open = open;
        if let Some((id, range, word)) = first {
            if let Some(w) = replace_with {
                if let Some(n) = self.book.find_mut(id) {
                    n.body.replace_range(range.clone(), &w);
                    self.dirty = true;
                }
            }
            if ignore {
                self.spell.add_word(&word);
            }
            if add {
                self.spell.add_word(&word);
                self.book.custom_words.push(word.clone());
                self.dirty = true;
            }
            if show {
                self.goto_match(&Match { node: id, range });
            }
        }
    }

    fn settings_window(&mut self, ctx: &egui::Context) {
        let mut open = self.settings_open;
        egui::Window::new("Settings")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                ui.heading("Editor view");
                ui.radio_value(&mut self.view, View::Chapter, "Single section");
                ui.radio_value(&mut self.view, View::Manuscript, "Full manuscript");
                ui.checkbox(&mut self.show_synopsis_in_manuscript, "Show synopses in manuscript");
                ui.separator();
                ui.add(egui::Slider::new(&mut self.font_size, 12.0..=36.0).text("Font size"));
                ui.checkbox(&mut self.manuscript_format, "Manuscript layout")
                    .on_hover_text("Double-spaced, 0.5\" first-line indent, left-aligned, 6.5\" text column");
                ui.separator();
                ui.horizontal(|ui| {
                    ui.label("Theme");
                    egui::widgets::global_theme_preference_buttons(ui);
                });
            });
        self.settings_open = open;
    }

    fn import_window(&mut self, ctx: &egui::Context) {
        let mut open = self.import_open;
        let mut do_import = false;
        let mut load_file = false;
        let mut cancel = false;
        egui::Window::new("Import")
            .open(&mut open)
            .collapsible(false)
            .default_size([620.0, 420.0])
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.radio_value(&mut self.import_mode, ImportMode::Combined, "All (outline, characters, background)");
                    ui.radio_value(&mut self.import_mode, ImportMode::Outline, "Outline only");
                    ui.radio_value(&mut self.import_mode, ImportMode::Characters, "Characters only");
                    ui.radio_value(&mut self.import_mode, ImportMode::World, "Background only");
                });
                ui.weak(match self.import_mode {
                    ImportMode::Outline => {
                        "Lines: 'Prelude: synopsis', 'Part I: Title (Chapters 1-7)', 'Chapter N: Title: synopsis'. Chapters nest under the preceding Part."
                    }
                    ImportMode::Characters => {
                        "Use '# Name' headings, 'Name: description' lines, or a name on its own line. Optional labels: Aliases, Role, Description, Synopsis, Notes."
                    }
                    ImportMode::World => {
                        "Paste background notes (world, places, timeline, research...). Short standalone lines (or '# Category') become categories; a short title line followed by text becomes an entry. 'Name - Alt - Alt' sets other names."
                    }
                    ImportMode::Combined => {
                        "Separate the parts with lines reading 'Outline', 'Characters' and 'Background' (text before the first header is the outline)."
                    }
                });
                egui::ScrollArea::vertical().max_height(ui.available_height() - 40.0).show(ui, |ui| {
                    ui.add(
                        egui::TextEdit::multiline(&mut self.import_text)
                            .desired_width(f32::INFINITY)
                            .desired_rows(16),
                    );
                });
                ui.horizontal(|ui| {
                    do_import = ui
                        .add_enabled(!self.import_text.trim().is_empty(), egui::Button::new("Import"))
                        .clicked();
                    load_file = ui.button("Load from file…").clicked();
                    cancel = ui.button("Cancel").clicked();
                });
            });
        if load_file {
            if let Some(p) = rfd::FileDialog::new().add_filter("Text", &["txt", "md"]).pick_file() {
                match std::fs::read_to_string(&p) {
                    Ok(s) => self.import_text = s,
                    Err(e) => self.message = format!("Read failed: {e}"),
                }
            }
        }
        if do_import {
            self.snapshot();
            let text = std::mem::take(&mut self.import_text);
            self.message = match self.import_mode {
                ImportMode::Outline => format!("Imported {} sections", self.book.import_outline(&text)),
                ImportMode::Characters => {
                    let (a, u) = self.book.import_characters(&text);
                    format!("Characters: {a} added, {u} updated")
                }
                ImportMode::World => {
                    let (a, u) = self.book.import_world(&text);
                    format!("Background: {a} added, {u} updated")
                }
                ImportMode::Combined => {
                    let (s, a, u, w) = self.book.import_combined(&text);
                    format!("Imported {s} sections; characters: {a} added, {u} updated; background entries: {w}")
                }
            };
            if self.selected_char.is_none() {
                self.selected_char = self.book.characters.first().map(|c| c.id);
            }
            if self.selected_world.is_none() {
                self.selected_world = self.book.world.first().map(|e| e.id);
            }
            self.dirty = true;
            self.selected = self.book.roots.first().map(|n| n.id);
            open = false;
        }
        if cancel {
            open = false;
        }
        self.import_open = open;
    }

    fn confirm_window(&mut self, ctx: &egui::Context) {
        let Some(action) = self.pending else { return };
        let mut choice: Option<u8> = None;
        egui::Window::new("Unsaved changes")
            .collapsible(false)
            .resizable(false)
            .anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0))
            .show(ctx, |ui| {
                ui.label("Save changes to the current novel first?");
                ui.horizontal(|ui| {
                    if ui.button("Save").clicked() {
                        choice = Some(0);
                    }
                    if ui.button("Don't save").clicked() {
                        choice = Some(1);
                    }
                    if ui.button("Cancel").clicked() {
                        choice = Some(2);
                    }
                });
            });
        match choice {
            Some(0) => {
                if self.save(false) {
                    self.pending = None;
                    self.perform(ctx, action);
                }
            }
            Some(1) => {
                self.pending = None;
                self.perform(ctx, action);
            }
            Some(_) => self.pending = None,
            None => {}
        }
    }

    fn launcher_ui(&mut self, ui: &mut egui::Ui) {
        const SIDEBAR_W: f32 = 240.0;
        const MENU_X: f32 = 40.0;
        const MARGIN: f32 = 34.0;
        let white = Color32::from_rgb(245, 245, 245);
        let dim = Color32::from_rgb(130, 130, 130);

        let screen_rect = ui.max_rect();
        let painter = ui.painter().clone();
        painter.rect_filled(screen_rect, 0.0, Color32::BLACK);

        // Splash image fills the area right of the sidebar, cropped to cover.
        let image_rect = egui::Rect::from_min_max(
            egui::pos2(screen_rect.left() + SIDEBAR_W + 6.0, screen_rect.top() + MARGIN),
            egui::pos2(screen_rect.right() - 18.0, screen_rect.bottom() - 20.0),
        );
        if let Some(tex) = &self.splash_texture
            && image_rect.width() > 0.0
            && image_rect.height() > 0.0
        {
            let area_aspect = image_rect.width() / image_rect.height();
            let [iw, ih] = tex.size();
            let img_aspect = iw as f32 / ih as f32;
            let uv = if img_aspect > area_aspect {
                let w = area_aspect / img_aspect;
                let x = (1.0 - w) / 2.0;
                egui::Rect::from_min_max(egui::pos2(x, 0.0), egui::pos2(x + w, 1.0))
            } else {
                let h = img_aspect / area_aspect;
                let y = (1.0 - h) / 2.0;
                egui::Rect::from_min_max(egui::pos2(0.0, y), egui::pos2(1.0, y + h))
            };
            painter.image(tex.id(), image_rect, uv, Color32::WHITE);
        }

        // Left sidebar menu
        let sidebar = egui::Rect::from_min_max(
            egui::pos2(screen_rect.left() + MENU_X, screen_rect.top() + MARGIN),
            egui::pos2(screen_rect.left() + SIDEBAR_W, screen_rect.bottom() - 20.0),
        );
        let menu_font = FontId::new(13.0, FontFamily::Name(editor::SANS_BOLD.into()));
        let mut new_clicked = false;
        let mut open_clicked = false;
        let mut to_open: Option<PathBuf> = None;
        let mut to_remove: Option<PathBuf> = None;

        ui.scope_builder(egui::UiBuilder::new().max_rect(sidebar), |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;

            let menu_item = |ui: &mut egui::Ui, text: &str| -> bool {
                let resp = ui.add(
                    egui::Label::new(egui::RichText::new(text).font(menu_font.clone()).color(white))
                        .sense(Sense::click()),
                );
                let resp = resp.on_hover_cursor(egui::CursorIcon::PointingHand);
                if resp.hovered() {
                    ui.painter().hline(
                        resp.rect.x_range(),
                        resp.rect.bottom() + 1.0,
                        Stroke::new(1.0, white),
                    );
                }
                ui.add_space(16.0);
                resp.clicked()
            };

            new_clicked = menu_item(ui, "New");
            open_clicked = menu_item(ui, "Open...");

            ui.label(egui::RichText::new("Recent").font(menu_font.clone()).color(white));
            ui.add_space(8.0);
            let (line, _) = ui.allocate_exact_size(vec2(ui.available_width(), 2.0), Sense::hover());
            ui.painter().rect_filled(line, 0.0, white);
            ui.add_space(12.0);

            if !self.launcher_error.is_empty() {
                ui.label(
                    egui::RichText::new(&self.launcher_error)
                        .small()
                        .color(Color32::LIGHT_RED),
                );
                ui.add_space(10.0);
            }

            if self.recents.is_empty() {
                ui.label(egui::RichText::new("No recent projects").small().color(dim));
                return;
            }

            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    for path in &self.recents {
                        let name = path
                            .file_stem()
                            .and_then(|s| s.to_str())
                            .unwrap_or("Untitled")
                            .to_string();
                        let dir = path
                            .parent()
                            .map(|p| p.display().to_string())
                            .unwrap_or_default();

                        let row = ui.scope(|ui| {
                            let name_resp = ui.add(
                                egui::Label::new(
                                    egui::RichText::new(&name).font(menu_font.clone()).color(white),
                                )
                                .truncate()
                                .sense(Sense::click()),
                            );
                            ui.add(
                                egui::Label::new(egui::RichText::new(&dir).small().color(dim))
                                    .truncate(),
                            );
                            name_resp
                        });
                        let row_rect = row.response.rect;
                        let hovered = ui.rect_contains_pointer(row_rect);
                        let name_resp = row.inner.on_hover_text(path.display().to_string());
                        if name_resp.clicked() {
                            to_open = Some(path.clone());
                        }
                        if hovered {
                            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                            // Remove button at the right edge of the row
                            let x_rect = egui::Rect::from_center_size(
                                egui::pos2(row_rect.right() - 8.0, row_rect.top() + 9.0),
                                vec2(16.0, 16.0),
                            );
                            let x_resp = ui
                                .interact(x_rect, ui.scope_id().with(("rm", path)), Sense::click())
                                .on_hover_text("Remove from list");
                            ui.painter().text(
                                x_rect.center(),
                                Align2::CENTER_CENTER,
                                "×",
                                FontId::proportional(14.0),
                                if x_resp.hovered() { white } else { dim },
                            );
                            if x_resp.clicked() {
                                to_remove = Some(path.clone());
                                to_open = None;
                            }
                        }
                        ui.add_space(12.0);
                    }
                });
        });

        if new_clicked {
            self.load_book(Book::default(), None);
        } else if open_clicked {
            self.open_dialog();
        } else if let Some(p) = to_remove {
            crate::recents::remove(&mut self.recents, &p);
        } else if let Some(p) = to_open {
            self.open_recent(p);
        }
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        let mut fmt: Option<FormatCmd> = None;

        #[cfg(target_os = "macos")]
        {
            if self.native_menu.is_none() {
                self.native_menu = Some(NativeMenu::new());
            }
            let menu = self.native_menu.as_ref().unwrap();
            menu.sync(self.spell_on, !self.undo.is_empty());
            for action in menu.poll() {
                match action {
                    MenuAction::New => self.request(&ctx, Pending::New),
                    MenuAction::Open => self.request(&ctx, Pending::Open),
                    MenuAction::Save => {
                        self.save(false);
                    }
                    MenuAction::SaveAs => {
                        self.save(true);
                    }
                    MenuAction::Import => self.import_open = true,
                    MenuAction::ExportMarkdown => self.export(true),
                    MenuAction::ExportText => self.export(false),
                    MenuAction::Quit => self.request(&ctx, Pending::Quit),
                    MenuAction::Undo => self.undo_outline(),
                    MenuAction::Find => {
                        self.find.open = true;
                        self.find.focus = true;
                    }
                    MenuAction::FindReplace => {
                        self.find.open = true;
                        self.find.replace = true;
                        self.find.focus = true;
                    }
                    MenuAction::CheckSpelling => self.spell_open = true,
                    MenuAction::ToggleSpell => self.spell_on = !self.spell_on,
                    MenuAction::Settings => self.settings_open = true,
                    MenuAction::Format(cmd) => fmt = Some(cmd),
                    MenuAction::CleanSpacing => {
                        self.clean_manuscript_spacing();
                    }
                }
            }
        }

        if self.launcher {
            self.launcher_ui(ui);
            return;
        }

        // Shortcuts
        let key = |k: Key| {
            ctx.input_mut(|i| i.consume_shortcut(&egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, k)))
        };
        if key(Key::S) {
            self.save(false);
        }
        if key(Key::O) {
            self.request(&ctx, Pending::Open);
        }
        if key(Key::N) {
            self.request(&ctx, Pending::New);
        }
        if key(Key::F) {
            self.find.open = true;
            self.find.focus = true;
        }
        if key(Key::R) {
            self.find.open = true;
            self.find.replace = true;
            self.find.focus = true;
        }
        if key(Key::B) {
            fmt = Some(FormatCmd::Bold);
        }
        if key(Key::I) {
            fmt = Some(FormatCmd::Italic);
        }
        if key(Key::U) {
            fmt = Some(FormatCmd::Underline);
        }

        if ctx.input(|i| i.viewport().close_requested()) && self.dirty && !self.allow_close {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.pending = Some(Pending::Quit);
        }

        let title = format!(
            "{}{} — Novelist",
            self.book.title,
            if self.dirty { " •" } else { "" }
        );
        if title != self.last_title {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            self.last_title = title;
        }

        // Outline (full-height left column)
        let mut acts = Vec::new();
        egui::Panel::left("outline")
            .default_size(300.0)
            .min_size(200.0)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut self.side, Side::Editor, "Editor");
                    ui.selectable_value(&mut self.side, Side::Characters, "Characters");
                    ui.selectable_value(&mut self.side, Side::World, "Background");
                });
                ui.separator();
                match self.side {
                    Side::Characters => {
                        self.character_list_ui(ui);
                        return;
                    }
                    Side::World => {
                        self.world_list_ui(ui);
                        return;
                    }
                    Side::Editor => {}
                }
                ui.horizontal(|ui| {
                    ui.label("Title:");
                    let r = ui.add(egui::TextEdit::singleline(&mut self.book.title).desired_width(f32::INFINITY));
                    self.dirty |= r.changed();
                });
                ui.horizontal(|ui| {
                    ui.label("Author:");
                    let r = ui.add(egui::TextEdit::singleline(&mut self.book.author).desired_width(f32::INFINITY));
                    self.dirty |= r.changed();
                });
                ui.separator();
                acts = self.outline_ui(ui);
            });
        for a in acts {
            self.apply(a);
        }

        // Menu bar
        egui::Panel::top("menu").show(ui, |ui| {
            egui::MenuBar::new().ui(ui, |ui| {
                #[cfg(not(target_os = "macos"))]
                ui.menu_button("File", |ui| {
                    if ui.button("New").clicked() {
                        self.request(&ctx, Pending::New);
                        ui.close();
                    }
                    if ui.button("Open…").clicked() {
                        self.request(&ctx, Pending::Open);
                        ui.close();
                    }
                    if ui.button("Save").clicked() {
                        self.save(false);
                        ui.close();
                    }
                    if ui.button("Save As…").clicked() {
                        self.save(true);
                        ui.close();
                    }
                    if ui.button("Import…").clicked() {
                        self.import_open = true;
                        ui.close();
                    }
                    ui.separator();
                    if ui.button("Export Markdown…").clicked() {
                        self.export(true);
                        ui.close();
                    }
                    if ui.button("Export plain text…").clicked() {
                        self.export(false);
                        ui.close();
                    }
                    ui.separator();
                    if ui.button("Quit").clicked() {
                        self.request(&ctx, Pending::Quit);
                        ui.close();
                    }
                });
                #[cfg(not(target_os = "macos"))]
                ui.menu_button("Edit", |ui| {
                    if ui
                        .add_enabled(!self.undo.is_empty(), egui::Button::new("Undo outline change"))
                        .clicked()
                    {
                        self.undo_outline();
                        ui.close();
                    }
                    ui.separator();
                    if ui.button("Find…").clicked() {
                        self.find.open = true;
                        self.find.focus = true;
                        ui.close();
                    }
                    if ui.button("Find & Replace…").clicked() {
                        self.find.open = true;
                        self.find.replace = true;
                        self.find.focus = true;
                        ui.close();
                    }
                    ui.separator();
                    if ui.button("Check spelling…").clicked() {
                        self.spell_open = true;
                        ui.close();
                    }
                    if ui
                        .button("Clean up manuscript spacing")
                        .on_hover_text("Remove tabs, leading and double spaces, and blank lines between paragraphs in every section")
                        .clicked()
                    {
                        self.clean_manuscript_spacing();
                        ui.close();
                    }
                    ui.checkbox(&mut self.spell_on, "Underline misspelled words");
                });
                #[cfg(not(target_os = "macos"))]
                if ui.button("Settings…").clicked() {
                    self.settings_open = true;
                }
                if self.side == Side::Editor {
                    #[cfg(not(target_os = "macos"))]
                    ui.separator();
                    ui.selectable_value(&mut self.view, View::Chapter, "Section");
                    ui.selectable_value(&mut self.view, View::Manuscript, "Manuscript");
                }
            });
        });

        egui::Panel::top("format_bar").show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                ui.add_space(2.0);
                for (label, cmd, hover) in [
                    (egui::RichText::new("B").strong(), FormatCmd::Bold, "Bold (Cmd/Ctrl+B)"),
                    (egui::RichText::new("I").italics(), FormatCmd::Italic, "Italic (Cmd/Ctrl+I)"),
                    (egui::RichText::new("U").underline(), FormatCmd::Underline, "Underline (Cmd/Ctrl+U)"),
                ] {
                    if ui
                        .add(egui::Button::new(label).min_size(vec2(30.0, 24.0)))
                        .on_hover_text(hover)
                        .clicked()
                    {
                        fmt = Some(cmd);
                    }
                }
                ui.separator();
                egui::ComboBox::from_id_salt("fmt_heading")
                    .selected_text("Heading")
                    .width(96.0)
                    .show_ui(ui, |ui| {
                        for (label, cmd) in [
                            ("Heading 1", FormatCmd::Heading(1)),
                            ("Heading 2", FormatCmd::Heading(2)),
                            ("Heading 3", FormatCmd::Heading(3)),
                        ] {
                            if ui.selectable_label(false, label).clicked() {
                                fmt = Some(cmd);
                            }
                        }
                    });
                egui::ComboBox::from_id_salt("fmt_list")
                    .selected_text("List")
                    .width(96.0)
                    .show_ui(ui, |ui| {
                        for (label, cmd) in [
                            ("\u{2022}  Bulleted list", FormatCmd::UnorderedList),
                            ("1.  Numbered list", FormatCmd::OrderedList),
                        ] {
                            if ui.selectable_label(false, label).clicked() {
                                fmt = Some(cmd);
                            }
                        }
                    });
            });
        });

        // Status bar
        egui::Panel::bottom("status").show(ui, |ui| {
            ui.horizontal(|ui| {
                let sec = self
                    .selected
                    .and_then(|s| self.book.find(s))
                    .map(|n| n.word_count())
                    .unwrap_or(0);
                ui.label(format!(
                    "{} words total · {} in section",
                    self.book.total_word_count(),
                    sec
                ));
                ui.separator();
                ui.weak(
                    self.path
                        .as_ref()
                        .map(|p| p.display().to_string())
                        .unwrap_or_else(|| "Unsaved".into()),
                );
                if !self.message.is_empty() {
                    ui.separator();
                    ui.weak(&self.message);
                }
            });
        });

        self.sync_spell_names();
        if self.side != Side::Editor {
            self.refresh_mentions();
        }

        // Document
        egui::CentralPanel::default().show(ui, |ui| {
            let matches = if self.find.open && !self.find.query.needle.is_empty() {
                search::find_all(&self.book, &self.find.query)
            } else {
                Vec::new()
            };
            if self.find.open {
                self.find_bar(ui, &matches);
                ui.separator();
            }
            let matches = if self.find.open && !self.find.query.needle.is_empty() {
                search::find_all(&self.book, &self.find.query)
            } else {
                Vec::new()
            };
            if !matches.is_empty() {
                self.find.current = self.find.current.min(matches.len() - 1);
            }
            if self.find.open && self.find.focus {
                self.side = Side::Editor;
            }
            let fmt = self.fmt_focus.zip(fmt);
            match self.side {
                Side::Characters => self.characters_ui(ui),
                Side::World => self.world_ui(ui, fmt),
                Side::Editor => self.document_ui(ui, &matches, fmt),
            }
        });

        if self.spell_open {
            self.spelling_window(&ctx);
        }
        if self.import_open {
            self.import_window(&ctx);
        }
        if self.settings_open {
            self.settings_window(&ctx);
        }
        self.confirm_window(&ctx);
        if self.goto.is_some() || self.scroll_request.is_some() {
            ctx.request_repaint();
        }
    }
}

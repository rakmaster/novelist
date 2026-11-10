use crate::format::{self, Style};
use crate::model::{Node, NodeId};
use crate::search::{self, Query};
use crate::spell::SpellChecker;
use eframe::egui::text::{CCursor, CCursorRange, LayoutJob, TextFormat};
use eframe::egui::text_edit::TextEditState;
use eframe::egui::{self, Align, Color32, FontFamily, FontId, Id, Key, Modifiers, Stroke, TextBuffer};
use std::ops::Range;

/// Word's "double" line spacing is about 2 x 1.15 of the font size.
const DOUBLE_SPACING: f32 = 2.3;
const SINGLE_SPACING: f32 = 1.45;
/// Standard manuscript: 0.5" first-line indent at 12 pt = 3 em.
const INDENT_EM: f32 = 3.0;
/// Standard manuscript: 6.5" text column at 12 pt = 39 em.
const COLUMN_EM: f32 = 39.0;

pub const SERIF: &str = "serif";
pub const SERIF_BOLD: &str = "serif-bold";
pub const SANS_BOLD: &str = "sans-bold";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FormatCmd {
    Bold,
    Italic,
    Underline,
    Heading(u8),
}

pub enum SpellAction {
    Ignore(String),
    Add(String),
}

pub struct Env<'a> {
    pub spell: Option<&'a SpellChecker>,
    pub spell_actions: &'a mut Vec<SpellAction>,
    pub query: Option<&'a Query>,
    pub current: Option<(NodeId, Range<usize>)>,
    pub fmt: Option<(NodeId, FormatCmd)>,
    pub goto: Option<(NodeId, usize)>,
    pub font_size: f32,
    /// Standard manuscript layout: double-spaced, first-line indent, 6.5" column.
    pub manuscript: bool,
    pub last_focus: &'a mut Option<NodeId>,
}

pub fn install_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert(
        SERIF.into(),
        std::sync::Arc::new(egui::FontData::from_static(include_bytes!(
            "../assets/fonts/CrimsonText-Regular.ttf"
        ))),
    );
    fonts.font_data.insert(
        SERIF_BOLD.into(),
        std::sync::Arc::new(egui::FontData::from_static(include_bytes!(
            "../assets/fonts/CrimsonText-Bold.ttf"
        ))),
    );
    fonts.font_data.insert(
        SANS_BOLD.into(),
        std::sync::Arc::new(egui::FontData::from_static(include_bytes!(
            "../assets/fonts/NotoSans-Bold.ttf"
        ))),
    );
    let fallback = fonts.families[&FontFamily::Proportional].clone();
    for name in [SERIF, SERIF_BOLD, SANS_BOLD] {
        let mut list = vec![name.to_string()];
        list.extend(fallback.iter().cloned());
        fonts.families.insert(FontFamily::Name(name.into()), list);
    }
    ctx.set_fonts(fonts);
}

fn serif(bold: bool, size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(if bold { SERIF_BOLD } else { SERIF }.into()))
}

fn contains(list: &[Range<usize>], pos: usize) -> bool {
    let i = list.partition_point(|r| r.end <= pos);
    list.get(i).is_some_and(|r| r.start <= pos)
}

fn build_job(
    text: &str,
    wrap: f32,
    size: f32,
    spell: Option<&SpellChecker>,
    highlights: &[Range<usize>],
    current: Option<&Range<usize>>,
    color: Color32,
    weak: Color32,
    manuscript: bool,
) -> LayoutJob {
    let spans = format::spans(text);
    let span_ranges: Vec<Range<usize>> = spans.iter().map(|s| s.0.clone()).collect();
    let bad = spell.map(|s| s.misspellings(text)).unwrap_or_default();

    let mut points = vec![0, text.len()];
    for r in span_ranges.iter().chain(&bad).chain(highlights) {
        points.push(r.start);
        points.push(r.end);
    }
    // Break sections at paragraph starts so each can carry the first-line indent.
    points.extend(text.match_indices('\n').map(|(i, _)| i + 1));
    points.sort_unstable();
    points.dedup();

    let mut job = LayoutJob::default();
    job.wrap.max_width = wrap;
    for w in points.windows(2) {
        let (a, b) = (w[0], w[1]);
        let si = span_ranges.partition_point(|r| r.end <= a);
        let style: Style = spans.get(si).map(|s| s.1).unwrap_or_default();
        let scale = match style.heading {
            1 => 1.7,
            2 => 1.4,
            3 => 1.2,
            _ => 1.0,
        };
        let mut fmt = TextFormat {
            font_id: serif(style.bold || style.heading > 0, size * scale),
            color: if style.marker { weak } else { color },
            italics: style.italic,
            line_height: Some(size * scale * if manuscript && style.heading == 0 { DOUBLE_SPACING } else { SINGLE_SPACING }),
            ..Default::default()
        };
        let para_start = a == 0 || text.as_bytes()[a - 1] == b'\n';
        let indent = if manuscript && para_start && style.heading == 0 { size * INDENT_EM } else { 0.0 };
        if style.underline && !style.marker {
            fmt.underline = Stroke::new(1.0, color);
        }
        if !style.marker && contains(&bad, a) {
            fmt.underline = Stroke::new(1.5, Color32::from_rgb(220, 50, 50));
        }
        if current.is_some_and(|c| c.start <= a && a < c.end) {
            fmt.background = Color32::from_rgb(255, 150, 40);
            fmt.color = Color32::BLACK;
        } else if contains(highlights, a) {
            fmt.background = Color32::from_rgba_unmultiplied(255, 220, 0, 120);
        }
        job.append(&text[a..b], indent, fmt);
    }
    job
}

fn byte_to_char(s: &str, b: usize) -> usize {
    s[..b.min(s.len())].chars().count()
}

fn char_to_byte(s: &str, c: usize) -> usize {
    s.char_indices().nth(c).map_or(s.len(), |(b, _)| b)
}

/// Draw the multiline body editor of a section. Returns true if text changed.
pub fn body_editor(ui: &mut egui::Ui, node: &mut Node, env: &mut Env, min_rows: usize) -> bool {
    let id = Id::unique(("body", node.id));
    let mut changed = false;

    if let Some((_, cmd)) = env.fmt.take_if(|(n, _)| *n == node.id) {
        let mut state = TextEditState::load(ui.ctx(), id).unwrap_or_default();
        let sel = state
            .cursor
            .char_range()
            .map(|r| {
                let [a, b] = r.sorted_cursors();
                a.index.0..b.index.0
            })
            .unwrap_or(0..0);
        let new = match cmd {
            FormatCmd::Bold => format::toggle_inline(&mut node.body, sel, "**"),
            FormatCmd::Italic => format::toggle_inline(&mut node.body, sel, "*"),
            FormatCmd::Underline => format::toggle_inline(&mut node.body, sel, "__"),
            FormatCmd::Heading(l) => format::toggle_heading(&mut node.body, sel, l),
        };
        state
            .cursor
            .set_char_range(Some(CCursorRange::two(CCursor::new(new.start), CCursor::new(new.end))));
        state.store(ui.ctx(), id);
        ui.memory_mut(|m| m.request_focus(id));
        changed = true;
    }

    let spell = env.spell;
    let query = env.query;
    let current = env.current.as_ref().filter(|(n, _)| *n == node.id).map(|(_, r)| r.clone());
    let size = env.font_size;
    let manuscript = env.manuscript;
    let color = ui.visuals().text_color();
    let weak = ui.visuals().weak_text_color();

    let mut layouter = |ui: &egui::Ui, text: &dyn TextBuffer, wrap: f32| {
        let t = text.as_str();
        let hl = query.map(|q| search::find_in_text(t, q)).unwrap_or_default();
        let job = build_job(t, wrap, size, spell, &hl, current.as_ref(), color, weak, manuscript);
        ui.fonts_mut(|f| f.layout_job(job))
    };

    // Indentation comes from the paragraph style, never from tab characters.
    if ui.memory(|m| m.has_focus(id)) {
        ui.input_mut(|i| {
            i.consume_key(Modifiers::NONE, Key::Tab);
            i.consume_key(Modifiers::SHIFT, Key::Tab);
        });
    }

    let width = if manuscript { (size * COLUMN_EM).min(ui.available_width()) } else { f32::INFINITY };
    let out = egui::TextEdit::multiline(&mut node.body)
        .id(id)
        .font(serif(false, size))
        .desired_width(width)
        .desired_rows(min_rows)
        .lock_focus(true)
        .layouter(&mut layouter)
        .show(ui);

    let menu_id = id.with("spell_menu");
    let menu_open = out.response.context_menu_opened();
    let mut menu_range = None;
    if let Some(spell) = spell.filter(|_| out.response.secondary_clicked() || menu_open) {
        let bad = spell.misspellings(&node.body);
        if out.response.secondary_clicked() {
            menu_range = out.response.interact_pointer_pos().and_then(|pos| {
                let rel = pos - out.galley_pos;
                let rel = egui::pos2(rel.x, rel.y);
                if !out
                    .galley
                    .rows
                    .iter()
                    .any(|row| row.rect_without_leading_space().contains(rel))
                {
                    return None;
                }
                let cursor = out.galley.cursor_from_pos(rel.to_vec2());
                let char_index = cursor.index.0;
                let byte = char_to_byte(&node.body, char_index);
                let previous = char_index
                    .checked_sub(1)
                    .map(|i| char_to_byte(&node.body, i));
                bad.iter()
                    .find(|r| {
                        (r.start <= byte && byte < r.end)
                            || previous.is_some_and(|p| r.start <= p && p < r.end)
                    })
                    .cloned()
            });
            if let Some(range) = &menu_range {
                ui.memory_mut(|m| m.data.insert_temp(menu_id, range.clone()));
            } else {
                ui.memory_mut(|m| m.data.remove::<Range<usize>>(menu_id));
            }
        } else {
            menu_range = ui
                .memory(|m| m.data.get_temp::<Range<usize>>(menu_id))
                .filter(|r| bad.contains(r));
            if menu_range.is_none() {
                ui.memory_mut(|m| m.data.remove::<Range<usize>>(menu_id));
            }
        }
    }

    if let Some(range) = menu_range {
        let word = node.body[range.clone()].to_string();
        let suggestions = spell.map(|s| s.suggest(&word)).unwrap_or_default();
        out.response.context_menu(|ui| {
            ui.label(egui::RichText::new(&word).strong());
            ui.separator();
            if suggestions.is_empty() {
                ui.weak("No suggestions");
            } else {
                for s in suggestions {
                    if ui.button(&s).clicked() {
                        node.body.replace_range(range.clone(), &s);
                        changed = true;
                        ui.close();
                    }
                }
            }
            ui.separator();
            if ui.button("Ignore for this session").clicked() {
                env.spell_actions.push(SpellAction::Ignore(word.clone()));
                ui.close();
            }
            if ui.button("Add to dictionary").clicked() {
                env.spell_actions.push(SpellAction::Add(word.clone()));
                ui.close();
            }
        });
    }

    if out.response.has_focus() {
        *env.last_focus = Some(node.id);
    }
    if let Some((_, byte)) = env.goto.take_if(|(n, _)| *n == node.id) {
        let c = CCursor::new(byte_to_char(&node.body, byte));
        let rect = out.galley.pos_from_cursor(c).translate(out.galley_pos.to_vec2());
        ui.scroll_to_rect(rect, Some(Align::Center));
    }
    changed || out.response.changed()
}

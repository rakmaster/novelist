//! Native macOS menu bar via muda.
//!
//! egui has no native menu integration, so these menus are installed once and
//! their events are polled each frame from `App::ui`.

use crate::editor::FormatCmd;
use muda::accelerator::{Accelerator, Code, Modifiers};
use muda::{CheckMenuItem, Menu, MenuEvent, MenuId, MenuItem, PredefinedMenuItem, Submenu};

#[derive(Clone, Copy)]
pub enum MenuAction {
    New,
    Open,
    Save,
    SaveAs,
    Import,
    ExportMarkdown,
    ExportText,
    Quit,
    Undo,
    Find,
    FindReplace,
    CheckSpelling,
    ToggleSpell,
    Settings,
    CleanSpacing,
    Format(FormatCmd),
}

impl MenuAction {
    fn from_id(id: &str) -> Option<Self> {
        Some(match id {
            "new" => Self::New,
            "open" => Self::Open,
            "save" => Self::Save,
            "save_as" => Self::SaveAs,
            "import" => Self::Import,
            "export_md" => Self::ExportMarkdown,
            "export_txt" => Self::ExportText,
            "quit" => Self::Quit,
            "undo" => Self::Undo,
            "find" => Self::Find,
            "replace" => Self::FindReplace,
            "spell" => Self::CheckSpelling,
            "spell_on" => Self::ToggleSpell,
            "settings" => Self::Settings,
            "clean_spacing" => Self::CleanSpacing,
            "fmt_bold" => Self::Format(FormatCmd::Bold),
            "fmt_italic" => Self::Format(FormatCmd::Italic),
            "fmt_underline" => Self::Format(FormatCmd::Underline),
            "fmt_h1" => Self::Format(FormatCmd::Heading(1)),
            "fmt_h2" => Self::Format(FormatCmd::Heading(2)),
            "fmt_h3" => Self::Format(FormatCmd::Heading(3)),
            "fmt_ul" => Self::Format(FormatCmd::UnorderedList),
            "fmt_ol" => Self::Format(FormatCmd::OrderedList),
            _ => return None,
        })
    }
}

pub struct NativeMenu {
    _menu: Menu,
    undo: MenuItem,
    spell_on: CheckMenuItem,
}

impl NativeMenu {
    /// Build the app/File/Edit menus and install them as the application menu.
    /// Must run on the main thread once the app is running; the first `ui()`
    /// frame is a safe point.
    pub fn new() -> Self {
        let menu = Menu::new();
        let cmd = Modifiers::SUPER;
        let accel = |code: Code| Some(Accelerator::new(Some(cmd), code));
        let item = |id: &str, text: &str, acc: Option<Accelerator>| {
            MenuItem::with_id(MenuId::new(id), text, true, acc)
        };
        let sep = PredefinedMenuItem::separator;

        // The first submenu becomes the application menu on macOS.
        let app_menu = Submenu::new("Novelist", true);
        let quit = item("quit", "Quit Novelist", accel(Code::KeyQ));
        let settings = item("settings", "Settings…", accel(Code::Comma));
        let _ = app_menu.append_items(&[
            &PredefinedMenuItem::about(Some("Novelist"), None),
            &sep(),
            &settings,
            &sep(),
            &PredefinedMenuItem::hide(Some("Novelist")),
            &PredefinedMenuItem::hide_others(None),
            &PredefinedMenuItem::show_all(None),
            &sep(),
            &quit,
        ]);

        let file_menu = Submenu::new("File", true);
        let _ = file_menu.append_items(&[
            &item("new", "New", accel(Code::KeyN)),
            &item("open", "Open…", accel(Code::KeyO)),
            &sep(),
            &item("save", "Save", accel(Code::KeyS)),
            &item(
                "save_as",
                "Save As…",
                Some(Accelerator::new(Some(cmd | Modifiers::SHIFT), Code::KeyS)),
            ),
            &sep(),
            &item("import", "Import…", None),
            &sep(),
            &item("export_md", "Export Markdown…", None),
            &item("export_txt", "Export plain text…", None),
        ]);

        let edit_menu = Submenu::new("Edit", true);
        let undo = item("undo", "Undo outline change", accel(Code::KeyZ));
        let spell_on = CheckMenuItem::with_id(
            MenuId::new("spell_on"),
            "Underline misspelled words",
            true,
            true,
            None,
        );
        let _ = edit_menu.append_items(&[
            &undo,
            &sep(),
            &item("find", "Find…", accel(Code::KeyF)),
            &item("replace", "Find & Replace…", accel(Code::KeyR)),
            &sep(),
            &item("spell", "Check spelling…", None),
            &item("clean_spacing", "Clean up manuscript spacing", None),
            &sep(),
            &spell_on,
        ]);

        let format_menu = Submenu::new("Format", true);
        let _ = format_menu.append_items(&[
            &item("fmt_bold", "Bold", accel(Code::KeyB)),
            &item("fmt_italic", "Italic", accel(Code::KeyI)),
            &item("fmt_underline", "Underline", accel(Code::KeyU)),
            &sep(),
            &item("fmt_h1", "Heading 1", None),
            &item("fmt_h2", "Heading 2", None),
            &item("fmt_h3", "Heading 3", None),
            &sep(),
            &item("fmt_ul", "Bulleted List", None),
            &item("fmt_ol", "Numbered List", None),
        ]);

        let _ = menu.append_items(&[&app_menu, &file_menu, &edit_menu, &format_menu]);
        let _ = menu.init_for_nsapp();

        Self { _menu: menu, undo, spell_on }
    }

    /// Reflect app state that can change outside the menu each frame.
    pub fn sync(&self, spell_on: bool, can_undo: bool) {
        self.spell_on.set_checked(spell_on);
        self.undo.set_enabled(can_undo);
    }

    /// Drain pending native menu events into actions.
    pub fn poll(&self) -> Vec<MenuAction> {
        MenuEvent::receiver()
            .try_iter()
            .filter_map(|ev| MenuAction::from_id(ev.id().0.as_str()))
            .collect()
    }
}

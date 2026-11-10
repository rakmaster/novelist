# Novelist

A desktop novel-writing app built in Rust with [egui](https://github.com/emilk/egui)/eframe. Organize your book as an outline of sections, keep character and world notes beside the editor, and write in a distraction-free manuscript view.

## Features

- **Outline editor** — sections arranged in a tree with drag-and-drop rearrangement, collapse/expand, and chapter numbering.
- **Manuscript & section views** — write one section at a time or the whole manuscript continuously.
- **Characters & Background** — side views for character sheets (with portraits) and world entries, each searchable from the manuscript via mentions.
- **Spellcheck** — offline dictionary with character/world names automatically recognized, right-click suggestions, ignore-for-session, add-to-dictionary, and smart handling of possessives (`Ragman's`, `Ragmans'`).
- **Find & Replace** — book-wide search with match highlighting and section jumping.
- **Import & Export** — import Markdown or plain text into an outline; export the book back to Markdown or plain text.
- **Settings** — font size, manuscript layout (double-spaced, first-line indent), theme, and synopsis display.
- **Native macOS menus** — File, Edit, and Settings… (⌘,) live in the Apple menu bar; in-window menus on other platforms.

## Build & Run

Requires a recent stable [Rust toolchain](https://rustup.rs/).

```sh
cargo run        # build and launch
cargo test       # run the test suite
```

## Keyboard shortcuts

| Shortcut | Action |
| --- | --- |
| ⌘/Ctrl+N | New book |
| ⌘/Ctrl+O | Open |
| ⌘/Ctrl+S | Save |
| ⇧⌘/Ctrl+S | Save As |
| ⌘/Ctrl+Z | Undo outline change |
| ⌘/Ctrl+F | Find |
| ⌘/Ctrl+R | Find & Replace |
| ⌘/Ctrl+B / I / U | Bold / Italic / Underline |
| ⌘, (macOS) | Settings |
| Shift+Enter / Enter | Previous / next find match |

## Project layout

```
src/
  app.rs         application shell, menus, panels, windows
  editor.rs      manuscript editor widget and formatting commands
  model.rs       book/document data model
  format.rs      markdown/inline format parsing and serialization
  import.rs      markdown/plain-text outline import
  search.rs      find/replace and mention detection
  spell.rs       dictionary spellcheck, possessive handling
  portrait.rs    character portrait loading/scaling
  macos_menu.rs  native macOS menu bar (muda, macOS only)
  recents.rs     recently opened books for the launcher
  templates.rs   starter document templates
assets/
  en_US.dic/aff  spellcheck dictionary
  fonts/         bundled UI and manuscript fonts
```

Files are saved in the app's own text-based book format (`.novel` by convention) via `save`/`load` in `model.rs`.

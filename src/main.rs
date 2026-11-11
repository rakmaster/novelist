mod app;
mod editor;
mod format;
mod import;
#[cfg(target_os = "macos")]
mod macos_menu;
mod model;
mod portrait;
mod recents;
mod search;
mod spell;
mod templates;
mod theme;

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 820.0])
            .with_title("Novelist"),
        ..Default::default()
    };
    eframe::run_native(
        "Novelist",
        options,
        Box::new(|cc| Ok(Box::new(app::App::new(cc)))),
    )
}

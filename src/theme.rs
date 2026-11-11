//! Apple Human Interface Guidelines look for the app chrome: system colours for
//! light and dark appearance, 13 pt text, 24 pt controls, 6 pt corner radius and
//! an 8 pt spacing grid. The manuscript text keeps its own serif fonts.

use eframe::egui::{
    self, Color32, CornerRadius, FontFamily, FontId, Margin, Stroke, TextStyle, Theme, Vec2,
    epaint::Shadow,
    style::{Selection, WidgetVisuals, Widgets},
};
use std::path::Path;

const RADIUS: u8 = 6;

struct Palette {
    /// Window and panel background.
    window: Color32,
    /// Text fields and content surfaces.
    content: Color32,
    /// Control face at rest, hover, pressed.
    control: Color32,
    control_hover: Color32,
    control_active: Color32,
    separator: Color32,
    control_border: Color32,
    label: Color32,
    secondary_label: Color32,
    accent: Color32,
    destructive: Color32,
    warning: Color32,
    dark: bool,
}

const fn rgb(hex: u32) -> Color32 {
    Color32::from_rgb((hex >> 16) as u8, (hex >> 8) as u8, hex as u8)
}

const LIGHT: Palette = Palette {
    window: rgb(0xF2F2F7),
    content: rgb(0xFFFFFF),
    control: rgb(0xFFFFFF),
    control_hover: rgb(0xF0F0F5),
    control_active: rgb(0xE2E2E8),
    separator: rgb(0xD9D9DE),
    control_border: rgb(0xC9C9CF),
    label: rgb(0x1C1C1E),
    secondary_label: rgb(0x6C6C70),
    accent: rgb(0x007AFF),
    destructive: rgb(0xFF3B30),
    warning: rgb(0xFF9500),
    dark: false,
};

const DARK: Palette = Palette {
    window: rgb(0x1C1C1E),
    content: rgb(0x2C2C2E),
    control: rgb(0x3A3A3C),
    control_hover: rgb(0x48484A),
    control_active: rgb(0x636366),
    separator: rgb(0x38383A),
    control_border: rgb(0x545456),
    label: rgb(0xF2F2F7),
    secondary_label: rgb(0x98989D),
    accent: rgb(0x0A84FF),
    destructive: rgb(0xFF453A),
    warning: rgb(0xFF9F0A),
    dark: true,
};

fn widget(fill: Color32, border: Color32, fg: Color32) -> WidgetVisuals {
    WidgetVisuals {
        bg_fill: fill,
        weak_bg_fill: fill,
        bg_stroke: Stroke::new(1.0, border),
        corner_radius: CornerRadius::same(RADIUS),
        fg_stroke: Stroke::new(1.0, fg),
        expansion: 0.0,
    }
}

fn visuals(p: &Palette) -> egui::Visuals {
    let mut v = if p.dark { egui::Visuals::dark() } else { egui::Visuals::light() };
    v.panel_fill = p.window;
    v.window_fill = p.window;
    v.extreme_bg_color = p.content;
    v.faint_bg_color = p.separator.gamma_multiply(0.35);
    v.code_bg_color = p.separator.gamma_multiply(0.5);
    v.window_stroke = Stroke::new(1.0, p.separator);
    v.window_corner_radius = CornerRadius::same(10);
    v.menu_corner_radius = CornerRadius::same(8);
    v.popup_shadow = Shadow {
        offset: [0, 4],
        blur: 16,
        spread: 0,
        color: Color32::from_black_alpha(if p.dark { 110 } else { 45 }),
    };
    v.window_shadow = Shadow {
        offset: [0, 8],
        blur: 24,
        spread: 0,
        color: Color32::from_black_alpha(if p.dark { 130 } else { 55 }),
    };
    v.hyperlink_color = p.accent;
    v.error_fg_color = p.destructive;
    v.warn_fg_color = p.warning;
    v.override_text_color = None;
    v.weak_text_color = Some(p.secondary_label);
    v.selection = Selection {
        bg_fill: p.accent.gamma_multiply(0.35),
        stroke: Stroke::new(1.0, p.accent),
    };
    v.widgets = Widgets {
        noninteractive: WidgetVisuals {
            weak_bg_fill: p.window,
            bg_fill: p.window,
            bg_stroke: Stroke::new(1.0, p.separator),
            corner_radius: CornerRadius::same(RADIUS),
            fg_stroke: Stroke::new(1.0, p.label),
            expansion: 0.0,
        },
        inactive: widget(p.control, p.control_border, p.label),
        hovered: widget(p.control_hover, p.control_border, p.label),
        active: widget(p.control_active, p.accent, p.label),
        open: widget(p.control_active, p.control_border, p.label),
    };
    v.widgets.hovered.bg_stroke = Stroke::new(1.0, p.accent.gamma_multiply(0.6));
    v
}

fn style(s: &mut egui::Style) {
    s.spacing.item_spacing = Vec2::new(8.0, 6.0);
    s.spacing.button_padding = Vec2::new(10.0, 4.0);
    s.spacing.interact_size = Vec2::new(40.0, 24.0);
    s.spacing.window_margin = Margin::same(16);
    s.spacing.menu_margin = Margin::same(6);
    s.spacing.indent = 16.0;
    s.text_styles = [
        (TextStyle::Heading, FontId::new(17.0, FontFamily::Proportional)),
        (TextStyle::Body, FontId::new(13.0, FontFamily::Proportional)),
        (TextStyle::Button, FontId::new(13.0, FontFamily::Proportional)),
        (TextStyle::Small, FontId::new(11.0, FontFamily::Proportional)),
        (TextStyle::Monospace, FontId::new(12.0, FontFamily::Monospace)),
    ]
    .into();
}

/// Install the Apple-style visuals for both appearances.
pub fn apply(ctx: &egui::Context) {
    for (theme, palette) in [(Theme::Light, &LIGHT), (Theme::Dark, &DARK)] {
        ctx.style_mut_of(theme, style);
        ctx.set_visuals_of(theme, visuals(palette));
    }
}

/// Put the macOS system UI font (SF) first in the proportional family when it
/// exists on this machine; otherwise egui's bundled font is used.
pub fn add_system_ui_font(fonts: &mut egui::FontDefinitions) {
    const SF: &str = "/System/Library/Fonts/SFNS.ttf";
    if !Path::new(SF).exists() {
        return;
    }
    let Ok(bytes) = std::fs::read(SF) else { return };
    fonts
        .font_data
        .insert("system-ui".into(), std::sync::Arc::new(egui::FontData::from_owned(bytes)));
    if let Some(list) = fonts.families.get_mut(&FontFamily::Proportional) {
        list.insert(0, "system-ui".into());
    }
}

//! Colour themes. egui's default light theme is grey-on-grey with borderless
//! buttons, which makes the window look disabled; this one has dark text,
//! outlined controls and a blue accent.

use eframe::egui::{self, Color32, CornerRadius, Stroke, Visuals};

const TEXT: Color32 = Color32::from_rgb(0x1B, 0x1F, 0x24);
const PANEL: Color32 = Color32::from_rgb(0xEE, 0xF0, 0xF3);
const CONTROL: Color32 = Color32::from_rgb(0xFF, 0xFF, 0xFF);
const BORDER: Color32 = Color32::from_rgb(0xA9, 0xB1, 0xBB);
const BORDER_SOFT: Color32 = Color32::from_rgb(0xC9, 0xCF, 0xD6);
const ACCENT: Color32 = Color32::from_rgb(0x1F, 0x6F, 0xEB);
const ACCENT_BG: Color32 = Color32::from_rgb(0xDD, 0xEA, 0xFC);

pub fn light() -> Visuals {
    let mut v = Visuals::light();
    let radius = CornerRadius::same(3);

    v.panel_fill = PANEL;
    v.window_fill = Color32::from_rgb(0xF7, 0xF8, 0xFA);
    v.window_stroke = Stroke::new(1.0, BORDER);
    v.extreme_bg_color = Color32::WHITE; // text boxes and data views
    v.faint_bg_color = Color32::from_rgb(0xE6, 0xE9, 0xED); // ruler
    v.code_bg_color = Color32::from_rgb(0xE6, 0xE9, 0xED);
    v.warn_fg_color = Color32::from_rgb(0xB4, 0x5A, 0x00);
    v.error_fg_color = Color32::from_rgb(0xC6, 0x28, 0x28);
    v.hyperlink_color = ACCENT;

    // Labels and separators.
    let w = &mut v.widgets;
    w.noninteractive.fg_stroke = Stroke::new(1.0, TEXT);
    w.noninteractive.bg_stroke = Stroke::new(1.0, BORDER_SOFT);
    w.noninteractive.bg_fill = PANEL;
    w.noninteractive.weak_bg_fill = PANEL;

    // Buttons, checkboxes, combo boxes at rest: white with a visible border.
    w.inactive.bg_fill = CONTROL;
    w.inactive.weak_bg_fill = CONTROL;
    w.inactive.bg_stroke = Stroke::new(1.0, BORDER);
    w.inactive.fg_stroke = Stroke::new(1.0, TEXT);
    w.inactive.corner_radius = radius;

    // Hovered: light blue with an accent border.
    w.hovered.bg_fill = ACCENT_BG;
    w.hovered.weak_bg_fill = ACCENT_BG;
    w.hovered.bg_stroke = Stroke::new(1.0, ACCENT);
    w.hovered.fg_stroke = Stroke::new(1.5, TEXT);
    w.hovered.corner_radius = radius;

    // Pressed, and open menus/combo boxes.
    let pressed = Color32::from_rgb(0xC4, 0xDA, 0xFA);
    w.active.bg_fill = pressed;
    w.active.weak_bg_fill = pressed;
    w.active.bg_stroke = Stroke::new(1.0, ACCENT);
    w.active.fg_stroke = Stroke::new(1.5, TEXT);
    w.active.corner_radius = radius;
    w.open.bg_fill = ACCENT_BG;
    w.open.weak_bg_fill = ACCENT_BG;
    w.open.bg_stroke = Stroke::new(1.0, ACCENT);
    w.open.fg_stroke = Stroke::new(1.0, TEXT);
    w.open.corner_radius = radius;

    // Selected toggles (DTR/RTS on), text selection, focus ring.
    v.selection.bg_fill = Color32::from_rgb(0xA8, 0xC8, 0xF8);
    v.selection.stroke = Stroke::new(1.5, ACCENT);
    v.text_cursor.stroke = Stroke::new(2.0, ACCENT);

    v
}

/// Install the custom themes. The dark one is egui's default.
pub fn install(ctx: &egui::Context) {
    ctx.set_visuals_of(egui::Theme::Light, light());
}

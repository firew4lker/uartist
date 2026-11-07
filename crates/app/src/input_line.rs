//! The chip-style input line (`docs/spec/input.md`).
//!
//! Each token is drawn as a coloured chip. Typed characters go to the core
//! `InputLine`, which filters them for the current type.

use eframe::egui::{
    self, Align2, Color32, CornerRadius, EventFilter, FontId, Key, Rect, Sense, Stroke, StrokeKind,
    Ui, pos2, vec2,
};
use hterm_core::format::Format;
use hterm_core::input::{History, InputLine, TokenKind};

use crate::view::{FONT_SIZE, format_color};

/// What the user did in the input line this frame.
#[derive(Debug, PartialEq, Eq)]
pub enum InputAction {
    None,
    /// Enter was pressed.
    Submit,
}

fn kind_color(kind: TokenKind, dark: bool) -> Color32 {
    match kind {
        TokenKind::Asc => format_color(Format::Ascii, dark),
        TokenKind::Hex => format_color(Format::Hex, dark),
        TokenKind::Dec => format_color(Format::Dec, dark),
        TokenKind::Bin => format_color(Format::Bin, dark),
        TokenKind::Cmd if dark => Color32::from_rgb(0x6A, 0x4A, 0x14),
        TokenKind::Cmd => Color32::from_rgb(0xF6, 0xC8, 0x80),
    }
}

/// Draw and handle the input line. `history` is used for Up/Down.
pub fn input_line_ui(
    ui: &mut Ui,
    line: &mut InputLine,
    kind: TokenKind,
    history: &mut History,
) -> InputAction {
    let font = FontId::monospace(FONT_SIZE);
    let row_h = ui.ctx().fonts_mut(|f| f.row_height(&font));
    let height = row_h + 10.0;
    let (rect, response) =
        ui.allocate_exact_size(vec2(ui.available_width(), height), Sense::click());
    let id = response.id;

    if response.clicked() {
        response.request_focus();
    }
    let focused = response.has_focus();
    let mut action = InputAction::None;

    if focused {
        // Keep arrow keys and Tab for ourselves instead of moving focus.
        ui.memory_mut(|m| {
            m.set_focus_lock_filter(
                id,
                EventFilter {
                    tab: true,
                    horizontal_arrows: true,
                    vertical_arrows: true,
                    escape: false,
                },
            )
        });
        let events = ui.input(|i| i.events.clone());
        for event in events {
            match event {
                egui::Event::Text(t) | egui::Event::Paste(t) => line.type_str(kind, &t),
                egui::Event::Key {
                    key, pressed: true, ..
                } => match key {
                    Key::Backspace => line.backspace(),
                    Key::Enter => action = InputAction::Submit,
                    Key::ArrowUp => {
                        if let Some(old) = history.older() {
                            *line = old.clone();
                        }
                    }
                    Key::ArrowDown => match history.newer() {
                        Some(newer) => *line = newer.clone(),
                        None => line.clear(),
                    },
                    Key::Escape => line.clear(),
                    _ => {}
                },
                _ => {}
            }
        }
    }

    let visuals = ui.style().interact(&response);
    let painter = ui.painter_at(rect);
    painter.rect(
        rect,
        CornerRadius::same(2),
        ui.visuals().extreme_bg_color,
        if focused {
            ui.visuals().selection.stroke
        } else {
            visuals.bg_stroke
        },
        StrokeKind::Inside,
    );

    let dark = ui.visuals().dark_mode;
    let text_color = ui.visuals().strong_text_color();
    let mut x = rect.min.x + 6.0;
    let y = rect.center().y;
    for token in line.tokens() {
        let galley = painter.layout_no_wrap(token.display(), font.clone(), text_color);
        let chip = Rect::from_min_size(
            pos2(x, y - row_h / 2.0 - 1.0),
            vec2(galley.size().x + 6.0, row_h + 2.0),
        );
        painter.rect_filled(chip, 2.0, kind_color(token.kind, dark));
        painter.galley(pos2(x + 3.0, y - galley.size().y / 2.0), galley, text_color);
        x = chip.max.x + 4.0;
    }
    if focused {
        // Cursor, blinking.
        let t = ui.input(|i| i.time);
        if (t * 2.0) as i64 % 2 == 0 {
            painter.line_segment(
                [pos2(x, y - row_h / 2.0), pos2(x, y + row_h / 2.0)],
                Stroke::new(1.5, text_color),
            );
        }
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_millis(500));
    } else if line.is_empty() {
        painter.text(
            pos2(rect.min.x + 6.0, y),
            Align2::LEFT_CENTER,
            "Click here and type; Enter sends",
            FontId::proportional(FONT_SIZE),
            ui.visuals().weak_text_color(),
        );
    }

    action
}

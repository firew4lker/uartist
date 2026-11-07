//! A data view (received or transmitted): lines from the core line engine,
//! wrapped to the window width and drawn in ASCII or grid mode.
//!
//! See `docs/spec/views.md`.

use std::time::Instant;

use eframe::egui::{
    self, Align2, Color32, FontId, Pos2, Rect, Sense, Stroke, StrokeKind, Ui, Vec2, pos2, vec2,
};
use hterm_core::format::{AsciiGlyph, Format, Formats, ascii_glyph, cell_text};
use hterm_core::lines::{Cell, LineBuilder, LineRules};
use hterm_core::log::{DataLog, Direction};

/// Font size of the data views.
pub const FONT_SIZE: f32 = 13.0;

/// One wrapped display row: a slice of a line.
#[derive(Debug, Clone, Copy)]
struct Row {
    line: u32,
    start: u32,
    len: u32,
}

pub struct DataView {
    dir: Direction,
    builder: LineBuilder,
    /// Wrapped rows for the current width.
    rows: Vec<Row>,
    /// Columns the rows were wrapped for (0 = needs a full rewrap).
    cols: usize,
}

impl DataView {
    pub fn new(dir: Direction, rules: LineRules) -> Self {
        Self {
            dir,
            builder: LineBuilder::new(rules),
            rows: Vec::new(),
            cols: 0,
        }
    }

    pub fn rules(&self) -> &LineRules {
        self.builder.rules()
    }

    /// Rebuild all lines from the log (after a rule change, clear or trim).
    pub fn rebuild(&mut self, rules: LineRules, log: &DataLog) {
        self.builder = LineBuilder::from_chunks(rules, log.chunks_for(Some(self.dir)));
        self.cols = 0;
    }

    /// Add new data. Only the rows of changed lines are rewrapped.
    pub fn push(&mut self, dir: Direction, at: Instant, data: &[u8]) {
        if dir != self.dir {
            return;
        }
        // Lines before the one being filled are complete and won't change.
        let first_changed = self.builder.lines().len() - 1;
        self.builder.push_chunk(dir, at, data);
        if self.cols > 0 {
            let keep = self
                .rows
                .partition_point(|r| (r.line as usize) < first_changed);
            self.rows.truncate(keep);
            wrap_lines(&self.builder, first_changed, self.cols, &mut self.rows);
        }
    }

    pub fn ui(&mut self, ui: &mut Ui, formats: &Formats, autoscroll: bool) {
        let font = FontId::monospace(FONT_SIZE);
        let (char_w, row_h) = ui
            .ctx()
            .fonts_mut(|f| (f.glyph_width(&font, '0'), f.row_height(&font)));
        let grid = formats.is_grid();
        let enabled: Vec<Format> = formats.enabled().collect();
        let enabled = if enabled.is_empty() {
            vec![Format::Ascii]
        } else {
            enabled
        };
        let cell_chars = if grid {
            enabled.iter().map(|f| f.cell_width()).max().unwrap_or(1) + 1
        } else {
            1
        };
        let cell_w = cell_chars as f32 * char_w;
        let pad = 4.0;
        let scrollbar = ui.spacing().scroll.bar_width + 4.0;
        let cols =
            (((ui.available_width() - 2.0 * pad - scrollbar) / cell_w).floor() as usize).max(1);

        if cols != self.cols {
            self.cols = cols;
            self.rows.clear();
            wrap_lines(&self.builder, 0, cols, &mut self.rows);
        }

        let frame = egui::Frame::new()
            .fill(ui.visuals().extreme_bg_color)
            .stroke(ui.visuals().widgets.noninteractive.bg_stroke)
            .inner_margin(pad);
        frame.show(ui, |ui| {
            ruler(ui, &font, char_w, row_h, cell_w, cols, grid);

            let block_h = if grid {
                enabled.len() as f32 * (row_h + 2.0) + 4.0
            } else {
                row_h
            };
            let lines = self.builder.lines();
            let rows = &self.rows;
            egui::ScrollArea::vertical()
                .id_salt(("view", self.dir))
                .auto_shrink(false)
                .stick_to_bottom(autoscroll)
                .show_rows(ui, block_h, rows.len(), |ui, range| {
                    ui.spacing_mut().item_spacing.y = 0.0;
                    for row in &rows[range] {
                        let line = &lines[row.line as usize];
                        let cells = &line[row.start as usize..(row.start + row.len) as usize];
                        let (rect, _) = ui.allocate_exact_size(
                            vec2(ui.available_width(), block_h),
                            Sense::hover(),
                        );
                        if grid {
                            draw_grid_row(ui, rect, cells, &enabled, &font, cell_w, row_h);
                        } else {
                            draw_ascii_row(ui, rect, cells, &font, cell_w);
                        }
                    }
                });
        });
    }
}

/// Wrap lines `from..` into rows of at most `cols` visible cells.
fn wrap_lines(builder: &LineBuilder, from: usize, cols: usize, rows: &mut Vec<Row>) {
    let lines = builder.lines();
    let last = lines.len() - 1;
    for (i, line) in lines.iter().enumerate().skip(from) {
        // Hidden cells (a hidden newline sequence) are always at the end.
        let visible = line.iter().take_while(|c| !c.hidden).count();
        if visible == 0 {
            // Keep an empty row for a line that was only a hidden newline,
            // but not for the (empty) line still being filled.
            if i != last {
                rows.push(Row {
                    line: i as u32,
                    start: 0,
                    len: 0,
                });
            }
            continue;
        }
        let mut start = 0;
        while start < visible {
            let len = (visible - start).min(cols);
            rows.push(Row {
                line: i as u32,
                start: start as u32,
                len: len as u32,
            });
            start += len;
        }
    }
}

fn ruler(
    ui: &mut Ui,
    font: &FontId,
    char_w: f32,
    row_h: f32,
    cell_w: f32,
    cols: usize,
    grid: bool,
) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), row_h + 2.0), Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 0.0, ui.visuals().faint_bg_color);
    let color = ui.visuals().weak_text_color();
    for col in 1..=cols {
        let label = if grid { true } else { col == 1 || col % 5 == 0 };
        if label {
            let x = if grid {
                rect.min.x + (col - 1) as f32 * cell_w + (cell_w - char_w) / 2.0
            } else {
                rect.min.x + (col - 1) as f32 * cell_w
            };
            let anchor = if grid {
                Align2::CENTER_TOP
            } else {
                Align2::LEFT_TOP
            };
            let x = if grid { x + char_w / 2.0 } else { x };
            painter.text(pos2(x, rect.min.y + 1.0), anchor, col, font.clone(), color);
        }
    }
}

fn draw_ascii_row(ui: &Ui, rect: Rect, cells: &[Cell], font: &FontId, cell_w: f32) {
    let painter = ui.painter_at(rect);
    let text = ui.visuals().text_color();
    let weak = ui.visuals().weak_text_color();
    for (i, cell) in cells.iter().enumerate() {
        let r = Rect::from_min_size(
            pos2(rect.min.x + i as f32 * cell_w, rect.min.y),
            vec2(cell_w, rect.height()),
        );
        draw_ascii_cell(&painter, r, cell.byte, font, text, weak);
    }
}

fn draw_ascii_cell(
    painter: &egui::Painter,
    r: Rect,
    byte: u8,
    font: &FontId,
    text: Color32,
    weak: Color32,
) {
    match ascii_glyph(byte) {
        AsciiGlyph::Char(c) => {
            painter.text(r.center(), Align2::CENTER_CENTER, c, font.clone(), text);
        }
        AsciiGlyph::Control(sym) => {
            // Small two-character symbol squeezed into one cell.
            let small = FontId::monospace(font.size * 0.55);
            painter.text(
                r.center() + vec2(0.0, 2.0),
                Align2::CENTER_CENTER,
                sym,
                small,
                weak,
            );
        }
        AsciiGlyph::Unprintable => {
            let b =
                Rect::from_center_size(r.center(), Vec2::splat(r.width().min(r.height()) * 0.55));
            painter.rect_stroke(b, 0.0, Stroke::new(1.0, weak), StrokeKind::Inside);
        }
    }
}

/// Grid row colours, like HTerm: cyan, blue, pink, green.
pub fn format_color(f: Format, dark: bool) -> Color32 {
    match (f, dark) {
        (Format::Ascii, false) => Color32::from_rgb(0xB4, 0xEC, 0xEA),
        (Format::Hex, false) => Color32::from_rgb(0xC8, 0xC8, 0xF6),
        (Format::Dec, false) => Color32::from_rgb(0xF8, 0xC4, 0xC4),
        (Format::Bin, false) => Color32::from_rgb(0xC4, 0xF4, 0xC0),
        (Format::Ascii, true) => Color32::from_rgb(0x1E, 0x4E, 0x50),
        (Format::Hex, true) => Color32::from_rgb(0x32, 0x34, 0x66),
        (Format::Dec, true) => Color32::from_rgb(0x5E, 0x2E, 0x34),
        (Format::Bin, true) => Color32::from_rgb(0x2C, 0x52, 0x2C),
    }
}

fn draw_grid_row(
    ui: &Ui,
    rect: Rect,
    cells: &[Cell],
    formats: &[Format],
    font: &FontId,
    cell_w: f32,
    row_h: f32,
) {
    let painter = ui.painter_at(rect);
    let dark = ui.visuals().dark_mode;
    let text = ui.visuals().strong_text_color();
    let weak = ui.visuals().weak_text_color();
    for (i, cell) in cells.iter().enumerate() {
        let x = rect.min.x + i as f32 * cell_w;
        for (k, &f) in formats.iter().enumerate() {
            let y = rect.min.y + k as f32 * (row_h + 2.0);
            let r = Rect::from_min_size(Pos2::new(x, y), vec2(cell_w - 2.0, row_h));
            painter.rect_filled(r, 2.0, format_color(f, dark));
            if f == Format::Ascii {
                draw_ascii_cell(&painter, r, cell.byte, font, text, weak);
            } else {
                painter.text(
                    r.center(),
                    Align2::CENTER_CENTER,
                    cell_text(f, cell.byte),
                    font.clone(),
                    text,
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wrapping_and_incremental_push_agree() {
        let rules = LineRules {
            newline_at: b"\n".to_vec(),
            show_newline: false,
            ..Default::default()
        };
        let mut log = DataLog::new();
        let mut view = DataView::new(Direction::Rx, rules.clone());
        view.cols = 4;
        let now = Instant::now();
        for chunk in [&b"abcdefghij\n"[..], b"\n", b"xy", b"z12345"] {
            log.push(Direction::Rx, now, chunk.to_vec());
            view.push(Direction::Rx, now, chunk);
        }
        let incremental: Vec<_> = view.rows.iter().map(|r| (r.line, r.start, r.len)).collect();

        let mut fresh = Vec::new();
        let b = LineBuilder::from_chunks(rules, log.chunks());
        wrap_lines(&b, 0, 4, &mut fresh);
        let full: Vec<_> = fresh.iter().map(|r| (r.line, r.start, r.len)).collect();

        assert_eq!(incremental, full);
        // "abcdefghij" -> 3 rows, hidden-only line -> 1 empty row, "xyz12345" -> 2 rows
        assert_eq!(full.len(), 6);
    }
}

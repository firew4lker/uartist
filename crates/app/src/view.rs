//! A data view (received or transmitted): lines from the core line engine,
//! wrapped to the window width and drawn in ASCII or grid mode.
//!
//! See `docs/spec/views.md`.

use std::time::Instant;

use eframe::egui::{
    self, Align2, Color32, FontId, Pos2, Rect, Sense, Stroke, StrokeKind, Ui, Vec2, pos2, vec2,
};
use uartist_core::format::{AsciiGlyph, Format, Formats, ascii_glyph, cell_text, copy_text};
use uartist_core::lines::{Cell, LineBuilder, LineRules};
use uartist_core::log::{DataLog, Direction};

/// Font size of the data views.
pub const FONT_SIZE: f32 = 13.0;

/// One wrapped display row: a slice of a line.
#[derive(Debug, Clone, Copy)]
struct Row {
    line: u32,
    start: u32,
    len: u32,
}

/// Selected bytes, as positions in the view's byte stream (all bytes of this
/// direction, in order). `anchor` is where the selection started.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Selection {
    anchor: u64,
    cursor: u64,
}

impl Selection {
    fn range(self) -> (u64, u64) {
        (self.anchor.min(self.cursor), self.anchor.max(self.cursor))
    }
}

/// What happened in a view this frame.
#[derive(Default)]
pub struct ViewResponse {
    /// The user clicked into this view (it becomes the one Ctrl+C copies from).
    pub pressed: bool,
    /// Text to put on the clipboard.
    pub copy: Option<String>,
}

pub struct DataView {
    dir: Direction,
    /// Show both directions (HTerm's "Combined view transmit and receive").
    /// Set it, then call `rebuild`.
    pub combined: bool,
    builder: LineBuilder,
    /// Position of each line's first byte in the view's byte stream.
    line_offsets: Vec<u64>,
    /// Wrapped rows for the current width.
    rows: Vec<Row>,
    /// Columns the rows were wrapped for (0 = needs a full rewrap).
    cols: usize,
    selection: Option<Selection>,
    dragging: bool,
    /// Selection bar text, cached per selection: (selection, left, right).
    bar: Option<(Selection, String, String)>,
}

impl DataView {
    pub fn new(dir: Direction, rules: LineRules) -> Self {
        Self {
            dir,
            combined: false,
            builder: LineBuilder::new(rules),
            line_offsets: vec![0],
            rows: Vec::new(),
            cols: 0,
            selection: None,
            dragging: false,
            bar: None,
        }
    }

    pub fn rules(&self) -> &LineRules {
        self.builder.rules()
    }

    /// Rebuild all lines from the log. `keep_selection` is false when the
    /// byte stream itself changed (clear or trim), so old positions are wrong.
    pub fn rebuild(&mut self, rules: LineRules, log: &DataLog, keep_selection: bool) {
        self.builder = LineBuilder::from_chunks(rules, log.chunks_for(self.filter()));
        self.line_offsets = vec![0];
        self.update_offsets(0);
        self.cols = 0;
        if !keep_selection {
            self.selection = None;
            self.dragging = false;
        }
        self.bar = None;
    }

    /// Recompute line offsets for lines after `from`.
    fn update_offsets(&mut self, from: usize) {
        let lines = self.builder.lines();
        self.line_offsets.truncate(from + 1);
        for i in from + 1..lines.len() {
            let prev = self.line_offsets[i - 1] + lines[i - 1].len() as u64;
            self.line_offsets.push(prev);
        }
    }

    /// Add new data. Only the rows of changed lines are rewrapped.
    pub fn push(&mut self, dir: Direction, at: Instant, data: &[u8]) {
        if self.filter().is_some_and(|d| d != dir) {
            return;
        }
        // Lines before the one being filled are complete and won't change.
        let first_changed = self.builder.lines().len() - 1;
        self.builder.push_chunk(dir, at, data);
        self.update_offsets(first_changed);
        if self.cols > 0 {
            let keep = self
                .rows
                .partition_point(|r| (r.line as usize) < first_changed);
            self.rows.truncate(keep);
            wrap_lines(&self.builder, first_changed, self.cols, &mut self.rows);
        }
    }

    /// Which data this view shows: one direction, or both when combined.
    fn filter(&self) -> Option<Direction> {
        (!self.combined).then_some(self.dir)
    }

    /// The selected bytes, if any.
    pub fn selected_bytes(&self) -> Option<Vec<u8>> {
        let (a, b) = self.selection?.range();
        let lines = self.builder.lines();
        // Line holding byte `a`.
        let mut li = self.line_offsets.partition_point(|&o| o <= a) - 1;
        let mut out = Vec::with_capacity((b - a + 1) as usize);
        let mut pos = a;
        while pos <= b && li < lines.len() {
            let off = self.line_offsets[li];
            let line = &lines[li];
            let from = (pos - off) as usize;
            let to = ((b - off) as usize + 1).min(line.len());
            out.extend(line[from.min(line.len())..to].iter().map(|c| c.byte));
            pos = off + line.len() as u64;
            li += 1;
        }
        Some(out)
    }

    /// Copy the selection in `format`.
    pub fn copy(&self, format: Format) -> Option<String> {
        self.selected_bytes().map(|b| copy_text(&b, format))
    }

    pub fn ui(
        &mut self,
        ui: &mut Ui,
        formats: &Formats,
        autoscroll: bool,
        log: &DataLog,
    ) -> ViewResponse {
        let mut response = ViewResponse::default();
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

        let (pointer, pressed, down, shift) = ui.input(|i| {
            (
                i.pointer.interact_pos(),
                i.pointer.primary_pressed(),
                i.pointer.primary_down(),
                i.modifiers.shift,
            )
        });

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
            // Leave room for the selection bar.
            // Exactly fit the space we have: content taller than that would
            // make a resizable panel grow every frame.
            let bar_h = row_h + 6.0;
            let gap = ui.spacing().item_spacing.y;
            let scroll_h = (ui.available_height() - bar_h - gap).max(block_h);
            let lines = self.builder.lines();
            let rows = &self.rows;
            let offsets = &self.line_offsets;
            let sel = self.selection.map(Selection::range);
            let combined = self.combined;
            let sel_color = ui.visuals().selection.bg_fill;
            // Grid cells are already coloured, so outline the selection in the
            // strong accent colour instead of filling it.
            let sel_outline = ui.visuals().selection.stroke.color;
            let mut visible: Vec<(Rect, Row)> = Vec::new();
            let out = egui::ScrollArea::vertical()
                .id_salt(("view", self.dir))
                .auto_shrink(false)
                .max_height(scroll_h)
                .stick_to_bottom(autoscroll && !self.dragging)
                .show_rows(ui, block_h, rows.len(), |ui, range| {
                    ui.spacing_mut().item_spacing.y = 0.0;
                    for row in &rows[range] {
                        let line = &lines[row.line as usize];
                        let cells = &line[row.start as usize..(row.start + row.len) as usize];
                        let (rect, _) = ui.allocate_exact_size(
                            vec2(ui.available_width(), block_h),
                            Sense::hover(),
                        );
                        let row_abs = offsets[row.line as usize] + row.start as u64;
                        let selected = |i: usize| {
                            sel.is_some_and(|(a, b)| (a..=b).contains(&(row_abs + i as u64)))
                        };
                        if !grid {
                            for i in (0..cells.len()).filter(|&i| selected(i)) {
                                let r = Rect::from_min_size(
                                    pos2(rect.min.x + i as f32 * cell_w, rect.min.y),
                                    vec2(cell_w, rect.height()),
                                );
                                ui.painter().rect_filled(r, 0.0, sel_color);
                            }
                            draw_ascii_row(ui, rect, cells, &font, cell_w, combined);
                        } else {
                            draw_grid_row(ui, rect, cells, &enabled, &font, cell_w, row_h);
                            if combined {
                                // Mark sent bytes with a bar under the column.
                                let color = ui.visuals().selection.stroke.color;
                                for (i, cell) in cells.iter().enumerate() {
                                    if cell.dir == Direction::Tx {
                                        let x = rect.min.x + i as f32 * cell_w;
                                        let bar = Rect::from_min_max(
                                            pos2(x, rect.max.y - 3.0),
                                            pos2(x + cell_w - 2.0, rect.max.y - 1.0),
                                        );
                                        ui.painter().rect_filled(bar, 0.0, color);
                                    }
                                }
                            }
                            for i in (0..cells.len()).filter(|&i| selected(i)) {
                                let r = Rect::from_min_size(
                                    pos2(rect.min.x + i as f32 * cell_w - 1.0, rect.min.y - 1.0),
                                    vec2(cell_w, block_h - 2.0),
                                );
                                ui.painter().rect_stroke(
                                    r,
                                    2.0,
                                    Stroke::new(2.0, sel_outline),
                                    StrokeKind::Inside,
                                );
                            }
                        }
                        visible.push((rect, *row));
                    }
                });

            // Mouse: click selects one byte, Shift+click or drag extends.
            let area = ui.interact(
                out.inner_rect,
                ui.scope_id().with(("view_area", self.dir)),
                Sense::click(),
            );
            let locate = |p: Pos2| locate_cell(&visible, offsets, cell_w, p);
            if pressed
                && area.contains_pointer()
                && let Some(idx) = pointer.and_then(locate)
            {
                response.pressed = true;
                self.selection = Some(match self.selection {
                    Some(s) if shift => Selection { cursor: idx, ..s },
                    _ => Selection {
                        anchor: idx,
                        cursor: idx,
                    },
                });
                self.dragging = true;
            } else if self.dragging && down {
                if let (Some(idx), Some(s)) = (pointer.and_then(locate), &mut self.selection) {
                    s.cursor = idx;
                }
            } else if !down {
                self.dragging = false;
            }

            area.context_menu(|ui| {
                let has = self.selection.is_some();
                for (label, f) in [
                    ("Copy as Ascii", Format::Ascii),
                    ("Copy as Hex", Format::Hex),
                    ("Copy as Decimal", Format::Dec),
                    ("Copy as Binary", Format::Bin),
                ] {
                    if ui.add_enabled(has, egui::Button::new(label)).clicked() {
                        response.copy = self.copy(f);
                        ui.close();
                    }
                }
            });

            self.selection_bar(ui, log, &font);
        });
        response
    }

    fn selection_bar(&mut self, ui: &mut Ui, log: &DataLog, font: &FontId) {
        let (left, right) = match self.selection {
            None => ("Selection (-)".to_string(), String::new()),
            Some(s) => {
                if self.bar.as_ref().map(|b| b.0) != Some(s) {
                    let (a, b) = s.range();
                    let times = byte_times(log, self.filter(), a, b);
                    let (diff, stamp) = match times {
                        Some((ta, tb)) => (
                            fmt_duration(tb.saturating_duration_since(ta)),
                            fmt_clock(log.wall_time(tb)),
                        ),
                        None => ("-".into(), "-".into()),
                    };
                    self.bar = Some((
                        s,
                        format!("Selection ({a}:{b}) {}  -  Timediff {diff}", b - a + 1),
                        format!("Timestamp: {stamp}"),
                    ));
                }
                let bar = self.bar.as_ref().expect("just set");
                (bar.1.clone(), bar.2.clone())
            }
        };
        // Fixed height, painted directly, so the view's size never changes.
        let row_h = ui.ctx().fonts_mut(|f| f.row_height(font));
        let (rect, _) =
            ui.allocate_exact_size(vec2(ui.available_width(), row_h + 6.0), Sense::hover());
        let painter = ui.painter_at(rect);
        let color = ui.visuals().weak_text_color();
        painter.line_segment(
            [rect.left_top(), rect.right_top()],
            ui.visuals().widgets.noninteractive.bg_stroke,
        );
        let y = rect.center().y + 2.0;
        painter.text(
            pos2(rect.min.x + 2.0, y),
            Align2::LEFT_CENTER,
            left,
            font.clone(),
            color,
        );
        painter.text(
            pos2(rect.max.x - 2.0, y),
            Align2::RIGHT_CENTER,
            right,
            font.clone(),
            color,
        );
    }
}

/// Which byte (stream position) is under `p`, using the rows drawn this frame.
/// Points above or below the visible rows snap to the first/last row.
fn locate_cell(visible: &[(Rect, Row)], offsets: &[u64], cell_w: f32, p: Pos2) -> Option<u64> {
    let (first, last) = (visible.first()?, visible.last()?);
    let (rect, row) = if p.y < first.0.min.y {
        first
    } else if p.y >= last.0.max.y {
        last
    } else {
        visible
            .iter()
            .find(|(r, _)| p.y >= r.min.y && p.y < r.max.y)?
    };
    let col = ((p.x - rect.min.x) / cell_w).floor().max(0.0) as u32;
    let col = col.min(row.len.saturating_sub(1));
    Some(offsets[row.line as usize] + (row.start + col) as u64)
}

/// Arrival times of the chunks holding bytes `a` and `b` of this direction.
fn byte_times(
    log: &DataLog,
    filter: Option<Direction>,
    a: u64,
    b: u64,
) -> Option<(Instant, Instant)> {
    let mut pos = 0u64;
    let mut ta = None;
    for c in log.chunks_for(filter) {
        let end = pos + c.data.len() as u64;
        if ta.is_none() && a < end {
            ta = Some(c.at);
        }
        if b < end {
            return ta.map(|ta| (ta, c.at));
        }
        pos = end;
    }
    None
}

/// `HH:MM:SS.mmm.uuu`, like HTerm.
fn fmt_duration(d: std::time::Duration) -> String {
    let us = d.as_micros();
    let (h, m, s) = (
        us / 3_600_000_000,
        us / 60_000_000 % 60,
        us / 1_000_000 % 60,
    );
    format!(
        "{h:02}:{m:02}:{s:02}.{:03}.{:03}",
        us / 1000 % 1000,
        us % 1000
    )
}

/// Local clock time `HH:MM:SS.mmm.uuu`, like HTerm.
fn fmt_clock(t: std::time::SystemTime) -> String {
    let dt = chrono::DateTime::<chrono::Local>::from(t);
    let us = dt.timestamp_subsec_micros();
    format!(
        "{}.{:03}.{:03}",
        dt.format("%H:%M:%S"),
        us / 1000,
        us % 1000
    )
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

fn draw_ascii_row(
    ui: &Ui,
    rect: Rect,
    cells: &[Cell],
    font: &FontId,
    cell_w: f32,
    underline_tx: bool,
) {
    let painter = ui.painter_at(rect);
    if underline_tx {
        // Combined view: underline sent bytes, like HTerm.
        let stroke = Stroke::new(2.0, ui.visuals().selection.stroke.color);
        for (i, cell) in cells.iter().enumerate() {
            if cell.dir == Direction::Tx {
                let x = rect.min.x + i as f32 * cell_w;
                let y = rect.max.y - 1.0;
                painter.line_segment([pos2(x, y), pos2(x + cell_w, y)], stroke);
            }
        }
    }
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
            if r.width() >= font.size * 1.5 {
                // Wide grid cell: room for the symbol at full size.
                painter.text(r.center(), Align2::CENTER_CENTER, sym, font.clone(), weak);
            } else {
                // One character cell: two small letters stacked, like the
                // control pictures of hardware terminals (CR, LF, NU).
                let (top, bottom) = match byte {
                    b'\r' => ("C", "R"),
                    b'\n' => ("L", "F"),
                    _ => ("N", "U"),
                };
                let small = FontId::monospace(font.size * 0.52);
                let x = r.center().x;
                painter.text(
                    pos2(x, r.min.y),
                    Align2::CENTER_TOP,
                    top,
                    small.clone(),
                    weak,
                );
                painter.text(pos2(x, r.max.y), Align2::CENTER_BOTTOM, bottom, small, weak);
            }
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
    // On the coloured cells the usual weak grey is too faint, especially in
    // light mode: use the text colour, slightly faded.
    let weak = text.gamma_multiply(0.7);
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
    fn selected_bytes_span_lines_and_include_hidden_newlines() {
        let rules = LineRules {
            newline_at: b"\n".to_vec(),
            show_newline: false,
            ..Default::default()
        };
        let mut view = DataView::new(Direction::Rx, rules);
        let now = Instant::now();
        view.push(Direction::Rx, now, b"ab\ncd");
        view.push(Direction::Rx, now, b"e\nf");
        assert_eq!(view.line_offsets, [0, 3, 7]);
        view.selection = Some(Selection {
            anchor: 5,
            cursor: 1,
        });
        assert_eq!(view.selected_bytes().unwrap(), b"b\ncde");
        assert_eq!(view.copy(Format::Hex).unwrap(), "62 0A 63 64 65");
    }

    #[test]
    fn duration_format_like_hterm() {
        let d = std::time::Duration::from_micros(69_136_548);
        assert_eq!(fmt_duration(d), "00:01:09.136.548");
    }

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

//! The main window: state, connection handling and layout.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use eframe::egui::{self, Color32, ComboBox, DragValue, Panel, RichText, Sense, Ui, vec2};
use hterm_core::connection::{Connection, Event, ModemLines};
use hterm_core::format::{Format, Formats};
use hterm_core::input::{History, InputLine, TokenKind};
use hterm_core::lines::LineRules;
use hterm_core::log::{DataLog, Direction};
use hterm_core::mfs::{self, NamedSequence};
use hterm_core::ports;
use hterm_core::settings::{BAUD_RATES, DataBits, Parity, PortSettings, StopBits, parse_baud};
use hterm_formats::settings::{self as saved, FormatFlags, Settings};

use crate::input_line::{InputAction, input_line_ui};
use crate::view::DataView;

pub struct HtermApp {
    // Connection settings, as edited in the toolbar.
    port_text: String,
    ports: Vec<String>,
    patterns: Vec<String>,
    baud_text: String,
    data_bits: DataBits,
    stop_bits: StopBits,
    parity: Parity,
    cts_flow: bool,

    conn: Option<Connection>,
    dtr: bool,
    rts: bool,
    lines: Option<ModemLines>,
    lines_unavailable: bool,

    log: DataLog,
    rx_view: DataView,
    tx_view: DataView,
    rx_formats: Formats,
    tx_formats: Formats,
    autoscroll: bool,

    // Newline control.
    newline_presets: Vec<NamedSequence>,
    newline_sel: usize,
    show_newline: bool,
    on_source_change: bool,
    newline_every: usize,
    pause_ms: u32,

    // Input.
    send_on_enter: Vec<NamedSequence>,
    send_on_enter_sel: usize,
    input: InputLine,
    input_kind: TokenKind,
    history: History,

    /// Last warning or error, shown in the status bar.
    message: Option<(String, bool)>,
    /// Where settings are saved; `None` if the platform has no config folder.
    settings_path: Option<PathBuf>,
    /// What was last written, to save only when something changed.
    saved: Option<Settings>,
    next_autosave: Instant,
}

/// How often changed settings are written while the app runs.
const AUTOSAVE_INTERVAL: Duration = Duration::from_secs(5);

impl HtermApp {
    pub fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        let mut app = Self::with_defaults();
        // HTERM_OPEN_CONFIG overrides the settings file (handy for testing).
        app.settings_path = std::env::var_os("HTERM_OPEN_CONFIG")
            .map(PathBuf::from)
            .or_else(saved::default_path);
        if let Some(path) = app.settings_path.clone() {
            match Settings::load(&path) {
                Ok(s) => {
                    app.apply_settings(&s);
                    app.saved = Some(s);
                }
                Err(e) => {
                    let backup = saved::back_up_broken(&path)
                        .map(|b| format!(" A copy was kept as {}.", b.display()))
                        .unwrap_or_default();
                    app.error(format!("{e}. Using defaults.{backup}"));
                }
            }
        }
        app.ports = ports::list_ports(&app.patterns);
        app
    }

    fn with_defaults() -> Self {
        let patterns = ports::default_patterns();
        let rules = LineRules::default();
        Self {
            // Unlike HTerm, don't pre-select a port: Connect must never open
            // something the user didn't choose.
            port_text: String::new(),
            ports: Vec::new(),
            patterns,
            baud_text: hterm_core::settings::DEFAULT_BAUD.to_string(),
            data_bits: DataBits::default(),
            stop_bits: StopBits::default(),
            parity: Parity::default(),
            cts_flow: false,
            conn: None,
            dtr: false,
            rts: false,
            lines: None,
            lines_unavailable: false,
            log: DataLog::new(),
            rx_view: DataView::new(Direction::Rx, rules.clone()),
            tx_view: DataView::new(Direction::Tx, rules),
            rx_formats: Formats::default(),
            tx_formats: Formats::default(),
            autoscroll: true,
            newline_presets: mfs::default_newline_presets(),
            newline_sel: 0,
            show_newline: true,
            on_source_change: true,
            newline_every: 0,
            pause_ms: 0,
            send_on_enter: mfs::default_send_on_enter_presets(),
            send_on_enter_sel: 0,
            input: InputLine::new(),
            input_kind: TokenKind::Asc,
            history: History::default(),
            message: None,
            settings_path: None,
            saved: None,
            next_autosave: Instant::now() + AUTOSAVE_INTERVAL,
        }
    }

    fn apply_settings(&mut self, s: &Settings) {
        let p = &s.port;
        // The user's own last choice; it is restored but never auto-connected.
        self.port_text = p.path.clone();
        self.baud_text = p.baud.clone();
        self.data_bits = DataBits::from_count(p.data_bits).unwrap_or_default();
        self.stop_bits = StopBits::from_label(&p.stop_bits).unwrap_or_default();
        self.parity = Parity::from_label(&p.parity).unwrap_or_default();
        self.cts_flow = p.cts_flow;
        self.patterns = p.patterns.clone();

        let v = &s.view;
        self.rx_formats = to_formats(v.receive_formats);
        self.tx_formats = to_formats(v.transmit_formats);
        self.autoscroll = v.autoscroll;
        self.newline_sel = preset_index(&self.newline_presets, &v.newline_at);
        self.show_newline = v.show_newline;
        self.on_source_change = v.newline_on_source_change;
        self.newline_every = v.newline_every;
        self.pause_ms = v.newline_pause_ms;
        self.log.set_counted_byte(v.count_byte);

        self.input_kind = TokenKind::from_label(&s.input.kind).unwrap_or_default();
        self.send_on_enter_sel = preset_index(&self.send_on_enter, &s.input.send_on_enter);
    }

    fn to_settings(&self) -> Settings {
        Settings {
            port: saved::PortSection {
                path: self.port_text.trim().to_string(),
                baud: self.baud_text.trim().to_string(),
                data_bits: self.data_bits.count(),
                stop_bits: self.stop_bits.label().into(),
                parity: self.parity.label().into(),
                cts_flow: self.cts_flow,
                patterns: self.patterns.clone(),
            },
            view: saved::ViewSection {
                receive_formats: to_flags(self.rx_formats),
                transmit_formats: to_flags(self.tx_formats),
                autoscroll: self.autoscroll,
                newline_at: self.newline_presets[self.newline_sel].name.clone(),
                show_newline: self.show_newline,
                newline_on_source_change: self.on_source_change,
                newline_every: self.newline_every,
                newline_pause_ms: self.pause_ms,
                count_byte: self.log.counter().value,
            },
            input: saved::InputSection {
                kind: self.input_kind.label().into(),
                send_on_enter: self.send_on_enter[self.send_on_enter_sel].name.clone(),
            },
        }
    }

    /// Write the settings file if anything changed since the last write.
    fn save_settings(&mut self) {
        let Some(path) = self.settings_path.clone() else {
            return;
        };
        let current = self.to_settings();
        if self.saved.as_ref() == Some(&current) {
            return;
        }
        match current.save(&path) {
            Ok(()) => self.saved = Some(current),
            Err(e) => self.error(e.to_string()),
        }
    }

    fn line_rules(&self) -> LineRules {
        LineRules {
            newline_at: self.newline_presets[self.newline_sel].prefix.clone(),
            show_newline: self.show_newline,
            every: self.newline_every,
            on_source_change: self.on_source_change,
            pause: (self.pause_ms > 0).then(|| Duration::from_millis(self.pause_ms.into())),
        }
    }

    fn rebuild_views(&mut self) {
        let rules = self.line_rules();
        self.rx_view.rebuild(rules.clone(), &self.log);
        self.tx_view.rebuild(rules, &self.log);
    }

    fn info(&mut self, msg: impl Into<String>) {
        self.message = Some((msg.into(), false));
    }

    fn error(&mut self, msg: impl Into<String>) {
        self.message = Some((msg.into(), true));
    }

    fn connect(&mut self, ctx: &egui::Context) {
        let Some(baud) = parse_baud(&self.baud_text) else {
            self.error("Baud rate has to be a number greater than zero");
            return;
        };
        let path = self.port_text.trim().to_string();
        if path.is_empty() {
            self.error("Choose a port first");
            return;
        }
        let settings = PortSettings {
            path,
            baud,
            data_bits: self.data_bits,
            stop_bits: self.stop_bits,
            parity: self.parity,
            cts_flow: self.cts_flow,
        };
        let ctx = ctx.clone();
        match Connection::open(settings, move || ctx.request_repaint()) {
            Ok(conn) => {
                self.dtr = false;
                self.rts = self.cts_flow;
                self.lines = None;
                self.lines_unavailable = false;
                self.message = None;
                self.conn = Some(conn);
                // Save now too, so a crash later doesn't lose these settings.
                self.save_settings();
            }
            Err(e) => self.error(format!("Could not open {}: {e}", self.port_text.trim())),
        }
    }

    fn disconnect(&mut self) {
        if let Some(conn) = self.conn.take() {
            let events = conn.close();
            self.handle_events(events);
        }
    }

    fn handle_events(&mut self, events: Vec<Event>) {
        let mut trimmed = false;
        for event in events {
            match event {
                Event::Received { at, data } => {
                    self.rx_view.push(Direction::Rx, at, &data);
                    trimmed |= self.log.push(Direction::Rx, at, data);
                }
                Event::Sent { at, data } => {
                    self.tx_view.push(Direction::Tx, at, &data);
                    trimmed |= self.log.push(Direction::Tx, at, data);
                }
                Event::Lines(l) => self.lines = Some(l),
                Event::LinesUnavailable => self.lines_unavailable = true,
                Event::Warning(w) => self.info(w),
                Event::Error(e) => self.error(e),
                Event::Closed => {
                    self.conn = None;
                    self.lines = None;
                }
            }
        }
        if trimmed {
            self.rebuild_views();
        }
    }

    fn send_input(&mut self) {
        let Some(conn) = &self.conn else {
            self.error("Not connected");
            return;
        };
        let line = match self.input.to_bytes() {
            Ok(b) => b,
            Err(e) => {
                self.error(e.to_string());
                return;
            }
        };
        let soe = &self.send_on_enter[self.send_on_enter_sel];
        let mut data = soe.prefix.clone();
        data.extend(line);
        data.extend(&soe.suffix);
        conn.send(data);
        let sent = std::mem::take(&mut self.input);
        self.history.push(sent);
    }
}

impl eframe::App for HtermApp {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Ctrl+Q quits, like HTerm's File > Quit.
        if ctx.input(|i| i.modifiers.command && i.key_pressed(egui::Key::Q)) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        if let Some(conn) = &self.conn {
            let events = conn.poll_events();
            self.handle_events(events);
        }
        let rules = self.line_rules();
        if *self.rx_view.rules() != rules {
            self.rebuild_views();
        }
        if Instant::now() >= self.next_autosave {
            self.save_settings();
            self.next_autosave = Instant::now() + AUTOSAVE_INTERVAL;
        }
        // Make sure a pending autosave happens even when the app is idle.
        ctx.request_repaint_after(AUTOSAVE_INTERVAL);
    }

    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        Panel::top("toolbar").show(ui, |ui| {
            ui.add_space(4.0);
            self.connection_bar(ui, &ctx);
            ui.separator();
            self.counters_bar(ui);
            ui.separator();
            self.output_bar(ui);
            ui.add_space(2.0);
        });
        Panel::bottom("status").show(ui, |ui| self.status_bar(ui));
        Panel::bottom("transmitted")
            .resizable(true)
            .default_size(140.0)
            .show(ui, |ui| {
                ui.add_space(4.0);
                ui.label(RichText::new("Transmitted data").small().weak());
                self.tx_view.ui(ui, &self.tx_formats, self.autoscroll);
            });
        Panel::bottom("input").show(ui, |ui| self.input_panel(ui));
        egui::CentralPanel::default().show(ui, |ui| {
            ui.label(RichText::new("Received data").small().weak());
            self.rx_view.ui(ui, &self.rx_formats, self.autoscroll);
        });
    }

    fn on_exit(&mut self) {
        self.disconnect();
        self.save_settings();
    }
}

impl HtermApp {
    fn connection_bar(&mut self, ui: &mut Ui, ctx: &egui::Context) {
        let connected = self.conn.is_some();
        ui.horizontal_wrapped(|ui| {
            let label = if connected { "Disconnect" } else { "Connect" };
            if ui
                .add_sized(vec2(90.0, 22.0), egui::Button::new(label))
                .clicked()
            {
                if connected {
                    self.disconnect();
                } else {
                    self.connect(ctx);
                }
            }
            ui.add_enabled_ui(!connected, |ui| {
                ui.label("Port");
                editable_combo(ui, "port", &mut self.port_text, &self.ports, 140.0);
                if ui
                    .button("R")
                    .on_hover_text("Refresh the port list")
                    .clicked()
                {
                    self.ports = ports::list_ports(&self.patterns);
                }
                ui.label("Baud");
                let bauds: Vec<String> = BAUD_RATES.iter().map(u32::to_string).collect();
                editable_combo(ui, "baud", &mut self.baud_text, &bauds, 80.0);
                ui.label("Data");
                ComboBox::from_id_salt("data")
                    .width(40.0)
                    .selected_text(self.data_bits.to_string())
                    .show_ui(ui, |ui| {
                        for d in DataBits::ALL {
                            ui.selectable_value(&mut self.data_bits, d, d.to_string());
                        }
                    });
                ui.label("Stop");
                ComboBox::from_id_salt("stop")
                    .width(40.0)
                    .selected_text(self.stop_bits.label())
                    .show_ui(ui, |ui| {
                        for s in StopBits::ALL {
                            ui.selectable_value(&mut self.stop_bits, s, s.label());
                        }
                    });
                ui.label("Parity");
                ComboBox::from_id_salt("parity")
                    .width(60.0)
                    .selected_text(self.parity.label())
                    .show_ui(ui, |ui| {
                        for p in Parity::ALL {
                            ui.selectable_value(&mut self.parity, p, p.label());
                        }
                    });
            });
            if ui
                .checkbox(&mut self.cts_flow, "CTS flow control")
                .changed()
                && let Some(conn) = &mut self.conn
            {
                conn.set_cts_flow(self.cts_flow);
                if self.cts_flow {
                    self.rts = true;
                }
            }
        });
    }

    fn counters_bar(&mut self, ui: &mut Ui) {
        ui.horizontal_wrapped(|ui| {
            ui.label("Rx");
            counter_box(ui, self.log.rx_total());
            if ui.small_button("Reset").clicked() {
                self.log.reset_rx_total();
            }
            ui.add_space(8.0);
            ui.label("Tx");
            counter_box(ui, self.log.tx_total());
            if ui.small_button("Reset").clicked() {
                self.log.reset_tx_total();
            }
            ui.separator();

            ui.label("Count");
            let mut value = self.log.counter().value;
            if ui
                .add(DragValue::new(&mut value).range(0..=255))
                .on_hover_text("Byte value to count in received data")
                .changed()
            {
                self.log.set_counted_byte(value);
            }
            counter_box(ui, self.log.counter().count);
            if ui.small_button("Reset").clicked() {
                self.log.reset_counter();
            }
            ui.separator();

            ui.label("Newline at");
            ComboBox::from_id_salt("newline_at")
                .width(70.0)
                .selected_text(self.newline_presets[self.newline_sel].name.clone())
                .show_ui(ui, |ui| {
                    for (i, p) in self.newline_presets.iter().enumerate() {
                        ui.selectable_value(&mut self.newline_sel, i, &p.name);
                    }
                });
            ui.checkbox(&mut self.show_newline, "Show newline characters");
            ui.checkbox(&mut self.on_source_change, "Newline on source change");
        });
    }

    fn output_bar(&mut self, ui: &mut Ui) {
        ui.horizontal_wrapped(|ui| {
            if ui.button("Clear received").clicked() {
                self.log.clear(Direction::Rx);
                self.rebuild_views();
            }
            format_checkboxes(ui, &mut self.rx_formats);
            ui.separator();
            ui.label("Newline every");
            ui.add(DragValue::new(&mut self.newline_every).range(0..=4096))
                .on_hover_text("Characters per line (0 = off)");
            ui.label("chars");
            ui.separator();
            ui.label("Newline after");
            ui.add(DragValue::new(&mut self.pause_ms).range(0..=60_000))
                .on_hover_text("Start a new line after this receive pause (0 = off)");
            ui.label("ms pause");
            ui.separator();
            ui.checkbox(&mut self.autoscroll, "Autoscroll");
            ui.separator();
            for (name, on) in self.line_states() {
                led(ui, name, on);
            }
        });
    }

    fn line_states(&self) -> [(&'static str, Option<bool>); 4] {
        let l = if self.lines_unavailable {
            None
        } else {
            self.lines
        };
        [
            ("CTS", l.map(|l| l.cts)),
            ("DSR", l.map(|l| l.dsr)),
            ("RI", l.map(|l| l.ri)),
            ("DCD", l.map(|l| l.dcd)),
        ]
    }

    fn input_panel(&mut self, ui: &mut Ui) {
        ui.add_space(4.0);
        ui.horizontal_wrapped(|ui| {
            if ui.button("Clear transmitted").clicked() {
                self.log.clear(Direction::Tx);
                self.rebuild_views();
            }
            format_checkboxes(ui, &mut self.tx_formats);
            ui.separator();
            ui.label("Send on enter");
            ComboBox::from_id_salt("send_on_enter")
                .width(80.0)
                .selected_text(self.send_on_enter[self.send_on_enter_sel].name.clone())
                .show_ui(ui, |ui| {
                    for (i, p) in self.send_on_enter.iter().enumerate() {
                        ui.selectable_value(&mut self.send_on_enter_sel, i, &p.name);
                    }
                });
            ui.separator();
            let connected = self.conn.is_some();
            ui.add_enabled_ui(connected, |ui| {
                if ui.toggle_value(&mut self.dtr, "DTR").changed()
                    && let Some(c) = &self.conn
                {
                    c.set_dtr(self.dtr);
                }
                if ui.toggle_value(&mut self.rts, "RTS").changed()
                    && let Some(c) = &self.conn
                {
                    c.set_rts(self.rts);
                }
            });
        });
        ui.horizontal(|ui| {
            ui.label("Type");
            ComboBox::from_id_salt("input_type")
                .width(50.0)
                .selected_text(self.input_kind.label())
                .show_ui(ui, |ui| {
                    for k in TokenKind::ALL {
                        ui.selectable_value(&mut self.input_kind, k, k.label());
                    }
                });
            let send_w = 60.0;
            let mut submit = false;
            ui.scope(|ui| {
                ui.set_max_width(ui.available_width() - send_w - 8.0);
                submit = input_line_ui(ui, &mut self.input, self.input_kind, &mut self.history)
                    == InputAction::Submit;
            });
            if ui
                .add_enabled(self.conn.is_some(), egui::Button::new("Send"))
                .clicked()
            {
                submit = true;
            }
            if submit {
                self.send_input();
            }
        });
        ui.add_space(4.0);
    }

    fn status_bar(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            let pos = self
                .history
                .position()
                .map_or("-".to_string(), |p| p.to_string());
            ui.label(format!(
                "History {pos}/{}/{}",
                self.history.len(),
                self.history.max()
            ));
            ui.separator();
            match &self.conn {
                Some(c) => ui.label(format!(
                    "Connected to {} ({})",
                    c.settings().path,
                    c.settings().summary()
                )),
                None => ui.label("Not connected"),
            };
            if let Some((msg, is_error)) = &self.message {
                ui.separator();
                let color = if *is_error {
                    ui.visuals().error_fg_color
                } else {
                    ui.visuals().warn_fg_color
                };
                ui.colored_label(color, msg);
            }
        });
    }
}

fn to_formats(f: FormatFlags) -> Formats {
    Formats {
        ascii: f.ascii,
        hex: f.hex,
        dec: f.dec,
        bin: f.bin,
    }
}

fn to_flags(f: Formats) -> FormatFlags {
    FormatFlags {
        ascii: f.ascii,
        hex: f.hex,
        dec: f.dec,
        bin: f.bin,
    }
}

/// Index of the preset with this name, or 0 ("None") if it no longer exists.
fn preset_index(presets: &[NamedSequence], name: &str) -> usize {
    presets.iter().position(|p| p.name == name).unwrap_or(0)
}

/// A text box with a dropdown of suggestions (HTerm's editable combo boxes).
fn editable_combo(ui: &mut Ui, id: &str, text: &mut String, options: &[String], width: f32) {
    ui.spacing_mut().item_spacing.x = 2.0;
    ui.add(
        egui::TextEdit::singleline(text)
            .id_salt(id)
            .desired_width(width),
    );
    ui.menu_button("\u{23F7}", |ui| {
        egui::ScrollArea::vertical()
            .max_height(300.0)
            .show(ui, |ui| {
                if options.is_empty() {
                    ui.label("(none found)");
                }
                for o in options {
                    if ui.selectable_label(text == o, o).clicked() {
                        *text = o.clone();
                        ui.close();
                    }
                }
            });
    });
    ui.spacing_mut().item_spacing.x = 8.0;
}

/// A fixed-width, right-aligned number box.
fn counter_box(ui: &mut Ui, value: u64) {
    let (rect, _) = ui.allocate_exact_size(vec2(80.0, 18.0), Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect(
        rect,
        2.0,
        ui.visuals().extreme_bg_color,
        ui.visuals().widgets.noninteractive.bg_stroke,
        egui::StrokeKind::Inside,
    );
    painter.text(
        rect.right_center() - vec2(4.0, 0.0),
        egui::Align2::RIGHT_CENTER,
        value,
        egui::FontId::monospace(12.0),
        ui.visuals().text_color(),
    );
}

fn format_checkboxes(ui: &mut Ui, formats: &mut Formats) {
    for f in Format::ALL {
        ui.checkbox(formats.get_mut(f), f.label());
    }
}

/// A status-line LED: bright green = on, dark green = off, grey = unknown.
fn led(ui: &mut Ui, name: &str, on: Option<bool>) {
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = 1.0;
        ui.small(name);
        let (rect, _) = ui.allocate_exact_size(vec2(12.0, 12.0), Sense::hover());
        let color = match on {
            Some(true) => Color32::from_rgb(0x30, 0xE0, 0x30),
            Some(false) => Color32::from_rgb(0x14, 0x50, 0x20),
            None => Color32::GRAY,
        };
        ui.painter().circle_filled(rect.center(), 5.0, color);
        ui.painter()
            .circle_stroke(rect.center(), 5.0, (1.0, Color32::BLACK));
    });
}

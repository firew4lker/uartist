//! The main window: state, connection handling and layout.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use eframe::egui::{self, Color32, ComboBox, DragValue, Panel, RichText, Sense, Ui, vec2};
use uartist_core::connection::{Connection, Event, ModemLines};
use uartist_core::format::{Format, Formats};
use uartist_core::input::{History, InputLine, TokenKind};
use uartist_core::lines::{LineRules, ReceiveMask};
use uartist_core::log::{DataLog, Direction};
use uartist_core::mfs::{self, NamedSequence};
use uartist_core::ports;
use uartist_core::settings::{BAUD_RATES, DataBits, Parity, PortSettings, StopBits, parse_baud};
use uartist_formats::settings::{self as saved, FormatFlags, Settings};

use crate::input_line::{InputAction, input_line_ui};
use crate::save_output::{SaveOutputDialog, SaveResult};
use crate::send_dialog::{SendAction, SendDialog, SendSource};
use crate::sequences_panel::{SeqAction, SequencesPanel};
use crate::view::{DataView, ViewResponse};

pub struct UartistApp {
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
    /// The port can't set DTR/RTS.
    outputs_unavailable: bool,

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

    /// Last message, shown in the status bar.
    message: Option<(String, Level)>,
    /// Where settings are saved; `None` if the platform has no config folder.
    settings_path: Option<PathBuf>,
    /// What was last written, to save only when something changed.
    saved: Option<Settings>,
    next_autosave: Instant,
    save_output: SaveOutputDialog,
    send: SendDialog,
    sequences: SequencesPanel,
    /// The view clicked last; Ctrl+C copies its selection.
    active_view: Direction,
    theme: egui::ThemePreference,
    clear_at_enabled: bool,
    clear_at: u8,
    rx_mask: ReceiveMask,
}

/// Status bar message importance.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Level {
    Info,
    Warning,
    Error,
}

/// How often changed settings are written while the app runs.
const AUTOSAVE_INTERVAL: Duration = Duration::from_secs(5);

impl UartistApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let mut app = Self::with_defaults();
        // UARTIST_CONFIG overrides the settings file (handy for testing).
        app.settings_path = std::env::var_os("UARTIST_CONFIG")
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
        crate::theme::install(&cc.egui_ctx);
        cc.egui_ctx.set_theme(app.theme);
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
            baud_text: uartist_core::settings::DEFAULT_BAUD.to_string(),
            data_bits: DataBits::default(),
            stop_bits: StopBits::default(),
            parity: Parity::default(),
            cts_flow: false,
            conn: None,
            dtr: false,
            rts: false,
            lines: None,
            lines_unavailable: false,
            outputs_unavailable: false,
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
            save_output: SaveOutputDialog::default(),
            send: SendDialog::default(),
            sequences: SequencesPanel::default(),
            active_view: Direction::Rx,
            theme: egui::ThemePreference::System,
            clear_at_enabled: false,
            clear_at: 0,
            rx_mask: ReceiveMask::None,
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
        self.theme = theme_from_label(&v.theme);
        self.clear_at_enabled = v.clear_at_enabled;
        self.clear_at = v.clear_at_byte;
        self.rx_mask = ReceiveMask::from_label(&v.bitmask).unwrap_or_default();
        self.rx_view.combined = v.combined;

        self.input_kind = TokenKind::from_label(&s.input.kind).unwrap_or_default();
        self.send_on_enter_sel = preset_index(&self.send_on_enter, &s.input.send_on_enter);
        self.save_output.apply(&s.save_output);
        self.send.apply(&s.send);
        self.sequences.visible = s.sequences.visible;
        let errors = self.sequences.open_paths(&s.sequences.files);
        if let Some(e) = errors.first() {
            self.warn(e.clone());
        }
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
                theme: theme_label(self.theme).into(),
                clear_at_enabled: self.clear_at_enabled,
                clear_at_byte: self.clear_at,
                bitmask: self.rx_mask.label().into(),
                combined: self.rx_view.combined,
            },
            input: saved::InputSection {
                kind: self.input_kind.label().into(),
                send_on_enter: self.send_on_enter[self.send_on_enter_sel].name.clone(),
            },
            save_output: self.save_output.to_section(),
            send: self.send.to_section(),
            sequences: saved::SequencesSection {
                visible: self.sequences.visible,
                files: self.sequences.paths(),
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
            rx_mask: self.rx_mask.bits(),
        }
    }

    /// Rebuild both views. `keep_rx`/`keep_tx`: whether that view's byte
    /// stream is unchanged (only display rules changed), so its selection stays.
    fn rebuild_views(&mut self, keep_rx: bool, keep_tx: bool) {
        let rules = self.line_rules();
        self.rx_view.rebuild(rules.clone(), &self.log, keep_rx);
        self.tx_view.rebuild(rules, &self.log, keep_tx);
    }

    fn view_response(&mut self, dir: Direction, r: ViewResponse, ctx: &egui::Context) {
        if r.pressed {
            self.active_view = dir;
        }
        if let Some(text) = r.copy {
            ctx.copy_text(text);
        }
    }

    fn info(&mut self, msg: impl Into<String>) {
        self.message = Some((msg.into(), Level::Info));
    }

    fn warn(&mut self, msg: impl Into<String>) {
        self.message = Some((msg.into(), Level::Warning));
    }

    fn error(&mut self, msg: impl Into<String>) {
        self.message = Some((msg.into(), Level::Error));
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
                self.outputs_unavailable = false;
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
        let mut rx_cleared = false;
        for event in events {
            match event {
                Event::Received { at, data } => {
                    if self.clear_at_enabled {
                        let (t, cleared) =
                            self.log.push_rx_clear_at(at, data.clone(), self.clear_at);
                        trimmed |= t;
                        if cleared {
                            rx_cleared = true;
                            continue;
                        }
                    } else {
                        trimmed |= self.log.push(Direction::Rx, at, data.clone());
                    }
                    self.rx_view.push(Direction::Rx, at, &data);
                }
                Event::Sent { at, data } => {
                    self.send.on_sent(data.len(), at);
                    // Each view keeps what it shows (the combined view both).
                    self.tx_view.push(Direction::Tx, at, &data);
                    self.rx_view.push(Direction::Tx, at, &data);
                    trimmed |= self.log.push(Direction::Tx, at, data);
                }
                Event::Lines(l) => self.lines = Some(l),
                Event::LinesUnavailable => self.lines_unavailable = true,
                Event::OutputLinesUnavailable => self.outputs_unavailable = true,
                Event::Warning(w) => self.warn(w),
                Event::Error(e) => self.error(e),
                Event::Closed => {
                    self.send.stop();
                    self.conn = None;
                    self.lines = None;
                }
            }
        }
        if trimmed {
            self.rebuild_views(false, false);
        } else if rx_cleared {
            self.rebuild_views(false, true);
        }
    }

    /// The input line's bytes with the send-on-enter prefix and suffix.
    fn input_bytes(&self) -> Result<Vec<u8>, String> {
        let line = self.input.to_bytes().map_err(|e| e.to_string())?;
        let soe = &self.send_on_enter[self.send_on_enter_sel];
        let mut data = soe.prefix.clone();
        data.extend(line);
        data.extend(&soe.suffix);
        Ok(data)
    }

    /// Load settings from an HTerm `hterm.cfg` chosen by the user.
    fn import_hterm_config(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .set_title("Import HTerm config")
            .add_filter("HTerm config", &["cfg"])
            .pick_file()
        else {
            return;
        };
        self.import_hterm_config_from(&path);
    }

    fn import_hterm_config_from(&mut self, path: &std::path::Path) {
        let text = match std::fs::read_to_string(path) {
            Ok(t) => t,
            Err(e) => return self.error(format!("Could not read {}: {e}", path.display())),
        };
        match uartist_formats::hterm_cfg::import(&text, &self.to_settings()) {
            Ok(imported) => {
                self.apply_settings(&imported.settings);
                self.ports = ports::list_ports(&self.patterns);
                let mut msg = format!("Imported {}", path.display());
                if !imported.notes.is_empty() {
                    msg.push_str(&format!(". Note: {}", imported.notes.join("; ")));
                }
                self.info(msg);
            }
            Err(e) => self.error(format!("{}: {e}", path.display())),
        }
    }

    fn sequence_action(&mut self, action: SeqAction) {
        match action {
            SeqAction::None => {}
            SeqAction::Send(tokens) => self.send_tokens(tokens),
            SeqAction::Insert(tokens) => self.input.extend(tokens),
            SeqAction::Info(msg) => self.info(msg),
            SeqAction::Error(msg) => self.error(msg),
        }
    }

    /// Send a sequence's chips as they are (no send-on-enter prefix/suffix).
    fn send_tokens(&mut self, tokens: Vec<uartist_core::input::Token>) {
        if self.send.is_running() {
            self.error("Wait for the running send to finish, or stop it");
            return;
        }
        let Some(conn) = &self.conn else {
            self.error("Not connected");
            return;
        };
        match InputLine::from_tokens(tokens).to_bytes() {
            Ok(data) => conn.send(data),
            Err(e) => self.error(e.to_string()),
        }
    }

    fn start_send(&mut self) {
        let data = match self.send.source() {
            SendSource::InputLine => self.input_bytes(),
            SendSource::File => self.send.load_file(),
        };
        match data {
            Ok(data) if data.is_empty() => self.error("Nothing to send"),
            Ok(data) => self.send.start(data, Instant::now()),
            Err(e) => self.error(e),
        }
    }

    fn stop_send(&mut self) {
        self.send.stop();
        if let Some(conn) = &self.conn {
            conn.cancel_send();
        }
        self.info("Sending stopped");
    }

    fn send_input(&mut self) {
        if self.send.is_running() {
            self.error("Wait for the running send to finish, or stop it");
            return;
        }
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

impl eframe::App for UartistApp {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Ctrl+Q quits, like HTerm's File > Quit.
        if ctx.input(|i| i.modifiers.command && i.key_pressed(egui::Key::Q)) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        if let Some(conn) = &self.conn {
            let events = conn.poll_events();
            self.handle_events(events);
        }
        // Autosend / Send file: queue the next repetition when it's due.
        if let Some(conn) = &self.conn
            && let Some(data) = self.send.poll(Instant::now())
        {
            conn.send(data);
        }
        if let Some(bytes) = self.send.take_finished() {
            self.send.open = false;
            self.info(format!("Sent {bytes} bytes"));
        }
        let rules = self.line_rules();
        if *self.rx_view.rules() != rules {
            self.rebuild_views(true, true);
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
                let r = self
                    .tx_view
                    .ui(ui, &self.tx_formats, self.autoscroll, &self.log);
                self.view_response(Direction::Tx, r, &ctx);
            });
        Panel::bottom("input").show(ui, |ui| self.input_panel(ui));
        if self.sequences.visible {
            let action = Panel::left("sequences")
                .resizable(true)
                .default_size(200.0)
                .show(ui, |ui| {
                    ui.add_space(4.0);
                    self.sequences.ui(ui)
                })
                .inner;
            self.sequence_action(action);
        }
        egui::CentralPanel::default().show(ui, |ui| {
            ui.label(RichText::new("Received data").small().weak());
            let r = self
                .rx_view
                .ui(ui, &self.rx_formats, self.autoscroll, &self.log);
            self.view_response(Direction::Rx, r, &ctx);
        });
        // Ctrl+C copies from the view clicked last, unless a text box has focus.
        let copy_pressed = ctx.input(|i| i.events.iter().any(|e| matches!(e, egui::Event::Copy)));
        if copy_pressed && ctx.memory(|m| m.focused().is_none()) {
            let (view, formats) = match self.active_view {
                Direction::Rx => (&self.rx_view, &self.rx_formats),
                Direction::Tx => (&self.tx_view, &self.tx_formats),
            };
            let format = formats.enabled().next().unwrap_or(Format::Ascii);
            if let Some(text) = view.copy(format) {
                ctx.copy_text(text);
            }
        }
        if self.save_output.open {
            match self.save_output.ui(&ctx, &self.log) {
                Some(SaveResult::Saved(msg)) => self.info(msg),
                Some(SaveResult::Failed(msg)) => self.error(msg),
                None => {}
            }
        }
        let action = self.sequences.editor_ui(&ctx);
        self.sequence_action(action);
        if self.send.open {
            match self.send.ui(&ctx, self.conn.is_some()) {
                SendAction::Start => self.start_send(),
                SendAction::Stop => self.stop_send(),
                SendAction::None => {}
            }
        }
    }

    fn on_exit(&mut self) {
        self.disconnect();
        self.save_settings();
    }
}

impl UartistApp {
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
            ui.separator();
            for (name, on) in self.line_states() {
                led(ui, name, on);
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
                self.rebuild_views(false, true);
            }
            format_checkboxes(ui, &mut self.rx_formats);
            ui.separator();
            ui.toggle_value(&mut self.sequences.visible, "Sequences")
                .on_hover_text("Show the sequences panel");
            if ui.button("Save output…").clicked() {
                self.save_output.open = true;
            }
            ui.separator();
            ui.checkbox(&mut self.clear_at_enabled, "Clear at")
                .on_hover_text("Clear the received view when this byte arrives");
            ui.add(DragValue::new(&mut self.clear_at).range(0..=255));
            let mut combined = self.rx_view.combined;
            if ui
                .checkbox(&mut combined, "Combined view")
                .on_hover_text("Show sent data in the received view too (underlined)")
                .changed()
            {
                self.rx_view.combined = combined;
                self.rebuild_views(false, true);
            }
            ComboBox::from_id_salt("bitmask")
                .width(100.0)
                .selected_text(self.rx_mask.label())
                .show_ui(ui, |ui| {
                    for m in ReceiveMask::ALL {
                        ui.selectable_value(&mut self.rx_mask, m, m.label());
                    }
                })
                .response
                .on_hover_text("Clear high bits of received bytes in the view");
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
                self.rebuild_views(true, false);
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
            if ui
                .button("Send file…")
                .on_hover_text("Send a file, optionally repeated")
                .clicked()
            {
                self.send.open_for(SendSource::File);
            }
            ui.separator();
            let connected = self.conn.is_some();
            let outputs = connected && !self.outputs_unavailable;
            ui.add_enabled_ui(outputs, |ui| {
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
            })
            .response
            .on_disabled_hover_text(if connected {
                "This port can't set DTR/RTS (for example a virtual port)"
            } else {
                "Connect first"
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
            let send_w = 150.0;
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
            if ui
                .button("Autosend…")
                .on_hover_text("Send the input line repeatedly")
                .clicked()
            {
                self.send.open_for(SendSource::InputLine);
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
            if let Some((msg, level)) = &self.message {
                ui.separator();
                let color = match level {
                    Level::Info => ui.visuals().text_color(),
                    Level::Warning => ui.visuals().warn_fg_color,
                    Level::Error => ui.visuals().error_fg_color,
                };
                ui.colored_label(color, msg);
            }
            // Theme selector and settings menu at the right end.
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let connected = self.conn.is_some();
                ui.menu_button("Settings", |ui| {
                    if ui
                        .add_enabled(!connected, egui::Button::new("Import HTerm config…"))
                        .on_disabled_hover_text("Disconnect first")
                        .clicked()
                    {
                        ui.close();
                        self.import_hterm_config();
                    }
                });
                let before = self.theme;
                ComboBox::from_id_salt("theme")
                    .width(70.0)
                    .selected_text(theme_label(self.theme))
                    .show_ui(ui, |ui| {
                        for t in [
                            egui::ThemePreference::System,
                            egui::ThemePreference::Light,
                            egui::ThemePreference::Dark,
                        ] {
                            ui.selectable_value(&mut self.theme, t, theme_label(t));
                        }
                    });
                ui.label("Theme");
                if self.theme != before {
                    ui.ctx().set_theme(self.theme);
                }
            });
        });
    }
}

fn theme_label(t: egui::ThemePreference) -> &'static str {
    match t {
        egui::ThemePreference::System => "System",
        egui::ThemePreference::Light => "Light",
        egui::ThemePreference::Dark => "Dark",
    }
}

fn theme_from_label(s: &str) -> egui::ThemePreference {
    match s.trim().to_ascii_lowercase().as_str() {
        "light" => egui::ThemePreference::Light,
        "dark" => egui::ThemePreference::Dark,
        _ => egui::ThemePreference::System,
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

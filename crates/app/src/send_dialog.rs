//! The Autosend / Send file dialog (`docs/spec/send-dialog.md`).

use std::path::Path;
use std::time::{Duration, Instant};

use eframe::egui::{self, DragValue, ProgressBar};
use uartist_core::sender::{JobState, Repeat, SendJob};
use uartist_formats::settings::SendSection;

/// Largest file Send file will load.
const MAX_FILE: u64 = 64 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SendSource {
    InputLine,
    File,
}

/// What the user asked for this frame.
#[derive(Debug, PartialEq, Eq)]
pub enum SendAction {
    None,
    /// Start: the app supplies the data (input line or file) and calls `start`.
    Start,
    Stop,
}

pub struct SendDialog {
    pub open: bool,
    source: SendSource,
    repetitions: u32,
    forever: bool,
    delay_ms: u32,
    file_path: String,
    job: Option<SendJob>,
}

impl Default for SendDialog {
    fn default() -> Self {
        Self {
            open: false,
            source: SendSource::InputLine,
            repetitions: 1,
            forever: false,
            delay_ms: 0,
            file_path: String::new(),
            job: None,
        }
    }
}

impl SendDialog {
    pub fn apply(&mut self, s: &SendSection) {
        self.repetitions = s.repetitions.max(1);
        self.forever = s.forever;
        self.delay_ms = s.delay_ms;
        self.file_path = s.file_path.clone();
    }

    pub fn to_section(&self) -> SendSection {
        SendSection {
            repetitions: self.repetitions,
            forever: self.forever,
            delay_ms: self.delay_ms,
            file_path: self.file_path.trim().to_string(),
        }
    }

    pub fn source(&self) -> SendSource {
        self.source
    }

    pub fn is_running(&self) -> bool {
        self.job.as_ref().is_some_and(SendJob::is_running)
    }

    /// Open the dialog for a source. Ignored while a job is running.
    pub fn open_for(&mut self, source: SendSource) {
        if !self.is_running() {
            self.source = source;
            self.job = None;
        }
        self.open = true;
    }

    /// Read the chosen file for Send file.
    pub fn load_file(&self) -> Result<Vec<u8>, String> {
        let path = self.file_path.trim();
        if path.is_empty() {
            return Err("Choose a file to send".into());
        }
        let meta = std::fs::metadata(path).map_err(|e| format!("{path}: {e}"))?;
        if meta.len() > MAX_FILE {
            return Err(format!("{path} is larger than 64 MiB"));
        }
        std::fs::read(path).map_err(|e| format!("{path}: {e}"))
    }

    pub fn start(&mut self, data: Vec<u8>, now: Instant) {
        let repeat = if self.forever {
            Repeat::Forever
        } else {
            Repeat::Times(self.repetitions.max(1))
        };
        self.job = Some(SendJob::new(
            data,
            repeat,
            Duration::from_millis(self.delay_ms.into()),
            now,
        ));
    }

    /// Data to queue now, if a repetition is due.
    pub fn poll(&mut self, now: Instant) -> Option<Vec<u8>> {
        self.job.as_mut()?.poll(now)
    }

    pub fn on_sent(&mut self, n: usize, now: Instant) {
        if let Some(job) = &mut self.job {
            job.on_sent(n, now);
        }
    }

    pub fn stop(&mut self) {
        if let Some(job) = &mut self.job {
            job.stop();
        }
    }

    /// True once, when the job has just finished all repetitions.
    pub fn take_finished(&mut self) -> Option<u64> {
        match &self.job {
            Some(job) if job.state() == JobState::Finished => {
                let sent = job.total_sent();
                self.job = None;
                Some(sent)
            }
            _ => None,
        }
    }

    pub fn ui(&mut self, ctx: &egui::Context, connected: bool) -> SendAction {
        let mut action = SendAction::None;
        let running = self.is_running();
        let now = Instant::now();
        let title = match self.source {
            SendSource::InputLine => "Autosend",
            SendSource::File => "Send file",
        };
        let mut open = self.open;
        egui::Window::new(title)
            .id(egui::Id::unique("send_dialog"))
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                egui::Grid::new("send_grid")
                    .num_columns(2)
                    .spacing([12.0, 8.0])
                    .show(ui, |ui| {
                        ui.label("Source");
                        match self.source {
                            SendSource::InputLine => {
                                ui.label("[Input line]");
                            }
                            SendSource::File => {
                                ui.add_enabled_ui(!running, |ui| {
                                    ui.horizontal(|ui| {
                                        ui.add(
                                            egui::TextEdit::singleline(&mut self.file_path)
                                                .id_salt("send_path")
                                                .desired_width(240.0),
                                        );
                                        if ui.button("Browse…").clicked()
                                            && let Some(p) = browse(&self.file_path)
                                        {
                                            self.file_path = p;
                                        }
                                    });
                                });
                            }
                        }
                        ui.end_row();

                        ui.label("Repetitions");
                        ui.add_enabled_ui(!running, |ui| {
                            ui.horizontal(|ui| {
                                ui.add_enabled(
                                    !self.forever,
                                    DragValue::new(&mut self.repetitions).range(1..=100_000),
                                );
                                ui.checkbox(&mut self.forever, "Forever");
                            });
                        });
                        ui.end_row();

                        ui.label("Delay");
                        ui.add_enabled_ui(!running, |ui| {
                            ui.horizontal(|ui| {
                                ui.add(DragValue::new(&mut self.delay_ms).range(0..=3_600_000));
                                ui.label("ms between repetitions");
                            });
                        });
                        ui.end_row();
                    });

                ui.separator();
                self.status(ui, now);
                ui.separator();
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(!running && connected, egui::Button::new("Start"))
                        .on_disabled_hover_text(if connected {
                            "Running"
                        } else {
                            "Connect first"
                        })
                        .clicked()
                    {
                        action = SendAction::Start;
                    }
                    if ui.add_enabled(running, egui::Button::new("Stop")).clicked() {
                        action = SendAction::Stop;
                    }
                    if ui.button("Close").clicked() {
                        self.open = false;
                    }
                });
            });
        // Closing the window (X) also stops a running job.
        if !open && running {
            action = SendAction::Stop;
        }
        self.open = self.open && open;
        if running {
            ctx.request_repaint_after(Duration::from_millis(50));
        }
        action
    }

    fn status(&self, ui: &mut egui::Ui, now: Instant) {
        let Some(job) = &self.job else {
            // Same height as the running status (4 lines), so nothing jumps.
            ui.weak("Not started");
            ui.label(" ");
            ui.add(ProgressBar::new(0.0));
            ui.label(" ");
            return;
        };
        let of = match job.repeat() {
            Repeat::Times(n) => n.to_string(),
            Repeat::Forever => "∞".into(),
        };
        let state = match job.state() {
            JobState::Finished => "finished",
            JobState::Stopped => "stopped",
            _ => "",
        };
        ui.horizontal(|ui| {
            ui.label(format!("Repetition {} of {of}", job.current()));
            if !state.is_empty() {
                ui.weak(format!("({state})"));
            }
        });
        // Always one line here, so the buttons below never move.
        match job.next_in(now).filter(|d| !d.is_zero()) {
            Some(next) => ui.label(format!("Next in {:.1} s", next.as_secs_f64())),
            None => ui.label(" "),
        };
        let len = job.data_len().max(1);
        let done = job.sent_in_repetition();
        ui.add(
            ProgressBar::new(done as f32 / len as f32)
                .text(format!("{done} of {} bytes", job.data_len())),
        );
        ui.horizontal(|ui| {
            ui.label(format!("Elapsed {}", fmt_time(job.elapsed(now))));
            if let Some(left) = job.remaining(now) {
                ui.label(format!("  Remaining ~{}", fmt_time(left)));
            }
            if let Some(speed) = job.speed(now) {
                ui.label(format!("  {:.2} KiB/s", speed / 1024.0));
            }
        });
    }
}

fn browse(current: &str) -> Option<String> {
    let mut dialog = rfd::FileDialog::new().set_title("Send file");
    if let Some(dir) = Path::new(current.trim()).parent().filter(|d| d.is_dir()) {
        dialog = dialog.set_directory(dir);
    }
    dialog.pick_file().map(|p| p.display().to_string())
}

/// `m:ss`, or `h:mm:ss` from an hour on.
fn fmt_time(d: Duration) -> String {
    let s = d.as_secs();
    if s >= 3600 {
        format!("{}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60)
    } else {
        format!("{}:{:02}", s / 60, s % 60)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn time_format() {
        assert_eq!(fmt_time(Duration::from_secs(5)), "0:05");
        assert_eq!(fmt_time(Duration::from_secs(125)), "2:05");
        assert_eq!(fmt_time(Duration::from_secs(3725)), "1:02:05");
    }
}

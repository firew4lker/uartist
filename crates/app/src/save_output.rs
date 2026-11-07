//! The "Save output" dialog (`docs/spec/save-output.md`).

use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

use eframe::egui::{self, ComboBox};
use hterm_core::input::encode_latin1;
use hterm_core::log::DataLog;
use hterm_core::output::{OutputFormat, OutputOptions, Source, write_output};
use hterm_formats::settings::SaveOutputSection;

#[derive(Default)]
pub struct SaveOutputDialog {
    pub open: bool,
    format: OutputFormat,
    source: Source,
    separator: String,
    timestamps: bool,
    path: String,
}

/// Result of a save, for the status bar.
pub enum SaveResult {
    Saved(String),
    Failed(String),
}

impl SaveOutputDialog {
    pub fn apply(&mut self, s: &SaveOutputSection) {
        self.format = OutputFormat::from_label(&s.format).unwrap_or_default();
        self.source = Source::from_label(&s.source).unwrap_or_default();
        self.separator = s.separator.clone();
        self.timestamps = s.timestamps;
        self.path = s.path.clone();
    }

    pub fn to_section(&self) -> SaveOutputSection {
        SaveOutputSection {
            format: self.format.label().into(),
            source: self.source.label().into(),
            separator: self.separator.clone(),
            timestamps: self.timestamps,
            path: self.path.trim().into(),
        }
    }

    pub fn ui(&mut self, ctx: &egui::Context, log: &DataLog) -> Option<SaveResult> {
        let mut result = None;
        let mut open = self.open;
        let mut close = false;
        egui::Window::new("Save output")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                egui::Grid::new("save_output_grid")
                    .num_columns(2)
                    .spacing([12.0, 8.0])
                    .show(ui, |ui| {
                        ui.label("Format");
                        ComboBox::from_id_salt("so_format")
                            .selected_text(self.format.label())
                            .show_ui(ui, |ui| {
                                for f in OutputFormat::ALL {
                                    ui.selectable_value(&mut self.format, f, f.label());
                                }
                            });
                        ui.end_row();

                        ui.label("Source");
                        ComboBox::from_id_salt("so_source")
                            .selected_text(self.source.label())
                            .show_ui(ui, |ui| {
                                for s in Source::ALL {
                                    ui.selectable_value(&mut self.source, s, s.label());
                                }
                            });
                        ui.end_row();

                        let text = self.format.is_text();
                        ui.add_enabled(text, egui::Label::new("Separator"));
                        ui.add_enabled(
                            text,
                            egui::TextEdit::singleline(&mut self.separator).desired_width(80.0),
                        )
                        .on_hover_text("Written between bytes, e.g. a space or a comma");
                        ui.end_row();

                        ui.add_enabled(text, egui::Label::new("Timestamps"));
                        ui.add_enabled(text, egui::Checkbox::new(&mut self.timestamps, "Enable"));
                        ui.end_row();

                        ui.label("File");
                        ui.horizontal(|ui| {
                            ui.add(
                                egui::TextEdit::singleline(&mut self.path)
                                    .id_salt("so_path")
                                    .desired_width(260.0),
                            );
                            if ui.button("Browse…").clicked()
                                && let Some(p) = self.browse()
                            {
                                self.path = p.display().to_string();
                            }
                        });
                        ui.end_row();
                    });
                if !self.format.is_text() {
                    ui.weak("Raw saves the exact bytes, without separator or timestamps.");
                }
                ui.separator();
                ui.horizontal(|ui| {
                    if ui.button("Save").clicked() {
                        result = Some(self.save(log));
                        if matches!(result, Some(SaveResult::Saved(_))) {
                            close = true;
                        }
                    }
                    if ui.button("Cancel").clicked() {
                        close = true;
                    }
                });
            });
        self.open = open && !close;
        result
    }

    fn browse(&self) -> Option<PathBuf> {
        let mut dialog = rfd::FileDialog::new().set_title("Save output");
        let current = Path::new(self.path.trim());
        if let Some(dir) = current.parent().filter(|d| d.is_dir()) {
            dialog = dialog.set_directory(dir);
        }
        if let Some(name) = current.file_name() {
            dialog = dialog.set_file_name(name.to_string_lossy());
        }
        dialog.save_file()
    }

    fn options(&self) -> Result<OutputOptions, String> {
        let separator = encode_latin1(&self.separator).map_err(|e| e.to_string())?;
        Ok(OutputOptions {
            format: self.format,
            source: self.source,
            separator,
            timestamps: self.timestamps,
        })
    }

    fn save(&self, log: &DataLog) -> SaveResult {
        let path = self.path.trim();
        if path.is_empty() {
            return SaveResult::Failed("Choose a file to save to".into());
        }
        let opts = match self.options() {
            Ok(o) => o,
            Err(e) => return SaveResult::Failed(e),
        };
        let write = || -> std::io::Result<()> {
            let mut out = BufWriter::new(File::create(path)?);
            write_output(&mut out, log.chunks(), &opts, |at| {
                chrono::DateTime::<chrono::Local>::from(log.wall_time(at))
                    .format("%H:%M:%S%.3f")
                    .to_string()
            })?;
            out.flush()
        };
        match write() {
            Ok(()) => SaveResult::Saved(format!("Saved output to {path}")),
            Err(e) => SaveResult::Failed(format!("Could not save to {path}: {e}")),
        }
    }
}

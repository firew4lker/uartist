//! The Sequences side panel and its editor (`docs/spec/sequences.md`).
//!
//! Every change is saved to its file right away.

use std::path::{Path, PathBuf};

use eframe::egui::{self, ComboBox, Ui};
use uartist_core::input::{History, InputLine, Token, TokenKind};
use uartist_formats::sequences::{SequenceFile, SequenceItem};

use crate::input_line::{InputAction, input_line_ui};

/// What the panel asks the app to do.
pub enum SeqAction {
    None,
    Send(Vec<Token>),
    Insert(Vec<Token>),
    Info(String),
    Error(String),
}

struct OpenFile {
    path: PathBuf,
    file: SequenceFile,
}

impl OpenFile {
    fn name(&self) -> String {
        self.path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| self.path.display().to_string())
    }
}

struct Editor {
    file: usize,
    /// `None` for a new item.
    index: Option<usize>,
    name: String,
    description: String,
    line: InputLine,
    kind: TokenKind,
    history: History,
}

#[derive(Default)]
pub struct SequencesPanel {
    pub visible: bool,
    files: Vec<OpenFile>,
    editor: Option<Editor>,
}

enum ItemOp {
    Send,
    Insert,
    Edit,
    MoveUp,
    MoveDown,
    Delete,
}

impl SequencesPanel {
    /// Open the files remembered in the settings. Returns errors to show.
    pub fn open_paths(&mut self, paths: &[String]) -> Vec<String> {
        paths
            .iter()
            .filter_map(|p| self.load(Path::new(p)).err())
            .collect()
    }

    pub fn paths(&self) -> Vec<String> {
        self.files
            .iter()
            .map(|f| f.path.display().to_string())
            .collect()
    }

    fn load(&mut self, path: &Path) -> Result<(), String> {
        if self.files.iter().any(|f| f.path == path) {
            return Ok(());
        }
        let file = SequenceFile::load(path).map_err(|e| format!("{}: {e}", path.display()))?;
        self.files.push(OpenFile {
            path: path.to_owned(),
            file,
        });
        Ok(())
    }

    fn save(&self, i: usize) -> SeqAction {
        let f = &self.files[i];
        match f.file.save(&f.path) {
            Ok(()) => SeqAction::None,
            Err(e) => SeqAction::Error(format!("Could not save {}: {e}", f.path.display())),
        }
    }

    pub fn ui(&mut self, ui: &mut Ui) -> SeqAction {
        let mut action = SeqAction::None;
        ui.strong("Sequences");
        ui.horizontal(|ui| {
            if ui
                .small_button("Load…")
                .on_hover_text("Open an .hts sequence file (HTerm format)")
                .clicked()
            {
                action = self.load_dialog();
            }
            if ui
                .small_button("New…")
                .on_hover_text("Create a new sequence file")
                .clicked()
            {
                action = self.new_file();
            }
        });
        ui.separator();
        if self.files.is_empty() {
            ui.weak("No sequence files.\nLoad an HTerm .hts file or create a new one.");
            return action;
        }

        let mut item_op: Option<(usize, usize, ItemOp)> = None;
        let mut file_op: Option<(usize, bool)> = None; // (file, true = close / false = new item)
        egui::ScrollArea::vertical()
            .auto_shrink([false, true])
            .show(ui, |ui| {
                for (fi, f) in self.files.iter().enumerate() {
                    let header = egui::CollapsingHeader::new(f.name())
                        .id_salt(("seqfile", &f.path))
                        .default_open(true)
                        .show(ui, |ui| {
                            if f.file.items.is_empty() {
                                ui.weak("(empty)");
                            }
                            for (ii, item) in f.file.items.iter().enumerate() {
                                let r = ui.selectable_label(false, &item.name);
                                let r = if item.description.is_empty() {
                                    r
                                } else {
                                    r.on_hover_text(&item.description)
                                };
                                if r.double_clicked() {
                                    item_op = Some((fi, ii, ItemOp::Send));
                                }
                                r.context_menu(|ui| {
                                    for (label, op) in [
                                        ("Send (double-click)", ItemOp::Send),
                                        ("Insert into input line", ItemOp::Insert),
                                        ("Edit…", ItemOp::Edit),
                                        ("Move up", ItemOp::MoveUp),
                                        ("Move down", ItemOp::MoveDown),
                                        ("Delete", ItemOp::Delete),
                                    ] {
                                        if ui.button(label).clicked() {
                                            item_op = Some((fi, ii, op));
                                            ui.close();
                                        }
                                    }
                                });
                            }
                            if ui.small_button("+ New item").clicked() {
                                file_op = Some((fi, false));
                            }
                        });
                    header.header_response.context_menu(|ui| {
                        if ui.button("New item…").clicked() {
                            file_op = Some((fi, false));
                            ui.close();
                        }
                        if ui.button("Close file").clicked() {
                            file_op = Some((fi, true));
                            ui.close();
                        }
                    });
                }
            });

        if let Some((fi, close)) = file_op {
            if close {
                self.files.remove(fi);
            } else {
                self.open_editor(fi, None);
            }
        }
        if let Some((fi, ii, op)) = item_op {
            action = self.item_op(fi, ii, op);
        }
        action
    }

    fn item_op(&mut self, fi: usize, ii: usize, op: ItemOp) -> SeqAction {
        let items = &mut self.files[fi].file.items;
        match op {
            ItemOp::Send => return SeqAction::Send(items[ii].tokens.clone()),
            ItemOp::Insert => return SeqAction::Insert(items[ii].tokens.clone()),
            ItemOp::Edit => {
                self.open_editor(fi, Some(ii));
                return SeqAction::None;
            }
            ItemOp::MoveUp if ii > 0 => items.swap(ii, ii - 1),
            ItemOp::MoveDown if ii + 1 < items.len() => items.swap(ii, ii + 1),
            ItemOp::Delete => {
                items.remove(ii);
            }
            _ => return SeqAction::None,
        }
        self.save(fi)
    }

    fn open_editor(&mut self, file: usize, index: Option<usize>) {
        let item = index
            .map(|i| self.files[file].file.items[i].clone())
            .unwrap_or_else(|| SequenceItem {
                name: "New sequence".into(),
                ..Default::default()
            });
        self.editor = Some(Editor {
            file,
            index,
            name: item.name,
            description: item.description,
            line: InputLine::from_tokens(item.tokens),
            kind: TokenKind::Asc,
            history: History::new(0),
        });
    }

    /// The editor window, if open.
    pub fn editor_ui(&mut self, ctx: &egui::Context) -> SeqAction {
        let Some(ed) = &mut self.editor else {
            return SeqAction::None;
        };
        let mut ok = false;
        let mut cancel = false;
        let mut open = true;
        let title = if ed.index.is_some() {
            "Edit sequence"
        } else {
            "New sequence"
        };
        egui::Window::new(title)
            .id(egui::Id::unique("sequence_editor"))
            .open(&mut open)
            .collapsible(false)
            .default_width(460.0)
            .show(ctx, |ui| {
                egui::Grid::new("seq_editor_grid")
                    .num_columns(2)
                    .spacing([12.0, 8.0])
                    .show(ui, |ui| {
                        ui.label("Name");
                        ui.add(egui::TextEdit::singleline(&mut ed.name).desired_width(320.0));
                        ui.end_row();
                        ui.label("Description");
                        ui.add(
                            egui::TextEdit::multiline(&mut ed.description)
                                .desired_rows(3)
                                .desired_width(320.0),
                        );
                        ui.end_row();
                        ui.label("Type");
                        ComboBox::from_id_salt("seq_kind")
                            .width(60.0)
                            .selected_text(ed.kind.label())
                            .show_ui(ui, |ui| {
                                for k in TokenKind::ALL {
                                    ui.selectable_value(&mut ed.kind, k, k.label());
                                }
                            });
                        ui.end_row();
                    });
                ui.label("Sequence (click and type; Backspace deletes)");
                ui.set_min_width(440.0);
                if input_line_ui(ui, &mut ed.line, ed.kind, &mut ed.history) == InputAction::Submit
                {
                    ok = true;
                }
                if let Err(e) = ed.line.to_bytes() {
                    ui.colored_label(ui.visuals().warn_fg_color, e.to_string());
                }
                ui.separator();
                ui.horizontal(|ui| {
                    if ui.button("OK").clicked() {
                        ok = true;
                    }
                    if ui.button("Cancel").clicked() {
                        cancel = true;
                    }
                });
            });
        if ok {
            let ed = self.editor.take().expect("editor is open");
            let item = SequenceItem {
                name: if ed.name.trim().is_empty() {
                    "Unnamed".into()
                } else {
                    ed.name.trim().into()
                },
                description: ed.description,
                tokens: ed.line.tokens().to_vec(),
            };
            let items = &mut self.files[ed.file].file.items;
            match ed.index {
                Some(i) => items[i] = item,
                None => items.push(item),
            }
            return self.save(ed.file);
        }
        if cancel || !open {
            self.editor = None;
        }
        SeqAction::None
    }

    fn load_dialog(&mut self) -> SeqAction {
        let Some(path) = rfd::FileDialog::new()
            .set_title("Load sequence file")
            .add_filter("Sequence files", &["hts"])
            .pick_file()
        else {
            return SeqAction::None;
        };
        match self.load(&path) {
            Ok(()) => SeqAction::Info(format!("Loaded {}", path.display())),
            Err(e) => SeqAction::Error(e),
        }
    }

    fn new_file(&mut self) -> SeqAction {
        let Some(mut path) = rfd::FileDialog::new()
            .set_title("New sequence file")
            .add_filter("Sequence files", &["hts"])
            .set_file_name("sequences.hts")
            .save_file()
        else {
            return SeqAction::None;
        };
        if path.extension().is_none() {
            path.set_extension("hts");
        }
        self.files.retain(|f| f.path != path);
        self.files.push(OpenFile {
            path,
            file: SequenceFile::default(),
        });
        self.save(self.files.len() - 1)
    }
}

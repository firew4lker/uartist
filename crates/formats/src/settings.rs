//! The app's own settings file (TOML).
//!
//! Stored at the platform config location, e.g.
//! `~/.config/hterm-open/settings.toml` on Linux and
//! `%APPDATA%\hterm-open\config\settings.toml` on Windows.
//!
//! Every field has a default, so older or hand-edited files with missing or
//! unknown keys still load. Values are stored as plain labels ("Even", "1.5",
//! "HEX") so the file is easy to read and edit.

use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

pub const FILE_NAME: &str = "settings.toml";

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub port: PortSection,
    pub view: ViewSection,
    pub input: InputSection,
    pub save_output: SaveOutputSection,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct PortSection {
    /// Last port the user chose. Restored into the port box; never auto-connected.
    pub path: String,
    /// Baud rate as typed (may be a custom value).
    pub baud: String,
    pub data_bits: u8,
    pub stop_bits: String,
    pub parity: String,
    pub cts_flow: bool,
    /// Glob patterns used to find ports (Unix).
    pub patterns: Vec<String>,
}

impl Default for PortSection {
    fn default() -> Self {
        Self {
            path: String::new(),
            baud: hterm_core::settings::DEFAULT_BAUD.to_string(),
            data_bits: 8,
            stop_bits: "1".into(),
            parity: "None".into(),
            cts_flow: false,
            patterns: hterm_core::ports::default_patterns(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct FormatFlags {
    pub ascii: bool,
    pub hex: bool,
    pub dec: bool,
    pub bin: bool,
}

impl Default for FormatFlags {
    fn default() -> Self {
        Self {
            ascii: true,
            hex: false,
            dec: false,
            bin: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ViewSection {
    pub receive_formats: FormatFlags,
    pub transmit_formats: FormatFlags,
    pub autoscroll: bool,
    /// Name of the "Newline at" preset.
    pub newline_at: String,
    pub show_newline: bool,
    pub newline_on_source_change: bool,
    /// 0 = off.
    pub newline_every: usize,
    /// 0 = off.
    pub newline_pause_ms: u32,
    /// Byte value counted by "Count".
    pub count_byte: u8,
}

impl Default for ViewSection {
    fn default() -> Self {
        Self {
            receive_formats: FormatFlags::default(),
            transmit_formats: FormatFlags::default(),
            autoscroll: true,
            newline_at: "None".into(),
            show_newline: true,
            newline_on_source_change: true,
            newline_every: 0,
            newline_pause_ms: 0,
            count_byte: 0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct InputSection {
    /// Input type label: ASC, HEX, DEC, BIN or CMD.
    pub kind: String,
    /// Name of the "Send on enter" preset.
    pub send_on_enter: String,
}

impl Default for InputSection {
    fn default() -> Self {
        Self {
            kind: "ASC".into(),
            send_on_enter: "None".into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct SaveOutputSection {
    /// Raw, Asc, Hex, Dec or Bin.
    pub format: String,
    /// Received, Transmitted or Both.
    pub source: String,
    pub separator: String,
    pub timestamps: bool,
    /// Last file saved to.
    pub path: String,
}

impl Default for SaveOutputSection {
    fn default() -> Self {
        Self {
            format: "Raw".into(),
            source: "Received".into(),
            separator: String::new(),
            timestamps: false,
            path: String::new(),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum SettingsError {
    #[error("could not read {path}: {source}")]
    Read { path: PathBuf, source: io::Error },
    #[error("{path} is not a valid settings file: {}", source.message().trim())]
    Parse {
        path: PathBuf,
        source: toml::de::Error,
    },
    #[error("could not write {path}: {source}")]
    Write { path: PathBuf, source: io::Error },
}

/// Default location of the settings file, if the platform has a config dir.
pub fn default_path() -> Option<PathBuf> {
    directories::ProjectDirs::from("", "", "hterm-open").map(|d| d.config_dir().join(FILE_NAME))
}

impl Settings {
    /// Load from `path`. A missing file gives the defaults.
    pub fn load(path: &Path) -> Result<Self, SettingsError> {
        let text = match std::fs::read_to_string(path) {
            Ok(t) => t,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(source) => {
                return Err(SettingsError::Read {
                    path: path.to_owned(),
                    source,
                });
            }
        };
        toml::from_str(&text).map_err(|source| SettingsError::Parse {
            path: path.to_owned(),
            source,
        })
    }

    /// Save to `path`, creating the folder if needed. Writes a temporary file
    /// first and renames it, so a crash never leaves a half-written file.
    pub fn save(&self, path: &Path) -> Result<(), SettingsError> {
        let write_err = |source| SettingsError::Write {
            path: path.to_owned(),
            source,
        };
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(write_err)?;
        }
        let text = toml::to_string_pretty(self)
            .map_err(|e| write_err(io::Error::new(io::ErrorKind::InvalidData, e)))?;
        let tmp = path.with_extension("toml.tmp");
        std::fs::write(&tmp, text).map_err(write_err)?;
        std::fs::rename(&tmp, path).map_err(write_err)
    }
}

/// Keep a copy of a settings file that failed to load, so saving defaults
/// over it doesn't lose the user's edits. Returns the backup path.
pub fn back_up_broken(path: &Path) -> io::Result<PathBuf> {
    let backup = path.with_extension("toml.broken");
    std::fs::copy(path, &backup)?;
    Ok(backup)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_path(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("hterm-open-settings-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir.join("nested").join(FILE_NAME)
    }

    #[test]
    fn missing_file_gives_defaults() {
        let p = temp_path("missing");
        assert_eq!(Settings::load(&p).unwrap(), Settings::default());
    }

    #[test]
    fn round_trip() {
        let p = temp_path("roundtrip");
        let mut s = Settings::default();
        s.port.path = "/dev/ttyUSB0".into();
        s.port.baud = "250000".into();
        s.port.parity = "Even".into();
        s.port.cts_flow = true;
        s.view.receive_formats.hex = true;
        s.view.newline_at = "CR+LF".into();
        s.view.newline_every = 16;
        s.input.kind = "HEX".into();
        s.save(&p).unwrap();
        assert_eq!(Settings::load(&p).unwrap(), s);
        assert!(!p.with_extension("toml.tmp").exists());
        let _ = std::fs::remove_dir_all(p.parent().unwrap().parent().unwrap());
    }

    #[test]
    fn partial_file_fills_in_defaults() {
        let s: Settings = toml::from_str(
            r#"
            [port]
            baud = "9600"
            unknown_key = 1

            [view.receive_formats]
            bin = true
            "#,
        )
        .unwrap();
        assert_eq!(s.port.baud, "9600");
        assert_eq!(s.port.parity, "None");
        assert!(s.view.receive_formats.ascii && s.view.receive_formats.bin);
        assert_eq!(s.input, InputSection::default());
    }

    #[test]
    fn broken_file_is_an_error() {
        let p = temp_path("broken");
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(&p, "this is [not toml").unwrap();
        assert!(matches!(
            Settings::load(&p),
            Err(SettingsError::Parse { .. })
        ));
        let backup = back_up_broken(&p).unwrap();
        assert!(backup.exists());
        let _ = std::fs::remove_dir_all(p.parent().unwrap().parent().unwrap());
    }
}

#[cfg(test)]
mod message_tests {
    use super::*;

    #[test]
    fn parse_error_message_is_one_line() {
        let dir = std::env::temp_dir().join(format!("hterm-open-msg-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join(FILE_NAME);
        std::fs::write(&p, "this is [not toml").unwrap();
        let msg = Settings::load(&p).unwrap_err().to_string();
        assert!(!msg.contains('\n'), "{msg}");
        let _ = std::fs::remove_dir_all(dir);
    }
}

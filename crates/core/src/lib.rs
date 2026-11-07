//! Core logic for hterm-open: serial I/O, data model, formatting and
//! newline handling. Contains no GUI code so it can be unit-tested.
//!
//! Behaviour is specified in `docs/spec/`.

pub mod connection;
pub mod format;
pub mod input;
pub mod lines;
pub mod log;
pub mod mfs;
pub mod output;
pub mod ports;
pub mod settings;

/// Crate version, shown in the About dialog.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

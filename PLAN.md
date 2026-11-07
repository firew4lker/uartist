# Uartist — project plan

Name: **Uartist** (binary `uartist`). License: **GPL-3.0-or-later**.
Stack: **Rust + egui/eframe**. Targets: **Linux + Windows** (macOS later, code kept portable).

## Ground rules (clean-room)

- The original `hterm` binary is a **reference for behaviour only**. It never goes into the repo.
- Analysis (running it, ltrace/strace, Ghidra) produces **written specs** in `docs/spec/`
  describing *what* it does: options, UI, file formats, edge cases.
- Code is written from those specs. Decompiled code is never copied or translated into the project.
- Ghidra use is limited to questions observation can't answer (file-format fields, exact
  newline/clear semantics, input escape syntax).

## Architecture

Cargo workspace, GUI kept thin so the logic is unit-testable:

```
crates/
  core/     serial worker, data model, formatting, newline/clear logic, input parsing (no GUI)
  formats/  own settings format (TOML) + importers for HTerm hterm.cfg / .hts (XML)
  app/      eframe/egui binary: layout, widgets, dialogs, settings persistence
docs/spec/  behaviour specs written from analysis
```

- **Serial worker thread** owns the port (`serialport` crate): reads into chunks tagged with
  timestamp + direction (RX/TX), polls CTS/DSR/RI/DCD, applies RTS/DTR, writes queued data.
  Talks to the UI over `crossbeam-channel`; calls `ctx.request_repaint()` on new data.
- **Data model**: append-only byte log of tagged chunks. Views (ASCII/hex/dec/bin, combined
  TX/RX) are *renderings* of this log, so switching format or newline rules re-renders without
  data loss. Large logs use virtualized rows (`ScrollArea::show_rows`).
- Key crates: `eframe`, `serialport`, `crossbeam-channel`, `serde`, `toml`, `quick-xml`,
  `rfd` (file dialogs), `directories` (config path), `thiserror`/`anyhow`, `tracing`.

## Phases

### Phase 0 — Project setup
- `git init`, `.gitignore` (excludes `hterm`, `target/`), GPL-3.0 `LICENSE`, `README.md`.
- Workspace skeleton that builds and opens an empty egui window.
- GitHub Actions CI: build + `clippy` + tests on Linux and Windows.

### Phase 1 — Behaviour spec (analysis)
- Virtual port pair: `socat -d -d pty,raw,echo=0 pty,raw,echo=0`; run HTerm on one end,
  script data on the other.
- Document every panel and option in `docs/spec/`: connection settings, control/status lines,
  receive formats, newline options (at string / every N / by pause / on source change),
  clear-at, input types and escape syntax (`\c…` etc.), send-on-enter, counters, save output.
- `strace`/`ltrace`: exact termios/ioctl usage (baud mapping, flow control, TIOCMGET, TIOCGICOUNT).
- Capture sample `hterm.cfg` / `.hts` files and document their XML schema.

### Phase 2 — Core library (`crates/core`) — mostly done

Done: `settings`, `ports`, `connection` (worker thread), `log`, `format`, `lines`,
`input` (+ history), `mfs`. 29 unit tests, a socat integration test, and an opt-in
hardware loopback test (`UARTIST_TEST_PORT`).

Decisions:
- `/dev/pts/*` is not in the default port patterns (it lists the user's own terminals).
- Status lines are polled every 50 ms (the `serialport` crate has no wait-for-change call).
- **Not yet supported:** Mark/Space parity and 1.5 stop bits (not in the `serialport` crate;
  needs our own termios/Win32 code). The CMD input type is still unknown. Opening a port
  with these settings or sending a CMD token gives a clear "not supported yet" error.

Remaining: Clear at, receive bitmask, save output.

Original scope:
- Port enumeration (+ configurable patterns like `/dev/ttyUSB*`, `/dev/ttyACM*`, `COMx`),
  open/close, baud/data/parity/stop, CTS flow control, RTS/DTR, status-line polling, error counters.
- Formatters: ASCII (control chars shown), hex, decimal, binary; column/separator options.
- Newline engine and clear-at rules; input parser for each input type.
- Thorough unit tests + Linux pty integration tests.

### Phase 3 — GUI → **v0.1 "Core terminal"** — features done

Done (`crates/app`): connection bar (editable port and baud boxes), counters, Count byte,
newline controls, receive and transmit views (ASCII and colour grid, wrapped, virtualized
rows), chip input line with types, history (Up/Down) and send-on-enter, DTR/RTS toggles,
status LEDs, status bar. Checked on a socat pair and the FT232R loopback.

Decision: no port is pre-selected at startup (HTerm picks the first one).

Settings: saved as TOML (`uartist-formats::settings`) in the platform config folder
(`~/.config/uartist/settings.toml` on Linux; `UARTIST_CONFIG` overrides). Written on
connect, on exit (window close or Ctrl+Q) and every 5 s when something changed. The last
chosen port is restored but never auto-connected. A broken file is kept as
`settings.toml.broken` and defaults are used.

Save output: dialog with format (Raw/Asc/Hex/Dec/Bin), source (Received/Transmitted/Both),
separator and timestamps, matching HTerm's file layout (`docs/spec/save-output.md`), with
deliberate differences: the source really filters, Raw has no headers, Asc keeps line breaks.

Selection and copy: click, Shift+click or drag selects bytes; selection bar shows
`Selection (first:last) count - Timediff` and the last byte's timestamp, like HTerm;
right-click copies as Ascii/Hex/Decimal/Binary; Ctrl+C copies in the view's first format.

All v0.1 features are in.

Packaging (v0.1.0): icon (`assets/`), desktop entry, `packaging/build-linux.sh` (tarball
with a /usr layout), `packaging/build-windows.sh` (cross-built zip), AUR PKGBUILD
(`packaging/aur`, tested with makepkg) and `.github/workflows/release.yml` (builds Linux
on ubuntu-22.04 and Windows natively on a `v*` tag, publishes a GitHub release).

Remaining before releasing v0.1: a GitHub repository (the PKGBUILD has `OWNER` as a
placeholder), a first CI run there, then tag `v0.1.0`.

Original scope:
- Connection bar (port, baud, data, stop, parity, connect/disconnect, refresh).
- Control/status lines panel (RTS/DTR toggles, CTS/DSR/RI/DCD indicators).
- Receive view with format toggles, autoscroll, newline + clear options, RX/TX byte counters.
- Transmitted-data view; input line with type selector and send-on-enter options.
- Save output to file; settings persisted as TOML (autosave on exit).
- Release v0.1: Linux binary + AUR PKGBUILD, Windows zip.

### Phase 4 — HTerm import
- Import `hterm.cfg` settings and `.hts` sequence files into our formats.

### Phase 5 — Toward parity (post v0.1)
- Sequences (overview, editor, send), send file with progress/time estimates,
  autosend with delay/repetitions, combined TX/RX view, newline-by-pause, ruler,
  highlight receive errors, logs pane, timestamps, copy as ASCII/hex/dec/bin,
  connect on focus / disconnect on focus loss, always-on-top.

### Phase 6 — Packaging & polish
- AppImage, Windows installer (MSI/NSIS), icons, docs/screenshots; macOS build.

## Testing strategy
- Unit tests in `core` and `formats` (formatters, newline engine, parsers, importers).
- Integration tests over Linux pty pairs (open, send, receive, line control).
- Manual side-by-side comparison against original HTerm using a socat pair.

## Risks
- egui render cost with very large logs → virtualized rows, capped scrollback option.
- Platform differences in status-line support (virtual ports lack TIOCMGET) → degrade gracefully.
- Exact HTerm format semantics → resolved in Phase 1 before coding them.

## Open decisions (later)
- Final project name and icon.
- Whether our own sequence format stays TOML or becomes something else.

## v0.2 (in progress, uncommitted)

Done: send dialog (Autosend + Send file), sequences panel with `.hts` load/edit/save,
Mark/Space parity and 1.5 stop bits (native code in `crates/core/src/native.rs`),
Clear at, receive bitmask, combined view, HTerm `hterm.cfg` import (Settings menu),
polish (virtual-port warnings, CR/LF/NUL symbols, LEDs moved to first row).
Specs: `docs/spec/send-dialog.md`, `docs/spec/sequences.md`, views.md additions.

Not yet done: GUI test of the HTerm config import, Windows run of the native parity
code, README/feature list update, commit, tag v0.2.0.

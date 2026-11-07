# Uartist

An open-source, cross-platform serial terminal for Linux and Windows, written in
Rust with [egui](https://github.com/emilk/egui). It aims to offer the feature set
of the freeware HTerm by Tobias Hammer. It is an independent clean-room
implementation and is not affiliated with HTerm.

> Status: v0.2 in preparation. See [PLAN.md](PLAN.md).

## Features

- Port settings: baud (presets or any value), data bits, stop bits (1, 1.5, 2),
  parity (None, Even, Odd, Mark, Space), CTS flow control
- DTR/RTS toggles and CTS/DSR/RI/DCD indicators
- Received and transmitted views in ASCII, hex, decimal and binary (colour grid),
  optionally combined in one view
- Newline rules: at a sequence (CR, LF, CR+LF, …), every N bytes, after a pause,
  on direction change; Clear at a byte; receive bitmask for 7-bit devices
- Input line that mixes ASCII, hex, decimal and binary values, with history and
  send-on-enter suffixes
- Autosend (repeat the input line) and Send file, with repetitions, delay and progress
- Sequences: saved messages sent with a double-click, in HTerm's `.hts` format
- Counters, including a counter for one chosen byte value
- Select bytes and copy them as ASCII, hex, decimal or binary, with timing information
- Save output as raw bytes or text, with optional timestamps
- Import settings from HTerm's `hterm.cfg`
- Light and dark themes; settings saved automatically

Not yet supported: HTerm's CMD input type.

## Install

**Linux (tarball):** download `uartist-<version>-x86_64-linux.tar.gz` from the
[releases page](https://github.com/firew4lker/uartist/releases) and unpack it into `/usr/local`:

```bash
sudo tar -xzf uartist-0.1.0-x86_64-linux.tar.gz -C /usr/local --strip-components=2
```

**Arch Linux:** a PKGBUILD is in [packaging/aur](packaging/aur/PKGBUILD).

**Windows:** download `uartist-<version>-x86_64-windows.zip`, unpack it and run `uartist.exe`.

On Linux, your user must be in the `uucp` (Arch) or `dialout` (Debian/Ubuntu)
group to open serial ports.

## Building

```bash
cargo run --release -p uartist
```

Linux needs the usual X11/Wayland development libraries.

Packages:

- `packaging/build-linux.sh` builds `dist/uartist-<version>-<arch>-linux.tar.gz`.
- `packaging/build-windows.sh` cross-builds a Windows zip from Linux
  (needs `mingw-w64-gcc` and `rustup target add x86_64-pc-windows-gnu`).
- Pushing a tag like `v0.1.0` to GitHub runs `.github/workflows/release.yml`,
  which builds both and publishes a GitHub release.

## Layout

- `crates/core` — serial I/O, data model, formatting (no GUI)
- `crates/formats` — settings file and (later) HTerm file importers
- `crates/app` — the egui application (`uartist`)
- `docs/spec` — behaviour specifications written from black-box analysis
- `assets`, `packaging` — icon, desktop entry, packaging scripts, PKGBUILD

## Clean-room policy

Contributions must not contain code copied or translated from the original
HTerm binary. Behaviour is documented in `docs/spec/` first and implemented
from those specs.

## License

GPL-3.0-or-later. See [LICENSE](LICENSE).

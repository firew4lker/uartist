# hterm-open

An open-source, cross-platform serial terminal for Linux and Windows, written in
Rust with [egui](https://github.com/emilk/egui). It aims to offer the feature set
of the freeware HTerm by Tobias Hammer. It is an independent clean-room
implementation and is not affiliated with HTerm.

> Status: early development (Phase 0). See [PLAN.md](PLAN.md).

## Building

```bash
cargo run -p hterm-open
```

Linux needs the usual X11/Wayland development libraries. On Linux, your user
must be in the `uucp` (Arch) or `dialout` (Debian/Ubuntu) group to open serial
ports.

## Layout

- `crates/core` — serial I/O, data model, formatting (no GUI)
- `crates/formats` — settings format and HTerm file importers
- `crates/app` — the egui application
- `docs/spec` — behaviour specifications written from black-box analysis

## Clean-room policy

Contributions must not contain code copied or translated from the original
HTerm binary. Behaviour is documented in `docs/spec/` first and implemented
from those specs.

## License

GPL-3.0-or-later. See [LICENSE](LICENSE).

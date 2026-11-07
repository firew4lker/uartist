//! Hardware test for a USB-serial adapter wired as a loopback:
//!
//! ```text
//! TXD ── RXD     DTR ──┬── DSR     RTS ──┬── CTS
//!                      └── DCD           └── RI
//! ```
//!
//! Opt-in: `UARTIST_TEST_PORT=/dev/ttyUSB0 cargo test -p uartist-core --test hardware_loopback -- --ignored`

use std::time::{Duration, Instant};

use uartist_core::connection::{Connection, Event, ModemLines};
use uartist_core::settings::PortSettings;

fn latest_lines(conn: &Connection, want: ModemLines) -> ModemLines {
    let deadline = Instant::now() + Duration::from_secs(2);
    let mut last = ModemLines::default();
    while Instant::now() < deadline {
        for e in conn.poll_events() {
            if let Event::Lines(l) = e {
                last = l;
            }
        }
        if last == want {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    last
}

#[test]
#[ignore = "needs a wired USB-serial adapter; set UARTIST_TEST_PORT"]
fn status_lines_and_data_loopback() {
    let path = std::env::var("UARTIST_TEST_PORT").expect("set UARTIST_TEST_PORT");
    let conn = Connection::open(PortSettings::new(path), || {}).expect("open");

    let off = ModemLines::default();
    let dtr = ModemLines {
        dsr: true,
        dcd: true,
        ..off
    };
    let rts = ModemLines {
        cts: true,
        ri: true,
        ..off
    };
    let both = ModemLines {
        cts: true,
        dsr: true,
        ri: true,
        dcd: true,
    };

    assert_eq!(latest_lines(&conn, off), off, "lines start off");
    conn.set_dtr(true);
    assert_eq!(latest_lines(&conn, dtr), dtr, "DTR -> DSR + DCD");
    conn.set_rts(true);
    assert_eq!(latest_lines(&conn, both), both, "DTR + RTS -> all");
    conn.set_dtr(false);
    assert_eq!(latest_lines(&conn, rts), rts, "RTS -> CTS + RI");
    conn.set_rts(false);
    assert_eq!(latest_lines(&conn, off), off, "all off");

    conn.send(b"loopback".to_vec());
    let deadline = Instant::now() + Duration::from_secs(2);
    let mut got = Vec::new();
    while got.len() < 8 && Instant::now() < deadline {
        for e in conn.poll_events() {
            if let Event::Received { data, .. } = e {
                got.extend(data);
            }
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(got, b"loopback");
}

#[test]
#[ignore = "needs a wired USB-serial adapter; set UARTIST_TEST_PORT"]
fn hardware_flow_control_keeps_rts_on() {
    let path = std::env::var("UARTIST_TEST_PORT").expect("set UARTIST_TEST_PORT");
    let mut settings = PortSettings::new(path);
    settings.cts_flow = true;
    let conn = Connection::open(settings, || {}).expect("open");

    // RTS on at open, so (via RTS -> CTS) data can flow.
    let rts = ModemLines {
        cts: true,
        ri: true,
        ..ModemLines::default()
    };
    assert_eq!(latest_lines(&conn, rts), rts, "RTS is on with flow control");

    conn.send(b"flow".to_vec());
    let deadline = Instant::now() + Duration::from_secs(2);
    let mut got = Vec::new();
    while got.len() < 4 && Instant::now() < deadline {
        for e in conn.poll_events() {
            if let Event::Received { data, .. } = e {
                got.extend(data);
            }
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(got, b"flow");
}

/// Full check of Mark/Space parity and 1.5 stop bits on a real port, read
/// back with `stty` after closing (the port is opened exclusively).
#[test]
#[ignore = "needs a USB-serial adapter; set UARTIST_TEST_PORT"]
#[cfg(target_os = "linux")]
fn mark_space_parity_and_one_and_a_half_stop_bits() {
    use uartist_core::settings::{DataBits, Parity, StopBits};
    let path = std::env::var("UARTIST_TEST_PORT").expect("set UARTIST_TEST_PORT");
    let stty = || {
        let out = std::process::Command::new("stty")
            .args(["-F", &path, "-a"])
            .output()
            .expect("stty");
        String::from_utf8_lossy(&out.stdout).into_owned()
    };
    let has = |flags: &str, f: &str| flags.split_whitespace().any(|w| w == f);

    for (parity, parodd) in [(Parity::Mark, true), (Parity::Space, false)] {
        let mut s = PortSettings::new(path.clone());
        s.parity = parity;
        Connection::open(s, || {}).expect("open").close();
        let f = stty();
        assert!(
            has(&f, "parenb") && has(&f, "cmspar") && has(&f, "inpck"),
            "{parity:?}: {f}"
        );
        assert_eq!(has(&f, "parodd"), parodd, "{parity:?}: {f}");
    }

    let mut s = PortSettings::new(path.clone());
    s.data_bits = DataBits::Five;
    s.stop_bits = StopBits::OnePointFive;
    Connection::open(s, || {}).expect("open").close();
    let f = stty();
    assert!(has(&f, "cs5") && has(&f, "cstopb"), "{f}");
}

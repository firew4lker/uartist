//! End-to-end test of `Connection` over a socat virtual port pair.
//! Skipped when socat isn't installed (or not on Unix).

#![cfg(unix)]

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use hterm_core::connection::{Connection, Event};
use hterm_core::settings::PortSettings;

struct Socat {
    child: Child,
    dir: PathBuf,
}

impl Drop for Socat {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn start_socat() -> Option<(Socat, PathBuf, PathBuf)> {
    // Tests run in parallel: each needs its own folder.
    static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("hterm-open-test-{}-{n}", std::process::id()));
    std::fs::create_dir_all(&dir).ok()?;
    let (a, b) = (dir.join("a"), dir.join("b"));
    let child = Command::new("socat")
        .arg(format!("pty,raw,echo=0,link={}", a.display()))
        .arg(format!("pty,raw,echo=0,link={}", b.display()))
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let socat = Socat { child, dir };
    let deadline = Instant::now() + Duration::from_secs(5);
    while !(a.exists() && b.exists()) {
        if Instant::now() > deadline {
            return None;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    Some((socat, a, b))
}

fn wait_for<T>(conn: &Connection, mut f: impl FnMut(&Event) -> Option<T>) -> T {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        for e in conn.poll_events() {
            if let Some(v) = f(&e) {
                return v;
            }
        }
        assert!(Instant::now() < deadline, "timed out waiting for event");
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn read_exact_timeout(path: &Path, n: usize) -> Vec<u8> {
    let path = path.to_path_buf();
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut f = std::fs::File::open(path).unwrap();
        let mut buf = vec![0; n];
        f.read_exact(&mut buf).unwrap();
        let _ = tx.send(buf);
    });
    rx.recv_timeout(Duration::from_secs(5))
        .expect("timed out reading other end")
}

#[test]
fn send_and_receive_over_pty_pair() {
    let Some((_socat, a, b)) = start_socat() else {
        eprintln!("socat not available; skipping");
        return;
    };
    let conn = Connection::open(PortSettings::new(a.to_string_lossy()), || {}).expect("open");

    // HTerm-style: virtual ports can't report status lines.
    wait_for(&conn, |e| {
        matches!(e, Event::LinesUnavailable).then_some(())
    });

    // Tx: what we send arrives at the other end and is reported as Sent.
    let reader = std::thread::spawn({
        let b = b.clone();
        move || read_exact_timeout(&b, 5)
    });
    std::thread::sleep(Duration::from_millis(100));
    conn.send(b"hello".to_vec());
    assert_eq!(reader.join().unwrap(), b"hello");
    let sent = wait_for(&conn, |e| match e {
        Event::Sent { data, .. } => Some(data.clone()),
        _ => None,
    });
    assert_eq!(sent, b"hello");

    // Rx: what the other end writes is reported as Received.
    let mut other = std::fs::OpenOptions::new().write(true).open(&b).unwrap();
    other.write_all(b"world\r\n").unwrap();
    let mut got = Vec::new();
    while got.len() < 7 {
        got.extend(wait_for(&conn, |e| match e {
            Event::Received { data, .. } => Some(data.clone()),
            _ => None,
        }));
    }
    assert_eq!(got, b"world\r\n");

    let last = conn.close();
    assert_eq!(last.last(), Some(&Event::Closed));
}

#[test]
fn unsupported_settings_fail_to_open() {
    let mut s = PortSettings::new("/dev/null");
    s.parity = hterm_core::settings::Parity::Mark;
    let err = Connection::open(s, || {}).err().expect("should fail");
    assert!(err.to_string().contains("Mark parity"));
}

#[test]
fn closes_when_the_other_side_goes_away() {
    let Some((socat, a, _b)) = start_socat() else {
        eprintln!("socat not available; skipping");
        return;
    };
    let conn = Connection::open(PortSettings::new(a.to_string_lossy()), || {}).expect("open");
    std::thread::sleep(Duration::from_millis(100));
    drop(socat); // like unplugging the adapter

    let mut saw_error = false;
    wait_for(&conn, |e| {
        saw_error |= matches!(e, Event::Error(_));
        matches!(e, Event::Closed).then_some(())
    });
    assert!(saw_error, "an Error event comes before Closed");
}

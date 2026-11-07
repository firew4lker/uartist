//! Finding serial ports.
//!
//! On Unix, ports are found by glob patterns like HTerm's `ComportPatterns`.
//! Unlike HTerm, `/dev/pts/*` is not in the default list: those are usually
//! the user's own terminals, and connecting to one takes it over. Users can add
//! it back (handy for socat virtual ports).

/// Default patterns for Unix-like systems.
pub const DEFAULT_PATTERNS: [&str; 3] = ["/dev/ttyS*", "/dev/ttyUSB*", "/dev/ttyACM*"];

pub fn default_patterns() -> Vec<String> {
    DEFAULT_PATTERNS.iter().map(|s| s.to_string()).collect()
}

/// List available ports, sorted naturally (`ttyUSB2` before `ttyUSB10`).
///
/// `patterns` are glob patterns on Unix. On Windows they are ignored and the
/// system's list of COM ports is used.
pub fn list_ports(patterns: &[String]) -> Vec<String> {
    let mut ports = platform_ports(patterns);
    ports.sort_by(|a, b| natural_cmp(a, b));
    ports.dedup();
    ports
}

#[cfg(unix)]
fn platform_ports(patterns: &[String]) -> Vec<String> {
    let mut out = Vec::new();
    for pattern in patterns {
        let Ok(paths) = glob::glob(pattern) else {
            continue;
        };
        for path in paths.flatten() {
            if is_real_serial_port(&path) {
                out.push(path.to_string_lossy().into_owned());
            }
        }
    }
    out
}

/// `/dev/ttyS0`..`ttyS31` always exist on Linux even without hardware.
/// Skip the ones whose driver reports no UART behind them.
#[cfg(unix)]
fn is_real_serial_port(path: &std::path::Path) -> bool {
    let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
        return false;
    };
    if !name.starts_with("ttyS") {
        return true;
    }
    #[cfg(target_os = "linux")]
    {
        // Legacy 8250 ports without hardware show "type" 0 (PORT_UNKNOWN).
        let type_file = format!("/sys/class/tty/{name}/type");
        match std::fs::read_to_string(type_file) {
            Ok(t) => t.trim() != "0",
            Err(_) => true,
        }
    }
    #[cfg(not(target_os = "linux"))]
    {
        true
    }
}

#[cfg(not(unix))]
fn platform_ports(_patterns: &[String]) -> Vec<String> {
    serialport::available_ports()
        .map(|ports| ports.into_iter().map(|p| p.port_name).collect())
        .unwrap_or_default()
}

/// Compare strings so that digit runs compare by value.
fn natural_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    fn split(s: &str) -> Vec<(bool, &str)> {
        let mut parts = Vec::new();
        let mut start = 0;
        let bytes = s.as_bytes();
        for i in 1..=bytes.len() {
            if i == bytes.len() || bytes[i].is_ascii_digit() != bytes[start].is_ascii_digit() {
                parts.push((bytes[start].is_ascii_digit(), &s[start..i]));
                start = i;
            }
        }
        parts
    }
    let (pa, pb) = (split(a), split(b));
    for (x, y) in pa.iter().zip(pb.iter()) {
        let ord = match (x, y) {
            ((true, xs), (true, ys)) => {
                let xn: u128 = xs.parse().unwrap_or(0);
                let yn: u128 = ys.parse().unwrap_or(0);
                xn.cmp(&yn).then(xs.len().cmp(&ys.len()))
            }
            ((_, xs), (_, ys)) => xs.cmp(ys),
        };
        if ord.is_ne() {
            return ord;
        }
    }
    pa.len().cmp(&pb.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn natural_sort() {
        let mut v = vec![
            "/dev/ttyUSB10",
            "/dev/ttyACM0",
            "/dev/ttyUSB2",
            "/dev/ttyUSB1",
        ];
        v.sort_by(|a, b| natural_cmp(a, b));
        assert_eq!(
            v,
            [
                "/dev/ttyACM0",
                "/dev/ttyUSB1",
                "/dev/ttyUSB2",
                "/dev/ttyUSB10"
            ]
        );
    }

    #[test]
    fn defaults_exclude_pts() {
        assert!(!default_patterns().iter().any(|p| p.contains("pts")));
    }
}

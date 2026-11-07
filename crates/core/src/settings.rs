//! Serial port settings. See `docs/spec/connection.md`.

use std::fmt;

/// Baud rates offered in the dropdown. Any other positive value can be typed in.
pub const BAUD_RATES: [u32; 30] = [
    50, 75, 110, 134, 150, 200, 300, 600, 1200, 1800, 2400, 4800, 9600, 19200, 38400, 57600,
    115200, 230400, 460800, 500000, 576000, 921600, 1000000, 1152000, 1500000, 2000000, 2500000,
    3000000, 3500000, 4000000,
];

pub const DEFAULT_BAUD: u32 = 115200;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DataBits {
    Five,
    Six,
    Seven,
    #[default]
    Eight,
}

impl DataBits {
    pub const ALL: [DataBits; 4] = [Self::Five, Self::Six, Self::Seven, Self::Eight];

    pub fn count(self) -> u8 {
        match self {
            Self::Five => 5,
            Self::Six => 6,
            Self::Seven => 7,
            Self::Eight => 8,
        }
    }

    pub fn from_count(n: u8) -> Option<Self> {
        Self::ALL.into_iter().find(|d| d.count() == n)
    }
}

impl fmt::Display for DataBits {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.count())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StopBits {
    #[default]
    One,
    OnePointFive,
    Two,
}

impl StopBits {
    pub const ALL: [StopBits; 3] = [Self::One, Self::OnePointFive, Self::Two];

    pub fn label(self) -> &'static str {
        match self {
            Self::One => "1",
            Self::OnePointFive => "1.5",
            Self::Two => "2",
        }
    }

    pub fn from_label(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|v| v.label() == s.trim())
    }
}

impl fmt::Display for StopBits {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.label())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Parity {
    #[default]
    None,
    Even,
    Odd,
    Mark,
    Space,
}

impl Parity {
    pub const ALL: [Parity; 5] = [Self::None, Self::Even, Self::Odd, Self::Mark, Self::Space];

    pub fn label(self) -> &'static str {
        match self {
            Self::None => "None",
            Self::Even => "Even",
            Self::Odd => "Odd",
            Self::Mark => "Mark",
            Self::Space => "Space",
        }
    }

    pub fn from_label(s: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|v| v.label().eq_ignore_ascii_case(s.trim()))
    }
}

impl fmt::Display for Parity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.label())
    }
}

/// Everything needed to open a port.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PortSettings {
    pub path: String,
    pub baud: u32,
    pub data_bits: DataBits,
    pub stop_bits: StopBits,
    pub parity: Parity,
    /// Hardware RTS/CTS handshake.
    pub cts_flow: bool,
}

impl PortSettings {
    pub fn new(path: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            baud: DEFAULT_BAUD,
            data_bits: DataBits::default(),
            stop_bits: StopBits::default(),
            parity: Parity::default(),
            cts_flow: false,
        }
    }

    /// Short form for the status bar, e.g. `b:115200 d:8 s:1 p:None`.
    pub fn summary(&self) -> String {
        format!(
            "b:{} d:{} s:{} p:{}",
            self.baud, self.data_bits, self.stop_bits, self.parity
        )
    }
}

/// Parse a typed baud rate. Must be a whole number greater than zero.
pub fn parse_baud(s: &str) -> Option<u32> {
    s.trim().parse::<u32>().ok().filter(|&b| b > 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn summary_matches_hterm_status_bar() {
        assert_eq!(
            PortSettings::new("/dev/ttyUSB0").summary(),
            "b:115200 d:8 s:1 p:None"
        );
    }

    #[test]
    fn labels_round_trip() {
        for p in Parity::ALL {
            assert_eq!(Parity::from_label(p.label()), Some(p));
        }
        for s in StopBits::ALL {
            assert_eq!(StopBits::from_label(s.label()), Some(s));
        }
        for d in DataBits::ALL {
            assert_eq!(DataBits::from_count(d.count()), Some(d));
        }
    }

    #[test]
    fn baud_parsing() {
        assert_eq!(parse_baud(" 9600 "), Some(9600));
        assert_eq!(parse_baud("0"), None);
        assert_eq!(parse_baud("-5"), None);
        assert_eq!(parse_baud("abc"), None);
    }
}

//! Splitting the byte stream into display lines (`docs/spec/newline.md`).
//!
//! Newlines are display-only. A [`LineBuilder`] is fed chunks as they arrive;
//! when a rule changes, build a new one from the whole log.

use std::time::{Duration, Instant};

use crate::log::{Chunk, Direction};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LineRules {
    /// Break after this sequence ("Newline at"). Empty = off.
    pub newline_at: Vec<u8>,
    /// Show the bytes of the matched `newline_at` sequence.
    pub show_newline: bool,
    /// Break after this many bytes ("Newline every … characters"). 0 = off.
    pub every: usize,
    /// Break when data changes direction (combined view).
    pub on_source_change: bool,
    /// Break when no data arrived for this long ("Newline by pause").
    pub pause: Option<Duration>,
    /// Bits kept from received bytes ("Receive bitmask"); 0xFF = all.
    pub rx_mask: u8,
}

/// "Receive bitmask": clear high bits of received bytes in the view.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ReceiveMask {
    #[default]
    None,
    Bit8,
    Bits7To8,
    Bits6To8,
    Bits5To8,
}

impl ReceiveMask {
    pub const ALL: [ReceiveMask; 5] = [
        Self::None,
        Self::Bit8,
        Self::Bits7To8,
        Self::Bits6To8,
        Self::Bits5To8,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::None => "No mask",
            Self::Bit8 => "Mask bit 8",
            Self::Bits7To8 => "Mask bits 7-8",
            Self::Bits6To8 => "Mask bits 6-8",
            Self::Bits5To8 => "Mask bits 5-8",
        }
    }

    pub fn from_label(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|m| m.label() == s.trim())
    }

    /// The bits that are kept.
    pub fn bits(self) -> u8 {
        match self {
            Self::None => 0xFF,
            Self::Bit8 => 0x7F,
            Self::Bits7To8 => 0x3F,
            Self::Bits6To8 => 0x1F,
            Self::Bits5To8 => 0x0F,
        }
    }
}

impl Default for LineRules {
    fn default() -> Self {
        Self {
            newline_at: Vec::new(),
            show_newline: true,
            every: 0,
            on_source_change: true,
            pause: None,
            rx_mask: 0xFF,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cell {
    pub byte: u8,
    pub dir: Direction,
    /// Part of a newline sequence hidden by `show_newline = false`.
    pub hidden: bool,
}

#[derive(Debug, Clone)]
pub struct LineBuilder {
    rules: LineRules,
    /// Always holds at least one (possibly empty) line: the one being filled.
    lines: Vec<Vec<Cell>>,
    last: Option<(Direction, Instant)>,
}

impl LineBuilder {
    pub fn new(rules: LineRules) -> Self {
        Self {
            rules,
            lines: vec![Vec::new()],
            last: None,
        }
    }

    /// Build from existing chunks, e.g. after a rule change.
    pub fn from_chunks<'a>(rules: LineRules, chunks: impl IntoIterator<Item = &'a Chunk>) -> Self {
        let mut b = Self::new(rules);
        for c in chunks {
            b.push_chunk(c.dir, c.at, &c.data);
        }
        b
    }

    pub fn rules(&self) -> &LineRules {
        &self.rules
    }

    /// All lines. The last one is the line currently being filled and may be empty.
    pub fn lines(&self) -> &[Vec<Cell>] {
        &self.lines
    }

    /// Lines without a trailing empty one.
    pub fn visible_lines(&self) -> &[Vec<Cell>] {
        match self.lines.last() {
            Some(l) if l.is_empty() => &self.lines[..self.lines.len() - 1],
            _ => &self.lines,
        }
    }

    pub fn push_chunk(&mut self, dir: Direction, at: Instant, data: &[u8]) {
        if data.is_empty() {
            return;
        }
        if let Some((last_dir, last_at)) = self.last {
            let source_change = self.rules.on_source_change && last_dir != dir;
            let paused = self
                .rules
                .pause
                .is_some_and(|p| at.saturating_duration_since(last_at) >= p);
            if source_change || paused {
                self.break_line();
            }
        }
        for &byte in data {
            self.push_byte(byte, dir);
        }
        self.last = Some((dir, at));
    }

    fn current(&mut self) -> &mut Vec<Cell> {
        self.lines.last_mut().expect("always one line")
    }

    fn break_line(&mut self) {
        if !self.current().is_empty() {
            self.lines.push(Vec::new());
        }
    }

    fn push_byte(&mut self, byte: u8, dir: Direction) {
        let Self { rules, lines, .. } = self;
        let byte = match dir {
            Direction::Rx => byte & rules.rx_mask,
            Direction::Tx => byte,
        };
        let seq = rules.newline_at.as_slice();
        let line = lines.last_mut().expect("always one line");
        line.push(Cell {
            byte,
            dir,
            hidden: false,
        });
        // The sequence must come from one direction: a sent CR followed by a
        // received LF is not a CR+LF.
        let matched = !seq.is_empty()
            && line.len() >= seq.len()
            && line[line.len() - seq.len()..]
                .iter()
                .map(|c| c.byte)
                .eq(seq.iter().copied())
            && line[line.len() - seq.len()..].iter().all(|c| c.dir == dir);
        if matched && !rules.show_newline {
            let n = line.len();
            for c in &mut line[n - seq.len()..] {
                c.hidden = true;
            }
        }
        let full = rules.every > 0 && line.len() >= rules.every;

        if matched || full {
            lines.push(Vec::new());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn render(b: &LineBuilder) -> Vec<String> {
        b.visible_lines()
            .iter()
            .map(|l| {
                l.iter()
                    .filter(|c| !c.hidden)
                    .map(|c| match c.byte {
                        b'\r' => "\\r".to_string(),
                        b'\n' => "\\n".to_string(),
                        b => (b as char).to_string(),
                    })
                    .collect()
            })
            .collect()
    }

    fn rx(rules: LineRules, data: &[u8]) -> LineBuilder {
        let mut b = LineBuilder::new(rules);
        b.push_chunk(Direction::Rx, Instant::now(), data);
        b
    }

    const DATA: &[u8] = b"one\r\ntwo\nthree\rfour\n";

    #[test]
    fn newline_at_lf() {
        let rules = LineRules {
            newline_at: b"\n".to_vec(),
            ..Default::default()
        };
        assert_eq!(
            render(&rx(rules, DATA)),
            ["one\\r\\n", "two\\n", "three\\rfour\\n"]
        );
    }

    #[test]
    fn hidden_newline_keeps_other_controls() {
        let rules = LineRules {
            newline_at: b"\n".to_vec(),
            show_newline: false,
            ..Default::default()
        };
        assert_eq!(render(&rx(rules, DATA)), ["one\\r", "two", "three\\rfour"]);
    }

    #[test]
    fn every_five() {
        let rules = LineRules {
            every: 5,
            ..Default::default()
        };
        assert_eq!(
            render(&rx(rules, DATA)),
            ["one\\r\\n", "two\\nt", "hree\\r", "four\\n"]
        );
    }

    #[test]
    fn crlf_split_across_chunks() {
        let mut b = LineBuilder::new(LineRules {
            newline_at: b"\r\n".to_vec(),
            ..Default::default()
        });
        let now = Instant::now();
        b.push_chunk(Direction::Rx, now, b"ab\r");
        b.push_chunk(Direction::Rx, now, b"\ncd");
        assert_eq!(render(&b), ["ab\\r\\n", "cd"]);
    }

    #[test]
    fn newline_sequence_does_not_span_directions() {
        let mut b = LineBuilder::new(LineRules {
            newline_at: b"\r\n".to_vec(),
            on_source_change: false,
            ..Default::default()
        });
        let now = Instant::now();
        b.push_chunk(Direction::Tx, now, b"a\r");
        b.push_chunk(Direction::Rx, now, b"\nb\r\n");
        assert_eq!(render(&b), ["a\\r\\nb\\r\\n"]);
    }

    #[test]
    fn receive_mask_applies_to_received_bytes_only() {
        let mut b = LineBuilder::new(LineRules {
            rx_mask: ReceiveMask::Bit8.bits(),
            on_source_change: false,
            ..Default::default()
        });
        let now = Instant::now();
        b.push_chunk(Direction::Rx, now, &[0xC1, 0x8A]);
        b.push_chunk(Direction::Tx, now, &[0xC1]);
        let bytes: Vec<u8> = b.lines()[0].iter().map(|c| c.byte).collect();
        assert_eq!(bytes, [0x41, 0x0A, 0xC1]);
        assert_eq!(ReceiveMask::Bits5To8.bits(), 0x0F);
    }

    #[test]
    fn source_change() {
        let mut b = LineBuilder::new(LineRules::default());
        let now = Instant::now();
        b.push_chunk(Direction::Tx, now, b"ping");
        b.push_chunk(Direction::Rx, now, b"pong");
        b.push_chunk(Direction::Rx, now, b"!");
        assert_eq!(render(&b), ["ping", "pong!"]);
    }

    #[test]
    fn pause() {
        let mut b = LineBuilder::new(LineRules {
            pause: Some(Duration::from_millis(100)),
            ..Default::default()
        });
        let t0 = Instant::now();
        b.push_chunk(Direction::Rx, t0, b"a");
        b.push_chunk(Direction::Rx, t0 + Duration::from_millis(50), b"b");
        b.push_chunk(Direction::Rx, t0 + Duration::from_millis(200), b"c");
        assert_eq!(render(&b), ["ab", "c"]);
    }

    #[test]
    fn rebuild_from_chunks_matches_incremental() {
        let now = Instant::now();
        let chunks = vec![
            Chunk {
                dir: Direction::Rx,
                at: now,
                data: b"x\ny".to_vec(),
            },
            Chunk {
                dir: Direction::Rx,
                at: now,
                data: b"z\n".to_vec(),
            },
        ];
        let rules = LineRules {
            newline_at: b"\n".to_vec(),
            ..Default::default()
        };
        let b = LineBuilder::from_chunks(rules, &chunks);
        assert_eq!(render(&b), ["x\\n", "yz\\n"]);
    }
}

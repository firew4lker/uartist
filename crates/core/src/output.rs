//! Writing logged data to a file ("Save output", `docs/spec/save-output.md`).

use std::io::{self, Write};
use std::time::Instant;

use crate::format::{Format, cell_text};
use crate::log::{Chunk, Direction};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OutputFormat {
    /// The exact bytes; never any headers.
    #[default]
    Raw,
    /// Printable ASCII plus CR, LF and TAB; other bytes are dropped.
    Asc,
    Hex,
    Dec,
    Bin,
}

impl OutputFormat {
    pub const ALL: [OutputFormat; 5] = [Self::Raw, Self::Asc, Self::Hex, Self::Dec, Self::Bin];

    pub fn label(self) -> &'static str {
        match self {
            Self::Raw => "Raw",
            Self::Asc => "Asc",
            Self::Hex => "Hex",
            Self::Dec => "Dec",
            Self::Bin => "Bin",
        }
    }

    pub fn from_label(s: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|f| f.label().eq_ignore_ascii_case(s.trim()))
    }

    /// Text formats can carry headers and separators.
    pub fn is_text(self) -> bool {
        self != Self::Raw
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Source {
    #[default]
    Received,
    Transmitted,
    Both,
}

impl Source {
    pub const ALL: [Source; 3] = [Self::Received, Self::Transmitted, Self::Both];

    pub fn label(self) -> &'static str {
        match self {
            Self::Received => "Received",
            Self::Transmitted => "Transmitted",
            Self::Both => "Both",
        }
    }

    pub fn from_label(s: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|v| v.label().eq_ignore_ascii_case(s.trim()))
    }

    fn includes(self, dir: Direction) -> bool {
        match self {
            Self::Received => dir == Direction::Rx,
            Self::Transmitted => dir == Direction::Tx,
            Self::Both => true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct OutputOptions {
    pub format: OutputFormat,
    pub source: Source,
    /// Written between bytes (text formats only).
    pub separator: Vec<u8>,
    /// Header line with the time before each chunk (text formats only).
    pub timestamps: bool,
}

impl OutputOptions {
    /// Each chunk gets a header line: timestamps on, or both directions labelled.
    fn headers(&self) -> bool {
        self.format.is_text() && (self.timestamps || self.source == Source::Both)
    }
}

/// Write `chunks` according to `opts`. `time` formats a chunk's arrival time
/// for headers (e.g. local `HH:MM:SS.mmm`).
pub fn write_output<'a, W: Write>(
    out: &mut W,
    chunks: impl IntoIterator<Item = &'a Chunk>,
    opts: &OutputOptions,
    time: impl Fn(Instant) -> String,
) -> io::Result<()> {
    let chunks = chunks.into_iter().filter(|c| opts.source.includes(c.dir));

    if opts.format == OutputFormat::Raw {
        for c in chunks {
            out.write_all(&c.data)?;
        }
        return Ok(());
    }

    if opts.headers() {
        for (i, c) in chunks.enumerate() {
            if i > 0 {
                out.write_all(b"\n")?;
            }
            let label = match c.dir {
                Direction::Rx => "Received",
                Direction::Tx => "Transmitted",
            };
            let header = match (opts.source == Source::Both, opts.timestamps) {
                (true, true) => format!("{label} - {}:", time(c.at)),
                (false, true) => format!("{}:", time(c.at)),
                (true, false) => label.to_string(),
                (false, false) => unreachable!("headers() is false"),
            };
            out.write_all(header.as_bytes())?;
            out.write_all(b"\n")?;
            let mut first = true;
            write_bytes(out, &c.data, opts, &mut first)?;
        }
    } else {
        // One stream: the separator also goes between chunks.
        let mut first = true;
        for c in chunks {
            write_bytes(out, &c.data, opts, &mut first)?;
        }
    }
    Ok(())
}

fn write_bytes<W: Write>(
    out: &mut W,
    data: &[u8],
    opts: &OutputOptions,
    first: &mut bool,
) -> io::Result<()> {
    for &b in data {
        let item: Option<std::borrow::Cow<'static, str>> = match opts.format {
            OutputFormat::Asc => match b {
                0x20..=0x7E | b'\r' | b'\n' | b'\t' => Some(String::from(b as char).into()),
                _ => None,
            },
            OutputFormat::Hex => Some(cell_text(Format::Hex, b).into()),
            OutputFormat::Dec => Some(cell_text(Format::Dec, b).into()),
            OutputFormat::Bin => Some(cell_text(Format::Bin, b).into()),
            OutputFormat::Raw => unreachable!("handled by the caller"),
        };
        if let Some(item) = item {
            if !*first {
                out.write_all(&opts.separator)?;
            }
            *first = false;
            out.write_all(item.as_bytes())?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    /// The data from docs/spec/save-output.md.
    fn sample() -> (Instant, Vec<Chunk>) {
        let t0 = Instant::now();
        let c = |dir, ms, data: &[u8]| Chunk {
            dir,
            at: t0 + Duration::from_millis(ms),
            data: data.to_vec(),
        };
        (
            t0,
            vec![
                c(Direction::Rx, 0, b"Hello\r\n"),
                c(Direction::Rx, 1400, b"World\0\xff"),
                c(Direction::Tx, 2000, b"xyz"),
            ],
        )
    }

    fn render(opts: OutputOptions) -> String {
        let (t0, chunks) = sample();
        let mut out = Vec::new();
        write_output(&mut out, &chunks, &opts, |at| {
            format!("+{}ms", at.duration_since(t0).as_millis())
        })
        .unwrap();
        String::from_utf8_lossy(&out).into_owned()
    }

    #[test]
    fn raw_is_exact_bytes_without_headers() {
        let (_, chunks) = sample();
        let mut out = Vec::new();
        let opts = OutputOptions {
            format: OutputFormat::Raw,
            source: Source::Both,
            separator: b",".to_vec(),
            timestamps: true,
        };
        write_output(&mut out, &chunks, &opts, |_| String::new()).unwrap();
        assert_eq!(out, b"Hello\r\nWorld\0\xffxyz");
    }

    #[test]
    fn hex_stream_with_separator() {
        let s = render(OutputOptions {
            format: OutputFormat::Hex,
            separator: b",".to_vec(),
            ..Default::default()
        });
        assert_eq!(s, "48,65,6C,6C,6F,0D,0A,57,6F,72,6C,64,00,FF");
    }

    #[test]
    fn dec_and_bin_like_hterm() {
        let s = render(OutputOptions {
            format: OutputFormat::Dec,
            separator: b" ".to_vec(),
            ..Default::default()
        });
        assert!(s.starts_with("072 101 108 108 111 013 010 087"));
        assert!(s.ends_with("000 255"));
        let s = render(OutputFormat::Bin.into());
        assert!(s.starts_with("0100100001100101"));
        assert_eq!(s.len(), 14 * 8);
    }

    #[test]
    fn asc_keeps_line_breaks_and_drops_other_controls() {
        let s = render(OutputFormat::Asc.into());
        assert_eq!(s, "Hello\r\nWorld");
    }

    #[test]
    fn source_filters() {
        let s = render(OutputOptions {
            format: OutputFormat::Asc,
            source: Source::Transmitted,
            ..Default::default()
        });
        assert_eq!(s, "xyz");
    }

    #[test]
    fn headers_with_both_and_timestamps() {
        let s = render(OutputOptions {
            format: OutputFormat::Hex,
            source: Source::Both,
            separator: b" ".to_vec(),
            timestamps: true,
        });
        assert_eq!(
            s,
            "Received - +0ms:\n48 65 6C 6C 6F 0D 0A\n\
             Received - +1400ms:\n57 6F 72 6C 64 00 FF\n\
             Transmitted - +2000ms:\n78 79 7A"
        );
    }

    #[test]
    fn header_variants() {
        let s = render(OutputOptions {
            format: OutputFormat::Hex,
            timestamps: true,
            ..Default::default()
        });
        assert!(s.starts_with("+0ms:\n48"));
        let s = render(OutputOptions {
            format: OutputFormat::Asc,
            source: Source::Both,
            ..Default::default()
        });
        assert!(s.starts_with("Received\nHello"));
    }

    impl From<OutputFormat> for OutputOptions {
        fn from(format: OutputFormat) -> Self {
            Self {
                format,
                ..Default::default()
            }
        }
    }
}

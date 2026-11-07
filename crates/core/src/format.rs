//! How a single byte is shown in each display format (`docs/spec/views.md`).

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Format {
    Ascii,
    Hex,
    Dec,
    Bin,
}

impl Format {
    pub const ALL: [Format; 4] = [Self::Ascii, Self::Hex, Self::Dec, Self::Bin];

    pub fn label(self) -> &'static str {
        match self {
            Self::Ascii => "Ascii",
            Self::Hex => "Hex",
            Self::Dec => "Dec",
            Self::Bin => "Bin",
        }
    }

    /// Width of one cell in characters.
    pub fn cell_width(self) -> usize {
        match self {
            Self::Ascii => 1,
            Self::Hex => 2,
            Self::Dec => 3,
            Self::Bin => 8,
        }
    }
}

/// Which formats a view shows. More than one enabled = grid mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Formats {
    pub ascii: bool,
    pub hex: bool,
    pub dec: bool,
    pub bin: bool,
}

impl Default for Formats {
    fn default() -> Self {
        Self {
            ascii: true,
            hex: false,
            dec: false,
            bin: false,
        }
    }
}

impl Formats {
    pub fn get(&self, f: Format) -> bool {
        match f {
            Format::Ascii => self.ascii,
            Format::Hex => self.hex,
            Format::Dec => self.dec,
            Format::Bin => self.bin,
        }
    }

    pub fn get_mut(&mut self, f: Format) -> &mut bool {
        match f {
            Format::Ascii => &mut self.ascii,
            Format::Hex => &mut self.hex,
            Format::Dec => &mut self.dec,
            Format::Bin => &mut self.bin,
        }
    }

    /// Enabled formats, in display order (Ascii, Hex, Dec, Bin).
    pub fn enabled(&self) -> impl Iterator<Item = Format> + '_ {
        Format::ALL.into_iter().filter(|&f| self.get(f))
    }

    /// Grid mode: one column per byte with a row per format.
    pub fn is_grid(&self) -> bool {
        self.enabled().count() > 1
    }
}

/// How a byte looks in the ASCII format.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AsciiGlyph {
    /// Printable ASCII, shown as itself.
    Char(char),
    /// A control character with its own small symbol (CR, LF, NUL).
    Control(&'static str),
    /// Anything else, shown as an empty box.
    Unprintable,
}

pub fn ascii_glyph(b: u8) -> AsciiGlyph {
    match b {
        0x20..=0x7E => AsciiGlyph::Char(b as char),
        b'\r' => AsciiGlyph::Control("\\r"),
        b'\n' => AsciiGlyph::Control("\\n"),
        0 => AsciiGlyph::Control("\\0"),
        _ => AsciiGlyph::Unprintable,
    }
}

/// Text for a byte in the given format. ASCII control and unprintable bytes
/// return their symbol; the GUI can style those differently.
pub fn cell_text(format: Format, b: u8) -> String {
    match format {
        Format::Ascii => match ascii_glyph(b) {
            AsciiGlyph::Char(c) => c.to_string(),
            AsciiGlyph::Control(s) => s.to_string(),
            AsciiGlyph::Unprintable => "\u{25A1}".to_string(),
        },
        Format::Hex => format!("{b:02X}"),
        Format::Dec => format!("{b:03}"),
        Format::Bin => format!("{b:08b}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_hterm_grid_example() {
        // From the 'H' column in docs/spec/views.md.
        assert_eq!(cell_text(Format::Ascii, b'H'), "H");
        assert_eq!(cell_text(Format::Hex, b'H'), "48");
        assert_eq!(cell_text(Format::Dec, b'H'), "072");
        assert_eq!(cell_text(Format::Bin, b'H'), "01001000");
        assert_eq!(cell_text(Format::Hex, 0xff), "FF");
        assert_eq!(cell_text(Format::Dec, 0), "000");
    }

    #[test]
    fn ascii_glyphs() {
        assert_eq!(ascii_glyph(b'\r'), AsciiGlyph::Control("\\r"));
        assert_eq!(ascii_glyph(0), AsciiGlyph::Control("\\0"));
        assert_eq!(ascii_glyph(0x01), AsciiGlyph::Unprintable);
        assert_eq!(ascii_glyph(0xff), AsciiGlyph::Unprintable);
        assert_eq!(ascii_glyph(b' '), AsciiGlyph::Char(' '));
    }

    #[test]
    fn grid_mode() {
        let mut f = Formats::default();
        assert!(!f.is_grid());
        f.hex = true;
        assert!(f.is_grid());
        assert_eq!(
            f.enabled().collect::<Vec<_>>(),
            [Format::Ascii, Format::Hex]
        );
    }
}

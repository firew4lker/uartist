//! The input line: a sequence of typed tokens (`docs/spec/input.md`).
//!
//! Each token has a kind (ASC, HEX, DEC, BIN, CMD). One line can mix kinds.
//! Characters that don't fit the current kind are dropped while typing.

use std::collections::VecDeque;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum TokenKind {
    #[default]
    Asc,
    Hex,
    Dec,
    Bin,
    Cmd,
}

impl TokenKind {
    pub const ALL: [TokenKind; 5] = [Self::Asc, Self::Hex, Self::Dec, Self::Bin, Self::Cmd];

    pub fn label(self) -> &'static str {
        match self {
            Self::Asc => "ASC",
            Self::Hex => "HEX",
            Self::Dec => "DEC",
            Self::Bin => "BIN",
            Self::Cmd => "CMD",
        }
    }

    pub fn from_label(s: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|k| k.label().eq_ignore_ascii_case(s.trim()))
    }

    /// Kinds whose tokens are separated by spaces and have a size limit.
    fn is_numeric(self) -> bool {
        matches!(self, Self::Hex | Self::Dec | Self::Bin)
    }

    fn accepts(self, c: char) -> bool {
        match self {
            // One byte per character: Latin-1 (U+0000..U+00FF) only.
            Self::Asc => !c.is_control() && u32::from(c) <= 0xFF,
            Self::Cmd => !c.is_control(),
            Self::Hex => c.is_ascii_hexdigit(),
            Self::Dec => c.is_ascii_digit(),
            Self::Bin => c == '0' || c == '1',
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    pub kind: TokenKind,
    pub text: String,
}

impl Token {
    pub fn new(kind: TokenKind, text: impl Into<String>) -> Self {
        Self {
            kind,
            text: text.into(),
        }
    }

    /// Text as shown on the chip (HEX in uppercase).
    pub fn display(&self) -> String {
        match self.kind {
            TokenKind::Hex => self.text.to_ascii_uppercase(),
            _ => self.text.clone(),
        }
    }

    /// Would adding `c` keep this token within its size limit?
    fn fits(&self, c: char) -> bool {
        match self.kind {
            TokenKind::Asc | TokenKind::Cmd => true,
            TokenKind::Hex => self.text.len() < 2,
            TokenKind::Bin => self.text.len() < 8,
            TokenKind::Dec => {
                let mut s = self.text.clone();
                s.push(c);
                s.parse::<u32>().is_ok_and(|v| v <= u16::MAX as u32)
            }
        }
    }

    pub fn to_bytes(&self) -> Result<Vec<u8>, InputError> {
        match self.kind {
            TokenKind::Asc => encode_latin1(&self.text),
            TokenKind::Hex => encode_hex(&self.text),
            TokenKind::Dec => encode_dec(&self.text),
            TokenKind::Bin => encode_bin(&self.text),
            TokenKind::Cmd => Err(InputError::CmdUnsupported),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum InputError {
    #[error("CMD input is not supported yet")]
    CmdUnsupported,
    #[error("'{0}' is not a valid {1} value")]
    Invalid(String, &'static str),
}

/// Encode text one byte per character (Latin-1).
pub fn encode_latin1(s: &str) -> Result<Vec<u8>, InputError> {
    s.chars()
        .map(|c| {
            u8::try_from(u32::from(c)).map_err(|_| InputError::Invalid(c.to_string(), "ASCII"))
        })
        .collect()
}

/// Encode hex digits, two per byte: `"0d0a"` → `[0x0D, 0x0A]`, `"123"` → `[0x12, 0x03]`.
pub fn encode_hex(s: &str) -> Result<Vec<u8>, InputError> {
    let digits: Vec<u8> = s.bytes().filter(|b| !b.is_ascii_whitespace()).collect();
    digits
        .chunks(2)
        .map(|pair| {
            let p = std::str::from_utf8(pair).unwrap_or("");
            u8::from_str_radix(p, 16).map_err(|_| InputError::Invalid(s.to_string(), "hex"))
        })
        .collect()
}

/// Encode a decimal number in as few bytes as needed, most significant first:
/// `"65"` → `[0x41]`, `"256"` → `[0x01, 0x00]`, `"0"` → `[0x00]`.
pub fn encode_dec(s: &str) -> Result<Vec<u8>, InputError> {
    let v: u64 = s
        .trim()
        .parse()
        .map_err(|_| InputError::Invalid(s.to_string(), "decimal"))?;
    let bytes = v.to_be_bytes();
    let first = bytes
        .iter()
        .position(|&b| b != 0)
        .unwrap_or(bytes.len() - 1);
    Ok(bytes[first..].to_vec())
}

/// Encode up to 8 binary digits as one byte: `"1000010"` → `[0x42]`.
pub fn encode_bin(s: &str) -> Result<Vec<u8>, InputError> {
    let t = s.trim();
    if t.is_empty() || t.len() > 8 {
        return Err(InputError::Invalid(s.to_string(), "binary"));
    }
    u8::from_str_radix(t, 2)
        .map(|b| vec![b])
        .map_err(|_| InputError::Invalid(s.to_string(), "binary"))
}

/// The editable input line.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct InputLine {
    tokens: Vec<Token>,
    /// A space was typed after a numeric token: the next digit starts a new one.
    split_next: bool,
}

impl InputLine {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn tokens(&self) -> &[Token] {
        &self.tokens
    }

    pub fn is_empty(&self) -> bool {
        self.tokens.is_empty()
    }

    pub fn clear(&mut self) {
        self.tokens.clear();
        self.split_next = false;
    }

    /// Type one character with the current input kind.
    pub fn type_char(&mut self, kind: TokenKind, c: char) {
        if kind.is_numeric() && c == ' ' {
            self.split_next = true;
            return;
        }
        if !kind.accepts(c) {
            return;
        }
        let split = std::mem::take(&mut self.split_next);
        match self.tokens.last_mut() {
            Some(last) if last.kind == kind && !(split && kind.is_numeric()) && last.fits(c) => {
                last.text.push(c)
            }
            _ => self.tokens.push(Token::new(kind, c.to_string())),
        }
    }

    pub fn type_str(&mut self, kind: TokenKind, s: &str) {
        for c in s.chars() {
            self.type_char(kind, c);
        }
    }

    /// Delete the last character; removes the token when it becomes empty.
    pub fn backspace(&mut self) {
        self.split_next = false;
        if let Some(last) = self.tokens.last_mut() {
            last.text.pop();
            if last.text.is_empty() {
                self.tokens.pop();
            }
        }
    }

    /// The bytes this line sends (without any send-on-enter prefix/suffix).
    pub fn to_bytes(&self) -> Result<Vec<u8>, InputError> {
        let mut out = Vec::new();
        for t in &self.tokens {
            out.extend(t.to_bytes()?);
        }
        Ok(out)
    }
}

/// Previously sent lines (HTerm keeps 30).
#[derive(Debug, Clone)]
pub struct History {
    entries: VecDeque<InputLine>,
    max: usize,
    /// Position while browsing; `None` = not browsing.
    pos: Option<usize>,
}

impl Default for History {
    fn default() -> Self {
        Self::new(30)
    }
}

impl History {
    pub fn new(max: usize) -> Self {
        Self {
            entries: VecDeque::new(),
            max,
            pos: None,
        }
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn max(&self) -> usize {
        self.max
    }

    /// Current browse position (1-based, newest = 1), for the status bar.
    pub fn position(&self) -> Option<usize> {
        self.pos.map(|p| p + 1)
    }

    pub fn push(&mut self, line: InputLine) {
        self.pos = None;
        if line.is_empty() {
            return;
        }
        if self.entries.front() != Some(&line) {
            self.entries.push_front(line);
            self.entries.truncate(self.max);
        }
    }

    /// Go to an older entry.
    pub fn older(&mut self) -> Option<&InputLine> {
        let next = self.pos.map_or(0, |p| p + 1);
        if next < self.entries.len() {
            self.pos = Some(next);
        }
        self.pos.and_then(|p| self.entries.get(p))
    }

    /// Go to a newer entry; `None` when back at the (empty) new line.
    pub fn newer(&mut self) -> Option<&InputLine> {
        match self.pos {
            Some(0) | None => {
                self.pos = None;
                None
            }
            Some(p) => {
                self.pos = Some(p - 1);
                self.entries.get(p - 1)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use TokenKind::*;

    fn chips(line: &InputLine) -> Vec<String> {
        line.tokens().iter().map(Token::display).collect()
    }

    #[test]
    fn spec_mixed_line() {
        // docs/spec/input.md: ASC "ab", HEX "0d 123", DEC "70000 1000", ASC "Z".
        let mut l = InputLine::new();
        l.type_str(Asc, "ab");
        l.type_str(Hex, "0d 123");
        l.type_str(Dec, "70000 1000");
        l.type_str(Asc, "Z");
        assert_eq!(chips(&l), ["ab", "0D", "12", "3", "7000", "0", "1000", "Z"]);
        assert_eq!(
            l.to_bytes().unwrap(),
            [
                0x61, 0x62, 0x0D, 0x12, 0x03, 0x1B, 0x58, 0x00, 0x03, 0xE8, 0x5A
            ]
        );
    }

    #[test]
    fn filters_invalid_characters() {
        let mut l = InputLine::new();
        l.type_str(Hex, "zz41g42");
        assert_eq!(l.to_bytes().unwrap(), [0x41, 0x42]);

        let mut l = InputLine::new();
        l.type_str(Bin, "65 66 13 10 255");
        assert_eq!(l.to_bytes().unwrap(), [0x01, 0x02]);
    }

    #[test]
    fn dec_encoding() {
        let mut l = InputLine::new();
        l.type_str(Dec, "65 66 13 10 255 256");
        assert_eq!(
            l.to_bytes().unwrap(),
            [0x41, 0x42, 0x0D, 0x0A, 0xFF, 0x01, 0x00]
        );
    }

    #[test]
    fn asc_is_literal() {
        let mut l = InputLine::new();
        l.type_str(Asc, r"a\r\n\x41\c");
        assert_eq!(l.to_bytes().unwrap(), br"a\r\n\x41\c");
    }

    #[test]
    fn asc_is_one_byte_per_character() {
        let mut l = InputLine::new();
        l.type_str(Asc, "\u{e9}\u{b0}\u{20ac}x");
        // \u{20ac} (euro sign) is outside Latin-1 and is dropped.
        assert_eq!(chips(&l), ["\u{e9}\u{b0}x"]);
        assert_eq!(l.to_bytes().unwrap(), [0xE9, 0xB0, b'x']);
    }

    #[test]
    fn bin_splits_after_eight_digits() {
        let mut l = InputLine::new();
        l.type_str(Bin, "010000010");
        assert_eq!(chips(&l), ["01000001", "0"]);
    }

    #[test]
    fn backspace_removes_empty_tokens() {
        let mut l = InputLine::new();
        l.type_str(Asc, "a");
        l.type_str(Hex, "4");
        l.backspace();
        assert_eq!(chips(&l), ["a"]);
        l.backspace();
        assert!(l.is_empty());
    }

    #[test]
    fn cmd_reports_unsupported() {
        let mut l = InputLine::new();
        l.type_str(Cmd, "test");
        assert_eq!(l.to_bytes(), Err(InputError::CmdUnsupported));
    }

    #[test]
    fn history_browsing() {
        let mut h = History::new(2);
        for s in ["one", "two", "three"] {
            let mut l = InputLine::new();
            l.type_str(Asc, s);
            h.push(l);
        }
        assert_eq!(h.len(), 2);
        assert_eq!(h.older().unwrap().tokens()[0].text, "three");
        assert_eq!(h.older().unwrap().tokens()[0].text, "two");
        assert_eq!(
            h.older().unwrap().tokens()[0].text,
            "two",
            "stops at oldest"
        );
        assert_eq!(h.newer().unwrap().tokens()[0].text, "three");
        assert!(h.newer().is_none());
    }
}

//! HTerm's "multi-format string" notation for byte sequences, used in its
//! config for "Newline at" and "Send on enter" (`docs/spec/file-formats.md`).
//!
//! A sequence is a list of tagged groups: `d[13 10]` (decimal), `h[0]` (hex),
//! `b[1010]` (binary, assumed). Groups can follow each other: `d[2]h[ff]`.
//! Values use the same encoding as input-line tokens.

use crate::input::{InputError, encode_bin, encode_dec, encode_hex};

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum MfsError {
    #[error("unknown format tag '{0}'")]
    UnknownTag(char),
    #[error("missing ']'")]
    Unclosed,
    #[error("expected a tag followed by '[' at position {0}")]
    Syntax(usize),
    #[error(transparent)]
    Value(#[from] InputError),
}

pub fn parse(s: &str) -> Result<Vec<u8>, MfsError> {
    let mut out = Vec::new();
    let mut rest = s.trim_start();
    while !rest.is_empty() {
        let pos = s.len() - rest.len();
        let mut chars = rest.chars();
        let tag = chars.next().ok_or(MfsError::Syntax(pos))?;
        if chars.next() != Some('[') {
            return Err(MfsError::Syntax(pos));
        }
        let body_start = tag.len_utf8() + 1;
        let close = rest[body_start..].find(']').ok_or(MfsError::Unclosed)?;
        let body = &rest[body_start..body_start + close];
        let encode: fn(&str) -> Result<Vec<u8>, InputError> = match tag {
            'd' => encode_dec,
            'h' => encode_hex,
            'b' => encode_bin,
            other => return Err(MfsError::UnknownTag(other)),
        };
        for value in body.split_whitespace() {
            out.extend(encode(value)?);
        }
        rest = rest[body_start + close + 1..].trim_start();
    }
    Ok(out)
}

/// Write bytes as a decimal group, e.g. `[13, 10]` → `d[13 10]`. Empty → `""`.
pub fn to_dec(bytes: &[u8]) -> String {
    if bytes.is_empty() {
        return String::new();
    }
    let values: Vec<String> = bytes.iter().map(u8::to_string).collect();
    format!("d[{}]", values.join(" "))
}

/// A named byte sequence, like an HTerm `mfsDatabase` entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NamedSequence {
    pub name: String,
    pub prefix: Vec<u8>,
    pub suffix: Vec<u8>,
}

impl NamedSequence {
    fn new(name: &str, prefix: &[u8], suffix: &[u8]) -> Self {
        Self {
            name: name.to_string(),
            prefix: prefix.to_vec(),
            suffix: suffix.to_vec(),
        }
    }
}

/// HTerm's default "Newline at" presets. Only `prefix` is used: the sequence to match.
pub fn default_newline_presets() -> Vec<NamedSequence> {
    vec![
        NamedSequence::new("None", b"", b""),
        NamedSequence::new("CR", b"\r", b""),
        NamedSequence::new("LF", b"\n", b""),
        NamedSequence::new("CR+LF", b"\r\n", b""),
        NamedSequence::new("Null", b"\0", b""),
        NamedSequence::new("ETX", b"\x03", b""),
    ]
}

/// HTerm's default "Send on enter" presets: bytes added before and after the line.
pub fn default_send_on_enter_presets() -> Vec<NamedSequence> {
    vec![
        NamedSequence::new("None", b"", b""),
        NamedSequence::new("CR", b"", b"\r"),
        NamedSequence::new("LF", b"", b"\n"),
        NamedSequence::new("CR-LF", b"", b"\r\n"),
        NamedSequence::new("Space", b"", b" "),
        NamedSequence::new("STX/ETX", b"\x02", b"\x03"),
        NamedSequence::new("Null", b"", b"\0"),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_hterm_config_values() {
        assert_eq!(parse("").unwrap(), b"");
        assert_eq!(parse("d[13]").unwrap(), b"\r");
        assert_eq!(parse("d[13 10]").unwrap(), b"\r\n");
        assert_eq!(parse("h[0]").unwrap(), b"\0");
        assert_eq!(parse("d[2]h[ff] b[1]").unwrap(), [2, 0xff, 1]);
    }

    #[test]
    fn errors() {
        assert_eq!(parse("x[1]"), Err(MfsError::UnknownTag('x')));
        assert_eq!(parse("d[1"), Err(MfsError::Unclosed));
        assert_eq!(parse("13"), Err(MfsError::Syntax(0)));
    }

    #[test]
    fn round_trip_dec() {
        assert_eq!(to_dec(b"\r\n"), "d[13 10]");
        assert_eq!(parse(&to_dec(&[0, 255, 7])).unwrap(), [0, 255, 7]);
    }

    #[test]
    fn presets_match_hterm_defaults() {
        let soe = default_send_on_enter_presets();
        let stx = soe.iter().find(|p| p.name == "STX/ETX").unwrap();
        assert_eq!(
            (stx.prefix.as_slice(), stx.suffix.as_slice()),
            (&b"\x02"[..], &b"\x03"[..])
        );
        assert_eq!(default_newline_presets().len(), 6);
    }
}

//! HTerm's "multi-format string" notation for byte sequences, used in its
//! config ("Newline at", "Send on enter") and in `.hts` sequence files
//! (`docs/spec/file-formats.md`, `docs/spec/sequences.md`).
//!
//! A sequence is a list of tagged groups separated by spaces:
//! `a[hi] h[0D] d[300] b[101] c[cmd]`.
//! - `a[...]` is literal ASCII text, spaces included; `]` is written `\]`
//!   and `\` is written `\\`. `c[...]` (CMD) is written the same way.
//! - `h`, `d`, `b` groups hold one or more values separated by spaces
//!   (`d[13 10]`), each encoded like an input-line chip.

use crate::input::{InputError, InputLine, Token, TokenKind};

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

/// Parse into input-line chips (one chip per value).
pub fn parse_tokens(s: &str) -> Result<Vec<Token>, MfsError> {
    let mut tokens = Vec::new();
    let mut chars = s.char_indices().peekable();
    loop {
        while chars.next_if(|(_, c)| c.is_whitespace()).is_some() {}
        let Some((pos, tag)) = chars.next() else {
            break;
        };
        if chars.next().map(|(_, c)| c) != Some('[') {
            return Err(MfsError::Syntax(pos));
        }
        // Read the body up to an unescaped ']'.
        let mut body = String::new();
        let mut closed = false;
        while let Some((_, c)) = chars.next() {
            match c {
                '\\' => match chars.next() {
                    Some((_, e)) => body.push(e),
                    None => return Err(MfsError::Unclosed),
                },
                ']' => {
                    closed = true;
                    break;
                }
                c => body.push(c),
            }
        }
        if !closed {
            return Err(MfsError::Unclosed);
        }
        let kind = match tag {
            'a' => TokenKind::Asc,
            'c' => TokenKind::Cmd,
            'h' => TokenKind::Hex,
            'd' => TokenKind::Dec,
            'b' => TokenKind::Bin,
            other => return Err(MfsError::UnknownTag(other)),
        };
        match kind {
            TokenKind::Asc | TokenKind::Cmd => {
                if !body.is_empty() {
                    tokens.push(Token::new(kind, body));
                }
            }
            _ => {
                for value in body.split_whitespace() {
                    let t = Token::new(kind, value);
                    t.to_bytes()?; // validate
                    tokens.push(t);
                }
            }
        }
    }
    Ok(tokens)
}

/// Parse straight to bytes. CMD groups are an error (not supported yet).
pub fn parse(s: &str) -> Result<Vec<u8>, MfsError> {
    Ok(InputLine::from_tokens(parse_tokens(s)?).to_bytes()?)
}

/// Write chips the way HTerm saves them: `a[hi] h[0D] d[300]`.
pub fn format_tokens(tokens: &[Token]) -> String {
    tokens
        .iter()
        .map(|t| {
            let (tag, text) = match t.kind {
                TokenKind::Asc => ('a', escape(&t.text)),
                TokenKind::Cmd => ('c', escape(&t.text)),
                TokenKind::Hex => ('h', t.display()),
                TokenKind::Dec => ('d', t.text.clone()),
                TokenKind::Bin => ('b', t.text.clone()),
            };
            format!("{tag}[{text}]")
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace(']', "\\]")
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
    fn hts_values_from_hterm() {
        // docs/spec/sequences.md
        let s = "a[hi] h[0D] h[0A] d[300] b[101] a[a b]";
        let tokens = parse_tokens(s).unwrap();
        assert_eq!(tokens.len(), 6);
        assert_eq!(tokens[5], Token::new(TokenKind::Asc, "a b"));
        assert_eq!(
            parse(s).unwrap(),
            [0x68, 0x69, 0x0D, 0x0A, 0x01, 0x2C, 0x05, 0x61, 0x20, 0x62]
        );
        assert_eq!(format_tokens(&tokens), s, "round trip");
    }

    #[test]
    fn escapes_and_cmd() {
        let s = r"a[x\]y[z\\] c[test]";
        let tokens = parse_tokens(s).unwrap();
        assert_eq!(tokens[0], Token::new(TokenKind::Asc, r"x]y[z\"));
        assert_eq!(tokens[1], Token::new(TokenKind::Cmd, "test"));
        assert_eq!(format_tokens(&tokens), s);
        assert!(matches!(
            parse(s),
            Err(MfsError::Value(InputError::CmdUnsupported))
        ));
    }

    #[test]
    fn errors() {
        assert_eq!(parse("x[1]"), Err(MfsError::UnknownTag('x')));
        assert_eq!(parse("d[1"), Err(MfsError::Unclosed));
        assert_eq!(parse("a[abc"), Err(MfsError::Unclosed));
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

//! Sequence files in HTerm's `.hts` format (`docs/spec/sequences.md`).

use std::path::Path;

use uartist_core::input::Token;
use uartist_core::mfs::{self, MfsError};

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SequenceItem {
    pub name: String,
    pub description: String,
    pub tokens: Vec<Token>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SequenceFile {
    pub items: Vec<SequenceItem>,
}

#[derive(Debug, thiserror::Error)]
pub enum SequenceError {
    #[error("could not read {0}: {1}")]
    Io(String, std::io::Error),
    #[error("not a valid sequence file: {0}")]
    Xml(#[from] roxmltree::Error),
    #[error("not an HTerm sequence file (root element is <{0}>)")]
    WrongRoot(String),
    #[error("sequence \"{name}\": {source}")]
    Item { name: String, source: MfsError },
}

impl SequenceFile {
    pub fn parse(xml: &str) -> Result<Self, SequenceError> {
        let doc = roxmltree::Document::parse(xml)?;
        let root = doc.root_element();
        if !root.has_tag_name("HtermSequenceFile") {
            return Err(SequenceError::WrongRoot(root.tag_name().name().into()));
        }
        let mut items = Vec::new();
        for node in root
            .descendants()
            .filter(|n| n.has_tag_name("SequenceItem"))
        {
            let name = node.attribute("name").unwrap_or_default().to_string();
            let child = |tag: &str| node.children().find(|c| c.has_tag_name(tag));
            let value = child("sequence")
                .and_then(|n| n.attribute("value"))
                .unwrap_or_default();
            let description = child("description")
                .and_then(|n| n.text())
                .unwrap_or_default()
                .to_string();
            let tokens = mfs::parse_tokens(value).map_err(|source| SequenceError::Item {
                name: name.clone(),
                source,
            })?;
            items.push(SequenceItem {
                name,
                description,
                tokens,
            });
        }
        Ok(Self { items })
    }

    pub fn load(path: &Path) -> Result<Self, SequenceError> {
        let text = std::fs::read_to_string(path)
            .map_err(|e| SequenceError::Io(path.display().to_string(), e))?;
        Self::parse(&text)
    }

    /// The file in HTerm's layout, so HTerm can open it too.
    pub fn to_xml(&self) -> String {
        let mut s = String::from(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
             <HtermSequenceFile name=\"Hterm Sequence File\" version=\"0.8.9\">\n  <ItemList>\n",
        );
        for item in &self.items {
            s.push_str(&format!(
                "    <SequenceItem name=\"{}\">\n      <sequence value=\"{}\"/>\n      \
                 <description>{}</description>\n    </SequenceItem>\n",
                escape_xml(&item.name),
                escape_xml(&mfs::format_tokens(&item.tokens)),
                escape_xml(&item.description),
            ));
        }
        s.push_str("  </ItemList>\n</HtermSequenceFile>");
        s
    }

    /// Save, writing a temporary file first so a crash can't corrupt it.
    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        let tmp = path.with_extension("hts.tmp");
        std::fs::write(&tmp, self.to_xml())?;
        std::fs::rename(&tmp, path)
    }
}

fn escape_xml(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            c => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use uartist_core::input::{InputLine, TokenKind};

    const SAMPLE: &str = include_str!("../tests/fixtures/hterm-0.8.9-sequences.hts");

    #[test]
    fn reads_hterm_file() {
        let f = SequenceFile::parse(SAMPLE).unwrap();
        assert_eq!(f.items.len(), 2);
        let g = &f.items[0];
        assert_eq!(g.name, "Greeting");
        assert_eq!(g.description, "says hi");
        assert_eq!(
            InputLine::from_tokens(g.tokens.clone()).to_bytes().unwrap(),
            [0x68, 0x69, 0x0D, 0x0A, 0x01, 0x2C, 0x05, 0x61, 0x20, 0x62]
        );
        let t = &f.items[1];
        assert_eq!(t.tokens[0], Token::new(TokenKind::Asc, r"x]y[z\"));
        assert_eq!(t.tokens[1].kind, TokenKind::Cmd);
    }

    #[test]
    fn writes_the_same_file_hterm_wrote() {
        let f = SequenceFile::parse(SAMPLE).unwrap();
        assert_eq!(f.to_xml(), SAMPLE.trim_end());
    }

    #[test]
    fn escapes_xml_special_characters() {
        let f = SequenceFile {
            items: vec![SequenceItem {
                name: "A & \"B\" <C>".into(),
                description: "x < y".into(),
                tokens: vec![Token::new(TokenKind::Asc, "<&>")],
            }],
        };
        let back = SequenceFile::parse(&f.to_xml()).unwrap();
        assert_eq!(back, f);
    }

    #[test]
    fn rejects_other_xml() {
        assert!(matches!(
            SequenceFile::parse("<CfgFile/>"),
            Err(SequenceError::WrongRoot(_))
        ));
    }
}

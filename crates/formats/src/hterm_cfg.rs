//! Import HTerm's `hterm.cfg` (`docs/spec/file-formats.md`).
//!
//! Settings HTerm stores are copied onto the current settings; anything
//! Uartist doesn't have is left as it is. Values that can't be carried over
//! are listed as notes for the user.

use uartist_core::mfs;

use crate::settings::{FormatFlags, Settings};

#[derive(Debug, thiserror::Error)]
pub enum CfgError {
    #[error("not a valid config file: {0}")]
    Xml(#[from] roxmltree::Error),
    #[error("not an HTerm config file (root element is <{0}>)")]
    WrongRoot(String),
}

/// The result of an import: new settings and notes about what wasn't taken over.
pub struct Imported {
    pub settings: Settings,
    pub notes: Vec<String>,
}

pub fn import(xml: &str, base: &Settings) -> Result<Imported, CfgError> {
    let doc = roxmltree::Document::parse(xml)?;
    let root = doc.root_element();
    if !root.has_tag_name("CfgFile") {
        return Err(CfgError::WrongRoot(root.tag_name().name().into()));
    }
    let mut s = base.clone();
    let mut notes = Vec::new();

    // First element with this tag anywhere in the file, and an attribute of it.
    let el = |tag: &str| root.descendants().find(|n| n.has_tag_name(tag));
    let attr = |tag: &str, a: &str| el(tag).and_then(|n| n.attribute(a)).map(str::to_string);
    let flag = |tag: &str, a: &str| attr(tag, a).map(|v| v == "true");
    let num = |tag: &str, a: &str| attr(tag, a).and_then(|v| v.trim().parse::<u64>().ok());

    // --- Port
    if let Some(v) = attr("Baudrate", "selected") {
        s.port.baud = v;
    }
    if let Some(v) = num("Databits", "value").and_then(|n| u8::try_from(n).ok()) {
        s.port.data_bits = v;
    }
    if let Some(v) = attr("Stopbits", "value") {
        s.port.stop_bits = v;
    }
    if let Some(v) = attr("ParityMode", "value") {
        s.port.parity = v;
    }
    if let Some(v) = flag("FlowControlCTS", "status") {
        s.port.cts_flow = v;
    }
    if let Some(v) = attr("Selection", "value") {
        s.port.path = v;
    }
    if let Some(p) = el("ComportPatterns") {
        let patterns: Vec<String> = p
            .children()
            .filter(|c| c.has_tag_name("entry"))
            .filter_map(|c| c.text())
            .map(str::to_string)
            .collect();
        // Left out on purpose: /dev/pts/* also lists the user's own terminals.
        let kept: Vec<String> = patterns
            .iter()
            .filter(|p| p.as_str() != "/dev/pts/*")
            .cloned()
            .collect();
        if kept.len() < patterns.len() {
            notes.push("Left out the port pattern /dev/pts/* (it lists your own terminals)".into());
        }
        if !kept.is_empty() {
            s.port.patterns = kept;
        }
    }

    // --- Views
    let formats = |tag: &str| {
        let n = el(tag)?;
        let b = |a: &str| n.attribute(a) == Some("true");
        Some(FormatFlags {
            ascii: b("Ascii"),
            hex: b("Hex"),
            dec: b("Dec"),
            bin: b("Bin"),
        })
    };
    if let Some(f) = formats("ReceiveBoxFormat") {
        s.view.receive_formats = f;
    }
    if let Some(f) = formats("TransmitBoxFormat") {
        s.view.transmit_formats = f;
    }
    if let Some(v) = flag("Autoscroll", "state") {
        s.view.autoscroll = v;
    }
    if let Some(v) = flag("NewlineOnSourceChange", "state") {
        s.view.newline_on_source_change = v;
    }
    if let Some(v) = num("NewlineByPause", "interval") {
        s.view.newline_pause_ms = v.min(u64::from(u32::MAX)) as u32;
    }
    if let Some(v) = num("NewlineInterval", "value") {
        s.view.newline_every = v as usize;
    }
    if let Some(v) = flag("CombinedViewTxRx", "state") {
        s.view.combined = v;
    }
    if let Some(v) = num("UserCharCounter", "value").and_then(|n| u8::try_from(n).ok()) {
        s.view.count_byte = v;
    }
    if let Some(v) = flag("ClearAtByte", "status") {
        s.view.clear_at_enabled = v;
    }
    if let Some(v) = num("ClearAtByte", "value").and_then(|n| u8::try_from(n).ok()) {
        s.view.clear_at_byte = v;
    }
    match num("Bitmask", "bits") {
        Some(0) => s.view.bitmask = "No mask".into(),
        Some(b) => notes.push(format!(
            "Receive bitmask {b} wasn't imported (unknown HTerm value)"
        )),
        None => {}
    }
    if let Some(n) = el("NewlineAt") {
        if let Some(sel) = n.attribute("selection") {
            s.view.newline_at = sel.to_string();
        }
        if let Some(state) = n.attribute("state") {
            s.view.show_newline = state == "true";
        }
        note_custom_presets(n, &mut notes, "Newline at");
    }

    // --- Input
    // <Inputline><Format value="ASC"/> (SaveOutput has its own <Format>).
    if let Some(v) = el("Inputline")
        .and_then(|n| n.children().find(|c| c.has_tag_name("Format")))
        .and_then(|n| n.attribute("value"))
    {
        s.input.kind = v.to_string();
    }
    if let Some(n) = el("SendOnEnter") {
        if let Some(v) = n.attribute("value") {
            s.input.send_on_enter = v.to_string();
        }
        note_custom_presets(n, &mut notes, "Send on enter");
    }

    // --- Autosend / Send file (HTerm's delay is in tenths of a second)
    if let Some(v) = num("AutosendRepititions", "value") {
        s.send.repetitions = v.clamp(1, 100_000) as u32;
    }
    if let Some(v) = num("AutosendDelay", "value") {
        s.send.delay_ms = (v * 100).min(3_600_000) as u32;
    }
    if let Some(v) = attr("AutoSendLocation", "file").filter(|v| !v.is_empty()) {
        s.send.file_path = v;
    }

    // --- Save output
    if let Some(n) = el("SaveOutput") {
        let child = |tag: &str, a: &str| {
            n.children()
                .find(|c| c.has_tag_name(tag))
                .and_then(|c| c.attribute(a))
                .map(str::to_string)
        };
        if let Some(v) = child("Format", "value") {
            s.save_output.format = v;
        }
        if let Some(v) = child("Source", "value") {
            s.save_output.source = v;
        }
        if let Some(v) = child("Timestamps", "show") {
            s.save_output.timestamps = v == "true";
        }
        if let Some(v) = child("Separator", "value") {
            match mfs::parse(&v).map(String::from_utf8) {
                Ok(Ok(text)) => s.save_output.separator = text,
                _ => notes.push(format!("Save output separator \"{v}\" wasn't imported")),
            }
        }
    }

    Ok(Imported { settings: s, notes })
}

/// HTerm lets users edit the Newline-at / Send-on-enter lists. Uartist uses
/// the default lists, so mention entries that aren't among them.
fn note_custom_presets(node: roxmltree::Node, notes: &mut Vec<String>, what: &str) {
    const DEFAULTS: [&str; 9] = [
        "None", "CR", "LF", "CR+LF", "CR-LF", "Null", "ETX", "Space", "STX/ETX",
    ];
    let custom: Vec<&str> = node
        .descendants()
        .filter(|c| c.has_tag_name("entry"))
        .filter_map(|c| c.attribute("name"))
        .filter(|n| !DEFAULTS.contains(n))
        .collect();
    if !custom.is_empty() {
        notes.push(format!(
            "Custom {what} entries weren't imported: {}",
            custom.join(", ")
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = include_str!("../tests/fixtures/hterm-0.8.9-default.cfg");

    #[test]
    fn imports_hterm_defaults() {
        let mut base = Settings::default();
        base.view.theme = "Dark".into();
        let Imported { settings: s, notes } = import(SAMPLE, &base).unwrap();
        assert_eq!(s.port.baud, "115200");
        assert_eq!(s.port.data_bits, 8);
        assert_eq!(s.port.stop_bits, "1");
        assert_eq!(s.port.parity, "None");
        assert_eq!(s.port.path, "/dev/pts/0");
        assert!(!s.port.patterns.iter().any(|p| p.contains("pts")));
        assert!(s.port.patterns.contains(&"/dev/ttyUSB*".to_string()));
        assert!(s.view.receive_formats.ascii && !s.view.receive_formats.hex);
        assert!(s.view.autoscroll && s.view.show_newline && s.view.newline_on_source_change);
        assert_eq!(s.view.newline_at, "None");
        assert_eq!(s.input.kind, "ASC");
        assert_eq!(s.input.send_on_enter, "None");
        assert_eq!(s.send.repetitions, 1);
        assert_eq!(s.save_output.format, "Raw");
        assert_eq!(s.save_output.source, "Received");
        assert_eq!(s.view.theme, "Dark", "settings HTerm doesn't have are kept");
        assert_eq!(notes.len(), 1, "{notes:?}");
        assert!(notes[0].contains("/dev/pts/*"));
    }

    #[test]
    fn converts_units_and_reports_custom_entries() {
        let xml = r#"<CfgFile><Default>
            <Autosend><AutosendRepititions value="5"/><AutosendDelay value="7"/></Autosend>
            <Controls>
              <NewlineAt state="false" selection="CR+LF"><mfsDatabase>
                <entry name="None" mfs0=""/><entry name="Weird" mfs0="d[9]"/>
              </mfsDatabase></NewlineAt>
              <ClearAtByte status="true" value="88"/>
            </Controls>
            <Comport><Stopbits value="1.5"/><ParityMode value="Mark"/></Comport>
        </Default></CfgFile>"#;
        let Imported { settings: s, notes } = import(xml, &Settings::default()).unwrap();
        assert_eq!((s.send.repetitions, s.send.delay_ms), (5, 700));
        assert_eq!(s.view.newline_at, "CR+LF");
        assert!(!s.view.show_newline);
        assert!(s.view.clear_at_enabled && s.view.clear_at_byte == 88);
        assert_eq!(
            (s.port.stop_bits.as_str(), s.port.parity.as_str()),
            ("1.5", "Mark")
        );
        assert!(notes.iter().any(|n| n.contains("Weird")), "{notes:?}");
    }

    #[test]
    fn rejects_other_files() {
        assert!(matches!(
            import("<HtermSequenceFile/>", &Settings::default()),
            Err(CfgError::WrongRoot(_))
        ));
    }
}

# HTerm file formats

## Config file (`hterm.cfg`)

- Read from the **current working directory** at startup (`access("<cwd>/hterm.cfg")`).
- UTF-8 XML. Root element: `<CfgFile name="HTerm config file" version="0.8.9">`,
  with one child `<Default>`.
- Sample: [`crates/formats/tests/fixtures/hterm-0.8.9-default.cfg`](../../crates/formats/tests/fixtures/hterm-0.8.9-default.cfg)
  (defaults, saved via File → Save config as…).
- Booleans are written as `"true"` / `"false"`. The attribute name varies: `state`, `status`, `value`, `show`.

Sections under `<Default>` (the misspellings `Repititions` and `AlwayOnTop` are
HTerm's own and must be matched exactly):

| Element | Contents |
|---|---|
| `SequenceManager` | `OpenFiles` (list of .hts files), `WorkingDir@value` |
| `Autosend` | `AutosendRepititions@value`, `AutosendDelay@value` (ms) |
| `Global` | `SaveOutputLocation@dir`, `AutoSendLocation@dir,file` |
| `Inputline` | `Format@value` (`ASC`, `HEX`, `DEC`, `BIN`, `CMD`) |
| `SaveOutput` | `Source@value` (`Received`…), `Timestamps@show`, `Separator@value`, `Format@value` (`Raw`…) |
| `OutputBox` | `NewlineOnSourceChange@state`, `CombinedViewTxRx@state`, `NewlineByPause@interval`, `SourceLineHeight@value`, `Font@transmitBox,receiveBox` (Pango font strings, e.g. `Monospace 9`), `ShowReceiveErrors@state`, `Bitmask@bits`, `Autoscroll@state`, `DisconnectOnFocusLoss@state`, `Ruler@state,start`, `ReceiveBoxFormat@Ascii,Hex,Dec,Bin`, `TransmitBoxFormat@…`, `NewlineInterval@value` |
| `Controls` | `SendOnEnter@value` + `mfsDatabase`; `ClearAtByte@status,value`; `NewlineAt@state,selection` + `mfsDatabase`; `UserCharCounter@value` |
| `GUI` | `Window@x,y,width,height,maximized`, `Tooltip@active,delay`, `AlwayOnTop@state`, `Perspective` (wxAUI layout string — ignore on import), `Autosave@status` |
| `Comport` | `ConnectionStatus@status`, `FlowControlCTS@status`, `StatusLines@DTR,RTS`, `ParityMode@value`, `Databits@value`, `Stopbits@value`, `Baudrate@selected` + text = comma-separated baud list, `Selection@value`, `ComportPatterns/entry*` |

### `mfsDatabase` (named byte sequences)

```xml
<entry name="STX/ETX" mfs0="d[2]" mfs1="d[3]"/>
```

`mfs0` / `mfs1` are multi-format strings (see below). For **Newline At**, only
`mfs0` is used, as the sequence to match. For **Send on enter**, `mfs0` is the
prefix and `mfs1` the suffix.

### Multi-format string ("mfs") notation

A byte sequence written as format-tagged groups: `<tag>[<values separated by spaces>]`.
Seen so far:
- `d[13 10]`: decimal values
- `h[0]`: hex values

Expected but not seen yet: `a[...]` or similar for ASCII text, `b[...]` for binary,
and some form for CMD tokens. It probably also stores whole input lines and sequences.
**To confirm** using a saved `.hts` file with mixed tokens.

## Sequence file (`.hts`)

- "Hterm Sequence File", root element probably `HtermSequenceFile`, elements `SequenceItem`.
- Errors in the binary: "Sequence and names count mismatch", "Sequence has empty name",
  "Sequence name already in use".
- **To capture**: create sequences in HTerm, save them, and document them here.

# Save output

Observed on HTerm 0.8.9 over a socat pair. The received data was `Hello\r\n` and then
`World\0\xFF` (two chunks), and `abc`/`xyz` was sent.

## UI

- **Save output** is a split button. The main part asks for a file name and saves.
  The arrow opens an options popup with OK and Cancel:

| Option | Choices | Config (`SaveOutput`) |
|---|---|---|
| Storage format | Raw, Asc, Hex, Dec, Bin | `Format@value` |
| Select source | Received, Transmitted, Both | `Source@value` |
| Character separator | an input-type selector (ASC/HEX/DEC/BIN/CMD) plus a token field, like the input line | `Separator@value` |
| Timestamps | Enable checkbox | `Timestamps@show` |

## What gets saved

HTerm saves **the contents of the received-data box**. In the combined TX/RX view
that box also holds sent data, so only then do sent bytes appear in the file.

Without timestamps and without "Both", the data is one stream: every byte is
formatted and the separator goes between bytes, also across chunk boundaries.

| Format | Output for `Hello\r\nWorld\0\xFF` |
|---|---|
| Raw | the exact bytes |
| Asc | `HelloWorld`: non-printable bytes (CR, LF, NUL, 0xFF) are **dropped** |
| Hex (sep `,`) | `48,65,6C,6C,6F,0D,0A,…` (uppercase, 2 digits) |
| Dec (sep ` `) | `072 101 108 108 111 013 010 087 …` (zero-padded, 3 digits) |
| Bin (no sep) | `0100100001100101…` (8 digits per byte) |

### Chunk headers

When **Timestamps** are on, or **Source = Both**, each received/sent chunk gets
its own header line, then its data on the next line. Chunks are separated by LF,
and there's no trailing newline at the end of the file.

```
Received - 16:36:36.988:
48 65 6C 6C 6F 0D 0A
Received - 16:36:38.390:
57 6F 72 6C 64 00 FF
Transmitted - 16:38:22.679:
78 79 7A
```

- Header = `<Source> - <time>:` with Both + timestamps, `<time>:` with timestamps
  only, and `<Source>` alone with Both and no timestamps.
- Time is local wall-clock `HH:MM:SS.mmm` of when the chunk arrived.
- With **Raw**, the headers are still written: text lines between raw data.

### Unclear

- Choosing "Transmitted" (non-combined view, and once in combined view) still
  saved the received data. The source filter may not work in 0.8.9, or my test
  selection may not have taken. Not confirmed.

## Our version

- **Source really filters**: Received, Transmitted, or Both (in time order, each
  chunk labelled). It doesn't depend on the combined view.
- **Raw writes only data bytes**, never headers, so a raw file is always exactly
  what was received/sent. Timestamps and labels need a text format.
- **Asc keeps CR, LF and TAB** so text logs keep their lines. Other non-printable
  bytes are dropped, as in HTerm.
- The separator is typed as plain text (ASC), not as input tokens.
- Saving goes through a dialog with the options, a file path box and a
  "Browse…" button.

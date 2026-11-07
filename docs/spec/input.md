# Input line and sending

Observed on HTerm 0.8.9 by typing into the input line and reading what came out
of the other end of a socat pair.

## Token editor

The input line is not a plain text box. It is a sequence of **tokens**, each
drawn as a coloured chip in its type's colour (the same colours as the
data-view rows). The **Type** selector sets the type for new tokens, and
**one line can mix types**.

| Type | Chip colour | Accepted characters | Encoding |
|---|---|---|---|
| ASC | cyan | any text | bytes of the text as typed. Escapes like `\r`, `\x41` are **not** interpreted. |
| HEX | blue | `0-9a-fA-F`, others silently dropped | every 2 digits = 1 byte. `123` → `12 03`. Displayed uppercase. |
| DEC | red | `0-9`, others dropped | each number → its minimal number of bytes, big-endian. `65` → `41`, `256` → `01 00`, `7000` → `1B 58`, `0` → `00`. Max 65535 per token. Typing past the max starts a new token (`70000` → `7000`, `0`). |
| BIN | green | `0`/`1`, others dropped | each group → 1 byte. `1000010` → `42`. |
| CMD | orange | any text | unknown. `test` sent nothing. The binary has the error "\c must be followed by a printable ASCII character". See open-questions.md. |

> Our version: ASC is one byte per character, using Latin-1 (U+0000–U+00FF).
> Characters outside that range are dropped while typing, the same way HEX drops
> non-hex characters. HTerm's handling of non-ASCII text hasn't been tested yet.

Test: a line built as ASC `ab`, HEX `0d 123`, DEC `70000 1000`, ASC `Z` gave the chips
`ab | 0D | 12 | 3 | 7000 | 0 | 1000 | Z` and sent
`61 62 0D 12 03 1B 58 00 03 E8 5A`.

## Sending

- **Enter** sends the line and clears it. The line goes into a history (max 30;
  the status bar shows `History -/n/30`).
- **Send on enter** adds bytes around the line. Each preset has a prefix (`mfs0`)
  and a suffix (`mfs1`):

  | Preset | Prefix | Suffix |
  |---|---|---|
  | None | – | – |
  | CR | – | `d[13]` |
  | LF | – | `d[10]` |
  | CR-LF | – | `d[13 10]` |
  | Space | – | `d[32]` |
  | STX/ETX | `d[2]` | `d[3]` |
  | Null | – | `h[0]` |

  The list is editable via Options → Modify Send on enter….
- **Autosend**: sends the line repeatedly (config `AutosendRepititions`, `AutosendDelay`). Not tested yet.
- **Send file**: sends the contents of a file, with progress and elapsed/remaining time ("Elapsed time:", "Estimated time:", "Remaining time:"). Not tested yet.
- Sent bytes appear in the Transmitted data view and increase the Tx counter.

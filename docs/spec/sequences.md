# Sequences

Observed on HTerm 0.8.9. Sample file:
[`crates/formats/tests/fixtures/hterm-0.8.9-sequences.hts`](../../crates/formats/tests/fixtures/hterm-0.8.9-sequences.hts).

## UI

- **Sequence Overview** pane (left). Right-click empty space: **New File…**,
  **Load File…**. A file is created when chosen, but only written to disk on save.
- Files show as tree nodes. Right-click a file: **New item**, **Save file**,
  **Save file as…**, **Unload file**, **Paste item(s)**.
- Right-click an item: **Send** (left double-click), **Insert to inputline**
  (Ctrl+middle click), **Edit** (middle click), **Copy item(s)**,
  **Paste item(s) after this**, **Delete**.
- **Edit Sequence** opens as a tab next to "Received Data", with:
  Item name, Description (multi-line), Shortcut (was greyed out), and Sequence
  (a type selector and chip field, like the input line). Buttons: OK, Undo, Cancel.
- **Sending** an item sends its bytes, encoded exactly as the input line would encode them.
  The test item `a[hi] h[0D] h[0A] d[300] b[101] a[a b]` sent
  `68 69 0D 0A 01 2C 05 61 20 62`.

## `.hts` file format

```xml
<?xml version="1.0" encoding="UTF-8"?>
<HtermSequenceFile name="Hterm Sequence File" version="0.8.9">
  <ItemList>
    <SequenceItem name="Greeting">
      <sequence value="a[hi] h[0D] h[0A] d[300] b[101] a[a b]"/>
      <description>says hi</description>
    </SequenceItem>
    <SequenceItem name="Tricky">
      <sequence value="a[x\]y[z\\] c[test]"/>
      <description></description>
    </SequenceItem>
  </ItemList>
</HtermSequenceFile>
```

The `sequence` value is the chip list. Each chip is `tag[value]`, and chips are
separated by single spaces:

| Tag | Type | Value |
|---|---|---|
| `a` | ASC | text as typed, **spaces included**; `]` is written `\]` and `\` is written `\\` |
| `h` | HEX | one byte as 2 hex digits (each HEX chip is its own group) |
| `d` | DEC | the decimal number (up to 65535) |
| `b` | BIN | the binary digits |
| `c` | CMD | the command text (meaning still unknown) |

In `hterm.cfg`, a group can hold several values separated by spaces, e.g.
`d[13 10]`; see file-formats.md. A parser must allow both forms, and must treat
`a[...]` as literal text.

## Our version

- A **Sequences** side panel: load HTerm `.hts` files, create new files, add,
  edit, delete and reorder items, and save. Files are saved in the `.hts` format,
  so they can be shared with HTerm users.
- Double-click an item to send it. The right-click menu has Send, Insert into
  input line, Edit and Delete.
- Open sequence files are remembered between sessions.
- Items with a CMD chip are shown but can't be sent (CMD is unsupported).

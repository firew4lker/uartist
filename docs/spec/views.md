# Layout, data views and counters

Observed on HTerm 0.8.9 (Linux).

## Window layout

The window is made of dockable panes. Names come from the saved layout string;
captions are what the UI shows.

- **Menu bar**
  - File: Load config…, Save config, Save config as…, Autosave config on exit, Quit (Ctrl+Q).
  - Options: Show ruler (checkable), Set ruler start…, Combined view transmit and receive,
    Clear all, Focus loss disconnect, Receive box font…, Transmit box font…,
    Receive box source underline height…, Modify Newline At…, Modify Send on enter…,
    Receive bitmask (submenu).
  - View: Show all panes, Reset layout to last loaded, Reset layout to default, Always on top.
  - Help: Show logs, About…
- **Toolbars**
  - Row 0: connection (Connect/Disconnect, Port + R, Baud, Data, Stop, Parity, CTS flow control).
  - Row 1:
    - Stream counter: Rx count + Reset, Tx count + Reset.
    - User char counter: "Count" byte value spinner, its count, and Reset.
    - Newline control: Newline at, Show newline characters, Newline on source change.
  - Row 2:
    - Clear received.
    - Output format: Ascii, Hex, Dec, Bin.
    - Save output (split button with options).
    - Clear at character: checkbox and byte value.
    - Auto insert newline: "Newline every … characters".
    - Output options: Autoscroll, Show errors.
    - Newline by pause: "Newline after … ms receive pause (0=off)".
    - Incoming status lines: CTS, DSR, RI, DCD LEDs.
- **Sequence Overview**: left dock pane, a list of loaded sequences.
- **Received Data**: central pane with tabs.
- **Input control**: bottom pane.
  - Clear transmitted.
  - Ascii/Hex/Dec/Bin formats for the transmitted view.
  - Send on enter.
  - Send file.
  - DTR and RTS toggles.
  - Type selector and input line.
  - Autosend.
- **Transmitted data**: bottom pane, laid out like the received view.

## Data views (received and transmitted)

- **Ruler**: a grey header row of column numbers, labelled 1, 5, 10, 15, …
  It can be hidden, and its start number can be changed.
- **ASCII-only mode**: one character per cell in a monospace font.
  - Printable ASCII is drawn as itself.
  - CR, LF and NUL are drawn as small symbol glyphs (`\r`, `\n`, `\0`).
  - Other non-printable bytes, such as 0x01 and 0xFF, are drawn as empty boxes.
- **Multi-format mode** (more than one of Ascii/Hex/Dec/Bin checked): a grid with one
  column per byte. Each enabled format is a coloured row, in this order:

  | Row | Colour | Example (`H`) |
  |---|---|---|
  | Ruler (byte number) | grey | `1` |
  | Ascii | cyan | `H` |
  | Hex | blue/lavender | `48`, uppercase, 2 digits |
  | Dec | pink/red | `072`, zero-padded to 3 digits |
  | Bin | green | `01001000` |

  Rows wrap at the window width.
- **Selection bar**: `Selection (-)` sits at the bottom of each view. It probably shows
  information about the selected range.
- **Re-rendering**: changing the format, newline or "every N" settings re-renders data
  that has already been received. Nothing is lost.
- Context menu items found in the binary's strings: Copy as Ascii/Hex/Decimal/Binary, and Copy selection.

## Selection and copy

Observed on HTerm 0.8.9 by receiving `Hello\r\nWorld\0\xFF end` and then, a minute later, `X`.

- **Click** selects one byte. **Shift+click** extends the selection from the
  first clicked byte. (Dragging didn't extend it in an automated test.)
- Selected cells get a blue/lavender background.
- **Selection bar** under each view:
  - Nothing selected: `Selection (-)`.
  - Bytes 1 to 7 selected: `Selection (1:7) 7  -  Timediff 00:00:00.000.0`
    (zero-based first and last byte index, then the count).
  - `Timediff`: time between the first and last selected bytes, written
    `HH:MM:SS.mmm.uuu` (e.g. `00:01:09.136.548` across two chunks a minute apart).
  - On the right: `Timestamp: 16:45:53.140.952`, the arrival time of the last
    selected byte. All bytes of one chunk share its time.
- **Right-click menu**: Copy as Ascii, Copy as Hex, Copy as Decimal, Copy as Binary.
  For `e l l o CR LF W`:

  | Copy as | Clipboard |
  |---|---|
  | Ascii | `ello<\r><\n>W`; NUL is `<\0>`, and 0xFF became `?` |
  | Hex | `65 6C 6C 6F 0D 0A 57` |
  | Decimal | `101 108 108 111 013 010 087` |
  | Binary | `01100101 01101100 01101100 01101111 00001101 00001010 01010111` |

> Our version:
> - Dragging also selects.
> - Other non-printable bytes are copied as `<\xFF>` instead of `?`, so no information is lost.
> - Ctrl+C copies the selection in the view's first enabled format.

## Counters

- **Rx / Tx**: totals of received and transmitted bytes. Each has its own Reset.
- **Count**: the user picks a byte value (default 0). The field next to it counts how many
  times that byte has been received. During the test, one 0x00 received gave Count = 1.

## Other output options

- **Autoscroll**: default on.
- **Show errors**: config `ShowReceiveErrors`; "Highlight errors in received data".
- **Clear at**: config `ClearAtByte status/value`. Clears the output when that byte is received (assumed from the name; not tested yet).
- **Receive bitmask**: config `Bitmask bits`. A mask applied to received bytes (assumed; not tested yet).
- **Combined view**: shows transmitted and received data in one view (config `CombinedViewTxRx`).

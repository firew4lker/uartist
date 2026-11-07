# Newline handling in data views

Newlines here are **display-only**: they control how received bytes are broken
into rows. They never change the data. Every option re-renders existing data
when it changes.

## Newline at

- Presets: None, CR (`d[13]`), LF (`d[10]`), CR+LF (`d[13 10]`), Null (`d[0]`), ETX (`d[3]`).
  The list is editable via Options → Modify Newline At….
- A line ends **after** the matched sequence; the sequence stays in that line.
  - Data: `one\r\ntwo\nthree\rfour\n` with **LF** selected gives:
    ```
    one\r\n
    two\n
    three\rfour\n
    ```
- **Show newline characters** (default on): when off, only the matched newline
  sequence is hidden. Other control characters are still shown (with LF selected, `\r` stays visible).

## Newline every … characters

- When N > 0, a line is broken after every N bytes (0 = off).
  - With N = 5 and Newline at = None, the same data gives
    `one\r\n` / `two\nt` / `hree\r` / `four\n`.
- How it combines with "Newline at" is not tested yet. It most likely counts from the last break of either kind.

## Newline by pause

- "Newline after … ms receive pause (0=off)" (config `NewlineByPause interval`).
  Starts a new line when no data arrives for the given time. Not tested yet.
  This needs per-chunk receive timestamps in our data model.

## Newline on source change

- Default on (config `NewlineOnSourceChange`). Tooltip: "Add a newline when data changes source".
  Matters in the combined TX/RX view, where a line break separates sent and received data.

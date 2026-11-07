# Open questions

What we still need to find out, and how.

| # | Question | How to answer |
|---|---|---|
| 1 | CMD input type: syntax and what it does (`\c…`?) | Try `\cA`, `\c@`, and similar; if still unclear, a focused look in Ghidra at the "\c must be followed…" error |
| 2 | Full mfs notation (ASCII, binary, CMD tags) | Save a `.hts` with mixed tokens |
| 3 | `.hts` sequence file schema | Create and save sequences in HTerm |
| 4 | Clear at, Receive bitmask, Show errors behaviour | Test over a socat pair |
| 5 | Newline every N combined with Newline at; newline by pause | Test over a socat pair |
| 6 | Combined TX/RX view and "newline on source change" | Test over a socat pair |
| 7 | Save output options (Source, Timestamps, Separator, Format) | Open the split-button menu and save files |
| 8 | Autosend and Send file behaviour | Test over a socat pair |
| 9 | Parity/framing error display ("Show errors") | Needs two adapters with TX/RX crossed, set to mismatched parity. All status lines and flow control are done (see connection.md) |
| 10 | Data bits list includes 5? | Scroll the Data dropdown |
| 11 | Windows behaviour (port names, settings) | Not available here; follow `serialport` crate conventions |

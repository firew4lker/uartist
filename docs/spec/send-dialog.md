# Autosend and Send file

Observed on HTerm 0.8.9 over a socat pair.

Both buttons open **the same dialog**. Only the source differs:

| Button | Source shown | Data |
|---|---|---|
| Autosend (next to the input line) | `[Inputline]` | the current input line, as typed (chips) |
| Send file (Input control panel) | the file name, e.g. `sendme.bin` | the file's bytes, picked in a file dialog first |

## Dialog

- **Repetitions**: how many times to send the data (default 1).
- **Delay (0=none)**: pause between repetitions, in tenths of a second (`x0.1s`).
  Repetitions and delay are remembered between uses (config `Autosend`).
- Status:
  - `Repetition 1 of 3` and `Next in 0.1s` (countdown to the next repetition).
  - A progress bar for the current repetition, and `4 of 4 byte`.
  - `Sendtime 00:00:0` and `Speed 0.004 KiB/s`.
- Buttons: **Start**, **Stop** (only while running), **Cancel**.
- While running, repetitions and delay are greyed out.
- The dialog closes by itself when all repetitions are done.

Test: input line `ping`, 3 repetitions, delay 5 (0.5 s): `ping` was sent three
times (the Tx view showed `pingpingping`). No send-on-enter suffix was added
(it was set to None, so that case is still open).

## Our version

- One send dialog for both, as in HTerm.
- Repetitions: 1–100000, plus a "Repeat forever" checkbox. Delay in milliseconds.
- Shows repetition n of N, the countdown, bytes sent of total, elapsed time,
  estimated remaining time and speed.
- Stop ends the run. Unsent data that is already queued is discarded.
- Autosend applies the send-on-enter prefix and suffix, like pressing Enter.
  Send file sends the file's bytes unchanged.

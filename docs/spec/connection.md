# Connection and serial port behaviour

Observed on HTerm 0.8.9 (Linux), using socat virtual port pairs and strace.

## Port list

- Ports are found by matching glob patterns, stored in the config under
  `Comport/ComportPatterns`. Defaults:
  `/dev/ttyS*`, `/dev/ttyUSB*`, `/dev/pts/*`, `/dev/ttyACM*`.
- `/dev/pts/*` is included by default, so virtual (pty) ports show up.
- The **R** button next to the port box refreshes the list.
- The port box is editable: you can type a path that isn't in the list.
- At startup the selected port came from the refreshed list (the first entry),
  not from `Comport/Selection` in the config.

> Our version: don't auto-select a port at startup, and never auto-connect.
> HTerm's default list includes the user's own terminals (`/dev/pts/*`), and
> connecting to one of those takes over that terminal.

## Settings and option lists

| Setting | Options | Default |
|---|---|---|
| Baud | 50, 75, 110, 134, 150, 200, 300, 600, 1200, 1800, 2400, 4800, 9600, 19200, 38400, 57600, 115200, 230400, 460800, 500000, 576000, 921600, 1000000, 1152000, 1500000, 2000000, 2500000, 3000000, 3500000, 4000000. The box is editable for custom values. | 115200 |
| Data bits | 5–8 (8, 7 and 6 seen; 5 assumed) | 8 |
| Stop bits | 1, 1.5, 2 | 1 |
| Parity | None, Even, Odd, Mark, Space | None |
| CTS flow control | checkbox | off |

Error strings in the binary:
- "Baudrate has to be a valid number greater than zero"
- "Baudrate value of '%ld' not supported"

## Opening a port (Linux)

1. `open(path, O_RDWR | O_NOCTTY)`, in blocking mode.
2. Save the current termios settings (`TCGETS2`).
3. Apply raw mode with `TCSETS2`:
   - `c_iflag = INPCK`
   - `c_oflag` has no `OPOST`
   - `c_lflag` has no `ICANON`, `ECHO` or `ISIG`
   - `c_cflag = <baud> | CS<n> | CREAD`, plus the parity and stop flags
4. Try `TIOCGICOUNT` to read the line error counters. On a pty this fails with ENOTTY, which is tolerated.
5. Start a thread that waits on `TIOCMIWAIT` for status-line changes. A pty gives ENOTTY.
6. Read the status lines with `TIOCMGET`. On failure, log: "Device does not support reading status lines via ioctl(TIOCMGET). This can happen for virtual serial ports."
7. On connect, **clear DTR and RTS** (`TIOCMBIC`), because the config defaults to `StatusLines DTR=false RTS=false`.

## Closing a port

1. `TCFLSH(TCOFLUSH)`, which discards any unsent output.
2. Restore the termios settings saved at open.
3. `close()`.

## Status bar

- Connected: `Connected to /dev/pts/3 (b:115200 d:8 s:1 p:None)`
- Disconnected: `Not connected`
- `History -/1/30`: the input-history position, the number of entries, and the maximum (30).

## Control and status lines

Tested on an FTDI FT232R (`/dev/ttyUSB0`, `ftdi_sio` driver) with a 6-pin header,
wired TXD↔RXD and DTR→CTS. DSR, RI and DCD are not on that header, so only CTS was tested.

- **DTR** and **RTS** are **toggle buttons** in the Input control panel:
  - pressed (sunken) = line on; raised = off.
  - disabled while disconnected.
  - each click = one `TIOCMBIS` (on) or `TIOCMBIC` (off) for that single line.
- On connect, both lines are cleared (`TIOCMBIC DTR`, then `RTS`). Both buttons start raised.
- **CTS, DSR, RI, DCD LEDs** ("Incoming statuslines" toolbar): dark green = off,
  bright green = on. With the jumper, CTS lit up exactly while DTR was on.
- Status-line updates are **event-driven**, not polled. A background thread blocks in
  `TIOCMIWAIT`, and after each wake-up the lines are read with `TIOCMGET`.
  Over about 30 s of testing there were only 4 `TIOCMGET` calls.
  - Our version: the `serialport` crate has no wait-for-change call, so we poll
    (e.g. every 50 ms), which also works on Windows.
- `TIOCGICOUNT` (line error counters) succeeds on real hardware. It is read at connect.
  It probably feeds "Show errors" and the status line.

### All four LEDs (8-pin FT232R)

Retested on a board with all signal pins, wired TXD↔RXD, DTR→DSR+DCD, RTS→CTS+RI:

| DTR | RTS | CTS | DSR | RI | DCD |
|---|---|---|---|---|---|
| off | off | off | off | off | off |
| on | off | off | **on** | off | **on** |
| on | on | **on** | **on** | **on** | **on** |
| off | on | **on** | off | **on** | off |
| off | off | off | off | off | off |

- Each LED shows its own line. Nothing is swapped or inverted (the driver already converts the
  active-low pin levels).
- RI is shown as a plain level and follows both edges. It is not latched or pulsed.
- One `TIOCMIWAIT` wake-up and one `TIOCMGET` per change (6 each over the test).

## CTS flow control

- The "CTS Flow control" checkbox can be changed **while connected**. It re-applies the
  port settings with or without `CRTSCTS` (hardware RTS/CTS handshake).
- With flow control on and CTS low: `write()` still returns at once (the driver buffers),
  and the **Tx counter increases immediately**. The data is held back until CTS rises,
  then sent; in the test, 8 held-back bytes arrived when DTR (→ CTS) went on.
- So the Tx counter counts bytes handed to the driver, not bytes physically sent.
- On disconnect, `TCOFLUSH` **discards** any data still held back by flow control.

## Real-hardware port settings

On `/dev/ttyUSB0` the applied `c_cflag` was `B115200|CS8|CREAD|HUPCL|CLOCAL`:
- `CLOCAL`: ignore modem control lines.
- `HUPCL`: drop DTR and RTS on close.
(On a pty, `HUPCL` and `CLOCAL` weren't shown.) On close, the termios settings saved at open are
restored (this adapter's original was 9600 baud).

## Not tested (hardware limits)

- Parity, framing and overrun error display ("Show errors"): needs two ports, or a second
  device with different settings, because a loopback on one port always matches itself.

## Focus options

- Options menu → "Focus loss disconnect" (config: `DisconnectOnFocusLoss`).
- Strings also mention "Connect comport on focus".

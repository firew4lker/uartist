//! An open serial port, run by a background worker thread.
//!
//! The GUI talks to the worker through [`Connection`]: commands go in over a
//! channel and [`Event`]s come back. The worker calls a `wake` callback after
//! producing events so the GUI can repaint (e.g. `egui::Context::request_repaint`).
//!
//! Behaviour follows `docs/spec/connection.md`:
//! - DTR is switched off right after opening; RTS too, unless hardware flow
//!   control is on (then RTS must be on for the other side to send).
//! - The Tx count is bytes handed to the driver, not bytes physically sent.
//! - Closing discards output still waiting (e.g. held back by CTS flow control).
//! - Status lines are polled (every [`LINE_POLL_INTERVAL`]), since the
//!   serialport crate has no portable wait-for-change call.

use std::io;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use crossbeam_channel::{Receiver, Sender, TryRecvError};
use serialport::{ClearBuffer, FlowControl, SerialPort};

use crate::settings::{DataBits, Parity, PortSettings, StopBits};

pub const LINE_POLL_INTERVAL: Duration = Duration::from_millis(50);
const READ_TIMEOUT_IDLE: Duration = Duration::from_millis(10);
const IO_TIMEOUT_BUSY: Duration = Duration::from_millis(2);
const READ_BUF_SIZE: usize = 4096;

/// Levels of the incoming modem status lines.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ModemLines {
    pub cts: bool,
    pub dsr: bool,
    pub ri: bool,
    pub dcd: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    /// Bytes read from the port.
    Received { at: Instant, data: Vec<u8> },
    /// Bytes handed to the driver for sending.
    Sent { at: Instant, data: Vec<u8> },
    /// Status lines changed (also sent once after opening).
    Lines(ModemLines),
    /// The port can't report status lines (e.g. a virtual port). Sent once.
    LinesUnavailable,
    /// The port can't set DTR/RTS (e.g. a virtual port). Sent once.
    OutputLinesUnavailable,
    /// A non-fatal problem, e.g. DTR couldn't be set.
    Warning(String),
    /// A fatal error; a [`Event::Closed`] follows.
    Error(String),
    /// The worker has stopped and the port is closed.
    Closed,
}

#[derive(Debug, thiserror::Error)]
pub enum OpenError {
    #[error("{0} is not supported yet")]
    Unsupported(&'static str),
    #[error("baud rate has to be a number greater than zero")]
    InvalidBaud,
    #[error("{0}")]
    Invalid(&'static str),
    #[error("could not apply port settings: {0}")]
    Native(#[from] io::Error),
    #[error("{0}")]
    Serial(#[from] serialport::Error),
}

#[derive(Debug)]
enum Command {
    Send(Vec<u8>),
    SetDtr(bool),
    SetRts(bool),
    SetCtsFlow(bool),
    CancelSend,
    Close,
}

/// Handle to an open port. Dropping it closes the port.
pub struct Connection {
    settings: PortSettings,
    commands: Sender<Command>,
    events: Receiver<Event>,
    thread: Option<JoinHandle<()>>,
}

impl Connection {
    /// Open the port and start the worker. Errors are returned directly.
    pub fn open(
        settings: PortSettings,
        wake: impl Fn() + Send + 'static,
    ) -> Result<Self, OpenError> {
        let port = open_port(&settings)?;
        let cts_flow = settings.cts_flow;
        let (cmd_tx, cmd_rx) = crossbeam_channel::unbounded();
        let (ev_tx, ev_rx) = crossbeam_channel::unbounded();
        let thread = std::thread::Builder::new()
            .name(format!("serial {}", settings.path))
            .spawn(move || Worker::new(port, cmd_rx, ev_tx, Box::new(wake)).run(cts_flow))
            .map_err(|e| OpenError::Serial(e.into()))?;
        Ok(Self {
            settings,
            commands: cmd_tx,
            events: ev_rx,
            thread: Some(thread),
        })
    }

    pub fn settings(&self) -> &PortSettings {
        &self.settings
    }

    pub fn send(&self, data: Vec<u8>) {
        if !data.is_empty() {
            let _ = self.commands.send(Command::Send(data));
        }
    }

    /// Discard data queued for sending that hasn't gone out yet.
    pub fn cancel_send(&self) {
        let _ = self.commands.send(Command::CancelSend);
    }

    pub fn set_dtr(&self, on: bool) {
        let _ = self.commands.send(Command::SetDtr(on));
    }

    pub fn set_rts(&self, on: bool) {
        let _ = self.commands.send(Command::SetRts(on));
    }

    pub fn set_cts_flow(&mut self, on: bool) {
        self.settings.cts_flow = on;
        let _ = self.commands.send(Command::SetCtsFlow(on));
    }

    /// All events produced since the last call. Never blocks.
    pub fn poll_events(&self) -> Vec<Event> {
        self.events.try_iter().collect()
    }

    /// Close the port and wait for the worker to finish.
    /// Returns the last events, ending with [`Event::Closed`].
    pub fn close(mut self) -> Vec<Event> {
        self.shutdown();
        self.events.try_iter().collect()
    }

    fn shutdown(&mut self) {
        let _ = self.commands.send(Command::Close);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

impl Drop for Connection {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn open_port(s: &PortSettings) -> Result<Box<dyn SerialPort>, OpenError> {
    if s.baud == 0 {
        return Err(OpenError::InvalidBaud);
    }
    let data_bits = match s.data_bits {
        DataBits::Five => serialport::DataBits::Five,
        DataBits::Six => serialport::DataBits::Six,
        DataBits::Seven => serialport::DataBits::Seven,
        DataBits::Eight => serialport::DataBits::Eight,
    };
    let stop_bits = match s.stop_bits {
        StopBits::One => serialport::StopBits::One,
        StopBits::Two => serialport::StopBits::Two,
        // UARTs only do 1.5 stop bits with 5 data bits.
        StopBits::OnePointFive if s.data_bits != DataBits::Five => {
            return Err(OpenError::Invalid("1.5 stop bits need 5 data bits"));
        }
        // On Unix, "2 stop bits" with 5 data bits is 1.5; Windows sets it natively.
        StopBits::OnePointFive if cfg!(windows) => serialport::StopBits::One,
        StopBits::OnePointFive => serialport::StopBits::Two,
    };
    let parity = match s.parity {
        Parity::None => serialport::Parity::None,
        Parity::Even => serialport::Parity::Even,
        Parity::Odd => serialport::Parity::Odd,
        // Applied natively after opening.
        Parity::Mark | Parity::Space => serialport::Parity::None,
    };
    let builder = serialport::new(&s.path, s.baud)
        .data_bits(data_bits)
        .stop_bits(stop_bits)
        .parity(parity)
        .flow_control(flow(s.cts_flow))
        .timeout(READ_TIMEOUT_IDLE);
    if !crate::native::needed(s.parity, s.stop_bits) {
        return Ok(builder.open()?);
    }
    open_with_native(builder, s)
}

#[cfg(any(target_os = "linux", windows))]
fn open_with_native(
    builder: serialport::SerialPortBuilder,
    s: &PortSettings,
) -> Result<Box<dyn SerialPort>, OpenError> {
    let port = builder.open_native()?;
    crate::native::apply(&port, s.parity, s.stop_bits)?;
    Ok(Box::new(port))
}

#[cfg(not(any(target_os = "linux", windows)))]
fn open_with_native(
    _builder: serialport::SerialPortBuilder,
    s: &PortSettings,
) -> Result<Box<dyn SerialPort>, OpenError> {
    Err(OpenError::Unsupported(match s.parity {
        Parity::Mark => "Mark parity on this system",
        _ => "Space parity on this system",
    }))
}

fn flow(cts: bool) -> FlowControl {
    if cts {
        FlowControl::Hardware
    } else {
        FlowControl::None
    }
}

fn is_timeout(e: &io::Error) -> bool {
    matches!(
        e.kind(),
        io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
    )
}

struct Worker {
    port: Box<dyn SerialPort>,
    commands: Receiver<Command>,
    events: Sender<Event>,
    wake: Box<dyn Fn() + Send>,
    pending: Vec<u8>,
    lines: Option<ModemLines>,
    lines_state: LinesState,
    /// Whether setting DTR/RTS works: unknown until the first attempt.
    outputs_supported: Option<bool>,
    next_line_poll: Instant,
    timeout: Duration,
    produced: bool,
}

/// Whether the port can report status lines.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LinesState {
    /// No read has succeeded yet.
    Unknown,
    /// Reads work. A later failure is reported but polling continues.
    Supported { failing: bool },
    /// The first read failed: the port can't do it (e.g. a virtual port).
    Unsupported,
}

impl Worker {
    fn new(
        port: Box<dyn SerialPort>,
        commands: Receiver<Command>,
        events: Sender<Event>,
        wake: Box<dyn Fn() + Send>,
    ) -> Self {
        Self {
            port,
            commands,
            events,
            wake,
            pending: Vec::new(),
            lines: None,
            lines_state: LinesState::Unknown,
            outputs_supported: None,
            next_line_poll: Instant::now(),
            timeout: READ_TIMEOUT_IDLE,
            produced: false,
        }
    }

    fn emit(&mut self, e: Event) {
        let _ = self.events.send(e);
        self.produced = true;
    }

    fn run(mut self, cts_flow: bool) {
        // Match HTerm: output lines start off. With hardware flow control the
        // driver needs RTS on to tell the other side we're ready to receive.
        self.set_line(Command::SetDtr(false));
        self.set_line(Command::SetRts(cts_flow));

        let result = self.event_loop();
        if let Err(e) = result {
            self.emit(Event::Error(e.to_string()));
        }
        // Like HTerm: discard output that hasn't gone out yet.
        let _ = self.port.clear(ClearBuffer::Output);
        self.pending.clear();
        self.emit(Event::Closed);
        (self.wake)();
    }

    /// Returns Ok on a requested close, Err on a fatal port error.
    fn event_loop(&mut self) -> io::Result<()> {
        let mut buf = vec![0u8; READ_BUF_SIZE];
        loop {
            loop {
                match self.commands.try_recv() {
                    Ok(Command::Close) | Err(TryRecvError::Disconnected) => return Ok(()),
                    Ok(Command::Send(data)) => self.pending.extend_from_slice(&data),
                    Ok(Command::CancelSend) => {
                        self.pending.clear();
                        let _ = self.port.clear(ClearBuffer::Output);
                    }
                    Ok(cmd @ (Command::SetDtr(_) | Command::SetRts(_))) => self.set_line(cmd),
                    Ok(Command::SetCtsFlow(on)) => {
                        if let Err(e) = self.port.set_flow_control(flow(on)) {
                            self.emit(Event::Warning(format!("Could not set flow control: {e}")));
                        } else if on {
                            // RTS may have been switched off by hand; raise it
                            // so the other side is allowed to send.
                            self.set_line(Command::SetRts(true));
                        }
                    }
                    Err(TryRecvError::Empty) => break,
                }
            }

            let busy = !self.pending.is_empty();
            self.set_timeout(if busy {
                IO_TIMEOUT_BUSY
            } else {
                READ_TIMEOUT_IDLE
            })?;
            if busy {
                match self.port.write(&self.pending) {
                    Ok(n) if n > 0 => {
                        let data: Vec<u8> = self.pending.drain(..n).collect();
                        self.emit(Event::Sent {
                            at: Instant::now(),
                            data,
                        });
                    }
                    Ok(_) => {}
                    Err(e) if is_timeout(&e) => {}
                    Err(e) => return Err(e),
                }
            }

            match self.port.read(&mut buf) {
                // Timeouts come back as errors, so 0 bytes means end of file:
                // the port has gone away.
                Ok(0) => {
                    return Err(io::Error::new(
                        io::ErrorKind::UnexpectedEof,
                        "the port was closed",
                    ));
                }
                Ok(n) => self.emit(Event::Received {
                    at: Instant::now(),
                    data: buf[..n].to_vec(),
                }),
                Err(e) if is_timeout(&e) => {}
                Err(e) => return Err(e),
            }

            if self.lines_state != LinesState::Unsupported && Instant::now() >= self.next_line_poll
            {
                self.poll_lines();
                self.next_line_poll = Instant::now() + LINE_POLL_INTERVAL;
            }

            if std::mem::take(&mut self.produced) {
                (self.wake)();
            }
        }
    }

    /// Only touch the port when the timeout actually changes (on Windows each
    /// change is a system call).
    fn set_timeout(&mut self, timeout: Duration) -> io::Result<()> {
        if timeout != self.timeout {
            self.port.set_timeout(timeout)?;
            self.timeout = timeout;
        }
        Ok(())
    }

    fn set_line(&mut self, cmd: Command) {
        let (name, result) = match cmd {
            Command::SetDtr(on) => ("DTR", self.port.write_data_terminal_ready(on)),
            Command::SetRts(on) => ("RTS", self.port.write_request_to_send(on)),
            _ => return,
        };
        match (result, self.outputs_supported) {
            (Ok(()), _) => self.outputs_supported = Some(true),
            // The very first attempt (right after opening) failed: the port
            // can't do it, e.g. a virtual port. Say so once, then stay quiet.
            (Err(_), None) => {
                self.outputs_supported = Some(false);
                self.emit(Event::OutputLinesUnavailable);
            }
            (Err(_), Some(false)) => {}
            (Err(e), Some(true)) => {
                self.emit(Event::Warning(format!("Could not set {name}: {e}")));
            }
        }
    }

    fn poll_lines(&mut self) {
        let read = (|| {
            Ok::<_, serialport::Error>(ModemLines {
                cts: self.port.read_clear_to_send()?,
                dsr: self.port.read_data_set_ready()?,
                ri: self.port.read_ring_indicator()?,
                dcd: self.port.read_carrier_detect()?,
            })
        })();
        match (read, self.lines_state) {
            (Ok(lines), _) => {
                self.lines_state = LinesState::Supported { failing: false };
                if self.lines != Some(lines) {
                    self.lines = Some(lines);
                    self.emit(Event::Lines(lines));
                }
            }
            (Err(_), LinesState::Unknown) => {
                self.lines_state = LinesState::Unsupported;
                self.emit(Event::LinesUnavailable);
            }
            (Err(e), LinesState::Supported { failing: false }) => {
                // Possibly transient: report once and keep polling.
                self.lines_state = LinesState::Supported { failing: true };
                self.emit(Event::Warning(format!("Could not read status lines: {e}")));
            }
            (Err(_), _) => {}
        }
    }
}

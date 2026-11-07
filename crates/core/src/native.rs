//! Port settings the `serialport` crate can't make: Mark/Space parity and,
//! on Windows, 1.5 stop bits. They are applied right after opening, on the
//! port's native handle.
//!
//! This is the only module with `unsafe` code: borrowing the raw file
//! descriptor (Linux) and calling the Win32 comm-state functions (Windows).

#![allow(unsafe_code)]

use std::io;

use crate::settings::{Parity, StopBits};

/// Whether these settings need [`apply`] after opening.
pub fn needed(parity: Parity, stop_bits: StopBits) -> bool {
    matches!(parity, Parity::Mark | Parity::Space)
        || (cfg!(windows) && stop_bits == StopBits::OnePointFive)
}

#[cfg(target_os = "linux")]
pub type NativePort = serialport::TTYPort;
#[cfg(windows)]
pub type NativePort = serialport::COMPort;

/// Linux: Mark/Space parity is PARENB plus CMSPAR, with PARODD for Mark.
/// (1.5 stop bits need nothing here: CSTOPB with 5 data bits *is* 1.5.)
#[cfg(target_os = "linux")]
pub fn apply(port: &NativePort, parity: Parity, _stop_bits: StopBits) -> io::Result<()> {
    use nix::sys::termios::{ControlFlags, InputFlags, SetArg, tcgetattr, tcsetattr};
    use std::os::fd::{AsRawFd, BorrowedFd};

    // SAFETY: the descriptor belongs to `port`, which is borrowed for this
    // whole function, so it stays open and valid while we use it.
    let fd = unsafe { BorrowedFd::borrow_raw(port.as_raw_fd()) };
    let mut t = tcgetattr(fd)?;
    match parity {
        Parity::Mark => {
            t.control_flags |= ControlFlags::PARENB | ControlFlags::CMSPAR | ControlFlags::PARODD
        }
        Parity::Space => {
            t.control_flags |= ControlFlags::PARENB | ControlFlags::CMSPAR;
            t.control_flags.remove(ControlFlags::PARODD);
        }
        _ => return Ok(()),
    }
    // Check parity on input, as for Even/Odd.
    t.input_flags |= InputFlags::INPCK;
    tcsetattr(fd, SetArg::TCSANOW, &t)?;
    Ok(())
}

/// Windows: set the parity and stop bits fields of the port's DCB.
#[cfg(windows)]
pub fn apply(port: &NativePort, parity: Parity, stop_bits: StopBits) -> io::Result<()> {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Devices::Communication::{
        DCB, GetCommState, MARKPARITY, ONE5STOPBITS, SPACEPARITY, SetCommState,
    };

    const F_PARITY: u32 = 1 << 1; // DCB.fParity: enable parity checking
    let handle = port.as_raw_handle();
    let mut dcb = DCB {
        DCBlength: std::mem::size_of::<DCB>() as u32,
        ..Default::default()
    };
    // SAFETY: `handle` is the open comm handle owned by `port` (borrowed for
    // this function) and `dcb` is a correctly sized DCB.
    if unsafe { GetCommState(handle, &mut dcb) } == 0 {
        return Err(io::Error::last_os_error());
    }
    match parity {
        Parity::Mark => {
            dcb.Parity = MARKPARITY;
            dcb._bitfield |= F_PARITY;
        }
        Parity::Space => {
            dcb.Parity = SPACEPARITY;
            dcb._bitfield |= F_PARITY;
        }
        _ => {}
    }
    if stop_bits == StopBits::OnePointFive {
        dcb.StopBits = ONE5STOPBITS;
    }
    // SAFETY: as above.
    if unsafe { SetCommState(handle, &dcb) } == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

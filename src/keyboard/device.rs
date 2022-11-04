use crate::keyboard::{KeyEvent, Keyboard};
use crate::keylogger::KeyloggerResult;
use std::convert::TryFrom;
use std::fs::{self, File};
use std::io;
use std::mem;
use std::os::fd::{AsRawFd, RawFd};
use std::os::unix::fs::FileTypeExt;
use std::path::PathBuf;

const IOC_NRBITS: libc::c_ulong = 8;
const IOC_TYPEBITS: libc::c_ulong = 8;
const IOC_SIZEBITS: libc::c_ulong = 14;
const IOC_NRSHIFT: libc::c_ulong = 0;
const IOC_TYPESHIFT: libc::c_ulong = IOC_NRSHIFT + IOC_NRBITS;
const IOC_SIZESHIFT: libc::c_ulong = IOC_TYPESHIFT + IOC_TYPEBITS;
const IOC_DIRSHIFT: libc::c_ulong = IOC_SIZESHIFT + IOC_SIZEBITS;
const IOC_READ: libc::c_ulong = 2;

/// Read [`libc::input_event`s](libc::input_event) from the specified file descriptor.
pub(crate) fn read_key_events(fd: RawFd) -> io::Result<Vec<KeyEvent>> {
    Ok(read_input_events(fd)?
        .iter()
        .filter_map(|e| KeyEvent::try_from(e).ok())
        .collect())
}

fn read_input_events(fd: impl Into<RawFd>) -> io::Result<Vec<libc::input_event>> {
    const MAX_INPUT_EV: usize = 128;

    let mut input_events = [mem::MaybeUninit::<libc::input_event>::uninit(); MAX_INPUT_EV];

    let n = unsafe { libc::read(fd.into(), input_events.as_mut_ptr() as *mut _, MAX_INPUT_EV) };

    if n < 0 {
        return Err(io::Error::last_os_error());
    }

    let n = (n as usize) / mem::size_of::<libc::input_event>();

    // The first n elements of the array are initialized:
    Ok(input_events[..n]
        .iter()
        .map(|e| unsafe { e.assume_init() })
        .collect())
}

/// Find all available keyboard devices.
pub(crate) fn find_keyboard_devices() -> KeyloggerResult<impl Iterator<Item = Keyboard>> {
    Ok(find_char_devices()?.filter_map(|entry| Keyboard::try_from(entry.as_path()).ok()))
}

/// Set the `O_NONBLOCK` flag for the specified file descriptor.
pub(crate) fn set_nonblocking(f: &File) -> KeyloggerResult<()> {
    let res = unsafe { libc::fcntl(f.as_raw_fd(), libc::F_SETFL, libc::O_NONBLOCK) };

    if res < 0 {
        return Err(io::Error::last_os_error().into());
    }

    Ok(())
}

/// Read the name of the specified keyboard device using the `EVIOCGNAME` ioctl.
pub(crate) fn read_name(f: &File) -> KeyloggerResult<String> {
    const DEVICE_NAME_MAX_LEN: usize = 512;

    let mut device_name = [0u8; DEVICE_NAME_MAX_LEN];

    let eviocgname = (IOC_READ << IOC_DIRSHIFT)
        | (('E' as libc::c_ulong) << IOC_TYPESHIFT)
        | (0x06 << IOC_NRSHIFT)
        | ((device_name.len() as libc::c_ulong) << IOC_SIZESHIFT);

    ioctl(
        f.as_raw_fd(),
        eviocgname,
        device_name.as_mut_ptr() as *mut libc::c_ulong,
    )?;

    Ok(String::from_utf8_lossy(&device_name).into())
}

/// Read the features supported by the specified device using the `EVIOCGBIT` ioctl.
pub(crate) fn read_event_flags(f: &File) -> KeyloggerResult<libc::c_ulong> {
    let mut ev_flags: libc::c_ulong = 0;

    let eviocgbit = (IOC_READ << IOC_DIRSHIFT)
        | (('E' as libc::c_ulong) << IOC_TYPESHIFT)
        | (0x20 << IOC_NRSHIFT)
        | (((mem::size_of::<libc::c_ulong>()) as libc::c_ulong) << IOC_SIZESHIFT);

    ioctl(
        f.as_raw_fd(),
        eviocgbit,
        (&mut ev_flags) as *mut libc::c_ulong,
    )?;

    Ok(ev_flags)
}

/// Get all character devices from `/dev/input`.
fn find_char_devices() -> KeyloggerResult<impl Iterator<Item = PathBuf>> {
    const INPUT_DIR: &str = "/dev/input";

    Ok(fs::read_dir(INPUT_DIR)?.filter_map(|entry| {
        let entry = entry.ok()?;
        let file_type = fs::metadata(entry.path()).ok()?.file_type();

        if file_type.is_char_device() {
            Some(entry.path())
        } else {
            None
        }
    }))
}

fn ioctl(fd: RawFd, request: libc::c_ulong, buf: *mut libc::c_ulong) -> KeyloggerResult<()> {
    let res = unsafe { libc::ioctl(fd, request, buf) };

    if res < 0 {
        Err(io::Error::last_os_error().into())
    } else {
        Ok(())
    }
}

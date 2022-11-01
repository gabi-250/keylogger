mod device;

use crate::error::KeyloggerError;
use crate::key_code::KeyCode;
use crate::keylogger::KeyloggerResult;
use chrono::naive::NaiveDateTime;
use futures::ready;
use std::convert::TryFrom;
use std::fs::File;
use std::future::Future;
use std::os::fd::AsRawFd;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use tokio::io::unix::AsyncFd;

pub(crate) use device::find_keyboard_devices;

// Some interesting Event types (see [input-event-codes.h] and the [kernel docs]).
//
// [input-event-codes.h]: https://elixir.bootlin.com/linux/v5.19.17/source/include/uapi/linux/input-event-codes.h#L38)
// [kernel docs]: https://www.kernel.org/doc/html/latest/input/event-codes.html
const EV_SYN: libc::c_ulong = 0x00;
const EV_KEY: libc::c_ulong = 0x01;
const EV_MSC: libc::c_ulong = 0x04;
const EV_REP: libc::c_ulong = 0x14;

/// The `value` of an EV_KEY caused by a key being released.
const EV_KEY_RELEASE: i32 = 0;
/// The `value` of an EV_KEY caused by a key press.
const EV_KEY_PRESS: i32 = 1;

/// A keyboard device.
#[derive(Debug)]
pub(crate) struct Keyboard {
    /// The name of the device.
    pub(crate) name: String,
    /// The path of the input device (e.g. `/dev/input/event0`).
    pub(crate) device: PathBuf,
    /// The file descriptor of the open input device file.
    pub(crate) async_fd: AsyncFd<File>,
}

impl TryFrom<PathBuf> for Keyboard {
    type Error = KeyloggerError;

    fn try_from(device: PathBuf) -> Result<Self, Self::Error> {
        let file = File::open(&device)?;
        let flags = device::read_event_flags(&file)?;

        if !has_keyboard_flags(flags) {
            return Err(KeyloggerError::NotAKeyboard(device));
        }

        device::set_nonblocking(&file)?;

        let name = device::read_name(&file)?;

        Ok(Keyboard {
            name,
            device,
            async_fd: AsyncFd::new(file)?,
        })
    }
}

impl Keyboard {
    pub(crate) fn read_key_events(self: &Arc<Self>) -> KeyEventFuture {
        KeyEventFuture(Arc::clone(self))
    }
}

/// A future that resolves once a number of keyboard events have been received.
pub(crate) struct KeyEventFuture(Arc<Keyboard>);

/// A key event (EV_KEY).
#[derive(Debug, PartialEq)]
pub struct KeyEvent {
    /// The timestamp of the event.
    pub ts: NaiveDateTime,
    /// The action that triggered the event.
    pub cause: KeyEventCause,
    /// The key code of the key that triggered the event.
    pub code: KeyCode,
}

/// The reason a `KeyEvent` fired.
#[derive(Debug, PartialEq)]
pub enum KeyEventCause {
    /// The key was pressed.
    Press,
    /// The key was released.
    Release,
}

impl TryFrom<&libc::input_event> for KeyEvent {
    type Error = KeyloggerError;

    fn try_from(ev: &libc::input_event) -> Result<Self, Self::Error> {
        // The keylogger only supports EV_KEY
        if ev.type_ != EV_KEY as u16 {
            return Err(KeyloggerError::UnsupportedEventType(ev.type_));
        }

        let cause = match ev.value {
            EV_KEY_RELEASE => KeyEventCause::Release,
            EV_KEY_PRESS => KeyEventCause::Press,
            n => {
                return Err(KeyloggerError::InvalidKeyEvent(format!(
                    "invalid value for EV_KEY: {n}"
                )))
            }
        };

        let nsec = (ev.time.tv_usec * 1000)
            .try_into()
            .map_err(|_| KeyloggerError::InvalidTimestamp(ev.time.tv_sec, ev.time.tv_usec))?;
        let ts = NaiveDateTime::from_timestamp(ev.time.tv_sec, nsec);

        Ok(Self {
            ts,
            cause,
            code: KeyCode::try_from(ev.code)?,
        })
    }
}

impl Future for KeyEventFuture {
    type Output = KeyloggerResult<Vec<KeyEvent>>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        loop {
            let mut guard = ready!(self.0.async_fd.poll_read_ready(cx))?;

            match guard.try_io(|inner| device::read_key_events(inner.as_raw_fd())) {
                Ok(result) => return Poll::Ready(result.map_err(Into::into)),
                Err(_) => continue,
            }
        }
    }
}

/// Check whether the specified `flags` indicate the device is a keyboard.
fn has_keyboard_flags(flags: libc::c_ulong) -> bool {
    const KEYBOARD_FLAGS: libc::c_ulong =
        (1 << EV_SYN) | (1 << EV_KEY) | (1 << EV_MSC) | (1 << EV_REP);

    (flags & KEYBOARD_FLAGS) == KEYBOARD_FLAGS
}

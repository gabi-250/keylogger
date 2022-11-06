mod device;
mod event_codes;

use std::convert::TryFrom;
use std::fmt;
use std::future::Future;
use std::os::unix::io::AsRawFd;
use std::path::Path;

use chrono::naive::NaiveDateTime;

use crate::error::KeyloggerError;
use crate::key_code::KeyCode;
use crate::keyboard::event_codes::{EV_KEY, EV_KEY_PRESS, EV_KEY_RELEASE};
use crate::keylogger::KeyloggerResult;

pub(crate) use crate::keyboard::device::find_keyboard_devices;
pub(crate) use crate::keyboard::device::KeyboardDevice;

/// A keyboard device.
pub(crate) type KeyboardBox = Box<dyn KeyEventSource>;

pub(crate) trait KeyEventSource: AsRawFd + fmt::Debug + Send + Sync {
    fn name(&self) -> &str;

    fn path(&self) -> &Path;

    fn key_events(
        &self,
    ) -> Box<dyn Future<Output = KeyloggerResult<Vec<KeyEvent>>> + Send + Sync + Unpin>;
}

/// A key event (EV_KEY).
#[derive(Debug, Eq, PartialEq)]
pub struct KeyEvent {
    /// The timestamp of the event.
    pub ts: NaiveDateTime,
    /// The action that triggered the event.
    pub cause: KeyEventCause,
    /// The key code of the key that triggered the event.
    pub code: KeyCode,
}

/// The reason a `KeyEvent` fired.
#[derive(Debug, Eq, PartialEq)]
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

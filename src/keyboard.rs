pub(crate) mod device;
mod event_codes;

use std::convert::TryFrom;
use std::fmt;
use std::io::Cursor;
use std::marker::Unpin;
use std::path::Path;
use std::pin::Pin;
use std::task::{Context, Poll};

use chrono::naive::NaiveDateTime;
use futures::Stream;
use pin_project::pin_project;

use crate::error::KeyloggerError;
use crate::key_code::KeyCode;
use crate::KeyloggerResult;
use device::InputDevice;
use event_codes::{EV_KEY, EV_KEY_PRESS, EV_KEY_RELEASE};

pub use crate::keyboard::device::find_keyboards;

type KeyEventResult = KeyloggerResult<Vec<KeyEvent>>;

pub struct KeyboardDevice(Keyboard<InputDevice>);

impl KeyboardDevice {
    /// A human-readable description of the keyboard (e.g. "USB-HID Keyboard").
    pub fn name(&self) -> &str {
        self.0.inner.name()
    }

    /// The path of the device (e.g. `/dev/input/event4`)
    pub fn path(&self) -> &Path {
        self.0.inner.path()
    }
}

impl Stream for KeyboardDevice {
    type Item = KeyloggerResult<KeyEvent>;

    fn poll_next(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        Pin::new(&mut self.get_mut().0).poll_next(cx)
    }
}

/// A generic keyboard device.
#[pin_project]
pub(crate) struct Keyboard<K: KeyEventSource> {
    #[pin]
    pub(crate) inner: K,
    pub(crate) buffered_evs: Cursor<Vec<KeyEvent>>,
}

impl<K: KeyEventSource> Keyboard<K> {
    fn new(inner: K) -> Self {
        Self {
            inner,
            buffered_evs: Default::default(),
        }
    }
}

impl<K: KeyEventSource> Stream for Keyboard<K> {
    type Item = KeyloggerResult<KeyEvent>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.project();
        let current_pos = this.buffered_evs.position();
        let len = this.buffered_evs.get_ref().len() as u64;

        if current_pos >= len {
            let inner_pin = Pin::new(this.inner.get_mut());
            let evs = match KeyEventSource::poll_next(inner_pin, cx) {
                Poll::Pending => return Poll::Pending,
                Poll::Ready(Ok(evs)) if evs.is_empty() => return Poll::Pending,
                Poll::Ready(Ok(evs)) => evs,
                Poll::Ready(Err(e)) => return Poll::Ready(Some(Err(e))),
            };

            *this.buffered_evs = Cursor::new(evs);
            this.buffered_evs.set_position(0);
        }

        let pos = this.buffered_evs.position();
        let ev = this.buffered_evs.get_ref()[pos as usize];
        this.buffered_evs.set_position(pos + 1);

        Poll::Ready(Some(Ok(ev)))
    }
}

pub(crate) trait KeyEventSource: fmt::Debug + Unpin + Send + Sync {
    /// A human-readable description of the event source (e.g. "USB-HID Keyboard").
    fn name(&self) -> &str;

    /// The path of the device (e.g. `/dev/input/event4`)
    fn path(&self) -> &Path;

    /// Poll the event source.
    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<KeyEventResult>;
}

/// A key event (EV_KEY).
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct KeyEvent {
    /// The timestamp of the event.
    pub ts: NaiveDateTime,
    /// The action that triggered the event.
    pub cause: KeyEventCause,
    /// The key code of the key that triggered the event.
    pub code: KeyCode,
}

/// The reason a `KeyEvent` fired.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
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

        let ts = NaiveDateTime::from_timestamp_opt(ev.time.tv_sec, nsec).ok_or(
            KeyloggerError::InvalidTimestamp(ev.time.tv_sec, ev.time.tv_usec),
        )?;

        Ok(Self {
            ts,
            cause,
            code: KeyCode::try_from(ev.code)?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::key_code::KeyCode;
    use crate::keyboard::{KeyEventCause, KeyEventSource};
    use futures::StreamExt;
    use std::io::Cursor;
    use std::os::unix::io::{AsRawFd, RawFd};
    use tokio::sync::mpsc;

    const EV_QUEUE_SIZE: usize = 1;

    type EventStream = Cursor<Vec<KeyEventResult>>;

    impl Clone for KeyloggerError {
        fn clone(&self) -> Self {
            use KeyloggerError::*;

            match self {
                Io(_) => unimplemented!("unexpected error type"),
                NotAKeyboard(e) => NotAKeyboard(e.clone()),
                InvalidKeyEvent(e) => InvalidKeyEvent(e.clone()),
                InvalidKeyCode(e) => InvalidKeyCode(*e),
                InvalidTimestamp(s, ms) => InvalidTimestamp(*s, *ms),
                KeyCodeConversion(e) => KeyCodeConversion(*e),
                UnsupportedEventType(e) => UnsupportedEventType(*e),
                KeyloggerTasksExited => KeyloggerTasksExited,
            }
        }
    }

    impl PartialEq for KeyloggerError {
        fn eq(&self, other: &KeyloggerError) -> bool {
            use KeyloggerError::*;

            match (self, other) {
                (Io(_), _) => unimplemented!("unexpected error type"),
                (NotAKeyboard(e1), NotAKeyboard(e2)) => e1.eq(e2),
                (InvalidKeyEvent(e1), InvalidKeyEvent(e2)) => e1.eq(e2),
                (InvalidKeyCode(e1), InvalidKeyCode(e2)) => e1.eq(e2),
                (InvalidTimestamp(s1, ms1), InvalidTimestamp(s2, ms2)) => s1.eq(s2) && ms1.eq(ms2),
                (KeyCodeConversion(e1), KeyCodeConversion(e2)) => e1.eq(e2),
                (UnsupportedEventType(e1), UnsupportedEventType(e2)) => e1.eq(e2),
                (KeyloggerTasksExited, KeyloggerTasksExited) => true,
                _ => false,
            }
        }
    }

    #[derive(Debug)]
    struct TestEventSource {
        ev_stream: EventStream,
        tx_done: mpsc::Sender<()>,
    }

    impl TestEventSource {
        fn new(ev_stream: Vec<KeyEventResult>, tx_done: mpsc::Sender<()>) -> Self {
            Self {
                ev_stream: Cursor::new(ev_stream),
                tx_done,
            }
        }
    }

    impl AsRawFd for TestEventSource {
        fn as_raw_fd(&self) -> RawFd {
            -1
        }
    }

    impl KeyEventSource for TestEventSource {
        fn name(&self) -> &str {
            "test keeb"
        }

        fn path(&self) -> &Path {
            Path::new("/test/keeb")
        }

        fn poll_next(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<KeyEventResult> {
            let this = self.get_mut();
            let ev_stream = &mut this.ev_stream;
            let pos = ev_stream.position();
            let eos = pos == ev_stream.get_ref().len() as u64;

            if !eos {
                ev_stream.set_position(pos + 1);

                Poll::Ready(ev_stream.get_ref()[pos as usize].clone())
            } else {
                // We've run out of test events
                this.tx_done.try_send(()).unwrap();
                Poll::Pending
            }
        }
    }

    impl KeyEvent {
        fn press(code: KeyCode) -> Self {
            Self {
                ts: Default::default(),
                cause: KeyEventCause::Press,
                code,
            }
        }

        fn release(code: KeyCode) -> Self {
            Self {
                ts: Default::default(),
                cause: KeyEventCause::Release,
                code,
            }
        }
    }

    macro_rules! events {
        [$($ev:tt($key:tt),)*] => {
            Ok(vec![$(KeyEvent::$ev(KeyCode::$key),)*])
        }
    }

    #[tokio::test]
    async fn stream_with_errors() {
        let expected_event_batches = vec![
            events![press(KEY_1), release(KEY_1),],
            events![
                press(KEY_A),
                press(KEY_A),
                press(KEY_A),
                release(KEY_A),
                release(KEY_B),
            ],
            Err(KeyloggerError::InvalidKeyEvent("test event".to_string())),
            events![release(KEY_Z),],
            Err(KeyloggerError::InvalidKeyEvent("test event2".to_string())),
        ];

        let (tx_done, mut rx_done) = mpsc::channel::<()>(EV_QUEUE_SIZE);
        let keyboard = Keyboard::new(TestEventSource::new(
            expected_event_batches.clone(),
            tx_done,
        ));

        let recorded_events = keyboard
            .take_until(rx_done.recv())
            .collect::<Vec<_>>()
            .await;

        let mut expected_events = vec![];

        for event in expected_event_batches {
            match event {
                Ok(evs) => expected_events.extend(evs.into_iter().map(Ok)),
                Err(e) => expected_events.push(Err(e)),
            }
        }

        assert_eq!(recorded_events, expected_events);
    }
}

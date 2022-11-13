use std::path::Path;
use std::sync::Arc;

use async_trait::async_trait;
use futures::future::join_all;

use crate::error::KeyloggerError;
use crate::keyboard::KeyboardDevice;
use crate::keyboard::{find_keyboard_devices, KeyEvent, KeyboardBox};

pub(crate) type KeyloggerResult<T> = Result<T, KeyloggerError>;

/// Handle keystroke events.
///
/// # Notes
///
/// The [`Keylogger`](crate::Keylogger) spawns a separate task for each watched keyboard. Each task
/// receives a reference to the [`KeyEventHandler`](crate::KeyEventHandler) provided, so if
/// `handle_events` and `handle_err` need to block on some condition, implementors must ensure
/// these methods only block for the keyboard task the condition pertains to rather than _all_
/// tasks.
#[async_trait]
pub trait KeyEventHandler: Send + Sync {
    /// Receive some [`KeyEvent`s](crate::KeyEvent) for processing.
    async fn handle_events(&self, kb_device: &Path, kb_name: &str, ev: &[KeyEvent]);

    /// Handle an error that occurred while trying to capture keystrokes.
    ///
    /// This enables implementors to choose how capture errors are handled:
    /// * returning an `Err` aborts the capture task of the keyboard device that encountered the
    ///   error.
    /// * returning `Ok(())` causes the keylogger to ignore the error and try reading more key
    ///   events.
    async fn handle_err(
        &self,
        _kb_device: &Path,
        _kb_name: &str,
        _err: KeyloggerError,
    ) -> Result<(), KeyloggerError> {
        // Ignore the error and keep on logging
        Ok(())
    }
}

/// A keylogger than can detect keyboards and watch for keystroke events.
pub struct Keylogger {
    /// The keystroke handler.
    ev_handler: Arc<dyn KeyEventHandler>,
    /// The keyboard devices being watched.
    keyboards: Vec<KeyboardBox>,
}

impl Keylogger {
    /// Create a new `Keylogger`, auto-detecting the keyboard devices to monitor.
    ///
    /// This function returns an error if no keyboard devices are detected.
    pub fn new(ev_handler: impl KeyEventHandler + 'static) -> KeyloggerResult<Self> {
        let keyboards = find_keyboard_devices()?.collect::<Vec<_>>();

        if keyboards.is_empty() {
            return Err(KeyloggerError::NoDevicesFound);
        }

        Ok(Self {
            ev_handler: Arc::new(ev_handler),
            keyboards,
        })
    }

    /// Create a new `Keylogger`, monitoring the specified keyboard devices.
    ///
    /// Out of the specified `devices`, only those that appear to be keyboards will be monitored.
    /// If none of them appear to be keyboards, this function returns a
    /// [`KeyloggerError::NoDevicesFound`](crate::KeyloggerError::NoDevicesFound) error.
    pub fn with_devices<'p, P: AsRef<Path> + 'p>(
        devices: impl Iterator<Item = &'p P>,
        ev_handler: impl KeyEventHandler + 'static,
    ) -> KeyloggerResult<Self> {
        let keyboards = devices
            .filter_map(|d| {
                KeyboardDevice::try_from(d.as_ref())
                    .ok()
                    .map(|d| Box::new(d) as KeyboardBox)
            })
            .collect::<Vec<_>>();

        if keyboards.is_empty() {
            return Err(KeyloggerError::NoDevicesFound);
        }

        Ok(Self {
            ev_handler: Arc::new(ev_handler),
            keyboards,
        })
    }

    /// Begin capturing key events.
    ///
    /// This spawns a separate task for each watched keyboard.
    ///
    /// # Notes
    ///
    /// This method blocks until **all** capture tasks complete (i.e. by returning an error).
    pub async fn capture(self) -> KeyloggerResult<()> {
        let handles = self
            .keyboards
            .into_iter()
            .map(|kb| {
                let ev_handler = Arc::clone(&self.ev_handler);

                tokio::spawn(Self::handle_key_events(ev_handler, kb))
            })
            .collect::<Vec<_>>();

        // Wait for the tasks to exit and discard the result
        let _ = join_all(handles).await;

        Err(KeyloggerError::KeyloggerTasksExited)
    }

    async fn handle_key_events(
        ev_handler: Arc<dyn KeyEventHandler>,
        keyboard: KeyboardBox,
    ) -> KeyloggerResult<()> {
        let keyboard = Arc::new(keyboard);

        loop {
            let events = match keyboard.key_events().await {
                Ok(events) => events,
                Err(e) => {
                    ev_handler
                        .handle_err(keyboard.path(), keyboard.name(), e)
                        .await?;

                    continue;
                }
            };

            if events.is_empty() {
                continue;
            }

            ev_handler
                .handle_events(keyboard.path(), keyboard.name(), &events)
                .await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::key_code::KeyCode;
    use crate::keyboard::device::KeyEventResult;
    use crate::keyboard::{KeyEventCause, KeyEventSource};
    use std::io::Cursor;
    use std::iter;
    use std::os::unix::io::{AsRawFd, RawFd};
    use tokio::sync::{mpsc, RwLock};

    type EventStream = Arc<RwLock<Cursor<Vec<KeyEventResult>>>>;

    const EV_QUEUE_SIZE: usize = 1;

    impl Clone for KeyloggerError {
        fn clone(&self) -> Self {
            use KeyloggerError::*;

            match self {
                Io(_) => unimplemented!("unexpected error type"),
                NoDevicesFound => NoDevicesFound,
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
                (NoDevicesFound, NoDevicesFound) => true,
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

    #[derive(Debug, Clone)]
    struct TestEventSource(EventStream);

    impl AsRawFd for TestEventSource {
        fn as_raw_fd(&self) -> RawFd {
            -1
        }
    }

    #[async_trait::async_trait]
    impl KeyEventSource for TestEventSource {
        fn name(&self) -> &str {
            "test keeb"
        }

        fn path(&self) -> &Path {
            Path::new("/test/keeb")
        }

        async fn key_events(&self) -> KeyEventResult {
            let mut lock = self.0.write().await;
            let pos = lock.position();
            let eos = pos == lock.get_ref().len() as u64;

            if !eos {
                lock.set_position(pos + 1);

                lock.get_ref()[pos as usize].clone()
            } else {
                // We've run out of test events
                futures::future::pending::<KeyEventResult>().await
            }
        }
    }

    struct TestEventHandler {
        expected_events: EventStream,
        tx_done: mpsc::Sender<()>,
    }

    macro_rules! current_events {
        ($events:expr) => {{
            let pos = $events.position() - 1;
            let is_last = pos == $events.get_ref().len() as u64 - 1;

            ($events.get_ref().get(pos as usize).unwrap(), is_last)
        }};
    }

    #[async_trait]
    impl KeyEventHandler for TestEventHandler {
        async fn handle_events(&self, _: &Path, _: &str, ev: &[KeyEvent]) {
            let lock = self.expected_events.read().await;
            let (events, is_last) = current_events!(lock);

            match events {
                Ok(events) => assert_eq!(ev, events),
                Err(_) => panic!("expected failure, got {:?})", ev),
            }

            if is_last {
                self.tx_done.send(()).await.unwrap();
            }
        }

        async fn handle_err(
            &self,
            _kb_device: &Path,
            _kb_name: &str,
            err: KeyloggerError,
        ) -> Result<(), KeyloggerError> {
            let lock = self.expected_events.read().await;
            let (events, is_last) = current_events!(lock);

            match events {
                Ok(_) => panic!("expected success, got {:?})", err),
                Err(expected_err) => assert_eq!(&err, expected_err),
            }

            if is_last {
                self.tx_done.send(()).await.unwrap();
            }

            Ok(())
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

    fn spawn_keylogger<K: KeyEventSource + 'static>(
        keyboards: impl Iterator<Item = K>,
        ev_handler: impl KeyEventHandler + 'static,
    ) {
        let keyboards = keyboards.map(|k| Box::new(k) as KeyboardBox).collect();

        let keylogger = Keylogger {
            ev_handler: Arc::new(ev_handler),
            keyboards,
        };

        tokio::spawn(keylogger.capture());
    }

    macro_rules! events {
        [$($ev:tt($key:tt),)*] => {
            Ok(vec![$(KeyEvent::$ev(KeyCode::$key),)*])
        }
    }

    #[tokio::test]
    async fn call_event_handler() {
        let expected_events = vec![
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

        let expected_events = Arc::new(RwLock::new(Cursor::new(expected_events)));
        let ev_src = TestEventSource(Arc::clone(&expected_events));
        let ev_handler = TestEventHandler {
            expected_events,
            tx_done,
        };

        spawn_keylogger(iter::once(ev_src), ev_handler);
        rx_done.recv().await.unwrap();
    }
}

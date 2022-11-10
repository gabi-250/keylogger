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
    fn handle_err(
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
                    ev_handler.handle_err(keyboard.path(), keyboard.name(), e)?;
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
    use crate::keyboard::{KeyEventCause, KeyEventSource};
    use std::future::Future;
    use std::io::Cursor;
    use std::iter;
    use std::os::unix::io::{AsRawFd, RawFd};
    use std::sync::RwLock;
    use tokio::sync::mpsc;

    type EventStream = Arc<RwLock<Cursor<Vec<Vec<KeyEvent>>>>>;

    #[derive(Debug, Clone)]
    struct TestEventSource(EventStream);

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

        fn key_events(
            &self,
        ) -> Box<dyn Future<Output = KeyloggerResult<Vec<KeyEvent>>> + Send + Sync + Unpin>
        {
            let res = advance_ev_stream(&self.0).0;

            Box::new(Box::pin(async { Ok(res) }))
        }
    }

    enum ExpectedOutcome {
        Ok(EventStream),
        Err(KeyloggerError),
    }

    struct TestEventHandler {
        outcome: ExpectedOutcome,
        tx_done: mpsc::Sender<()>,
    }

    fn make_ev_stream(ev: Vec<Vec<KeyEvent>>) -> EventStream {
        Arc::new(RwLock::new(Cursor::new(ev)))
    }

    fn advance_ev_stream(stream: &EventStream) -> (Vec<KeyEvent>, bool) {
        let mut lock = stream.write().unwrap();
        let pos = lock.position();
        let res = lock.get_ref()[pos as usize].clone();
        let is_last = pos == lock.get_ref().len() as u64 - 1;

        if !is_last {
            lock.set_position(pos + 1);
        }

        (res, is_last)
    }

    #[async_trait]
    impl KeyEventHandler for TestEventHandler {
        async fn handle_events(&self, _: &Path, _: &str, ev: &[KeyEvent]) {
            match &self.outcome {
                ExpectedOutcome::Ok(expected_events) => {
                    let (events, last) = advance_ev_stream(expected_events);

                    assert_eq!(ev, events);

                    if last {
                        self.tx_done.send(()).await.unwrap();
                    }
                }
                ExpectedOutcome::Err(_) => {
                    panic!("expected failure, got {:?})", ev);
                }
            }
        }

        fn handle_err(
            &self,
            _kb_device: &Path,
            _kb_name: &str,
            err: KeyloggerError,
        ) -> Result<(), KeyloggerError> {
            match &self.outcome {
                ExpectedOutcome::Ok(_) => {
                    panic!("expected success, got {:?})", err);
                }
                ExpectedOutcome::Err(_expected_err) => Err(err),
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

    macro_rules! event_stream {
        [$([$($ev:tt($key:tt),)*],)*] => {
            vec![
                $(vec![$(KeyEvent::$ev(KeyCode::$key),)*],)*
            ]
        }
    }

    #[tokio::test]
    async fn call_event_handler() {
        const EV_QUEUE_SIZE: usize = 1;

        let expected_events = event_stream![
            [press(KEY_1), release(KEY_1),],
            [
                press(KEY_A),
                press(KEY_A),
                press(KEY_A),
                release(KEY_A),
                release(KEY_B),
            ],
            [release(KEY_Z),],
        ];

        let (tx_done, mut rx_done) = mpsc::channel::<()>(EV_QUEUE_SIZE);

        let outcome = ExpectedOutcome::Ok(make_ev_stream(expected_events.clone()));

        let ev_src = TestEventSource(make_ev_stream(expected_events));
        let ev_handler = TestEventHandler { outcome, tx_done };
        spawn_keylogger(iter::once(ev_src), ev_handler);

        rx_done.recv().await.unwrap();
    }
}

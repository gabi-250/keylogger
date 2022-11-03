use async_trait::async_trait;
use futures::future::join_all;
use std::path::Path;
use std::sync::Arc;

use crate::error::KeyloggerError;
use crate::keyboard::{find_keyboard_devices, KeyEvent, Keyboard};

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
    async fn handle_events(&self, kb_device: &Path, kb_name: &str, ev: Vec<KeyEvent>);

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
    keyboards: Vec<Keyboard>,
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
    pub fn with_devices<'a>(
        devices: impl Iterator<Item = &'a Path>,
        ev_handler: impl KeyEventHandler + 'static,
    ) -> KeyloggerResult<Self> {
        let keyboards = devices
            .filter_map(|entry| Keyboard::try_from(entry).ok())
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
        keyboard: Keyboard,
    ) -> KeyloggerResult<()> {
        let keyboard = Arc::new(keyboard);

        loop {
            let events = match keyboard.read_key_events().await {
                Ok(events) => events,
                Err(e) => {
                    ev_handler.handle_err(&keyboard.device, &keyboard.name, e)?;
                    continue;
                }
            };

            if events.is_empty() {
                continue;
            }

            ev_handler
                .handle_events(&keyboard.device, &keyboard.name, events)
                .await;
        }
    }
}

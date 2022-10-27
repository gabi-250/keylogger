use async_trait::async_trait;
use keylogger::{KeyEvent, KeyEventHandler, Keylogger, KeyloggerError};

use std::path::Path;

struct Beeper;

#[async_trait]
impl KeyEventHandler for Beeper {
    async fn handle_events(&self, kb_device: &Path, kb_name: &str, events: Vec<KeyEvent>) {
        println!("[{} @ {}]: ev={:?}", kb_name, kb_device.display(), events);
    }
}

#[tokio::main]
async fn main() -> Result<(), KeyloggerError> {
    let keylogger = Keylogger::new(Beeper)?;
    keylogger.start().await?;

    Ok(())
}

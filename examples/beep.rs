use async_trait::async_trait;
use keylogger::{KeyEvent, KeyEventHandler, Keylogger};
use std::io;
use std::path::Path;

struct Beeper;

#[async_trait]
impl KeyEventHandler for Beeper {
    async fn handle_ev(&self, kb_device: &Path, kb_name: &str, ev: Vec<KeyEvent>) {
        println!("[{} @ {}]: ev={:?}", kb_name, kb_device.display(), ev);
    }
}

#[tokio::main]
async fn main() -> io::Result<()> {
    let keylogger = Keylogger::new(Beeper)?;
    keylogger.spawn_loggers()?;

    loop {}

    Ok(())
}

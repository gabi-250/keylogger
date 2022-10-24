use keylogger::{KeyEventHandler, Keylogger};
use std::io;
use std::path::Path;

struct Beeper;

impl KeyEventHandler for Beeper {
    fn handle_ev(&self, kb_device: &Path, key: u32) {
        println!("KB={} key={}", kb_device.display(), key);
    }
}

#[tokio::main]
async fn main() -> io::Result<()> {
    let keylogger = Keylogger::new(Beeper)?;
    keylogger.spawn_loggers()?;

    loop {}

    Ok(())
}

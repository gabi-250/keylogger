# Keylogger

This crate provides the necessary scaffolding for handling keyboard input events on Linux.

The keystrokes are captured by the `Keylogger`, which needs to be initialized
with a `KeyEventHandler`. The `KeyEventHandler` receives the captured
`KeyEvent`s and must decide how to handle them.

# Example

A simple example that prints the captured keystrokes to stdout. Note the
keylogger needs to run with root privileges.

 ```rust
 use async_trait::async_trait;
 use keylogger::{KeyEvent, KeyEventCause, KeyEventHandler, Keylogger, KeyloggerError};
 use std::path::Path;

 struct Logger;

 #[async_trait]
 impl KeyEventHandler for Logger {
     async fn handle_events(&self, kb_device: &Path, kb_name: &str, events: &[KeyEvent]) {
         println!("[{} @ {}]: ev={:?}", kb_name, kb_device.display(), events);
     }
 }

 #[tokio::main]
 async fn main() -> Result<(), KeyloggerError> {
     let keylogger = Keylogger::new(Logger)?;
     keylogger.capture().await?;

     Ok(())
 }
 ```

# Disclaimer

This is intended for educational purposes only. Developers assume no liability
and are not responsible for any misuse or damage caused by this program.

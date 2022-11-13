# Keylogger

[![AGPL-3.0 license][agpl-badge]][agpl-url]
[![Build Status][actions-badge]][actions-url]

[agpl-badge]: https://img.shields.io/badge/license-AGPL-purple.svg
[agpl-url]: https://github.com/gabi-250/keylogger/blob/master/LICENSE
[actions-badge]: https://github.com/gabi-250/keylogger/actions/workflows/test.yaml/badge.svg
[actions-url]: https://github.com/gabi-250/keylogger/actions/workflows/test.yaml?query=branch%3Amaster+workflow%3A%22Run+tests%22

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

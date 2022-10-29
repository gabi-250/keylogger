//! This crate provides the necessary scaffolding for handling keyboard input events on Linux.
//!
//! For more information, see the [kernel docs].
//!
//! # Example
//!
//! A simple example that prints the captured keystrokes to stdout.
//! ```
//! use async_trait::async_trait;
//! use keylogger::{KeyEvent, KeyEventCause, KeyEventHandler, Keylogger, KeyloggerError};
//!
//! struct Logger;
//!
//! #[async_trait]
//! impl KeyEventHandler for Logger {
//!     async fn handle_events(&self, kb_device: &Path, kb_name: &str, events: Vec<KeyEvent>) {
//!         println!("[{} @ {}]: ev={:?}", kb_name, kb_device.display(), events);
//!     }
//! }
//!
//! #[tokio::main]
//! async fn main() -> Result<(), KeyloggerError> {
//!     let keylogger = Keylogger::new(Logger)?;
//!     keylogger.capture().await?;
//!
//!     Ok(())
//! }
//!
//! ```
//!
//! [kernel docs]: https://www.kernel.org/doc/html/latest/input/event-codes.html

#[cfg(not(target_os = "linux"))]
compile_error!("This crate only works on Linux");

mod error;
pub(crate) mod key_code;
mod keyboard;
mod keylogger;

pub use error::KeyloggerError;
pub use keyboard::{KeyEvent, KeyEventCause};
pub use keylogger::{KeyEventHandler, Keylogger};

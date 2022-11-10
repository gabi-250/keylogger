//! This crate provides the necessary scaffolding for handling keyboard input events on Linux.
//!
//! The keystrokes are captured by the [`Keylogger`](crate::Keylogger), which needs to be
//! initialized with a [`KeyEventHandler`](crate::KeyEventHandler). The
//! [`KeyEventHandler`](crate::KeyEventHandler) receives the captured [`KeyEvent`s](crate::KeyEvent)
//! and must decide how to handle them.
//!
//! # Example
//!
//! A simple example that prints the captured keystrokes to stdout. Note the keylogger needs to run
//! with root privileges.
//!
//! ```no_run
//! use async_trait::async_trait;
//! use keylogger::{KeyEvent, KeyEventCause, KeyEventHandler, Keylogger, KeyloggerError};
//! use std::path::Path;
//!
//! struct Logger;
//!
//! #[async_trait]
//! impl KeyEventHandler for Logger {
//!     async fn handle_events(&self, kb_device: &Path, kb_name: &str, events: &[KeyEvent]) {
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

#[cfg(not(target_os = "linux"))]
compile_error!("This crate only works on Linux");

mod error;
pub(crate) mod key_code;
mod keyboard;
mod keylogger;

pub use crate::keylogger::{KeyEventHandler, Keylogger};
pub use error::KeyloggerError;
pub use key_code::KeyCode;
pub use keyboard::{KeyEvent, KeyEventCause};

//! This crate provides the necessary scaffolding for handling keyboard input events on Linux.
//!
//! The installed [`KeyboardDevice`]s can be detected using [`find_keyboards`]. [`KeyboardDevice`]
//! implements [`Stream`], where each element is a [`KeyEvent`].
//!
//! # Example
//!
//! A simple example that prints the captured keystrokes to stdout. Note the keylogger needs to run
//! with root privileges.
//!
//! ```no_run
//! use futures::{future, StreamExt};
//! use keylogger::{find_keyboards, KeyloggerError};
//!
//! #[tokio::main]
//! async fn main() -> Result<(), KeyloggerError> {
//!     let keyboards = find_keyboards()?.into_iter().map(|mut k| async move {
//!         while let Some(events) = k.next().await {
//!             println!("[{} @ {}]: ev={events:?}", k.name(), k.path().display());
//!         }
//!     });
//!
//!     future::join_all(keyboards).await;
//!
//!     Ok(())
//! }
//! ```

#[cfg(not(target_os = "linux"))]
compile_error!("This crate only works on Linux");

mod error;
pub(crate) mod key_code;
mod keyboard;

pub use error::KeyloggerError;
pub use key_code::KeyCode;
pub use keyboard::{find_keyboards, KeyEvent, KeyEventCause, KeyboardDevice};

pub type KeyloggerResult<T> = Result<T, KeyloggerError>;

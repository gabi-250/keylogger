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
pub use keyboard::{KeyEvent, KeyEventCause};

use std::path::Path;
use std::pin::Pin;

use keyboard::find_keyboard_devices;
use keyboard::KeyEventSource;

use futures::Stream;

pub type KeyloggerResult<T> = Result<T, KeyloggerError>;

/// Auto-detect the keyboard devices to watch.
pub fn find_keyboards() -> KeyloggerResult<Vec<KeyboardDevice>> {
    let keyboards = find_keyboard_devices()?.collect::<Vec<_>>();

    Ok(keyboards)
}

pub struct KeyboardDevice(keyboard::Keyboard<keyboard::device::InputDevice>);

impl KeyboardDevice {
    pub fn name(&self) -> &str {
        self.0.inner.name()
    }

    pub fn path(&self) -> &Path {
        self.0.inner.path()
    }
}

impl Stream for KeyboardDevice {
    type Item = KeyloggerResult<KeyEvent>;

    fn poll_next(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        Pin::new(&mut self.get_mut().0).poll_next(cx)
    }
}

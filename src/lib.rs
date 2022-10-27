#[cfg(not(target_os = "linux"))]
compile_error!("This crate only works on Linux");

mod device;
mod error;
mod keylogger;
pub(crate) mod keys;

pub use device::{KeyEvent, KeyEventType};
pub use error::KeyloggerError;
pub use keylogger::{KeyEventHandler, Keylogger};

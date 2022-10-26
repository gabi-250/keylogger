#[cfg(not(target_os = "linux"))]
compile_error!("This crate only works on Linux");

mod device;
mod keylogger;
pub(crate) mod keys;

pub use device::KeyEvent;
pub use keylogger::{KeyEventHandler, Keylogger};

use std::io;
use std::path::PathBuf;

use thiserror::Error;

use crate::key_code::KeyCode;

/// Errors encountered by the keylogger.
#[derive(Error, Debug)]
pub enum KeyloggerError {
    #[error("I/O error: {0}")]
    Io(#[from] io::Error),
    #[error("no keyboard devices found")]
    NoDevicesFound,
    #[error("not a keyboard device: {0}")]
    NotAKeyboard(PathBuf),
    #[error("invalid EV_KEY event: {0}")]
    InvalidKeyEvent(String),
    #[error("invalid key code: {0}")]
    InvalidKeyCode(u16),
    #[error("invalid timestamp: sec={0} usec={1}")]
    InvalidTimestamp(i64, i64),
    #[error("failed to convert key code: {0:?}")]
    KeyCodeConversion(KeyCode),
    #[error("unsuported event type: {0}")]
    UnsupportedEventType(u16),
    #[error("all logging tasks exited")]
    KeyloggerTasksExited,
}

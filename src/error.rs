use std::io;
use thiserror::Error;

use crate::key_code::KeyCode;

#[derive(Error, Debug)]
pub enum KeyloggerError {
    #[error("I/O error: {0}")]
    Io(#[from] io::Error),
    #[error("no keyboard devices found")]
    NoDevicesFound,
    #[error("invalid event: {0}")]
    InvalidEvent(String),
    #[error("invalid key code: {0}")]
    InvalidKeyCode(u16),
    #[error("failed to convert key code: {0:?}")]
    KeyCodeConversion(KeyCode),
    #[error("unsuported event type: {0}")]
    UnsupportedEventType(u16),
    #[error("all logging tasks exited")]
    KeyloggerTasksExited,
}

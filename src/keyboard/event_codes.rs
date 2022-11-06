// Some interesting Event types (see [input-event-codes.h] and the [kernel docs]).
//
// [input-event-codes.h]: https://elixir.bootlin.com/linux/v5.19.17/source/include/uapi/linux/input-event-codes.h#L38)
// [kernel docs]: https://www.kernel.org/doc/html/latest/input/event-codes.html
pub(crate) const EV_SYN: libc::c_ulong = 0x00;
pub(crate) const EV_KEY: libc::c_ulong = 0x01;
pub(crate) const EV_MSC: libc::c_ulong = 0x04;
pub(crate) const EV_REP: libc::c_ulong = 0x14;

/// The `value` of an EV_KEY caused by a key being released.
pub(crate) const EV_KEY_RELEASE: i32 = 0;
/// The `value` of an EV_KEY caused by a key press.
pub(crate) const EV_KEY_PRESS: i32 = 1;

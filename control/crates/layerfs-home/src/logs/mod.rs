// The call log (#659): one line per call against this home. The face is
// shared; the implementation is Unix file logging, with a Windows stub
// that keeps every read refused as Unsupported until the port (the
// per-call file semantics are the gap, disclosed).
#[cfg(unix)]
mod unix;
#[cfg(windows)]
mod windows;

#[cfg(unix)]
pub use unix::{append, read, reset, Filter, ROTATE_AT};
#[cfg(unix)]
pub(crate) use unix::rotate;
#[cfg(windows)]
pub use windows::{append, read, reset, Filter, ROTATE_AT};
#[cfg(windows)]
pub(crate) use windows::rotate;

/// Whether the call log is on for this process.
pub fn enabled() -> bool {
    #[cfg(unix)]
    { unix::enabled() }
    #[cfg(windows)]
    { false }
}

/// Normalizes a log tag the way the call log stores it.
pub fn tag(tag: &str) -> Option<String> {
    #[cfg(unix)]
    { unix::tag(tag) }
    #[cfg(windows)]
    { let _ = tag; None }
}

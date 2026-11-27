//! Scoped absolute diagnostic budget, leaving ordinary transfer policy unchanged.
use std::cell::Cell;
use std::io;
use std::time::{Duration, Instant};
thread_local! { static DEADLINE: Cell<Option<Instant>> = const { Cell::new(None) }; }

/// Runs an observational request within one absolute budget on this thread.
pub fn within<T>(deadline: Instant, body: impl FnOnce() -> T) -> T {
    struct Reset(Option<Instant>);
    impl Drop for Reset {
        fn drop(&mut self) {
            DEADLINE.set(self.0);
        }
    }
    let _reset = Reset(DEADLINE.replace(Some(deadline)));
    body()
}
pub(crate) fn current() -> Option<Instant> {
    DEADLINE.get()
}
pub(crate) fn remaining(deadline: Option<Instant>, maximum: Duration) -> io::Result<Duration> {
    let remaining = deadline.map_or(maximum, |until| {
        until.saturating_duration_since(Instant::now()).min(maximum)
    });
    if remaining.is_zero() {
        Err(io::Error::new(
            io::ErrorKind::TimedOut,
            "diagnostic deadline exceeded",
        ))
    } else {
        Ok(remaining)
    }
}

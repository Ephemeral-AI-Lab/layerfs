//! The monotonic clock (#548 B5): elapsed time since this process started.
//!
//! Last activity and lease refreshes use this clock, never the wall clock,
//! so a clock correction cannot move a deadline or make an active Sandbox
//! look idle. A value is meaningful within one process; records keep it as
//! the evidence a reclaimer in the same process reads.

use std::sync::LazyLock;
use std::time::Instant;

/// The origin every monotonic value in this process counts from.
static ORIGIN: LazyLock<Instant> = LazyLock::new(Instant::now);

/// Milliseconds since this process started, never decreasing.
pub fn monotonic_ms() -> i64 {
    i64::try_from(ORIGIN.elapsed().as_millis()).unwrap_or(i64::MAX)
}

//! The launcher port the step runtime drives (#555): start one command,
//! observe a started one, cancel it. The port keeps the contract free of
//! daemon, Sandbox and process detail, so the concrete adapter stays thin
//! and the runtime can be exercised with a local launcher in tests.
use crate::step::contract::{ExecId, ExecRequest};
use layerfs_bridge::control::{SdkRefusal, SdkValue};

/// What a launched command looks like now.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Observation {
    /// It is still running.
    Running,
    /// It ended with this exit code.
    Ended(i32),
    /// It is gone and no exit code was recorded.
    Gone,
}

/// Starts, observes and cancels one command.
///
/// The returned `SdkValue` is the durable handle: the runtime records it in
/// the wait store and passes it back to `observe` and `cancel`, so a later
/// launcher instance that reads the entry can act on the same command.
/// `start` is called once per claimed idempotency key; the runtime never
/// retries it.
pub trait Launcher {
    /// Starts `request` as the exec `id` and returns its handle.
    fn start(&self, id: &ExecId, request: &ExecRequest) -> Result<SdkValue, SdkRefusal>;

    /// What the command behind `handle` looks like now.
    fn observe(&self, handle: &SdkValue) -> Observation;

    /// Asks the command behind `handle` to stop. The runtime records the
    /// cancellation before it calls this, so a failure here does not change
    /// the recorded outcome.
    fn cancel(&self, handle: &SdkValue) -> Result<(), SdkRefusal>;
}

/// The wall clock, in milliseconds since the Unix epoch.
pub trait Clock {
    /// Now.
    fn now_ms(&self) -> i64;
}

/// The system clock.
#[derive(Clone, Copy, Debug, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now_ms(&self) -> i64 {
        crate::common::home::now_ms()
    }
}

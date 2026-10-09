//! Fixed startup resources and exact terminal request/worker dispositions.
use std::{any::Any, error::Error, fmt, future::Future, io, pin::Pin};

pub const HANDOFFS: usize = 16;
/// Receive loops of one mount, and so the receive units it can hold.
pub const RECEIVE_SLOTS: usize = 1;
/// Largest copied data window plus two maximum length namespace components.
pub const MAX_INPUT_BYTES: usize = layerfs_overlay::WRITE_WINDOW + 2 * 255;

#[derive(Clone, Copy, Debug)]
pub struct DispatchConfig {
    pub read_handles: usize,
    pub namespaces: usize,
}
impl DispatchConfig {
    pub fn workers(self) -> Option<usize> {
        (self.read_handles != 0 && self.namespaces != 0)
            .then(|| self.read_handles.checked_add(2))
            .flatten()
    }
}
/// Completion means original reply/result/source consumers are disposed. A
/// retained error must itself keep the operation's original inputs and custody.
pub enum RequestDisposition {
    Complete,
    Retained(Box<dyn Error + Send + Sync>),
}
pub type RequestFuture = Pin<Box<dyn Future<Output = RequestDisposition> + Send>>;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DispatchError {
    InvalidConfig,
    Capacity,
    DuplicateMount,
    Stale,
    Stopped,
    Busy,
    Failed,
}
impl fmt::Display for DispatchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "native dispatch: {self:?}")
    }
}
impl Error for DispatchError {}

/// Every created worker is joined once; panic payloads remain original values.
pub struct Shutdown {
    pub joins: Vec<std::thread::Result<()>>,
    pub retained_requests: usize,
    pub failed: bool,
}
impl Shutdown {
    pub fn clean(&self) -> bool {
        !self.failed && self.retained_requests == 0 && self.joins.iter().all(Result::is_ok)
    }
}
impl fmt::Debug for Shutdown {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Shutdown")
            .field("joined", &self.joins.len())
            .field(
                "panicked",
                &self.joins.iter().filter(|v| v.is_err()).count(),
            )
            .field("retained_requests", &self.retained_requests)
            .field("failed", &self.failed)
            .finish()
    }
}
pub struct StartFailure {
    pub reason: DispatchError,
    pub original: Option<io::Error>,
    pub shutdown: Shutdown,
}
impl fmt::Debug for StartFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("StartFailure")
            .field("reason", &self.reason)
            .field("original", &self.original)
            .field("shutdown", &self.shutdown)
            .finish()
    }
}
impl fmt::Display for StartFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.reason.fmt(f)
    }
}
impl Error for StartFailure {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        self.original.as_ref().map(|error| error as _)
    }
}

pub(crate) enum Failure {
    Request(Box<dyn Error + Send + Sync>),
    Panic(Box<dyn Any + Send>),
}
pub enum FailureView<'a> {
    Request(&'a (dyn Error + Send + Sync + 'static)),
    Panic(&'a (dyn Any + Send)),
}

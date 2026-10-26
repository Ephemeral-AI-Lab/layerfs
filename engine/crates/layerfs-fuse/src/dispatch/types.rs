//! Fixed startup resources and exact terminal request/worker dispositions.
use std::{any::Any, error::Error, fmt, future::Future, pin::Pin};

/// Plain receive workers per Workspace. Two lets one request proceed while
/// another waits on a Store read; a second worker also contends for the one
/// Workspace lock, so more workers slow writers of the same Workspace.
pub const RECEIVE_SLOTS: usize = 2;
pub const MAX_INPUT_BYTES: usize = layerfs_overlay::WRITE_WINDOW + 2 * 255;

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
        write!(f, "kernel dispatch: {self:?}")
    }
}
impl Error for DispatchError {}

pub enum FailureView<'a> {
    Request(&'a (dyn Error + Send + Sync + 'static)),
    Panic(&'a (dyn Any + Send)),
}

//! Exact deciding failures, never followed by an automatic resend or guessed cancellation.
use std::{fmt, io};
/// Original ordinary-runtime refusal or I/O cause.
#[derive(Debug)]
pub enum RuntimeError {
    Io(io::Error),
    Protocol(&'static str),
    /// Explicit unsupported runtime capability, refused before effects.
    Unsupported(&'static str),
}
impl From<io::Error> for RuntimeError {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}
impl fmt::Display for RuntimeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(e) => write!(f, "runtime I/O: {e}"),
            Self::Protocol(e) => write!(f, "runtime protocol: {e}"),
            Self::Unsupported(c) => write!(f, "unsupported runtime capability: {c}"),
        }
    }
}
impl std::error::Error for RuntimeError {}
/// Exact original request transfer knowledge, independent of product effect inference.
#[derive(Debug)]
pub struct WireFailure {
    pub attempted: bool,
    pub sent_bytes: u64,
    /// Exact encoded HTTP header length; body transfer is counted separately.
    pub request_header_bytes: u64,
    /// Actual positive socket body transfer, excluding headers and buffered input.
    pub sent_body_bytes: u64,
    /// Original undelivered fixed request window; Debug redacts private payload.
    pub pending_request: super::PendingRequest,
    pub status: Option<u16>,
    pub cause: RuntimeError,
    pub fence_error: Option<io::Error>,
    /// Actual decoded container selector, distinct from the requested identity.
    pub observed_container: Option<super::ContainerId>,
    /// Original request selectors, never described as decoded response facts.
    pub requested_container: Option<super::ContainerId>,
}
/// The cause [`WireFailure::container_stopped`] reports.
pub(crate) const STOPPED: &str = "exact stopped container observation";
impl WireFailure {
    /// A non-HTTP cause before an attempt, with no invented transport facts.
    pub fn local(cause: RuntimeError) -> Box<Self> {
        Box::new(Self {
            attempted: false,
            sent_bytes: 0,
            request_header_bytes: 0,
            sent_body_bytes: 0,
            pending_request: Default::default(),
            status: None,
            cause,
            fence_error: None,
            observed_container: None,
            requested_container: None,
        })
    }
    /// The exact requested Sandbox is intact but not running: its daemon
    /// exited, so nothing it held survives.
    pub fn container_stopped(&self) -> bool {
        self.requested_container.is_some()
            && self.observed_container == self.requested_container
            && matches!(self.cause, RuntimeError::Protocol(STOPPED))
    }
}

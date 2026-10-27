//! Exact deciding failures, never followed by an automatic resend or guessed cancellation.
use std::{fmt, io};
/// Original ordinary-runtime refusal or I/O cause.
#[derive(Debug)]
pub enum RuntimeError {
    Io(io::Error),
    Protocol(&'static str),
    /// Original HTTP status, with a separately retained bounded error-body diagnostic.
    Http(u16),
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
            Self::Http(s) => write!(f, "runtime HTTP status {s}"),
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
    /// Original decoded identity even when the response does not complete normally.
    pub observed_exec: Option<super::ExecId>,
    /// Actual decoded container selector, distinct from the requested identity.
    pub observed_container: Option<super::ContainerId>,
    /// Original request selectors, never described as decoded response facts.
    pub requested_container: Option<super::ContainerId>,
    pub requested_exec: Option<super::ExecId>,
    /// Original error-body prefix/count/completion, never claimed as full command output.
    pub error_body: Option<ErrorDiagnostic>,
}
/// The cause [`WireFailure::container_stopped`] reports.
pub(crate) const STOPPED: &str = "exact stopped container observation";
impl WireFailure {
    /// The Engine still has the exact requested container, intact, but it is
    /// not running: its daemon exited, so nothing it held survives.
    pub fn container_stopped(&self) -> bool {
        self.requested_container.is_some()
            && self.observed_container == self.requested_container
            && matches!(self.cause, RuntimeError::Protocol(STOPPED))
    }

    /// The Engine answered that the requested container does not exist: a 404
    /// whose error body says so ("No such container"), not any other 404.
    pub fn container_gone(&self) -> bool {
        const GONE: &[u8] = b"No such container";
        self.requested_container.is_some()
            && self.status == Some(404)
            && matches!(self.cause, RuntimeError::Http(404))
            && self
                .error_body
                .as_ref()
                .is_some_and(|body| body.prefix.windows(GONE.len()).any(|w| w == GONE))
    }
}

/// Original bounded HTTP error diagnostic; Debug shows only the retained prefix as lossy UTF-8,
/// with controls escaped, while bytes beyond this window remain explicitly unobserved.
pub struct ErrorDiagnostic {
    pub prefix: Vec<u8>,
    pub observed_bytes: u64,
    pub complete: bool,
    pub read_error: Option<io::Error>,
}
impl fmt::Debug for ErrorDiagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let prefix = String::from_utf8_lossy(&self.prefix);
        f.debug_struct("ErrorDiagnostic")
            .field("prefix", &prefix)
            .field("prefix_bytes", &self.prefix.len())
            .field("observed_bytes", &self.observed_bytes)
            .field("complete", &self.complete)
            .field("read_error", &self.read_error)
            .finish()
    }
}

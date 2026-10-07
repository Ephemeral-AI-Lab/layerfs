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

/// Original bounded HTTP error diagnostic; bytes beyond this window are explicitly unobserved.
pub struct ErrorDiagnostic {
    pub prefix: Vec<u8>,
    pub observed_bytes: u64,
    pub complete: bool,
    pub read_error: Option<io::Error>,
}
impl fmt::Debug for ErrorDiagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ErrorDiagnostic")
            .field("prefix_bytes", &self.prefix.len())
            .field("observed_bytes", &self.observed_bytes)
            .field("complete", &self.complete)
            .field("read_error", &self.read_error)
            .finish()
    }
}

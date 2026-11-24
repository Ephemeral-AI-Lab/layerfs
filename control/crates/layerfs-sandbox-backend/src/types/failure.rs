//! Exact deciding failures, never followed by an automatic resend or guessed cancellation.
use std::{fmt, io};
/// Original ordinary-runtime refusal or I/O cause.
#[derive(Debug)]
pub enum RuntimeError {
    Io(io::Error),
    Protocol(&'static str),
    /// Explicit unsupported runtime capability, refused before effects.
    Unsupported(&'static str),
    /// The machine cannot give a Sandbox what it needs now (its network:
    /// a pool clash, forwarding off, a dropping forward policy, a resolver
    /// no Sandbox reaches); refused before effects, naming the fix.
    Unavailable(String),
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
            Self::Unavailable(why) => f.write_str(why),
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
/// What a failure whose request was sent says of its outcome, in words: an
/// answer that carries it is uncertain (`attempted: true`), never resent.
pub const OUTCOME_UNKNOWN: &str = "the request was sent; its outcome is not known";
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
    /// The request was sent and nothing observed settles what it did: its
    /// outcome is not known. A stopped daemon observed exactly is settled.
    pub fn unsettled(&self) -> bool {
        self.attempted && !self.container_stopped()
    }
}
/// In plain words, never `Debug` text: a daemon that is not running, else
/// the cause, the runtime's HTTP status when it answered one, whether
/// anything was sent (and then that its outcome is not known), and a
/// failed connection fence.
impl fmt::Display for WireFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.container_stopped() {
            return f.write_str("the Sandbox's daemon is not running");
        }
        write!(f, "{}", self.cause)?;
        if let Some(status) = self.status {
            write!(f, " (the runtime answered HTTP {status})")?;
        }
        match self.attempted {
            true => write!(f, "; {OUTCOME_UNKNOWN}")?,
            false => f.write_str("; nothing was sent")?,
        }
        if let Some(error) = &self.fence_error {
            write!(f, "; closing its connection also failed: {error}")?;
        }
        Ok(())
    }
}
impl std::error::Error for WireFailure {}

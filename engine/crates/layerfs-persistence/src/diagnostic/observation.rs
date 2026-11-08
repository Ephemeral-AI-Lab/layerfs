//! Audit options and streamed observations.
use std::time::Instant;

/// Requested coverage and deadline of one audit.
#[derive(Clone, Copy, Debug, Default)]
pub struct Options<'a> {
    /// Omit exhaustive pack, canonical object and namespace/content reads.
    pub quick: bool,
    /// Absolute deadline; expiration leaves coverage incomplete.
    pub deadline: Option<Instant>,
    /// Caller cancellation; checked by SQL interruption and bounded readers.
    pub cancelled: Option<&'a std::sync::atomic::AtomicBool>,
}

impl Options<'_> {
    pub(crate) fn check(&self) -> Result<(), crate::backend::records::BackendError> {
        if self.deadline.is_some_and(|d| Instant::now() >= d)
            || self
                .cancelled
                .is_some_and(|c| c.load(std::sync::atomic::Ordering::Relaxed))
        {
            Err(crate::backend::records::BackendError::Cancelled)
        } else {
            Ok(())
        }
    }
}

/// Outcome of one observed stage. Unsupported coverage is never a pass.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Outcome {
    /// Completed checks found no inconsistency.
    Pass,
    /// A concrete inconsistency was observed.
    Fail,
    /// Coverage was intentionally excluded.
    Skipped,
    /// Coverage could not be established.
    Unknown,
}

/// Streaming diagnostic observation; finding retention belongs to the caller.
#[derive(Clone, Debug)]
pub struct Observation {
    /// Stable check identifier within Store integrity.
    pub id: &'static str,
    /// Exact stage outcome.
    pub outcome: Outcome,
    /// Checked entity identity, counts, or coverage explanation; never credentials.
    pub message: String,
}

pub(crate) fn emit(
    observer: &mut dyn FnMut(Observation),
    id: &'static str,
    outcome: Outcome,
    message: impl Into<String>,
) {
    observer(Observation {
        id,
        outcome,
        message: message.into(),
    });
}

/// Bounded host resolution supplied by the network-owning control layer.
/// Implementations must finish by the deadline and reap any helper process.
pub type Resolver<'a> =
    dyn FnMut(&str, u16, Instant) -> std::io::Result<Vec<std::net::SocketAddr>> + 'a;

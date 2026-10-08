//! Typed facade failures over the single existing control exchange owner.
use crate::control::{Control, ControlFailure};
use layerfs_bridge::control::{ControlRefusal, Reply, Request};
use std::fmt;

/// Original facade operation and its exact exchange or daemon refusal custody.
#[derive(Debug)]
pub struct OperationFailure {
    /// Exact selected operation; never replayed by the facade.
    pub request: Request,
    /// Original deciding cause and any retained publication knowledge.
    pub cause: OperationCause,
}
/// Distinct original failure boundaries of one facade operation.
#[derive(Debug)]
pub enum OperationCause {
    /// Original transport/protocol phase, attempted flag, reply and fence.
    Exchange(Box<ControlFailure>),
    /// Original correlated daemon refusal; the channel may remain usable.
    Remote(ControlRefusal),
    /// A terminal operation stopped after effects; the daemon keeps its custody.
    Retained(Box<layerfs_bridge::control::TeardownCustody>),
    /// Original correlated reply that cannot supply the selected typed result.
    Unexpected(Reply),
}
pub(crate) fn exchange(
    control: &mut Control,
    request: Request,
) -> Result<Reply, Box<OperationFailure>> {
    let reply = control.call(request.clone()).map_err(|cause| {
        Box::new(OperationFailure {
            request: request.clone(),
            cause: OperationCause::Exchange(cause),
        })
    })?;
    match reply {
        Reply::Refused(cause) => Err(Box::new(OperationFailure {
            request,
            cause: OperationCause::Remote(cause),
        })),
        Reply::Retained(custody) => Err(Box::new(OperationFailure {
            request,
            cause: OperationCause::Retained(custody),
        })),
        other => Ok(other),
    }
}
impl OperationFailure {
    pub(crate) fn unexpected(request: Request, reply: Reply) -> Box<Self> {
        Box::new(Self {
            request,
            cause: OperationCause::Unexpected(reply),
        })
    }
}
impl fmt::Display for OperationFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "SDK operation: {:?}", self.cause)
    }
}
impl std::error::Error for OperationFailure {}

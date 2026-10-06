//! Native owning object/length/serial adapters used by ordinary Workspace callers.
use super::{
    CallFailure, Calls, ClientRequest, FailureDomain, FailureValue, Operation, RemoteFailure,
    ReplyView, SaveToken,
};
use layerfs_bridge::{codec::Message, contract::FrameError};
use layerfs_content::{AuthenticatedObjects, ContentError, ContentResult, ObjectId};
use layerfs_workspace::{FileLengths, InodeSerials, WorkspaceError, WorkspaceResult};
use std::{
    fmt,
    sync::{Arc, Mutex, MutexGuard},
};
/// Original call/refusal/codec failure kept by the consuming operation.
pub enum PortFailure {
    /// Exact original failed native call and any credited input/result custody.
    Call(CallFailure),
    /// Exact typed refused/dispatched remote result, in its credited body.
    Remote(Message),
    /// Unexpected original result and checked local interpretation failure.
    Shape {
        /// Original checked failure.
        error: FrameError,
        /// Original credited body.
        message: Message,
    },
}
impl PortFailure {
    /// Original received result, when there is one. Parsing never replays a call.
    pub fn reply(&self) -> Option<&Message> {
        match self {
            Self::Remote(message) | Self::Shape { message, .. } => Some(message),
            Self::Call(failure) => failure.received.as_ref(),
        }
    }
}
impl fmt::Debug for PortFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Call(e) => e.fmt(f),
            Self::Remote(m) => f.debug_tuple("RemoteRefusal").field(&m.envelope()).finish(),
            Self::Shape { error, message } => f
                .debug_struct("ReplyShape")
                .field("error", error)
                .field("envelope", &message.envelope())
                .finish(),
        }
    }
}
impl fmt::Display for PortFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "runtime port: {self:?}")
    }
}
impl std::error::Error for PortFailure {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Call(e) => Some(e),
            Self::Shape { error, .. } => Some(error),
            Self::Remote(_) => None,
        }
    }
}
/// One content operation's authenticated native demand owner. Same-Save reads
/// select the original remote capability, without opening another Store.
/// Its first failure is retained and subsequent reads refuse before another call.
pub struct RemoteObjects {
    calls: Arc<Calls>,
    save: Option<SaveToken>,
    failure: Mutex<Option<PortFailure>>,
}
impl RemoteObjects {
    /// Starts one local demand operation over the existing authenticated connection.
    pub fn new(calls: Arc<Calls>, save: Option<SaveToken>) -> Self {
        Self {
            calls,
            save,
            failure: Mutex::new(None),
        }
    }
    /// Exact retained first failure/credited remote result, not an absence inference.
    pub fn failure(&self) -> Result<MutexGuard<'_, Option<PortFailure>>, FrameError> {
        self.failure.lock().map_err(|_| FrameError::Poisoned)
    }
}
impl AuthenticatedObjects for RemoteObjects {
    fn read_canonical_batch(&self, ids: &[ObjectId]) -> ContentResult<Vec<Vec<u8>>> {
        let mut retained = self
            .failure
            .lock()
            .map_err(|_| ContentError::ResourceUnavailable {
                what: "native demand owner poisoned",
            })?;
        if retained.is_some() {
            return Err(ContentError::ResourceUnavailable {
                what: "native demand already failed",
            });
        }
        let message = match self.calls.call(ClientRequest::objects(self.save, ids)) {
            Ok(message) => message,
            Err(error) => {
                let class = if matches!(
                    &error.error,
                    super::CallError::Receive(super::ClientReceiveError::Rejected {
                        error: FrameError::IdentityMismatch,
                        ..
                    })
                ) {
                    ContentError::IdentityMismatch
                } else {
                    ContentError::ProviderFailure {
                        what: "native demand call failed",
                    }
                };
                *retained = Some(PortFailure::Call(error));
                return Err(class);
            }
        };
        let decoded = ReplyView::decode(message.bytes());
        let result = match decoded {
            Ok(ReplyView::Objects(values)) => {
                let mut output = Vec::new();
                if let Err(error) = output.try_reserve_exact(values.len()) {
                    *retained = Some(PortFailure::Shape {
                        error: FrameError::Allocation {
                            requested_bytes: values.len() * std::mem::size_of::<Vec<u8>>(),
                            error,
                        },
                        message,
                    });
                    return Err(ContentError::ResourceUnavailable {
                        what: "native demand delivery allocation",
                    });
                }
                for (_, canonical) in values {
                    let mut body = Vec::new();
                    if let Err(error) = body.try_reserve_exact(canonical.len()) {
                        *retained = Some(PortFailure::Shape {
                            error: FrameError::Allocation {
                                requested_bytes: canonical.len(),
                                error,
                            },
                            message,
                        });
                        return Err(ContentError::ResourceUnavailable {
                            what: "native canonical delivery allocation",
                        });
                    }
                    body.extend_from_slice(canonical);
                    output.push(body);
                }
                return Ok(output);
            }
            Ok(ReplyView::Failure { error, .. }) => content_class(error),
            _ => ContentError::ProviderFailure {
                what: "native demand result shape",
            },
        };
        *retained = Some(PortFailure::Remote(message));
        Err(result)
    }
}
fn content_class(mut error: RemoteFailure<'_>) -> ContentError {
    // Exact domain/variant fields determine absence. No Display-string parsing,
    // authority/refusal/unpublished error or unfenced transport absence does.
    for _ in 0..3 {
        match (error.domain(), error.code()) {
            (FailureDomain::Content, 12) | (FailureDomain::Storage, 3) => {
                return ContentError::MissingObject
            }
            (FailureDomain::Content, 11) => return ContentError::IdentityMismatch,
            (FailureDomain::Runtime, 7 | 8) | (FailureDomain::Storage, 1) => {
                if let Ok(Some(field)) = error.field(1) {
                    if let FailureValue::Node(bytes) = field.value {
                        if let Ok(child) = RemoteFailure::decode(bytes) {
                            error = child;
                            continue;
                        }
                    }
                }
            }
            _ => (),
        }
        break;
    }
    ContentError::ProviderFailure {
        what: "native owning provider refused demand",
    }
}
/// Owning native saved-file metadata port, without whole payload reads.
pub struct RemoteLengths {
    calls: Arc<Calls>,
}
impl RemoteLengths {
    /// Uses the existing authenticated runtime attachment.
    pub fn new(calls: Arc<Calls>) -> Self {
        Self { calls }
    }
}
impl FileLengths for RemoteLengths {
    fn file_length(&self, id: ObjectId) -> WorkspaceResult<u64> {
        let message = self
            .calls
            .call(ClientRequest::lengths(&[id]))
            .map_err(|error| WorkspaceError::Service(Box::new(error)))?;
        if let Ok(ReplyView::Lengths(mut values)) = ReplyView::decode(message.bytes()) {
            if let Some((_, length)) = values.next() {
                return Ok(length);
            }
        }
        Err(WorkspaceError::Service(Box::new(PortFailure::Remote(
            message,
        ))))
    }
}
/// Owning native scope allocator; successful ranges are consumed, never recycled.
pub struct RemoteSerials {
    calls: Arc<Calls>,
}
impl RemoteSerials {
    /// Uses the existing authenticated runtime attachment.
    pub fn new(calls: Arc<Calls>) -> Self {
        Self { calls }
    }
}
impl InodeSerials for RemoteSerials {
    fn reserve(&self, count: u64) -> WorkspaceResult<(u64, u64)> {
        let request = ClientRequest::control(Operation::ReserveInodes, None, count)
            .map_err(|_| ContentError::InvalidRecord("native serial request"))?;
        let message = self
            .calls
            .call(request)
            .map_err(|error| WorkspaceError::Service(Box::new(error)))?;
        if let Ok(ReplyView::Serials { start, count }) = ReplyView::decode(message.bytes()) {
            return Ok((start, count));
        }
        Err(WorkspaceError::Service(Box::new(PortFailure::Remote(
            message,
        ))))
    }
}

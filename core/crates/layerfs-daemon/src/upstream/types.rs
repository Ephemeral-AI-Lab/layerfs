//! Exact provisioned identities, completion profiles and original refusal custody.
use crate::{Completion, OwnerError};
use layerfs_bridge::{codec::Message, contract::FrameError};
use layerfs_content::ContentError;
use layerfs_history::{BranchSnapshot, CatalogId, WorkspaceId};
use layerfs_persistence::SqlitePersistenceProfile;
use layerfs_sdk::client::{Attachment, CallFailure, RemoteObjects};
use layerfs_storage::{StorageError, StoragePolicy};
use std::{fmt, sync::Arc};

/// Known attachment and its original successful bootstrap results.
/// Holding these receipts retains their actual owner/transport credits. They
/// establish no additional authority or native mount readiness.
pub struct AttachSuccess {
    /// Authority-bound upstream with the known opened local namespace.
    pub upstream: super::Upstream,
    /// Original attempted local Open, including all JobWork and result credit.
    pub open: Completion,
    /// Original validated Binding reply, retaining its receive credit.
    pub binding_reply: Message,
    /// Original validated Policy reply, retaining its receive credit.
    pub policy_reply: Message,
}

/// Trusted requested authority context, provisioned separately from wire replies.
#[derive(Clone, Debug)]
pub struct ExpectedBinding {
    /// Exact authenticated host responder static public key.
    pub host_peer: [u8; 32],
    /// Exact daemon initiator static public key authorized by the host.
    pub local_peer: [u8; 32],
    /// Authority-assigned runtime incarnation, never implicitly refreshed.
    pub runtime: [u8; 32],
    /// Exact global catalog identity.
    pub catalog: CatalogId,
    /// Original provider continuity incarnation.
    pub provider_incarnation: u64,
    /// Exact requested Workspace incarnation; also the new local namespace incarnation.
    pub workspace: WorkspaceId,
    /// Exact captured Branch/head/base/root/scope/filesystem-profile expectations.
    pub snapshot: BranchSnapshot,
    /// Exact immutable root inode serial.
    pub root_serial: u64,
    /// Exact expected owning construction/storage policy.
    pub policy: StoragePolicy,
    /// Required completion/durability profile, independent of snapshot.profile.
    pub persistence: SqlitePersistenceProfile,
}
/// Host-provisioned actual completion profile from Handles::profile().persistence.
/// This is trusted bootstrap context, not a claim derived from a filesystem hash
/// or network policy reply. Daemon never opens the global provider to obtain it.
#[derive(Clone, Copy, Debug)]
pub struct PersistenceBootstrap {
    /// Provisioned authenticated host responder for this fact.
    pub host_peer: [u8; 32],
    /// Exact runtime whose initialized host handles supplied this fact.
    pub runtime: [u8; 32],
    /// Exact catalog whose initialized host handles supplied this fact.
    pub catalog: CatalogId,
    /// Exact provider continuity incarnation for those handles.
    pub provider_incarnation: u64,
    /// Actual selected profile, captured before Runtime consumes Handles.
    pub profile: SqlitePersistenceProfile,
}
/// Exact field group refusing a provisioned or received authority context.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BindingMismatch {
    /// Invalid zero/incarnation/coherent-snapshot inputs.
    Provision,
    /// Authenticated host responder differs.
    HostPeer,
    /// Trusted completion-profile bootstrap belongs to another authority context.
    Bootstrap,
    /// Actual host completion profile differs from the required profile.
    Persistence,
    /// Returned local peer/runtime/catalog/provider/Workspace context differs.
    Authority,
    /// Returned captured Branch/head/base/root/scope/filesystem-profile differs.
    Snapshot,
    /// Returned or demanded immutable root serial differs.
    RootSerial,
    /// Returned owning storage/construction policy differs.
    Policy,
    /// Original received reply is not the requested typed response.
    ReplyShape,
}
/// Original boundary reached by one attachment attempt.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AttachPhase {
    /// Provisioned identity/profile checks before any wire request.
    Provision,
    /// One original Binding request and received reply.
    Binding,
    /// One original Policy request and received reply.
    Policy,
    /// Bounded canonical root acquisition before local namespace creation.
    Base,
    /// One local overlay Open and its exact result.
    Open,
}
/// Original upstream/local-owner refusal, without reconnect or guessed cleanup.
#[derive(Debug)]
pub enum UpstreamError {
    /// Exact checked framing/codec cause.
    Frame(FrameError),
    /// Exact original call failure and any received/partial-result custody.
    Call(Box<CallFailure>),
    /// Exact provision/response mismatch; original messages remain in AttachRefusal.
    Mismatch(BindingMismatch),
    /// Owning expected-policy validation refusal.
    Storage(StorageError),
    /// Original canonical acquisition/context error; retained RemoteObjects owns its cause.
    Content(ContentError),
    /// Original local owner admission/wait error, including unattempted command custody.
    Owner(OwnerError),
    /// Original attempted owner result/error with its retained completion credit.
    Completion(Box<Completion>),
}
impl fmt::Display for UpstreamError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "upstream attach: {self:?}")
    }
}
impl std::error::Error for UpstreamError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Frame(e) => Some(e),
            Self::Call(e) => Some(e.as_ref()),
            Self::Storage(e) => Some(e),
            Self::Content(e) => Some(e),
            Self::Owner(e) => Some(e),
            Self::Completion(e) => Some(e.as_ref()),
            Self::Mismatch(_) => None,
        }
    }
}
/// Original failed attach and every already-acquired owner returned to the caller.
/// A lost Open reply does not prove namespace absence and permits no guessed close.
pub struct AttachRefusal {
    /// Exact stopping boundary.
    pub phase: AttachPhase,
    /// Original deciding refusal/cause.
    pub error: UpstreamError,
    /// Original attachment for explicit fence/join; no reconnect/resend occurs.
    pub attachment: Attachment,
    /// Original trusted requested context, including the Open identity/root facts.
    pub expected: ExpectedBinding,
    /// Original separately trusted completion-profile bootstrap fact.
    pub bootstrap: PersistenceBootstrap,
    /// Original credited Binding reply when one was received.
    pub binding_reply: Option<Message>,
    /// Original credited Policy reply when one was received.
    pub policy_reply: Option<Message>,
    /// Bootstrap operation provider, preserving original PortFailure/credited result.
    pub objects: Option<Arc<RemoteObjects>>,
}
impl fmt::Debug for AttachRefusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AttachRefusal")
            .field("phase", &self.phase)
            .field("error", &self.error)
            .field("expected", &self.expected)
            .finish_non_exhaustive()
    }
}

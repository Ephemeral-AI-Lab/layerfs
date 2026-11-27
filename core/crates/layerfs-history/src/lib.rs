//! Immutable history metadata for LayerFS (C5).
//!
//! C5 owns history identities and records, LayerStack and Branch metadata,
//! immutable Commits and Layers published against an expected head, scope-wide
//! inode reservations and atomic conditional history transitions with bounded queries.
//! It opens no content object, reads no pack and never interprets a physical
//! save, locator or publication field: those remain C2's private ownership, and
//! C2 and C5 retain separate semantic transaction boundaries in the adapter.
//!
//! The crate is deliberately independent of the service, the bridge, the daemon,
//! the Workspace, FUSE and any executor. Its semantic operations take typed
//! values and return typed outcomes; environment facts (paths, sockets, clocks,
//! process identity) belong to the adapters that call it. A LayerStack ID, a
//! Branch ID and a Workspace incarnation are explicit checked inputs from the
//! application authority - this crate never invents one from a clock or a PID.
//!
#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub(crate) mod admission;
pub(crate) mod catalog;
pub mod error;
pub mod identity;
pub mod records;
pub use admission::authority::{
    compare_place, ordered_token_parts, Liveness, PlaceAuthority, PlaceOrder, PlaceRecord,
    PlaceView, SandboxPlace, ORDERED_TOKEN_PREFIX,
};
pub use admission::holder::MountHolderView;
pub use catalog::contract as catalog;
pub use catalog::path;
pub use catalog::query;

pub use catalog::{
    ChainRow, ContextTarget, HistoryCatalog, HistoryCatalogConfig, MountMarkers, NamedSnapshot,
    NodeRecord, PublishLane, ReadContext,
};
pub use error::{
    GcHolder, HistoryError, HistoryResult, MountHolders, OpConflict, UnknownOutcomeState,
};
pub use identity::{
    BranchId, CatalogId, CommitAttempt, CommitId, HistoryName, LayerId, LayerStackId, NodeId,
    WorkspaceId,
};
pub use records::{
    AddLayerOutcome, AddLayerRequest, BindingHint, BranchMergeRequest, BranchRecord,
    BranchSnapshot, CommitHistoryRequest, CommitMeta, CommitOutcome, CommitRecord, CommitRequest,
    CommitStatistics, CommitVia, FileDigest, ForkRequest, ForkSource, GcAuditAfter, GcAuditPage,
    GcBranchAuditPage, GcDeletion, GcIntent, GcIntentDone, GcIntentKind, GcReconcile, GcScopeKind,
    InitStatistics, LayerHistoryRequest, LayerRecord, LayerStackRecord, MergeCommit, MergeHead,
    MergeSide, Page, PageResult, PathRemovalKind, PathRemovalOutcome, PathRemovalRecord,
    PathRemovalRequest, RecordKind, Reservation, ReserveRequest, StackInitialization,
    StoreStatistics, FILE_DIGEST_WINDOW, MAXIMUM_AUDIT_PAGE, MAXIMUM_MESSAGE_BYTES,
    MAXIMUM_PAGE_RECORDS,
};

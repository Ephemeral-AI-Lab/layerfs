//! Immutable history metadata for LayerFS (C5).
//!
//! C5 owns history identities and records, LayerStack and Branch metadata,
//! immutable Commits and Layers, exact-token Workspace stages, scope-wide inode
//! reservations and atomic conditional history transitions with bounded queries.
//! It opens no content object, reads no pack and never interprets a physical
//! save, locator or publication field: those remain C2's private ownership, and
//! no foreign key crosses between the two databases.
//!
//! The crate is deliberately independent of the service, the bridge, the daemon,
//! the Workspace, FUSE and any executor. Its semantic operations take typed
//! values and return typed outcomes; environment facts (paths, sockets, clocks,
//! process identity) belong to the adapters that call it. A LayerStack ID, a
//! Branch ID and a Workspace incarnation are explicit checked inputs from the
//! application authority - this crate never invents one from a clock or a PID.
//!
//! The native SQLite provider behind the `native` feature is one implementation
//! of the [`catalog::HistoryCatalog`] contract. Building without that feature
//! proves the contract is separable; it claims no other persistence backend.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub mod catalog;
pub mod error;
pub mod identity;
pub mod records;

#[cfg(feature = "native")]
pub mod sqlite;

pub use catalog::{HistoryCatalog, HistoryCatalogConfig};
pub use error::{HistoryError, HistoryResult};
pub use identity::{
    BranchId, CatalogId, CommitId, HistoryName, LayerId, LayerStackId, StageToken, WorkspaceId,
};
pub use records::{
    AddLayerOutcome, AddLayerRequest, BranchRecord, BranchSnapshot, CommitHistoryRequest,
    CommitRecord, CommitStagedOutcome, CommitStagedRequest, DiscardOutcome, DiscardRequest,
    ForkRequest, ForkSource, LayerHistoryRequest, LayerRecord, LayerStackRecord, ManifestEntry,
    NamespaceManifest, Page, PageResult, RecordKind, Reservation, ReserveRequest,
    StackInitialization, StageRecord, StageRequest, MAXIMUM_PAGE_RECORDS,
};

//! Object-safe native workspace boundary.

mod durability;
mod error;
mod fact_values;
mod facts;
mod handles;
mod native;
mod projection;
mod timer;
mod workspace;

pub use durability::{DirectoryDurability, DurabilityClass, DurabilityClassCounts};
pub use error::{DriverError, Result};
pub use fact_values::{
    ProjectionCallFacts, ProjectionCleanupFacts, ProjectionReplaceFacts, ProjectionSyncFacts,
    ProjectionWriteFacts,
};
pub use facts::ProjectionFacts;
pub use handles::{DirectoryHandle, NamePreflight, OwnedTempHandle, RegularFileHandle};
pub use native::{
    NativeEntry, NativeKind, NativeMetadata, NativeXattrIter, NativeXattrNameIter, NativeXattrs,
    MAX_NATIVE_XATTR_BYTES,
};
pub use projection::{ProjectionDriver, WorkspacePolicy};
pub use timer::{ProjectionTimer, ProjectionTimerAvailability};
pub use workspace::ProjectionWorkspace;

//! Canonical objects and complete-file construction for LayerFS (C1).
//!
//! C1 owns canonical identity, framing, complete-file construction and bounded
//! logical reads. It opens no database, pack or file: construction emits
//! finalized canonical objects to a caller-supplied bounded consumer, and reads
//! ask a caller-supplied authenticated provider for canonical bytes. It depends
//! on no storage, Workspace, history, mount or runtime type.
//!
//! Every entry point takes a [`layerfs_telemetry::timer::TimingScope`] so that
//! construction-only, read-only and integrated callers can measure the same
//! production functions; recording enabled or disabled changes no product work.
//!
//! See the crate `README.md` for the accepted profile, capacities and the exact
//! commands that exercise them.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod contract;
pub use contract::error;
pub mod file;
pub mod filesystem;
pub mod object;
pub use contract::policy;

pub use error::{ContentError, ContentResult};
pub use file::{
    apply_edits, apply_edits_backed, construct_bytes, construct_bytes_with_predecessor,
    construct_runs, construct_stream, encode_whole_file_payload, read_all, read_all_bounded,
    read_range, whole_file_payload, ConstructedFile, Edit, EditRecordApply, EditRecordChange,
    EditRecordExpected, EditRecordKey, EditRequest, EditSequence, EditSource, FileContent, FileRun,
    FileRuns, FileView, IndexedEditBacking, PredecessorBase, RunConstruction,
};
/// Neutral names for the same opaque construction-record protocol. Existing
/// file-edit names remain public and refer to these identical types/trait.
pub use file::{
    EditRecordApply as ConstructionRecordApply, EditRecordChange as ConstructionRecordChange,
    EditRecordExpected as ConstructionRecordExpected, EditRecordKey as ConstructionRecordKey,
    IndexedEditBacking as IndexedConstructionBacking,
};
pub use filesystem::inode::InodeChange;
pub use filesystem::{
    build_filesystem, build_filesystem_streamed_backed, build_filesystem_streamed_backed_timed,
    update_filesystem, update_filesystem_streamed_backed, update_filesystem_streamed_backed_timed,
    DirectoryRoot, DirectoryUpdate, FilesystemInput, FilesystemObjects, FilesystemRead,
    FilesystemResources, FilesystemResult, FilesystemRoot, InodeIdentity, InodeScope, InodeUpdate,
    LogicalPath, ObjectWork, PathName, Stat,
};
pub use object::inode_leaf;
pub use object::{
    AdvisoryPredecessor, AdvisoryPredecessors, AuthenticatedObjects, DiscardingConsumer,
    FinalizedConsumer, FinalizedObject, ObjectId, ObjectRole, PredecessorProvenance, DIGEST_BYTES,
    MAXIMUM_ADVISORY_PREDECESSORS, OBJECT_DOMAIN,
};
pub use policy::{
    ConstructionCapacities, ConstructionPolicy, Representation, DEFAULT_CHUNK_DELTA_MAX_DEPTH,
    DEFAULT_SMALL_FILE_THRESHOLD_BYTES, DEFAULT_WHOLE_FILE_DELTA_MAX_DEPTH,
    MAXIMUM_DELTA_MAX_DEPTH, MAXIMUM_SMALL_FILE_THRESHOLD_BYTES,
    MINIMUM_SMALL_FILE_THRESHOLD_BYTES,
};

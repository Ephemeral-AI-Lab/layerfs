//! Immutable content-addressed storage ports and filesystem implementation.

mod admission;
mod catalog;
mod closure;
#[cfg(feature = "operation-polymorphism")]
mod closure_storage;
mod fs;
mod locator;
#[cfg(feature = "operation-polymorphism")]
mod locator_index;
#[cfg(feature = "operation-polymorphism")]
mod operation_admission;
mod port;

#[cfg(feature = "operation-polymorphism")]
pub(crate) use admission::admission_traversal_resident_bytes_v1;
#[cfg(test)]
pub(crate) use admission::admit_complete_immutable_v1;
pub(crate) use admission::AdmissionBuffersV1;
pub(crate) use catalog::CATALOG_MARKER_BYTES;
#[cfg(feature = "operation-polymorphism")]
pub(crate) use closure::FsCasClosureSpoolV1;
pub(crate) use closure::{
    compare_closure_object_ids_v1, AdmittedClosureV1, CompleteImmutableClosureReadPortV1,
};
#[cfg(feature = "operation-polymorphism")]
pub(crate) use closure_storage::{ClosureObjectRecordV1, FileClosureObjectSpoolV1};
pub(crate) use fs::FsCasOccupiedV1;
pub(crate) use fs::{
    locator_publication_receipt_preparation_bytes_bound_v1, FsCasBoundaryV1, FsCasCleanupTargetV1,
    FsCasControlV1, FsCasErrorV1, FsCasFilesystemBoundaryV1, FsCasV1, FsOperationObservedControlV1,
    FsPackAdmissionOutcomeV1, FsPrivatePackV1, CLOSURE_MARKER_BYTES,
};
#[cfg(test)]
pub(crate) use fs::{CarrierReceiptTransitionCheckV1, FsCasFailureCauseV1};
#[cfg(all(test, feature = "operation-polymorphism"))]
pub(crate) use fs::{
    FsCasFilesystemFailureV1, FsCasResidueAccountingBoundaryV1, FsCasResourceV1,
    ROOT_LOGICAL_STORAGE_BUDGET_V1, ROOT_NAMESPACE_ENTRY_BUDGET_V1,
};
#[cfg(feature = "operation-polymorphism")]
pub(crate) use fs::{
    FsClosureAdmissionErrorV1, FsOperationCapabilityV1, FsOperationKindV1,
    FsOperationSpoolConstructionUnwindV1, FsOperationSpoolV1, FsStorageEnvelopeV1,
    FsStorageOperationTokenV1,
};
pub(crate) use locator::PERSISTENT_LOCATOR_BYTES_V1;
#[cfg(all(test, feature = "operation-polymorphism"))]
pub(crate) use locator_index::{global_seen_hash_v1, GLOBAL_SEEN_MAXIMUM_PROBES_PER_LOOKUP_V1};
#[cfg(feature = "operation-polymorphism")]
pub(crate) use locator_index::{
    FileGlobalSeenSpoolV1, GlobalSeenErrorV1, GlobalSeenLookupV1, GlobalSeenRecordV1,
    GLOBAL_SEEN_RECORD_BYTES,
};
#[cfg(feature = "operation-polymorphism")]
pub(crate) use operation_admission::{
    authenticate_base_root_storage_v1, begin_storage_session_v1, complete_closure_fence_storage_v1,
    ClosureFenceStorageOutcomeV1, StorageSessionV1,
};
#[cfg(test)]
pub(crate) use port::{read_complete_immutable_v1, BoundedImmutableReadSinkV1, ClosureObjectV1};
pub(crate) use port::{
    ImmutablePortErrorV1, OccupiedImmutableReadPortV1, PreparedImmutableClosurePortV1,
    ValidatedOccupiedObjectV1,
};

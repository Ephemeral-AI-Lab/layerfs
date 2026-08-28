//! Minimal Phase 4A durable LayerFS engine.

#![forbid(unsafe_code)]

pub mod candidate;
pub mod error;
pub mod full;
pub mod generation;
pub mod integrity;
pub mod migration;
pub mod object;
pub mod publication;
pub mod refs;
mod schema;
pub mod scratch;
pub mod sqlite;
pub mod working;

pub(crate) use error::{io_engine_error, map_sqlite_error, sqlite_error_kind};
pub use error::{EngineError, EngineResult, SqliteErrorKind, StorageError};
pub(crate) use full::legacy_store::{
    checked_add, elapsed_ns, mark_sql_family, observe_time, BatchTimings, SQL_FAMILY_COMPACTION,
    SQL_FAMILY_LIVE_INTEGRITY, SQL_FAMILY_NONE, SQL_FAMILY_PRIMARY_READ, SQL_FAMILY_PUBLICATION,
};
pub use full::{
    branch::read::{
        BranchAncestry, BranchHead, BranchRollbackOutcome, BranchRollbackPublication, VersionRef,
    },
    layer_stack::read::{LayerStackHead, LayerStackMergeOutcome, LayerStackRollbackOutcome},
    legacy_store::{
        CompactionStorageObservation, Engine, EngineCounters, Storage, StorageObservation,
    },
    record_id::{
        derive_id, BranchId, LayerId, LayerStackId, LeaseId, OperationId, OperationVersionId,
        RequestId,
    },
    store::{FullStorage, FullStorageCounters},
    transfer::batch::{
        branch_push_bundle_page_digest, branch_push_page_digest, BranchPushBundle,
        BranchPushIdentityBuilder, BranchPushOutcome, BranchPushRequest, PushedBranchRollback,
        PushedChildMerge, PushedLayer, PushedLayerMerge, PushedLayerStack, PushedLayerStackAction,
        PushedLayerStackTransition, PushedOperation, PushedRelease, StoredTransferState,
        SyncTransferCounters, VerifiedFetchRequest, BRANCH_PUSH_IDENTITY_VERSION,
        MAX_HISTORY_PAGE_RECORDS, MAX_PUSH_OPERATION_RECORDS, MAX_TRANSITION_PAYLOAD_BYTES,
    },
};
pub(crate) use object::{
    authenticate_borrowed_unaccounted, payload_batch_sql,
    with_authenticated_canonical_on_connection, with_read_canonical_on_connection,
};
pub use object::{DeltaRecord, ObjectRecord, PutOutcome, RootId, RootRecord};
pub use schema::TRANSITION_FORMAT_VERSION;
pub(crate) use schema::{
    admitted_store_id_counted, initialize_schema_counted, note_statement, FORMAT_MARKER,
    SCHEMA_VERSION,
};
pub use schema::{
    SchemaContract, SchemaIdentity, StoreRole, FULL_SCHEMA, LEGACY_FULL_SCHEMA, WORKING_SCHEMA,
};
#[cfg(test)]
pub(crate) use sqlite::initial_verified_scrub;
pub use sqlite::SqliteProfile;
pub(crate) use sqlite::{
    add_retained_scrub_counters, add_verification_progress_counters, clear_known_trusted_history,
    configure_profile_counted, inspect_store_id_readonly, preflight_schema, schema_shape,
    trusted_history, CommitDispatch, ConnectionGuard, SchemaState, BUSY_TIMEOUT,
};
pub use working::branch::merge::*;
pub use working::layer_candidate::*;
pub use working::operation::record::*;

pub const COMPONENT: &str = "layerfs-storage";

#[cfg(test)]
mod tests;

//! WP4-M private candidate campaign.
//!
//! This executable is intentionally the only profile selector.  It owns the
//! candidate-only SQLite schema and never opens the production v1 engine.

use std::cell::Cell;
use std::collections::{BTreeMap, HashMap};
use std::env;
use std::fmt::Write as _;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::ops::Deref;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use blake3::Hasher;
use layerfs_core::cdc::FastCdc;
use layerfs_core::content::persistence as file_codec;
use layerfs_core::cow::persistence as dir_codec;
use layerfs_core::cow::{RootHandle, TreeNode};
use layerfs_core::delta::codec as delta_codec;
use layerfs_core::object::{DirectoryEntry, Object, ObjectKind, ObjectReference};
use layerfs_core::validation::ValidatedSnapshotReceiptV1;
use layerfs_core::{
    chunk_id, decode_object, encode_bytes_object_to, encode_object as encode_canonical_object,
    CanonicalName, CoreError, CoreResult, ObjectId,
};
use rusqlite::ffi;
use rusqlite::types::ValueRef;
use rusqlite::{params, Connection, OpenFlags, OptionalExtension};
use std::time::{SystemTime, UNIX_EPOCH};

const SOURCE_100: u64 = 100 * 1024 * 1024;
const SOURCE_512: u64 = 512 * 1024 * 1024;
const RETAINED_CDC_100: u64 = 5_284;
const RETAINED_CDC_512: u64 = 27_162;
const RETAINED_RAW_100: &str = "bb883eecf4ea85d80432953791dcc352243da94175e7503e2c476afe9bd0bab7";
const RETAINED_RAW_512: &str = "84f895c546504bd80a343c7c7300b26cc010dad27c7c897efc6f37fc2821efc2";
const RETAINED_CDC_SEQUENCE_100: &str =
    "5bb376c3c54d8724973a7b160acab599f2f5cee4b4a56e855ff0cbe987425994";
const RETAINED_CDC_SEQUENCE_512: &str =
    "8b9c305cc4e128acbbe16d6aea4d000f3a483604c7b5f914d953bcccd7225d0b";
const RETAINED_SEED: u64 = 0x4c41594552534653;
const DIRECTORY_ENTRIES: usize = 100_000;
const MAX_DEPTH: usize = 256;
const MAX_EDIT_ORACLE_BYTES: usize = 32 * 1024;
const MAX_PREPARED_EXPECTATION_BYTES: u64 = 128 * 1024;
const MAX_PREPARED_RANGE_PROBES: usize = 7;
const AMENDED_M45_EDIT_OFFSET: u64 = 52_480_416;
const AMENDED_M45_EDIT_LENGTH: usize = 18_854;
const AMENDED_M45_EDIT_POSITION: u64 = 2_642;
const AMENDED_M45_EDITED_FINGERPRINT: &str =
    "527b215f91735e023b23a2e970f86c9e25ea303d38a1e4006f3f3a2a98f9db49";
const AMENDED_M45_CDC_SEQUENCE: &str =
    "e6d6d858ab6ff9804839630df90a2e621ae06291e55ab12aea9957c566ec83f7";
const AMENDED_M45_BASE_ROOT: &str =
    "2d41c27f96b0332475fb8ec3c46a336c9c8a8084408bc545e5cbb24d51cb25d0";
const AMENDED_M45_BASE_TRANSITION: &str =
    "ba15fd20469414de99c135fc90a5c5ad028f99f115b8c0d138ace9ec98536412";
const AMENDED_M45_BASE_CLOSURE: &str =
    "d6aac6e40cc851dd6295dbeec6488f1c5ebefa7520f86b0cd12bdcdce1f0d54a";
const AMENDED_M45_BEFORE_FILE: &str =
    "a94d42f6357b621ea51e306fe0a242854ed95d02d3e3dc7a88e3c2a20c194786";
const AMENDED_M45_AFTER_FILE: &str =
    "ab1f98a2c44c60f1b88f8aaec368ab2bbd68de9580e6e79b4dbf859800f2e7c8";
const AMENDED_M45_RESULT_ROOT: &str =
    "d1a69475b0f8e25e44d7bd625a679b596ea2a8b3347ef8c15fafa13f654b299b";
const AMENDED_M45_RESULT_TRANSITION: &str =
    "f11cc9d84deae7f1871adca62cc562ab63dbb01e9c39771ed3522eab4007cee1";
const AMENDED_M45_RESULT_CLOSURE: &str =
    "c0f6a39bf9939c89301bedb564516c5ec851321a1d89c69b2e95d4b1844a9587";
const F1_STATUS_SCHEMA: &str = "f1-v2-status-codes-v1";
const STATUS_OBSERVED: &str = "O";
const STATUS_UNAVAILABLE_STATUS_API: &str = "U_STATUS_API";
const F1_Q_EQUATION: &str = "Q1";
const F1_STATUS_CODES: &[&str] = &[
    "O",
    "O_EXT",
    "D",
    "NA",
    "U_WD",
    "U_HEAP",
    "U_STATUS_API",
    "U_CACHE_HWM",
    "U_DIRTY_CUR",
    "U_VFS_IO",
    "U_VFS_SYNC",
    "U_JRN_PEAK",
    "U_TMP_PEAK",
    "U_PHYS_BYTES",
    "U_PLAN",
    "U_NATIVE_PREP",
    "MIXED_IO",
];
const F1_ROW_STATUS_REPLACEMENTS: &[(&str, &str)] = &[
    ("\"phase_counters\":\"Observed\"", "\"phase_counters\":\"O\""),
    ("\"identity_hash_bytes\":\"Observed\"", "\"identity_hash_bytes\":\"O\""),
    ("\"borrowed_bytes_encoding\":\"Observed\"", "\"borrowed_bytes_encoding\":\"O\""),
    ("\"object_id_authentication_reuse\":\"Observed\"", "\"object_id_authentication_reuse\":\"O\""),
    ("\"logical_q\":\"Observed\"", "\"logical_q\":\"O\""),
    ("\"w_d\":\"Unavailable: governing cumulative definitions are not implemented\"", "\"w_d\":\"U_WD\""),
    ("\"row_blob_copies\":\"Observed\"", "\"row_blob_copies\":\"O\""),
    ("\"borrowed_row_blob_path\":\"Observed\"", "\"borrowed_row_blob_path\":\"O\""),
    ("\"incremental_blob_api\":\"Observed\"", "\"incremental_blob_api\":\"O\""),
    ("\"cpu_rss\":\"Observed externally per child by /usr/bin/time -l\"", "\"cpu_rss\":\"O_EXT\""),
    ("\"other_heap_copy_bytes\":\"Unavailable\"", "\"other_heap_copy_bytes\":\"U_HEAP\""),
    ("\"query_plans\":\"Unavailable\"", "\"query_plans\":\"U_PLAN\""),
    ("\"native_sqlite_prepare_calls\":\"Unavailable\"", "\"native_sqlite_prepare_calls\":\"U_NATIVE_PREP\""),
    ("\"blob_api_status\":\"Observed\"", "\"blob_api_status\":\"O\""),
    ("\"sync_fsync_observations\":\"Unavailable\"", "\"sync_fsync_observations\":\"U_VFS_SYNC\""),
    ("\"host_physical_io\":\"Unavailable\"", "\"host_physical_io\":\"U_PHYS_BYTES\""),
    ("\"sqlite_page_cache_true_high_water\":\"Unavailable: SQLITE_DBSTATUS_CACHE_USED high-water is always zero by API contract\"", "\"sqlite_page_cache_true_high_water\":\"U_CACHE_HWM\""),
    ("\"dirty_pages_current\":\"Unavailable: SQLite exposes dirty writes/spills but not current dirty-page count\"", "\"dirty_pages_current\":\"U_DIRTY_CUR\""),
    ("\"main_db_io_calls_bytes\":\"Unavailable: requires VFS xRead/xWrite or privileged syscall trace\"", "\"main_db_io_calls_bytes\":\"U_VFS_IO\""),
    ("\"journal_io_calls_bytes\":\"Unavailable: requires VFS xRead/xWrite or privileged syscall trace\"", "\"journal_io_calls_bytes\":\"U_VFS_IO\""),
    ("\"sync_calls_wall\":\"Unavailable: VFS excluded; fs_usage/dtruss require unavailable privileges\"", "\"sync_calls_wall\":\"U_VFS_SYNC\""),
    ("\"journal_true_peak\":\"Unavailable: DELETE journal can grow/disappear between snapshots\"", "\"journal_true_peak\":\"U_JRN_PEAK\""),
    ("\"temporary_file_peak\":\"Unavailable: no filename/peak API under temp_store=FILE\"", "\"temporary_file_peak\":\"U_TMP_PEAK\""),
    ("\"host_physical_io_bytes\":\"Unavailable: not derived from logical/allocation/block-operation counters\"", "\"host_physical_io_bytes\":\"U_PHYS_BYTES\""),
    ("\"process_io\":\"Observed externally: separate user/system CPU and block-operation counts; byte-level physical I/O unavailable\"", "\"process_io\":\"MIXED_IO\""),
    ("\"physical_io_cache_sync_temp_journal_status\":\"Mixed: supported SQLite/filesystem snapshots observed; unsupported VFS/privileged facts unavailable with reasons\"", "\"physical_io_cache_sync_temp_journal_status\":\"MIXED_IO\""),
    ("\"q_equation\":\"pre_admitted_checked_sum:canonical+decoded_nodes+file_refs+tree_nodes+dfs+cdc+sql+expectations+ranges+receipts+report\"", "\"q_equation\":\"Q1\""),
    ("\"peak_journal_bytes\":\"Unavailable\"", "\"peak_journal_bytes\":\"U_JRN_PEAK\""),
    ("\"peak_temporary_bytes\":\"Unavailable\"", "\"peak_temporary_bytes\":\"U_TMP_PEAK\""),
    ("\"w_bytes\":\"Unavailable\"", "\"w_bytes\":\"U_WD\""),
    ("\"d_bytes\":\"Unavailable\"", "\"d_bytes\":\"U_WD\""),
];

type AnyResult<T> = Result<T, Box<dyn std::error::Error>>;
type StoreMetaRow = (
    Option<[u8; 32]>,
    Option<[u8; 16]>,
    Option<[u8; 32]>,
    Option<[u8; 8]>,
    [usize; 4],
);
type VisibleHeadRow = (
    Option<[u8; 8]>,
    Option<[u8; 32]>,
    Option<[u8; 32]>,
    Option<[u8; 216]>,
    [usize; 4],
);
type VisibleHead = (u64, ObjectId, ObjectId, [u8; 216]);
type FileObservations = (
    u64,
    String,
    String,
    Vec<Vec<u8>>,
    Vec<(&'static str, std::ops::Range<u64>)>,
);

fn fixed_blob<const N: usize>(value: ValueRef<'_>) -> (Option<[u8; N]>, usize) {
    match value {
        ValueRef::Blob(bytes) => (bytes.try_into().ok(), bytes.len()),
        _ => (None, 0),
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CandidateError {
    MissingObject(ObjectId),
}

impl std::fmt::Display for CandidateError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingObject(id) => write!(formatter, "object {id} is missing"),
        }
    }
}

impl std::error::Error for CandidateError {}

static NEXT_OPEN_IDENTITY: AtomicU64 = AtomicU64::new(1);

fn next_open_identity() -> CoreResult<u64> {
    NEXT_OPEN_IDENTITY
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
            value.checked_add(1)
        })
        .map_err(|_| CoreError::LengthOverflow)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PublishFault {
    BeforeCommit,
    AfterCommitBeforeAck,
    #[cfg(test)]
    AfterCommitDifferentHead,
    #[cfg(test)]
    AfterCommitUnavailable,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Reconciliation {
    NotAttempted,
    RequestedVisible,
    PriorVisible,
    DifferentHead,
    Ambiguous,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum QualificationMode {
    FullClosure,
    ChangedSpine,
}

fn qualification_mode() -> AnyResult<QualificationMode> {
    match env::var("WP4M_M45_QUALIFICATION_MODE")
        .unwrap_or_else(|_| "changed-spine".to_string())
        .as_str()
    {
        "full-closure" => Ok(QualificationMode::FullClosure),
        "changed-spine" => Ok(QualificationMode::ChangedSpine),
        value => Err(format!("unknown M4.5 qualification mode {value}").into()),
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum FailureCause {
    Core(CoreError),
    MissingObject(ObjectId),
}

fn failure_cause(error: &(dyn std::error::Error + 'static)) -> FailureCause {
    if let Some(CandidateError::MissingObject(id)) = error.downcast_ref::<CandidateError>() {
        FailureCause::MissingObject(*id)
    } else {
        FailureCause::Core(
            error
                .downcast_ref::<CoreError>()
                .copied()
                .unwrap_or(CoreError::Io),
        )
    }
}

fn core_failure(error: &(dyn std::error::Error + 'static)) -> CoreError {
    match failure_cause(error) {
        FailureCause::Core(error) => error,
        FailureCause::MissingObject(_) => CoreError::MissingObject,
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct FailureProvenance {
    first: Option<FailureCause>,
    cleanup_first: Option<FailureCause>,
    reconciliation: Reconciliation,
    reconciliation_error: Option<FailureCause>,
    dominant: Option<FailureCause>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PublicationStatus {
    Committed,
    RequestedVisible,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct PublicationOutcome {
    status: PublicationStatus,
    diagnostic: Option<FailureProvenance>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct PublicationFailure(FailureProvenance);

impl std::fmt::Display for PublicationFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "publication failed: {:?}", self.0.dominant)
    }
}

impl std::error::Error for PublicationFailure {}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct CommittedPublicationFailure {
    root: ObjectId,
    transition: ObjectId,
    cause: FailureCause,
}

impl std::fmt::Display for CommittedPublicationFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "publication committed as {}/{}; later verification failed: {:?}",
            self.root, self.transition, self.cause
        )
    }
}

impl std::error::Error for CommittedPublicationFailure {}

fn committed_result<T>(root: ObjectId, transition: ObjectId, result: AnyResult<T>) -> AnyResult<T> {
    result.map_err(|error| {
        CommittedPublicationFailure {
            root,
            transition,
            cause: failure_cause(error.as_ref()),
        }
        .into()
    })
}

fn failure_provenance(
    first: Option<FailureCause>,
    cleanup_first: Option<FailureCause>,
    reconciliation: Reconciliation,
    reconciliation_error: Option<FailureCause>,
) -> FailureProvenance {
    let dominant = match reconciliation {
        Reconciliation::NotAttempted | Reconciliation::PriorVisible => first,
        Reconciliation::RequestedVisible => None,
        Reconciliation::DifferentHead => Some(FailureCause::Core(CoreError::PublicationConflict)),
        Reconciliation::Ambiguous => Some(FailureCause::Core(CoreError::AmbiguousDurability)),
    };
    FailureProvenance {
        first,
        cleanup_first,
        reconciliation,
        reconciliation_error,
        dominant,
    }
}

#[derive(Clone, Copy, Debug)]
struct Candidate {
    name: &'static str,
    k: usize,
    f: usize,
    directory_page: usize,
}

const FILE_CANDIDATES: [Candidate; 3] = [
    Candidate {
        name: "K64-F64",
        k: 64,
        f: 64,
        directory_page: 256 * 1024,
    },
    Candidate {
        name: "K59-F101",
        k: 59,
        f: 101,
        directory_page: 256 * 1024,
    },
    Candidate {
        name: "K256-F256",
        k: 256,
        f: 256,
        directory_page: 256 * 1024,
    },
];

const DIR_CANDIDATES: [Candidate; 3] = [
    Candidate {
        name: "DIR64K",
        k: 64,
        f: 64,
        directory_page: 64 * 1024,
    },
    Candidate {
        name: "DIR256K",
        k: 64,
        f: 64,
        directory_page: 256 * 1024,
    },
    Candidate {
        name: "DIR1M",
        k: 64,
        f: 64,
        directory_page: 1024 * 1024,
    },
];

#[derive(Clone, Copy, Debug, Default)]
struct Metrics {
    statement_cache_acquisitions: u64,
    sql_query_calls: u64,
    sql_execute_calls: u64,
    sql_rows_returned: u64,
    sql_rows_changed: u64,
    blob_opens: u64,
    blob_reads: u64,
    blob_writes: u64,
    row_blob_reads: u64,
    row_blob_writes: u64,
    row_blob_copy_bytes: u64,
    borrowed_row_blob_reads: u64,
    borrowed_row_blob_bytes: u64,
    transactions: u64,
    commits: u64,
    commit_returns: u64,
    commit_return_successes: u64,
    commit_return_errors: u64,
    commit_reconciliation_calls: u64,
    commit_publish_call_wall_ns: u128,
    commit_dispatch_to_return_wall_ns: u128,
    commit_reconciliation_wall_ns: u128,
    measurement_sql_queries: u64,
    measurement_sql_rows: u64,
    measurement_status_reset_calls: u64,
    measurement_status_reset_errors: u64,
    sqlite_page_size: Option<u64>,
    sqlite_status_before: SqliteStatusSnapshot,
    sqlite_status_before_dispatch: SqliteStatusSnapshot,
    sqlite_status_after_return: SqliteStatusSnapshot,
    commit_dispatch_filesystem: PhysicalSnapshot,
    commit_return_filesystem: PhysicalSnapshot,
    objects_created: u64,
    objects_reused: u64,
    objects_authenticated: u64,
    canonical_bytes_authenticated: u64,
    canonical_bytes_written: u64,
    mapping_bytes_rewritten: u64,
    closure_occurrences: u64,
    chunks: u64,
    references: u64,
    pages: u64,
    branches: u64,
    suffix_references: u64,
    suffix_bytes: u64,
    suffix_objects: u64,
    q_current: u64,
    q_high_water: u64,
    q_cdc_base_live_bytes: u64,
    q_cdc_old_window_bytes: u64,
    q_cdc_old_chunk_slots_bytes: u64,
    q_cdc_scan_input_bytes: u64,
    q_cdc_overlap_current: u64,
    leaf_batch_queries: u64,
    leaf_batch_references: u64,
    leaf_batch_references_max: u64,
    leaf_batch_query_bytes_max: u64,
    source_bytes_read: u64,
    source_cdc_bytes_read: u64,
    canonical_stage_source_bytes_read: u64,
    raw_bytes_hashed: u64,
    raw_hashes: u64,
    canonical_id_bytes_hashed: u64,
    canonical_id_hashes: u64,
    reused_object_id_authentications: u64,
    reused_object_id_authentication_bytes: u64,
    borrowed_bytes_encode_calls: u64,
    borrowed_bytes_encode_input_bytes: u64,
    borrowed_source_encode_calls: u64,
    borrowed_source_encode_input_bytes: u64,
    incremental_qualification_calls: u64,
    incremental_prior_spine_objects_authenticated: u64,
    incremental_prior_spine_bytes_authenticated: u64,
    incremental_replacement_spine_objects_authenticated: u64,
    incremental_replacement_spine_bytes_authenticated: u64,
    incremental_receipt_covered_edges: u64,
    incremental_new_or_different_edges: u64,
    incremental_new_subtree_objects_authenticated: u64,
    incremental_new_subtree_bytes_authenticated: u64,
    construction_put_evidences: u64,
    construction_edges_covered: u64,
    construction_leaf_summaries: u64,
    construction_branch_summaries: u64,
    construction_file_summaries: u64,
    construction_workspace_summaries: u64,
    construction_transition_summaries: u64,
    construction_proof_consumptions: u64,
    construction_source_hash_bytes: u64,
    construction_source_hashes: u64,
    construction_cdc_entries: u64,
    w_bytes: u64,
    d_bytes: u64,
}

#[derive(Clone, Debug, Default)]
struct PhaseTimes {
    same_open_authority_establishment_ns: u128,
    canonical_cas_mapping_stage_ns: u128,
    precommit_closure_validation_ns: u128,
    sqlite_commit_durability_ns: u128,
    durable_capture_total_ns: u128,
    fresh_reopen_head_ns: u128,
    fresh_full_scrub_ns: u128,
    reconstruction_ns: u128,
    range_verification_ns: u128,
    complete_lifecycle_total_ns: u128,
}

type PhaseMetricInterval = (&'static str, Metrics, Metrics);

thread_local! {
    static Q_CURRENT: Cell<u64> = const { Cell::new(0) };
}

#[derive(Debug)]
struct CapacityCharge(u64);

impl CapacityCharge {
    fn absorb(&mut self, mut other: Self) -> CoreResult<()> {
        self.0 = self
            .0
            .checked_add(other.0)
            .ok_or(CoreError::LengthOverflow)?;
        other.0 = 0;
        Ok(())
    }
}

impl Drop for CapacityCharge {
    fn drop(&mut self) {
        Q_CURRENT.with(|current| {
            current.set(
                current
                    .get()
                    .checked_sub(self.0)
                    .expect("logical Q charge/decharge imbalance"),
            );
        });
    }
}

fn charge_capacity(metrics: &mut Metrics, capacity: usize) -> CoreResult<CapacityCharge> {
    let capacity = u64::try_from(capacity).map_err(|_| CoreError::LengthOverflow)?;
    let current = Q_CURRENT.with(|current| {
        let next = current
            .get()
            .checked_add(capacity)
            .ok_or(CoreError::LengthOverflow)?;
        if next > layerfs_core::limits::MAX_DURABLE_LIVE_ALLOCATION {
            return Err(CoreError::AllocationBudgetExceeded);
        }
        current.set(next);
        Ok(next)
    })?;
    metrics.q_current = current;
    metrics.q_high_water = metrics.q_high_water.max(current);
    Ok(CapacityCharge(capacity))
}

fn q_current() -> u64 {
    Q_CURRENT.with(Cell::get)
}

fn finish_q(metrics: &mut Metrics) -> CoreResult<()> {
    metrics.q_current = q_current();
    if metrics.q_current != 0 {
        return Err(CoreError::LengthMismatch {
            expected: 0,
            actual: metrics.q_current,
        });
    }
    Ok(())
}

fn validate_metric_equations(metrics: Metrics) -> CoreResult<()> {
    if metrics.commits > metrics.transactions
        || metrics.commit_returns > metrics.commits
        || metrics
            .commit_return_successes
            .checked_add(metrics.commit_return_errors)
            .ok_or(CoreError::LengthOverflow)?
            != metrics.commit_returns
        || metrics.commit_reconciliation_calls > metrics.commit_returns
        || metrics.commit_dispatch_to_return_wall_ns > metrics.commit_publish_call_wall_ns
        || metrics.commit_reconciliation_wall_ns
            > metrics
                .commit_publish_call_wall_ns
                .checked_sub(metrics.commit_dispatch_to_return_wall_ns)
                .ok_or(CoreError::LengthOverflow)?
        || metrics.measurement_sql_rows > metrics.measurement_sql_queries
        || metrics.measurement_status_reset_errors > metrics.measurement_status_reset_calls
        || metrics.sqlite_status_before.errors > metrics.sqlite_status_before.read_calls
        || metrics.sqlite_status_before_dispatch.errors
            > metrics.sqlite_status_before_dispatch.read_calls
        || metrics.sqlite_status_after_return.errors > metrics.sqlite_status_after_return.read_calls
        || metrics.borrowed_row_blob_reads > metrics.row_blob_reads
        || metrics.borrowed_row_blob_bytes > metrics.canonical_bytes_authenticated
        || metrics.incremental_new_subtree_objects_authenticated > metrics.objects_authenticated
        || metrics.incremental_new_subtree_bytes_authenticated
            > metrics.canonical_bytes_authenticated
        || metrics.construction_proof_consumptions > 1
    {
        return Err(CoreError::LengthMismatch {
            expected: 0,
            actual: 1,
        });
    }
    metrics
        .canonical_bytes_written
        .checked_add(
            metrics
                .canonical_bytes_authenticated
                .checked_sub(metrics.canonical_bytes_written)
                .ok_or(CoreError::LengthOverflow)?,
        )
        .filter(|total| *total == metrics.canonical_bytes_authenticated)
        .ok_or(CoreError::LengthOverflow)?;
    metrics
        .sql_query_calls
        .checked_add(metrics.sql_execute_calls)
        .ok_or(CoreError::LengthOverflow)?;
    metrics
        .blob_reads
        .checked_add(metrics.blob_writes)
        .ok_or(CoreError::LengthOverflow)?;
    Ok(())
}

#[derive(Debug)]
struct ChargedBytes {
    bytes: ChargedVec<u8>,
}

impl ChargedBytes {
    fn from_borrowed(bytes: &[u8], metrics: &mut Metrics) -> CoreResult<Self> {
        let mut owned = ChargedVec::with_capacity(bytes.len(), metrics)?;
        owned.extend_from_slice(bytes);
        Ok(Self { bytes: owned })
    }
}

impl Deref for ChargedBytes {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        &self.bytes
    }
}

impl PartialEq<Vec<u8>> for ChargedBytes {
    fn eq(&self, other: &Vec<u8>) -> bool {
        self.bytes.as_slice() == other
    }
}

#[derive(Debug)]
struct ChargedVec<T> {
    values: Vec<T>,
    _requested: CapacityCharge,
}

impl<T> ChargedVec<T> {
    fn with_capacity(capacity: usize, metrics: &mut Metrics) -> CoreResult<Self> {
        let requested_bytes = capacity
            .checked_mul(std::mem::size_of::<T>())
            .ok_or(CoreError::LengthOverflow)?;
        let requested = charge_capacity(metrics, requested_bytes)?;
        let mut values = Vec::new();
        values
            .try_reserve_exact(capacity)
            .map_err(|_| CoreError::AllocationFailed)?;
        if values.capacity() != capacity {
            return Err(CoreError::AllocationFailed);
        }
        Ok(Self {
            values,
            _requested: requested,
        })
    }

    fn from_exact_builder(
        capacity: usize,
        metrics: &mut Metrics,
        build: impl FnOnce() -> CoreResult<Vec<T>>,
    ) -> CoreResult<Self> {
        let requested_bytes = capacity
            .checked_mul(std::mem::size_of::<T>())
            .ok_or(CoreError::LengthOverflow)?;
        let requested = charge_capacity(metrics, requested_bytes)?;
        let values = build()?;
        if values.len() != capacity {
            return Err(CoreError::LengthMismatch {
                expected: u64::try_from(capacity).map_err(|_| CoreError::LengthOverflow)?,
                actual: u64::try_from(values.len()).map_err(|_| CoreError::LengthOverflow)?,
            });
        }
        if values.capacity() != capacity {
            return Err(CoreError::AllocationFailed);
        }
        Ok(Self {
            values,
            _requested: requested,
        })
    }

    fn with_item_charge(
        capacity: usize,
        bytes_per_item: usize,
        metrics: &mut Metrics,
    ) -> CoreResult<Self> {
        let requested = charge_capacity(
            metrics,
            capacity
                .checked_mul(bytes_per_item)
                .ok_or(CoreError::LengthOverflow)?,
        )?;
        let mut values = Vec::new();
        values
            .try_reserve_exact(capacity)
            .map_err(|_| CoreError::AllocationFailed)?;
        Ok(Self {
            values,
            _requested: requested,
        })
    }
}

impl<T> Deref for ChargedVec<T> {
    type Target = Vec<T>;

    fn deref(&self) -> &Self::Target {
        &self.values
    }
}

impl<T> std::ops::DerefMut for ChargedVec<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.values
    }
}

struct CountingWriter(usize);

impl std::fmt::Write for CountingWriter {
    fn write_str(&mut self, value: &str) -> std::fmt::Result {
        self.0 = self.0.checked_add(value.len()).ok_or(std::fmt::Error)?;
        Ok(())
    }
}

trait RowOutput: std::fmt::Write {
    fn remove_last_object_brace(&mut self) -> std::fmt::Result;
}

impl RowOutput for CountingWriter {
    fn remove_last_object_brace(&mut self) -> std::fmt::Result {
        self.0 = self.0.checked_sub(1).ok_or(std::fmt::Error)?;
        Ok(())
    }
}

impl RowOutput for String {
    fn remove_last_object_brace(&mut self) -> std::fmt::Result {
        (self.pop() == Some('}'))
            .then_some(())
            .ok_or(std::fmt::Error)
    }
}

struct CompactStatusWriter<'a, W>(&'a mut W);

impl<W: std::fmt::Write> std::fmt::Write for CompactStatusWriter<'_, W> {
    fn write_str(&mut self, mut value: &str) -> std::fmt::Result {
        while !value.is_empty() {
            let next = F1_ROW_STATUS_REPLACEMENTS
                .iter()
                .filter_map(|(from, to)| value.find(from).map(|index| (index, *from, *to)))
                .min_by_key(|(index, _, _)| *index);
            let Some((index, from, to)) = next else {
                return self.0.write_str(value);
            };
            self.0.write_str(&value[..index])?;
            self.0.write_str(to)?;
            value = &value[index + from.len()..];
        }
        Ok(())
    }
}

struct ChargedString {
    value: String,
    _charge: CapacityCharge,
}

impl ChargedString {
    fn with_capacity(capacity: usize, metrics: &mut Metrics) -> CoreResult<Self> {
        let charge = charge_capacity(metrics, capacity)?;
        let mut value = String::new();
        value
            .try_reserve_exact(capacity)
            .map_err(|_| CoreError::AllocationFailed)?;
        if value.capacity() != capacity {
            return Err(CoreError::AllocationFailed);
        }
        Ok(Self {
            value,
            _charge: charge,
        })
    }

    fn from_str(value: &str, metrics: &mut Metrics) -> CoreResult<Self> {
        let mut output = Self::with_capacity(value.len(), metrics)?;
        output.value.push_str(value);
        Ok(output)
    }
}

impl Deref for ChargedString {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        &self.value
    }
}

impl std::fmt::Display for ChargedString {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.value)
    }
}

fn encode_charged_bytes_object(value: &[u8], metrics: &mut Metrics) -> CoreResult<ChargedVec<u8>> {
    let capacity = value
        .len()
        .checked_add(layerfs_core::object::HEADER_LEN + 4)
        .ok_or(CoreError::LengthOverflow)?;
    let mut canonical = ChargedVec::with_capacity(capacity, metrics)?;
    encode_bytes_object_to(value, &mut *canonical)?;
    if canonical.len() != capacity {
        return Err(CoreError::LengthMismatch {
            expected: u64::try_from(capacity).map_err(|_| CoreError::LengthOverflow)?,
            actual: u64::try_from(canonical.len()).map_err(|_| CoreError::LengthOverflow)?,
        });
    }
    Ok(canonical)
}

const Q_FILE_REFERENCE_BYTES: usize = 96;
const Q_TREE_NODE_BYTES: usize = 256;
const Q_DFS_FRAME_BYTES: usize = 64;

fn u32_field(bytes: &[u8], offset: usize) -> CoreResult<usize> {
    let end = offset.checked_add(4).ok_or(CoreError::LengthOverflow)?;
    usize::try_from(u32::from_be_bytes(
        bytes
            .get(offset..end)
            .ok_or(CoreError::UnexpectedEof)?
            .try_into()
            .map_err(|_| CoreError::UnexpectedEof)?,
    ))
    .map_err(|_| CoreError::LengthOverflow)
}

fn charge_decoded_file_references(
    payload: &[u8],
    metrics: &mut Metrics,
) -> CoreResult<CapacityCharge> {
    let count = u32_field(payload, 0)?;
    charge_capacity(
        metrics,
        count
            .checked_mul(Q_FILE_REFERENCE_BYTES)
            .ok_or(CoreError::LengthOverflow)?,
    )
}

fn charge_decoded_file_children(
    payload: &[u8],
    root: bool,
    metrics: &mut Metrics,
) -> CoreResult<CapacityCharge> {
    let count = u32_field(payload, if root { 4 + 8 + 8 + 1 } else { 1 })?;
    charge_capacity(
        metrics,
        count
            .checked_mul(Q_TREE_NODE_BYTES)
            .ok_or(CoreError::LengthOverflow)?,
    )
}

fn charge_dfs_frames(capacity: usize, metrics: &mut Metrics) -> CoreResult<CapacityCharge> {
    charge_capacity(
        metrics,
        capacity
            .checked_mul(Q_DFS_FRAME_BYTES)
            .ok_or(CoreError::LengthOverflow)?,
    )
}

fn checked_mapping_len(body_len: usize) -> CoreResult<usize> {
    11usize
        .checked_add(body_len)
        .ok_or(CoreError::LengthOverflow)
}

fn encode_charged_file_leaf(
    references: &[file_codec::FileReference],
    metrics: &mut Metrics,
) -> CoreResult<ChargedVec<u8>> {
    let body_len = 4usize
        .checked_add(
            references
                .len()
                .checked_mul(file_codec::FILE_REF_BYTES)
                .ok_or(CoreError::LengthOverflow)?,
        )
        .ok_or(CoreError::LengthOverflow)?;
    ChargedVec::from_exact_builder(checked_mapping_len(body_len)?, metrics, || {
        file_codec::encode_file_leaf(references)
    })
}

fn encode_charged_file_branch(
    level: u8,
    children: &[file_codec::FileChild],
    metrics: &mut Metrics,
) -> CoreResult<ChargedVec<u8>> {
    let body_len = 5usize
        .checked_add(
            children
                .len()
                .checked_mul(file_codec::FILE_DESCRIPTOR_BYTES)
                .ok_or(CoreError::LengthOverflow)?,
        )
        .ok_or(CoreError::LengthOverflow)?;
    ChargedVec::from_exact_builder(checked_mapping_len(body_len)?, metrics, || {
        file_codec::encode_file_branch(level, children)
    })
}

fn encode_charged_file_root(
    mode: u32,
    total_raw: u64,
    reference_count: u64,
    level: u8,
    children: &[file_codec::FileChild],
    metrics: &mut Metrics,
) -> CoreResult<ChargedVec<u8>> {
    let body_len = 25usize
        .checked_add(
            children
                .len()
                .checked_mul(file_codec::FILE_DESCRIPTOR_BYTES)
                .ok_or(CoreError::LengthOverflow)?,
        )
        .ok_or(CoreError::LengthOverflow)?;
    ChargedVec::from_exact_builder(checked_mapping_len(body_len)?, metrics, || {
        file_codec::encode_file_root(mode, total_raw, reference_count, level, children)
    })
}

fn transition_operation_encoded_len(
    operation: &delta_codec::TransitionOperation,
) -> CoreResult<usize> {
    let (path, fixed): (&Vec<u8>, usize) = match operation {
        delta_codec::TransitionOperation::Add { path, .. }
        | delta_codec::TransitionOperation::Remove { path, .. } => (path, 1 + 4 + 32),
        delta_codec::TransitionOperation::Replace { path, .. } => (path, 1 + 4 + 64),
        delta_codec::TransitionOperation::Metadata { path, .. } => (path, 1 + 4 + 72),
    };
    fixed
        .checked_add(path.len())
        .ok_or(CoreError::LengthOverflow)
}

fn encode_charged_delta_page(
    operations: &[delta_codec::TransitionOperation],
    metrics: &mut Metrics,
) -> CoreResult<ChargedVec<u8>> {
    let body_len = operations.iter().try_fold(4usize, |total, operation| {
        total
            .checked_add(transition_operation_encoded_len(operation)?)
            .ok_or(CoreError::LengthOverflow)
    })?;
    ChargedVec::from_exact_builder(checked_mapping_len(body_len)?, metrics, || {
        delta_codec::encode_delta_page(operations)
    })
}

fn encode_charged_transition(
    parent: Option<ObjectId>,
    child: ObjectId,
    entry_count: u32,
    pages: &[ObjectId],
    metrics: &mut Metrics,
) -> CoreResult<ChargedVec<u8>> {
    let body_len = 1usize
        .checked_add(usize::from(parent.is_some()) * 32)
        .and_then(|value| value.checked_add(32 + 4 + 4))
        .and_then(|value| value.checked_add(pages.len().checked_mul(32)?))
        .ok_or(CoreError::LengthOverflow)?;
    ChargedVec::from_exact_builder(checked_mapping_len(body_len)?, metrics, || match parent {
        Some(parent) => delta_codec::encode_change(parent, child, entry_count, pages),
        None => delta_codec::encode_genesis(child),
    })
}

fn encode_charged_directory_metadata(
    mode: u32,
    metrics: &mut Metrics,
) -> CoreResult<ChargedVec<u8>> {
    ChargedVec::from_exact_builder(checked_mapping_len(4)?, metrics, || {
        dir_codec::encode_directory_metadata(mode)
    })
}

fn encode_charged_directory_index(
    total_entries: u32,
    pages: &[dir_codec::DirectoryPageRef],
    metrics: &mut Metrics,
) -> CoreResult<ChargedVec<u8>> {
    let body_len = pages.iter().try_fold(8usize, |total, page| {
        total
            .checked_add(4 + 2 + 32)
            .and_then(|value| value.checked_add(page.first_name.len()))
            .ok_or(CoreError::LengthOverflow)
    })?;
    ChargedVec::from_exact_builder(checked_mapping_len(body_len)?, metrics, || {
        dir_codec::encode_directory_index(total_entries, pages)
    })
}

fn decoded_object_q(bytes: &[u8]) -> CoreResult<usize> {
    if bytes.len() < layerfs_core::object::HEADER_LEN {
        return Err(CoreError::UnexpectedEof);
    }
    match ObjectKind::try_from(bytes[4])? {
        ObjectKind::Bytes => layerfs_core::decode_bytes_object(bytes)?
            .len()
            .checked_add(256)
            .ok_or(CoreError::LengthOverflow),
        ObjectKind::Directory => {
            let mut offset = layerfs_core::object::HEADER_LEN;
            let count_end = offset.checked_add(4).ok_or(CoreError::LengthOverflow)?;
            let count = usize::try_from(u32::from_be_bytes(
                bytes
                    .get(offset..count_end)
                    .ok_or(CoreError::UnexpectedEof)?
                    .try_into()
                    .map_err(|_| CoreError::UnexpectedEof)?,
            ))
            .map_err(|_| CoreError::LengthOverflow)?;
            offset = count_end;
            let mut charge = 256_usize;
            for _ in 0..count {
                let length_end = offset.checked_add(4).ok_or(CoreError::LengthOverflow)?;
                let name_len = usize::try_from(u32::from_be_bytes(
                    bytes
                        .get(offset..length_end)
                        .ok_or(CoreError::UnexpectedEof)?
                        .try_into()
                        .map_err(|_| CoreError::UnexpectedEof)?,
                ))
                .map_err(|_| CoreError::LengthOverflow)?;
                offset = length_end
                    .checked_add(name_len)
                    .and_then(|value| value.checked_add(1 + 32))
                    .ok_or(CoreError::LengthOverflow)?;
                if offset > bytes.len() {
                    return Err(CoreError::UnexpectedEof);
                }
                charge = charge
                    .checked_add(256)
                    .and_then(|value| value.checked_add(name_len))
                    .ok_or(CoreError::LengthOverflow)?;
            }
            Ok(charge)
        }
    }
}

fn charged_replace_operation(
    path: &[u8],
    before: ObjectId,
    after: ObjectId,
    metrics: &mut Metrics,
) -> CoreResult<(Vec<delta_codec::TransitionOperation>, CapacityCharge)> {
    let requested = 256usize
        .checked_add(path.len())
        .ok_or(CoreError::LengthOverflow)?;
    let charge = charge_capacity(metrics, requested)?;
    let mut owned_path = Vec::new();
    owned_path
        .try_reserve_exact(path.len())
        .map_err(|_| CoreError::AllocationFailed)?;
    owned_path.extend_from_slice(path);
    let mut operations = Vec::new();
    operations
        .try_reserve_exact(1)
        .map_err(|_| CoreError::AllocationFailed)?;
    operations.push(delta_codec::TransitionOperation::Replace {
        path: owned_path,
        before,
        after,
    });
    Ok((operations, charge))
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct PhysicalSnapshot {
    logical_database: Option<u64>,
    apparent_database: Option<u64>,
    apparent_journal: Option<u64>,
    apparent_authority: Option<u64>,
    allocated_database: Option<u64>,
    allocated_journal: Option<u64>,
    allocated_authority: Option<u64>,
    measurement_sql_queries: u64,
    measurement_sql_rows: u64,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct SqliteStatusSnapshot {
    page_cache_used_bytes: Option<u64>,
    cache_hits: Option<u64>,
    cache_misses: Option<u64>,
    dirty_pages_written: Option<u64>,
    cache_spill_pages: Option<u64>,
    read_calls: u64,
    errors: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SqliteStatusError {
    Sqlite(i32),
    CurrentOutOfRange(i32),
}

fn checked_sqlite_status_current(result: i32, current: i32) -> Result<u64, SqliteStatusError> {
    if result != ffi::SQLITE_OK {
        return Err(SqliteStatusError::Sqlite(result));
    }
    u64::try_from(current).map_err(|_| SqliteStatusError::CurrentOutOfRange(current))
}

impl PhysicalSnapshot {
    fn logical_store(self) -> Option<u64> {
        self.logical_database?
            .checked_add(self.apparent_journal?)?
            .checked_add(self.apparent_authority?)
    }

    fn apparent_store(self) -> Option<u64> {
        self.apparent_database?
            .checked_add(self.apparent_journal?)?
            .checked_add(self.apparent_authority?)
    }

    fn allocated_store(self) -> Option<u64> {
        self.allocated_database?
            .checked_add(self.allocated_journal?)?
            .checked_add(self.allocated_authority?)
    }
}

#[derive(Clone, Debug)]
struct RangeMeasurement {
    label: &'static str,
    range: std::ops::Range<u64>,
    wall_ns: u128,
    returned_bytes: usize,
    canonical_bytes_authenticated: u64,
    objects_authenticated: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct EditPoint {
    reference_count: u64,
    position: u64,
    byte_offset: u64,
    replacement_length: usize,
}

#[derive(Clone, Debug)]
struct PreparedExpectations {
    source_length: u64,
    source_fingerprint: String,
    edit_point: Option<EditPoint>,
    expected_reference_count: Option<u64>,
    expected_fingerprint: Option<String>,
    expected_sequence: Option<String>,
    expected_ranges: Vec<Vec<u8>>,
    expected_probes: Vec<(&'static str, std::ops::Range<u64>)>,
    base: Option<(ObjectId, ObjectId, [u8; 32])>,
    edit_oracle: Option<PreparedEditOracle>,
}

struct ChargedPreparedExpectations {
    value: PreparedExpectations,
    _charge: CapacityCharge,
}

fn require_amended_m45_expectations(
    candidate: Candidate,
    size: u64,
    operation: &str,
    expected: &PreparedExpectations,
) -> AnyResult<()> {
    if candidate.name != "K64-F64" || size != SOURCE_100 || operation != "same-middle" {
        return Ok(());
    }
    let point = expected.edit_point.ok_or(CoreError::PublicationConflict)?;
    let base = expected.base.ok_or(CoreError::PublicationConflict)?;
    let oracle = expected
        .edit_oracle
        .as_ref()
        .ok_or(CoreError::PublicationConflict)?;
    let expected_base = (
        AMENDED_M45_BASE_ROOT.parse::<ObjectId>()?,
        AMENDED_M45_BASE_TRANSITION.parse::<ObjectId>()?,
        AMENDED_M45_BASE_CLOSURE.parse::<ObjectId>()?.to_bytes(),
    );
    if expected.source_length != SOURCE_100
        || expected.source_fingerprint != RETAINED_RAW_100
        || point.reference_count != RETAINED_CDC_100
        || point.position != AMENDED_M45_EDIT_POSITION
        || point.byte_offset != AMENDED_M45_EDIT_OFFSET
        || point.replacement_length != AMENDED_M45_EDIT_LENGTH
        || expected.expected_reference_count != Some(RETAINED_CDC_100)
        || expected.expected_fingerprint.as_deref() != Some(AMENDED_M45_EDITED_FINGERPRINT)
        || expected.expected_sequence.as_deref() != Some(AMENDED_M45_CDC_SEQUENCE)
        || base != expected_base
        || oracle.operation != "same-middle"
        || oracle.offset != AMENDED_M45_EDIT_OFFSET
        || oracle.removed.len() != AMENDED_M45_EDIT_LENGTH
        || !is_same_middle_replacement(&oracle.removed, &oracle.inserted)
        || oracle.before_file != AMENDED_M45_BEFORE_FILE.parse::<ObjectId>()?
        || oracle.after_file != AMENDED_M45_AFTER_FILE.parse::<ObjectId>()?
        || oracle.result_root != AMENDED_M45_RESULT_ROOT.parse::<ObjectId>()?
        || oracle.result_transition != AMENDED_M45_RESULT_TRANSITION.parse::<ObjectId>()?
        || oracle.result_closure != AMENDED_M45_RESULT_CLOSURE.parse::<ObjectId>()?.to_bytes()
    {
        return Err(CoreError::PublicationConflict.into());
    }
    Ok(())
}

fn prepared_expectations_capacity(expected: &PreparedExpectations) -> CoreResult<usize> {
    let mut capacity = expected.source_fingerprint.capacity();
    for value in [
        expected.expected_fingerprint.as_ref(),
        expected.expected_sequence.as_ref(),
    ]
    .into_iter()
    .flatten()
    {
        capacity = capacity
            .checked_add(value.capacity())
            .ok_or(CoreError::LengthOverflow)?;
    }
    capacity = capacity
        .checked_add(
            expected
                .expected_ranges
                .capacity()
                .checked_mul(std::mem::size_of::<Vec<u8>>())
                .ok_or(CoreError::LengthOverflow)?,
        )
        .and_then(|value| {
            value.checked_add(
                expected
                    .expected_probes
                    .capacity()
                    .checked_mul(std::mem::size_of::<(&'static str, std::ops::Range<u64>)>())?,
            )
        })
        .ok_or(CoreError::LengthOverflow)?;
    for bytes in &expected.expected_ranges {
        capacity = capacity
            .checked_add(bytes.capacity())
            .ok_or(CoreError::LengthOverflow)?;
    }
    if let Some(oracle) = &expected.edit_oracle {
        capacity = capacity
            .checked_add(oracle.operation.capacity())
            .and_then(|value| value.checked_add(oracle.removed.capacity()))
            .and_then(|value| value.checked_add(oracle.inserted.capacity()))
            .ok_or(CoreError::LengthOverflow)?;
    }
    Ok(capacity)
}

#[derive(Clone, Debug)]
struct PreparedEditOracle {
    operation: String,
    offset: u64,
    removed: Vec<u8>,
    inserted: Vec<u8>,
    before_file: ObjectId,
    after_file: ObjectId,
    result_root: ObjectId,
    result_transition: ObjectId,
    result_closure: [u8; 32],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ExpectedEditResult {
    before_file: ObjectId,
    after_file: ObjectId,
    root: ObjectId,
    transition: ObjectId,
    closure: [u8; 32],
}

impl PreparedEditOracle {
    fn result(&self) -> ExpectedEditResult {
        ExpectedEditResult {
            before_file: self.before_file,
            after_file: self.after_file,
            root: self.result_root,
            transition: self.result_transition,
            closure: self.result_closure,
        }
    }
}

#[derive(Debug)]
struct SameOpenValidationWitness {
    open_identity: u64,
    store_instance_id: [u8; 16],
    validation_authority_id: [u8; 32],
    integrity_epoch: u64,
    profile: [u8; 32],
    generation: u64,
    root: ObjectId,
    transition: ObjectId,
    receipt: [u8; 216],
    authority_serial: u64,
    transaction_identity: u64,
    consumed: bool,
    _receipt_charge: CapacityCharge,
}

#[derive(Debug)]
struct SameOpenValidationPermit {
    open_identity: u64,
    store_instance_id: [u8; 16],
    validation_authority_id: [u8; 32],
    integrity_epoch: u64,
    profile: [u8; 32],
    generation: u64,
    root: ObjectId,
    transition: ObjectId,
    receipt: [u8; 216],
    authority_serial: u64,
    transaction_identity: u64,
    _receipt_charge: CapacityCharge,
}

#[repr(C)]
#[derive(Debug)]
struct PutEvidence {
    object_id: ObjectId,
    canonical_len: u64,
    open_identity: u64,
    transaction_identity: u64,
    authority_serial: u64,
    mutation_serial: u64,
    kind: ObjectKind,
}

#[repr(C)]
#[derive(Debug)]
struct ConstructionNodeProof {
    object_id: ObjectId,
    total_raw: u64,
    references: u64,
    transaction_identity: u64,
    level: u8,
}

impl SameOpenValidationWitness {
    fn consume(
        &mut self,
        store: &Store,
        metrics: &mut Metrics,
    ) -> AnyResult<SameOpenValidationPermit> {
        if self.consumed {
            return Err(CoreError::ValidationAuthorityUnavailable.into());
        }
        self.consumed = true;
        if self.open_identity != store.open_identity
            || self.authority_serial != store.same_open_authority_serial
            || store
                .active_transaction
                .map(|transaction| transaction.identity)
                != Some(self.transaction_identity)
        {
            return Err(CoreError::ValidationAuthorityUnavailable.into());
        }
        if self.store_instance_id != store.store_instance_id
            || self.validation_authority_id != store.validation_authority_id
            || self.integrity_epoch != store.integrity_epoch
            || self.profile != store.profile
        {
            return Err(CoreError::InvalidValidationReceipt.into());
        }
        let _head_receipt_charge = charge_capacity(metrics, 216)?;
        let current = store
            .current_head_accounted(metrics)?
            .ok_or(CoreError::InvalidValidationReceipt)?;
        if current.0 != self.generation
            || current.1 != self.root
            || current.2 != self.transition
            || current.3.as_slice() != self.receipt
        {
            return Err(CoreError::InvalidValidationReceipt.into());
        }
        let receipt_charge = charge_capacity(metrics, 216)?;
        Ok(SameOpenValidationPermit {
            open_identity: self.open_identity,
            store_instance_id: self.store_instance_id,
            validation_authority_id: self.validation_authority_id,
            integrity_epoch: self.integrity_epoch,
            profile: self.profile,
            generation: self.generation,
            root: self.root,
            transition: self.transition,
            receipt: self.receipt,
            authority_serial: self.authority_serial,
            transaction_identity: self.transaction_identity,
            _receipt_charge: receipt_charge,
        })
    }
}

impl SameOpenValidationPermit {
    fn covers(&self, store: &Store, head: &VisibleHead) -> bool {
        self.open_identity == store.open_identity
            && self.store_instance_id == store.store_instance_id
            && self.validation_authority_id == store.validation_authority_id
            && self.integrity_epoch == store.integrity_epoch
            && self.profile == store.profile
            && self.generation == head.0
            && self.root == head.1
            && self.transition == head.2
            && head.3.as_slice() == self.receipt
            && self.authority_serial == store.same_open_authority_serial
            && store
                .active_transaction
                .map(|transaction| transaction.identity)
                == Some(self.transaction_identity)
    }
}

#[derive(Clone, Copy, Debug)]
struct ActiveTransaction {
    identity: u64,
    witness_issued: bool,
    construction_proof_issued: bool,
    construction_proof_consumed: bool,
}

fn add(value: &mut u64, amount: u64) -> CoreResult<()> {
    *value = value.checked_add(amount).ok_or(CoreError::LengthOverflow)?;
    Ok(())
}

fn add_len(value: &mut u64, amount: usize) -> CoreResult<()> {
    add(
        value,
        u64::try_from(amount).map_err(|_| CoreError::LengthOverflow)?,
    )
}

fn observe_statement_cache_acquisition(metrics: &mut Metrics) -> CoreResult<()> {
    add(&mut metrics.statement_cache_acquisitions, 1)
}

fn observe_query_call(metrics: &mut Metrics) -> CoreResult<()> {
    add(&mut metrics.sql_query_calls, 1)
}

fn observe_execute_call(metrics: &mut Metrics, rows_changed: usize) -> CoreResult<()> {
    add(&mut metrics.sql_execute_calls, 1)?;
    add(
        &mut metrics.sql_rows_changed,
        u64::try_from(rows_changed).map_err(|_| CoreError::LengthOverflow)?,
    )
}

fn observe_rows_returned(metrics: &mut Metrics, rows: u64) -> CoreResult<()> {
    add(&mut metrics.sql_rows_returned, rows)
}

fn observe_row_blobs(metrics: &mut Metrics, lengths: &[usize]) -> CoreResult<()> {
    add(
        &mut metrics.row_blob_reads,
        u64::try_from(lengths.len()).map_err(|_| CoreError::LengthOverflow)?,
    )?;
    for &length in lengths {
        add_len(&mut metrics.row_blob_copy_bytes, length)?;
    }
    Ok(())
}

fn observe_borrowed_row_blob(metrics: &mut Metrics, length: usize) -> CoreResult<()> {
    add(&mut metrics.row_blob_reads, 1)?;
    add(&mut metrics.borrowed_row_blob_reads, 1)?;
    add_len(&mut metrics.borrowed_row_blob_bytes, length)
}

fn observe_file_references(metrics: &mut Metrics, count: usize) -> CoreResult<()> {
    let _ = metrics;
    let _ = u64::try_from(count).map_err(|_| CoreError::LengthOverflow)?;
    Ok(())
}

fn observe_file_references_and_output(
    metrics: &mut Metrics,
    count: usize,
    output_bytes: usize,
) -> CoreResult<()> {
    let _ = metrics;
    let _ = u64::try_from(count).map_err(|_| CoreError::LengthOverflow)?;
    let _ = u64::try_from(output_bytes).map_err(|_| CoreError::LengthOverflow)?;
    Ok(())
}

fn chunk_id_accounted(bytes: &[u8], metrics: &mut Metrics) -> CoreResult<ObjectId> {
    add_len(&mut metrics.raw_bytes_hashed, bytes.len())?;
    add(&mut metrics.raw_hashes, 1)?;
    Ok(chunk_id(bytes))
}

fn object_id_accounted(canonical: &[u8], metrics: &mut Metrics) -> CoreResult<ObjectId> {
    add_len(&mut metrics.canonical_id_bytes_hashed, canonical.len())?;
    add(&mut metrics.canonical_id_hashes, 1)?;
    Ok(ObjectId::for_bytes(canonical))
}

fn metric_delta(after: u64, before: u64) -> CoreResult<u64> {
    after.checked_sub(before).ok_or(CoreError::LengthOverflow)
}

fn write_phase_metric_json(
    writer: &mut impl std::fmt::Write,
    name: &str,
    before: Metrics,
    after: Metrics,
) -> CoreResult<()> {
    let raw_bytes_hashed = metric_delta(after.raw_bytes_hashed, before.raw_bytes_hashed)?;
    let canonical_id_bytes_hashed = metric_delta(
        after.canonical_id_bytes_hashed,
        before.canonical_id_bytes_hashed,
    )?;
    let canonical_bytes_authenticated = metric_delta(
        after.canonical_bytes_authenticated,
        before.canonical_bytes_authenticated,
    )?;
    let reused_object_id_authentications = metric_delta(
        after.reused_object_id_authentications,
        before.reused_object_id_authentications,
    )?;
    let reused_object_id_authentication_bytes = metric_delta(
        after.reused_object_id_authentication_bytes,
        before.reused_object_id_authentication_bytes,
    )?;
    let objects_authenticated =
        metric_delta(after.objects_authenticated, before.objects_authenticated)?;
    let canonical_authentication_hashes = objects_authenticated
        .checked_sub(reused_object_id_authentications)
        .ok_or(CoreError::LengthOverflow)?;
    let canonical_authentication_hash_bytes = canonical_bytes_authenticated
        .checked_sub(reused_object_id_authentication_bytes)
        .ok_or(CoreError::LengthOverflow)?;
    let identity_bytes_hashed = raw_bytes_hashed
        .checked_add(canonical_id_bytes_hashed)
        .and_then(|value| value.checked_add(canonical_authentication_hash_bytes))
        .ok_or(CoreError::LengthOverflow)?;
    let canonical_new_write_bytes = metric_delta(
        after.canonical_bytes_written,
        before.canonical_bytes_written,
    )?;
    let canonical_authenticated_nonnew_bytes = canonical_bytes_authenticated
        .checked_sub(canonical_new_write_bytes)
        .ok_or(CoreError::LengthOverflow)?;
    write!(
        writer,
        "{{\"phase\":\"{name}\",\"identity_bytes_hashed\":{identity_bytes_hashed},\"raw_bytes_hashed\":{raw_bytes_hashed},\"raw_hashes\":{},\"canonical_id_bytes_hashed\":{canonical_id_bytes_hashed},\"canonical_id_hashes\":{},\"canonical_bytes_authenticated\":{canonical_bytes_authenticated},\"canonical_new_write_bytes\":{canonical_new_write_bytes},\"canonical_authenticated_nonnew_bytes\":{canonical_authenticated_nonnew_bytes},\"canonical_authentication_hash_bytes\":{canonical_authentication_hash_bytes},\"canonical_authentication_hashes\":{canonical_authentication_hashes},\"reused_object_id_authentications\":{reused_object_id_authentications},\"reused_object_id_authentication_bytes\":{reused_object_id_authentication_bytes},\"borrowed_bytes_encode_calls\":{borrowed_bytes_encode_calls},\"borrowed_bytes_encode_input_bytes\":{borrowed_bytes_encode_input_bytes},\"borrowed_source_encode_calls\":{borrowed_source_encode_calls},\"borrowed_source_encode_input_bytes\":{borrowed_source_encode_input_bytes},\"objects_created\":{},\"objects_reused\":{},\"objects_authenticated\":{},\"statement_cache_acquisitions\":{},\"native_sqlite_prepare_calls\":\"Unavailable\",\"sql_query_calls\":{},\"sql_execute_calls\":{},\"sql_rows_returned\":{},\"sql_rows_changed\":{},\"row_blob_reads\":{},\"row_blob_writes\":{},\"row_blob_copy_bytes\":{},\"borrowed_row_blob_reads\":{},\"borrowed_row_blob_bytes\":{},\"incremental_blob_opens\":{},\"incremental_blob_reads\":{},\"incremental_blob_writes\":{},\"leaf_batch_queries\":{},\"leaf_batch_references\":{},\"leaf_batch_references_max\":{},\"leaf_batch_query_bytes_max\":{},\"commits\":{},\"references\":{},\"pages\":{},\"branches\":{},\"incremental_qualification_calls\":{},\"incremental_prior_spine_objects_authenticated\":{},\"incremental_prior_spine_bytes_authenticated\":{},\"incremental_replacement_spine_objects_authenticated\":{},\"incremental_replacement_spine_bytes_authenticated\":{},\"incremental_receipt_covered_edges\":{},\"incremental_new_or_different_edges\":{},\"incremental_new_subtree_objects_authenticated\":{},\"incremental_new_subtree_bytes_authenticated\":{},\"construction_put_evidences\":{construction_put_evidences},\"construction_edges_covered\":{construction_edges_covered},\"construction_leaf_summaries\":{construction_leaf_summaries},\"construction_branch_summaries\":{construction_branch_summaries},\"construction_file_summaries\":{construction_file_summaries},\"construction_workspace_summaries\":{construction_workspace_summaries},\"construction_transition_summaries\":{construction_transition_summaries},\"construction_proof_consumptions\":{construction_proof_consumptions},\"construction_source_hash_bytes\":{construction_source_hash_bytes},\"construction_source_hashes\":{construction_source_hashes},\"construction_cdc_entries\":{construction_cdc_entries},\"other_heap_copy_bytes\":\"Unavailable\"}}",
        metric_delta(after.raw_hashes, before.raw_hashes)?,
        metric_delta(after.canonical_id_hashes, before.canonical_id_hashes)?,
        metric_delta(after.objects_created, before.objects_created)?,
        metric_delta(after.objects_reused, before.objects_reused)?,
        objects_authenticated,
        metric_delta(
            after.statement_cache_acquisitions,
            before.statement_cache_acquisitions,
        )?,
        metric_delta(after.sql_query_calls, before.sql_query_calls)?,
        metric_delta(after.sql_execute_calls, before.sql_execute_calls)?,
        metric_delta(after.sql_rows_returned, before.sql_rows_returned)?,
        metric_delta(after.sql_rows_changed, before.sql_rows_changed)?,
        metric_delta(after.row_blob_reads, before.row_blob_reads)?,
        metric_delta(after.row_blob_writes, before.row_blob_writes)?,
        metric_delta(after.row_blob_copy_bytes, before.row_blob_copy_bytes)?,
        metric_delta(
            after.borrowed_row_blob_reads,
            before.borrowed_row_blob_reads,
        )?,
        metric_delta(
            after.borrowed_row_blob_bytes,
            before.borrowed_row_blob_bytes,
        )?,
        metric_delta(after.blob_opens, before.blob_opens)?,
        metric_delta(after.blob_reads, before.blob_reads)?,
        metric_delta(after.blob_writes, before.blob_writes)?,
        metric_delta(after.leaf_batch_queries, before.leaf_batch_queries)?,
        metric_delta(
            after.leaf_batch_references,
            before.leaf_batch_references,
        )?,
        metric_delta(
            after.leaf_batch_references_max,
            before.leaf_batch_references_max,
        )?,
        metric_delta(
            after.leaf_batch_query_bytes_max,
            before.leaf_batch_query_bytes_max,
        )?,
        metric_delta(after.commits, before.commits)?,
        metric_delta(after.references, before.references)?,
        metric_delta(after.pages, before.pages)?,
        metric_delta(after.branches, before.branches)?,
        metric_delta(
            after.incremental_qualification_calls,
            before.incremental_qualification_calls,
        )?,
        metric_delta(
            after.incremental_prior_spine_objects_authenticated,
            before.incremental_prior_spine_objects_authenticated,
        )?,
        metric_delta(
            after.incremental_prior_spine_bytes_authenticated,
            before.incremental_prior_spine_bytes_authenticated,
        )?,
        metric_delta(
            after.incremental_replacement_spine_objects_authenticated,
            before.incremental_replacement_spine_objects_authenticated,
        )?,
        metric_delta(
            after.incremental_replacement_spine_bytes_authenticated,
            before.incremental_replacement_spine_bytes_authenticated,
        )?,
        metric_delta(
            after.incremental_receipt_covered_edges,
            before.incremental_receipt_covered_edges,
        )?,
        metric_delta(
            after.incremental_new_or_different_edges,
            before.incremental_new_or_different_edges,
        )?,
        metric_delta(
            after.incremental_new_subtree_objects_authenticated,
            before.incremental_new_subtree_objects_authenticated,
        )?,
        metric_delta(
            after.incremental_new_subtree_bytes_authenticated,
            before.incremental_new_subtree_bytes_authenticated,
        )?,
        construction_put_evidences = metric_delta(
            after.construction_put_evidences,
            before.construction_put_evidences,
        )?,
        construction_edges_covered = metric_delta(
            after.construction_edges_covered,
            before.construction_edges_covered,
        )?,
        construction_leaf_summaries = metric_delta(
            after.construction_leaf_summaries,
            before.construction_leaf_summaries,
        )?,
        construction_branch_summaries = metric_delta(
            after.construction_branch_summaries,
            before.construction_branch_summaries,
        )?,
        construction_file_summaries = metric_delta(
            after.construction_file_summaries,
            before.construction_file_summaries,
        )?,
        construction_workspace_summaries = metric_delta(
            after.construction_workspace_summaries,
            before.construction_workspace_summaries,
        )?,
        construction_transition_summaries = metric_delta(
            after.construction_transition_summaries,
            before.construction_transition_summaries,
        )?,
        construction_proof_consumptions = metric_delta(
            after.construction_proof_consumptions,
            before.construction_proof_consumptions,
        )?,
        construction_source_hash_bytes = metric_delta(
            after.construction_source_hash_bytes,
            before.construction_source_hash_bytes,
        )?,
        construction_source_hashes = metric_delta(
            after.construction_source_hashes,
            before.construction_source_hashes,
        )?,
        construction_cdc_entries = metric_delta(
            after.construction_cdc_entries,
            before.construction_cdc_entries,
        )?,
        borrowed_bytes_encode_calls = metric_delta(
            after.borrowed_bytes_encode_calls,
            before.borrowed_bytes_encode_calls,
        )?,
        borrowed_bytes_encode_input_bytes = metric_delta(
            after.borrowed_bytes_encode_input_bytes,
            before.borrowed_bytes_encode_input_bytes,
        )?,
        borrowed_source_encode_calls = metric_delta(
            after.borrowed_source_encode_calls,
            before.borrowed_source_encode_calls,
        )?,
        borrowed_source_encode_input_bytes = metric_delta(
            after.borrowed_source_encode_input_bytes,
            before.borrowed_source_encode_input_bytes,
        )?,
    )
    .map_err(|_| CoreError::Io)
}

fn phase_metrics_json(
    intervals: &[PhaseMetricInterval],
    metrics: &mut Metrics,
) -> CoreResult<ChargedString> {
    let mut counter = CountingWriter(0);
    for (index, (name, before, after)) in intervals.iter().enumerate() {
        if index != 0 {
            counter.write_char(',').map_err(|_| CoreError::Io)?;
        }
        write_phase_metric_json(&mut counter, name, *before, *after)?;
    }
    let mut output = ChargedString::with_capacity(counter.0, metrics)?;
    for (index, (name, before, after)) in intervals.iter().enumerate() {
        if index != 0 {
            output.value.write_char(',').map_err(|_| CoreError::Io)?;
        }
        write_phase_metric_json(&mut output.value, name, *before, *after)?;
    }
    Ok(output)
}

fn observe_closure(
    hasher: &mut Hasher,
    role: &[u8],
    id: ObjectId,
    canonical: &[u8],
) -> CoreResult<()> {
    hasher.update(
        &u64::try_from(role.len())
            .map_err(|_| CoreError::LengthOverflow)?
            .to_be_bytes(),
    );
    hasher.update(role);
    hasher.update(id.as_bytes());
    hasher.update(
        &u64::try_from(canonical.len())
            .map_err(|_| CoreError::LengthOverflow)?
            .to_be_bytes(),
    );
    hasher.update(canonical);
    Ok(())
}

fn combined_closure_digest(transition: [u8; 32], content: [u8; 32]) -> [u8; 32] {
    let mut hasher = Hasher::new();
    hasher.update(b"layerfs/wp4m/ordered-closure/v1\0");
    hasher.update(&transition);
    hasher.update(&content);
    *hasher.finalize().as_bytes()
}

fn profile_id(candidate: Candidate) -> CoreResult<[u8; 32]> {
    let mut hasher = Hasher::new();
    hasher.update(b"layerfs/mapping-profile/wp4m/v1\0");
    hasher.update(
        &u32::try_from(candidate.k)
            .map_err(|_| CoreError::LengthOverflow)?
            .to_be_bytes(),
    );
    hasher.update(
        &u32::try_from(candidate.f)
            .map_err(|_| CoreError::LengthOverflow)?
            .to_be_bytes(),
    );
    hasher.update(
        &u32::try_from(candidate.directory_page)
            .map_err(|_| CoreError::LengthOverflow)?
            .to_be_bytes(),
    );
    hasher.update(&(8 * 1024 * 1024_u32).to_be_bytes());
    Ok(*hasher.finalize().as_bytes())
}

struct Store {
    path: PathBuf,
    authority_path: PathBuf,
    profile: [u8; 32],
    store_instance_id: [u8; 16],
    validation_authority_id: [u8; 32],
    validation_key: [u8; 32],
    integrity_epoch: u64,
    open_identity: u64,
    same_open_authority_serial: u64,
    mutation_serial: u64,
    next_transaction_identity: u64,
    active_transaction: Option<ActiveTransaction>,
    #[cfg(test)]
    next_publish_fault: Option<PublishFault>,
    connection: Connection,
}

fn authority_path(path: &Path) -> PathBuf {
    let mut value = path.as_os_str().to_os_string();
    value.push(".authority");
    PathBuf::from(value)
}

fn new_validation_key(path: &Path, profile: [u8; 32]) -> CoreResult<[u8; 32]> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| CoreError::LengthOverflow)?
        .as_nanos();
    let mut hasher = Hasher::new();
    hasher.update(b"layerfs/wp4m/validation-key/v2\0");
    hasher.update(&profile);
    hasher.update(&now.to_be_bytes());
    hasher.update(path.as_os_str().as_encoded_bytes());
    Ok(*hasher.finalize().as_bytes())
}

fn read_authority(path: &Path) -> AnyResult<[u8; 32]> {
    fs::read(path)
        .map_err(|_| CoreError::ValidationAuthorityUnavailable)?
        .try_into()
        .map_err(|_| CoreError::ValidationAuthorityUnavailable.into())
}

fn create_authority(path: &Path, profile: [u8; 32]) -> AnyResult<[u8; 32]> {
    let key = new_validation_key(path, profile)?;
    let mut file = match OpenOptions::new().write(true).create_new(true).open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            return read_authority(path);
        }
        Err(error) => return Err(error.into()),
    };
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        file.set_permissions(fs::Permissions::from_mode(0o600))?;
    }
    file.write_all(&key)?;
    file.sync_all()?;
    Ok(key)
}

impl Store {
    fn open(path: &Path, candidate: Candidate) -> AnyResult<Self> {
        Self::open_inner(path, candidate, None)
    }

    fn open_measured(path: &Path, candidate: Candidate, metrics: &mut Metrics) -> AnyResult<Self> {
        Self::open_inner(path, candidate, Some(metrics))
    }

    fn open_inner(
        path: &Path,
        candidate: Candidate,
        mut metrics: Option<&mut Metrics>,
    ) -> AnyResult<Self> {
        let connection = Connection::open(path)?;
        let authority_path = authority_path(path);
        let delete_journal = connection.query_row("PRAGMA journal_mode=DELETE", [], |row| {
            Ok(matches!(row.get_ref(0)?, ValueRef::Text(value) if value.eq_ignore_ascii_case(b"delete")))
        })?;
        if !delete_journal {
            return Err(CoreError::PublicationConflict.into());
        }
        connection.execute_batch(
            "PRAGMA synchronous=FULL; PRAGMA temp_store=FILE; PRAGMA mmap_size=0;",
        )?;
        connection.execute_batch(
            "CREATE TABLE IF NOT EXISTS wp4m_meta (
                id INTEGER PRIMARY KEY CHECK (id = 1),
                profile_id BLOB NOT NULL,
                store_instance_id BLOB NOT NULL,
                validation_authority_id BLOB NOT NULL,
                validation_key BLOB NOT NULL,
                integrity_epoch BLOB NOT NULL,
                schema_version INTEGER NOT NULL,
                journal_mode TEXT NOT NULL,
                synchronous INTEGER NOT NULL,
                temp_store INTEGER NOT NULL,
                mmap_size INTEGER NOT NULL
            );
            CREATE TABLE IF NOT EXISTS wp4m_objects (
                object_id BLOB PRIMARY KEY,
                kind INTEGER NOT NULL,
                canonical_length BLOB NOT NULL,
                canonical_bytes BLOB NOT NULL
            );
            CREATE TABLE IF NOT EXISTS wp4m_visible_head (
                id INTEGER PRIMARY KEY CHECK (id = 1),
                generation BLOB NOT NULL,
                child BLOB NOT NULL,
                transition BLOB NOT NULL,
                validation_receipt BLOB NOT NULL
            );",
        )?;
        if let Some(metrics) = metrics.as_deref_mut() {
            observe_query_call(metrics)?;
            observe_rows_returned(metrics, 1)?;
            observe_execute_call(metrics, 0)?;
            observe_execute_call(metrics, 0)?;
        }
        let profile = profile_id(candidate)?;
        let existing: Option<StoreMetaRow> = connection
            .query_row(
                "SELECT profile_id, store_instance_id, validation_authority_id, integrity_epoch
                 FROM wp4m_meta WHERE id = 1",
                [],
                |row| {
                    let profile = fixed_blob(row.get_ref(0)?);
                    let instance = fixed_blob(row.get_ref(1)?);
                    let authority = fixed_blob(row.get_ref(2)?);
                    let epoch = fixed_blob(row.get_ref(3)?);
                    Ok((
                        profile.0,
                        instance.0,
                        authority.0,
                        epoch.0,
                        [profile.1, instance.1, authority.1, epoch.1],
                    ))
                },
            )
            .optional()?;
        if let Some(metrics) = metrics.as_deref_mut() {
            observe_query_call(metrics)?;
            observe_rows_returned(metrics, u64::from(existing.is_some()))?;
        }
        if let (Some(metrics), Some((_, _, _, _, lengths))) =
            (metrics.as_deref_mut(), existing.as_ref())
        {
            observe_row_blobs(metrics, lengths)?;
        }
        let (store_instance_id, validation_authority_id, integrity_epoch, validation_key) =
            match existing {
                Some((Some(value), Some(instance), Some(authority), Some(epoch), _))
                    if value == profile =>
                {
                    let validation_key = read_authority(&authority_path)?;
                    (
                        instance,
                        authority,
                        u64::from_be_bytes(epoch),
                        validation_key,
                    )
                }
                Some(_) => return Err(CoreError::InvalidValidationReceipt.into()),
                None => {
                    let now = SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .map_err(|_| CoreError::LengthOverflow)?
                        .as_nanos();
                    let mut instance_hasher = Hasher::new();
                    instance_hasher.update(b"layerfs/wp4m/store-instance/v1\0");
                    instance_hasher.update(&profile);
                    instance_hasher.update(&now.to_be_bytes());
                    instance_hasher.update(path.as_os_str().as_encoded_bytes());
                    let digest = instance_hasher.finalize();
                    let store_instance_id: [u8; 16] = digest.as_bytes()[..16]
                        .try_into()
                        .map_err(|_| CoreError::InvalidValidationReceipt)?;
                    let validation_key = create_authority(&authority_path, profile)?;
                    let validation_authority_id =
                        ValidatedSnapshotReceiptV1::validation_authority_id(
                            store_instance_id,
                            &validation_key,
                        );
                    connection.execute(
                    "INSERT INTO wp4m_meta (id, profile_id, store_instance_id, validation_authority_id,
                         validation_key, integrity_epoch, schema_version, journal_mode, synchronous,
                         temp_store, mmap_size)
                    VALUES (1, ?1, ?2, ?3, ?4, ?5, 5, 'delete', 2, 1, 0)",
                    params![
                        profile.as_slice(),
                        store_instance_id.as_slice(),
                        validation_authority_id.as_slice(),
                        [0_u8; 32].as_slice(),
                        1_u64.to_be_bytes().as_slice()
                    ],
                    )?;
                    if let Some(metrics) = metrics {
                        observe_execute_call(metrics, 1)?;
                        add(&mut metrics.row_blob_writes, 5)?;
                    }
                    (
                        store_instance_id,
                        validation_authority_id,
                        1,
                        validation_key,
                    )
                }
            };
        if validation_authority_id
            != ValidatedSnapshotReceiptV1::validation_authority_id(
                store_instance_id,
                &validation_key,
            )
        {
            return Err(CoreError::InvalidValidationReceipt.into());
        }
        Ok(Self {
            path: path.to_path_buf(),
            authority_path,
            profile,
            store_instance_id,
            validation_authority_id,
            validation_key,
            integrity_epoch,
            open_identity: next_open_identity()?,
            same_open_authority_serial: 0,
            mutation_serial: 0,
            next_transaction_identity: 0,
            active_transaction: None,
            #[cfg(test)]
            next_publish_fault: None,
            connection,
        })
    }

    fn issue_same_open_witness(
        &mut self,
        head: &VisibleHead,
        metrics: &mut Metrics,
    ) -> AnyResult<SameOpenValidationWitness> {
        let receipt_charge = charge_capacity(metrics, 216)?;
        let transaction_identity = {
            let transaction = self
                .active_transaction
                .as_mut()
                .ok_or(CoreError::ValidationAuthorityUnavailable)?;
            if transaction.witness_issued {
                return Err(CoreError::ValidationAuthorityUnavailable.into());
            }
            transaction.witness_issued = true;
            transaction.identity
        };
        let receipt = head
            .3
            .as_slice()
            .try_into()
            .map_err(|_| CoreError::InvalidValidationReceipt)?;
        Ok(SameOpenValidationWitness {
            open_identity: self.open_identity,
            store_instance_id: self.store_instance_id,
            validation_authority_id: self.validation_authority_id,
            integrity_epoch: self.integrity_epoch,
            profile: self.profile,
            generation: head.0,
            root: head.1,
            transition: head.2,
            receipt,
            authority_serial: self.same_open_authority_serial,
            transaction_identity,
            consumed: false,
            _receipt_charge: receipt_charge,
        })
    }

    fn mark_construction_proof_issued(
        &mut self,
        proof: &FullCreateConstructionProof,
    ) -> CoreResult<()> {
        let scope = &proof.workspace.file.scope;
        let transaction = self
            .active_transaction
            .as_mut()
            .ok_or(CoreError::ValidationAuthorityUnavailable)?;
        if proof.expectation.is_none()
            || transaction.construction_proof_issued
            || transaction.identity != scope.transaction_identity
            || scope.open_identity != self.open_identity
            || scope.store_instance_id != self.store_instance_id
            || scope.validation_authority_id != self.validation_authority_id
            || scope.integrity_epoch != self.integrity_epoch
            || scope.profile != self.profile
            || scope.authority_serial != self.same_open_authority_serial
            || scope.last_mutation_serial != self.mutation_serial
        {
            return Err(CoreError::ValidationAuthorityUnavailable);
        }
        transaction.construction_proof_issued = true;
        Ok(())
    }

    fn begin(&mut self, metrics: &mut Metrics) -> AnyResult<()> {
        if self.active_transaction.is_some() {
            return Err(CoreError::PublicationConflict.into());
        }
        let next_transaction_identity = self
            .next_transaction_identity
            .checked_add(1)
            .ok_or(CoreError::LengthOverflow)?;
        let next_sql_execute_calls = metrics
            .sql_execute_calls
            .checked_add(1)
            .ok_or(CoreError::LengthOverflow)?;
        let next_transactions = metrics
            .transactions
            .checked_add(1)
            .ok_or(CoreError::LengthOverflow)?;
        self.connection.execute_batch("BEGIN IMMEDIATE")?;
        metrics.sql_execute_calls = next_sql_execute_calls;
        metrics.transactions = next_transactions;
        self.next_transaction_identity = next_transaction_identity;
        self.active_transaction = Some(ActiveTransaction {
            identity: next_transaction_identity,
            witness_issued: false,
            construction_proof_issued: false,
            construction_proof_consumed: false,
        });
        Ok(())
    }

    fn rollback(&mut self, metrics: &mut Metrics) -> AnyResult<()> {
        let next_sql_execute_calls = metrics
            .sql_execute_calls
            .checked_add(1)
            .ok_or(CoreError::LengthOverflow)?;
        let invalidated_authority_serial = self
            .same_open_authority_serial
            .checked_add(1)
            .ok_or(CoreError::LengthOverflow)?;
        self.active_transaction = None;
        self.same_open_authority_serial = invalidated_authority_serial;
        self.connection.execute_batch("ROLLBACK")?;
        metrics.sql_execute_calls = next_sql_execute_calls;
        Ok(())
    }

    fn transaction_attempt<T>(
        &mut self,
        metrics: &mut Metrics,
        attempt: impl FnOnce(&mut Self, &mut Metrics) -> AnyResult<T>,
    ) -> AnyResult<T> {
        self.begin(metrics)?;
        match attempt(self, metrics) {
            Ok(value) if self.active_transaction.is_none() => Ok(value),
            Ok(_) => {
                let cleanup_first = self
                    .rollback(metrics)
                    .err()
                    .map(|error| failure_cause(error.as_ref()));
                Err(PublicationFailure(failure_provenance(
                    Some(FailureCause::Core(CoreError::PublicationConflict)),
                    cleanup_first,
                    Reconciliation::NotAttempted,
                    None,
                ))
                .into())
            }
            Err(error) if self.active_transaction.is_some() => {
                let first = failure_cause(error.as_ref());
                let cleanup_first = self
                    .rollback(metrics)
                    .err()
                    .map(|cleanup| failure_cause(cleanup.as_ref()));
                Err(PublicationFailure(failure_provenance(
                    Some(first),
                    cleanup_first,
                    Reconciliation::NotAttempted,
                    None,
                ))
                .into())
            }
            Err(error) => Err(error),
        }
    }

    fn put(&mut self, id: ObjectId, canonical: &[u8], metrics: &mut Metrics) -> AnyResult<()> {
        let decoded_charge = charge_capacity(metrics, decoded_object_q(canonical)?)?;
        let object = layerfs_core::validate_identity(canonical, id)?;
        let kind = object.kind();
        drop(object);
        drop(decoded_charge);
        self.put_authenticated(id, kind, canonical, false, metrics)
    }

    fn put_generated_bytes(
        &mut self,
        value: &[u8],
        metrics: &mut Metrics,
    ) -> AnyResult<(ObjectId, usize)> {
        let canonical = encode_charged_bytes_object(value, metrics)?;
        add(&mut metrics.borrowed_bytes_encode_calls, 1)?;
        add_len(&mut metrics.borrowed_bytes_encode_input_bytes, value.len())?;
        let id = object_id_accounted(&canonical, metrics)?;
        let canonical_len = canonical.len();
        self.put_authenticated(id, ObjectKind::Bytes, &canonical, true, metrics)?;
        Ok((id, canonical_len))
    }

    fn put_generated_bytes_with_evidence(
        &mut self,
        value: &[u8],
        metrics: &mut Metrics,
    ) -> AnyResult<(ObjectId, usize, PutEvidence)> {
        let canonical = encode_charged_bytes_object(value, metrics)?;
        add(&mut metrics.borrowed_bytes_encode_calls, 1)?;
        add_len(&mut metrics.borrowed_bytes_encode_input_bytes, value.len())?;
        let id = object_id_accounted(&canonical, metrics)?;
        let canonical_len = canonical.len();
        self.put_authenticated(id, ObjectKind::Bytes, &canonical, true, metrics)?;
        let evidence = self.issue_put_evidence(id, ObjectKind::Bytes, canonical_len)?;
        Ok((id, canonical_len, evidence))
    }

    fn put_with_evidence(
        &mut self,
        id: ObjectId,
        canonical: &[u8],
        metrics: &mut Metrics,
    ) -> AnyResult<PutEvidence> {
        let decoded_charge = charge_capacity(metrics, decoded_object_q(canonical)?)?;
        let object = layerfs_core::validate_identity(canonical, id)?;
        let kind = object.kind();
        drop(object);
        drop(decoded_charge);
        self.put_authenticated(id, kind, canonical, false, metrics)?;
        self.issue_put_evidence(id, kind, canonical.len())
    }

    fn issue_put_evidence(
        &self,
        object_id: ObjectId,
        kind: ObjectKind,
        canonical_len: usize,
    ) -> AnyResult<PutEvidence> {
        let transaction = self
            .active_transaction
            .ok_or(CoreError::ValidationAuthorityUnavailable)?;
        Ok(PutEvidence {
            object_id,
            canonical_len: u64::try_from(canonical_len).map_err(|_| CoreError::LengthOverflow)?,
            open_identity: self.open_identity,
            transaction_identity: transaction.identity,
            authority_serial: self.same_open_authority_serial,
            mutation_serial: self.mutation_serial,
            kind,
        })
    }

    fn put_authenticated(
        &mut self,
        id: ObjectId,
        kind: ObjectKind,
        canonical: &[u8],
        reused_object_id: bool,
        metrics: &mut Metrics,
    ) -> AnyResult<()> {
        let next_mutation_serial = self
            .mutation_serial
            .checked_add(1)
            .ok_or(CoreError::LengthOverflow)?;
        add(&mut metrics.objects_authenticated, 1)?;
        add_len(&mut metrics.canonical_bytes_authenticated, canonical.len())?;
        if reused_object_id {
            add(&mut metrics.reused_object_id_authentications, 1)?;
            add_len(
                &mut metrics.reused_object_id_authentication_bytes,
                canonical.len(),
            )?;
        }
        add_len(&mut metrics.w_bytes, canonical.len())?;
        let mut statement = self.connection.prepare_cached(
            "INSERT INTO wp4m_objects (object_id, kind, canonical_length, canonical_bytes)
                 VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT(object_id) DO NOTHING",
        )?;
        observe_statement_cache_acquisition(metrics)?;
        let inserted = statement.execute(params![
            id.as_bytes().as_slice(),
            kind as u8,
            i64::try_from(canonical.len()).map_err(|_| CoreError::LengthOverflow)?,
            canonical
        ])?;
        observe_execute_call(metrics, inserted)?;
        if inserted == 1 {
            add(&mut metrics.objects_created, 1)?;
            add(&mut metrics.row_blob_writes, 2)?;
            add_len(&mut metrics.canonical_bytes_written, canonical.len())?;
            self.mutation_serial = next_mutation_serial;
            return Ok(());
        }
        let mut statement = self
            .connection
            .prepare_cached("SELECT canonical_bytes FROM wp4m_objects WHERE object_id = ?1")?;
        observe_statement_cache_acquisition(metrics)?;
        let mut rows = statement.query(params![id.as_bytes().as_slice()])?;
        observe_query_call(metrics)?;
        let row = rows.next()?.ok_or(CandidateError::MissingObject(id))?;
        observe_rows_returned(metrics, 1)?;
        let existing = row.get_ref(0)?.as_blob()?;
        observe_row_blobs(metrics, &[existing.len()])?;
        let decoded_charge = charge_capacity(metrics, decoded_object_q(existing)?)?;
        let existing_object = layerfs_core::validate_identity(existing, id)?;
        add(&mut metrics.objects_authenticated, 1)?;
        add_len(&mut metrics.canonical_bytes_authenticated, existing.len())?;
        if existing != canonical {
            return Err(CoreError::IdentityMismatch.into());
        }
        drop(existing_object);
        drop(decoded_charge);
        add(&mut metrics.objects_reused, 1)?;
        self.mutation_serial = next_mutation_serial;
        Ok(())
    }

    fn read_canonical(&self, id: ObjectId, metrics: &mut Metrics) -> AnyResult<ChargedBytes> {
        let mut statement = self
            .connection
            .prepare_cached("SELECT canonical_bytes FROM wp4m_objects WHERE object_id = ?1")?;
        observe_statement_cache_acquisition(metrics)?;
        let mut rows = statement.query(params![id.as_bytes().as_slice()])?;
        observe_query_call(metrics)?;
        let row = rows.next()?.ok_or(CandidateError::MissingObject(id))?;
        observe_rows_returned(metrics, 1)?;
        let bytes = row.get_ref(0)?.as_blob()?;
        observe_row_blobs(metrics, &[bytes.len()])?;
        add_len(&mut metrics.d_bytes, bytes.len())?;
        Ok(ChargedBytes::from_borrowed(bytes, metrics)?)
    }

    fn get(
        &self,
        id: ObjectId,
        metrics: &mut Metrics,
    ) -> AnyResult<(CapacityCharge, Object, ChargedBytes)> {
        let bytes = self.read_canonical(id, metrics)?;
        let decoded_charge = charge_capacity(metrics, decoded_object_q(&bytes)?)?;
        let object = layerfs_core::validate_identity(&bytes, id)?;
        add(&mut metrics.objects_authenticated, 1)?;
        add_len(&mut metrics.canonical_bytes_authenticated, bytes.len())?;
        // The guard is first so ordinary destructuring drops bytes/object before
        // their shared decoded-capacity charge.
        Ok((decoded_charge, object, bytes))
    }

    fn get_bytes(&self, id: ObjectId, metrics: &mut Metrics) -> AnyResult<ChargedBytes> {
        let bytes = self.read_canonical(id, metrics)?;
        layerfs_core::validate_bytes_identity(&bytes, id)?;
        add(&mut metrics.objects_authenticated, 1)?;
        add_len(&mut metrics.canonical_bytes_authenticated, bytes.len())?;
        Ok(bytes)
    }

    fn with_borrowed_bytes<T, F>(
        &self,
        id: ObjectId,
        metrics: &mut Metrics,
        callback: F,
    ) -> AnyResult<T>
    where
        F: FnOnce(&[u8], &mut Metrics) -> AnyResult<T>,
    {
        let mut statement = self
            .connection
            .prepare_cached("SELECT canonical_bytes FROM wp4m_objects WHERE object_id = ?1")?;
        observe_statement_cache_acquisition(metrics)?;
        let mut rows = statement.query(params![id.as_bytes().as_slice()])?;
        observe_query_call(metrics)?;
        let row = rows.next()?.ok_or(CandidateError::MissingObject(id))?;
        observe_rows_returned(metrics, 1)?;
        let canonical = row.get_ref(0)?.as_blob()?;
        observe_borrowed_row_blob(metrics, canonical.len())?;
        add_len(&mut metrics.d_bytes, canonical.len())?;
        layerfs_core::validate_bytes_identity(canonical, id)?;
        add(&mut metrics.objects_authenticated, 1)?;
        add_len(&mut metrics.canonical_bytes_authenticated, canonical.len())?;
        callback(canonical, metrics)
    }

    fn for_each_leaf_bytes<F>(
        &self,
        references: &[file_codec::FileReference],
        max_references: usize,
        metrics: &mut Metrics,
        callback: &mut F,
    ) -> AnyResult<()>
    where
        F: FnMut(file_codec::FileReference, &[u8], &mut Metrics) -> AnyResult<()>,
    {
        if references.is_empty() {
            return Ok(());
        }
        if references.len() > max_references {
            return Err(CoreError::NonCanonicalPagePartition.into());
        }
        const PREFIX: &str = "WITH requested(ordinal, object_id) AS (VALUES ";
        const SUFFIX: &str = ") SELECT requested.ordinal, objects.canonical_bytes
             FROM requested
             LEFT JOIN wp4m_objects AS objects ON objects.object_id = requested.object_id
             ORDER BY requested.ordinal";
        let requested_capacity = PREFIX
            .len()
            .checked_add(SUFFIX.len())
            .and_then(|length| length.checked_add(references.len().checked_mul(32)?))
            .ok_or(CoreError::LengthOverflow)?;
        let _sql_charge = charge_capacity(metrics, requested_capacity)?;
        let mut sql = String::new();
        sql.try_reserve_exact(requested_capacity)
            .map_err(|_| CoreError::AllocationFailed)?;
        if sql.capacity() != requested_capacity {
            return Err(CoreError::AllocationFailed.into());
        }
        sql.push_str(PREFIX);
        for index in 0..references.len() {
            write!(&mut sql, "{}({index},?)", if index == 0 { "" } else { "," })
                .map_err(|_| CoreError::Io)?;
        }
        sql.push_str(SUFFIX);
        let query_bytes = u64::try_from(sql.len()).map_err(|_| CoreError::LengthOverflow)?;
        metrics.leaf_batch_query_bytes_max = metrics.leaf_batch_query_bytes_max.max(query_bytes);
        let mut statement = self.connection.prepare(&sql)?;
        let parameters = rusqlite::params_from_iter(
            references
                .iter()
                .map(|reference| reference.object_id.as_bytes().as_slice()),
        );
        let mut rows = statement.query(parameters)?;
        observe_query_call(metrics)?;
        add(&mut metrics.leaf_batch_queries, 1)?;
        add(
            &mut metrics.leaf_batch_references,
            u64::try_from(references.len()).map_err(|_| CoreError::LengthOverflow)?,
        )?;
        metrics.leaf_batch_references_max = metrics
            .leaf_batch_references_max
            .max(u64::try_from(references.len()).map_err(|_| CoreError::LengthOverflow)?);
        let mut index = 0_usize;
        while let Some(row) = rows.next()? {
            observe_rows_returned(metrics, 1)?;
            let ordinal: i64 = row.get(0)?;
            if usize::try_from(ordinal).map_err(|_| CoreError::NonCanonicalOrdering)? != index
                || index >= references.len()
            {
                return Err(CoreError::NonCanonicalOrdering.into());
            }
            let canonical = match row.get_ref(1)? {
                ValueRef::Blob(canonical) => canonical,
                ValueRef::Null => {
                    return Err(CandidateError::MissingObject(references[index].object_id).into())
                }
                _ => return Err(CoreError::WrongLogicalRole.into()),
            };
            observe_borrowed_row_blob(metrics, canonical.len())?;
            add_len(&mut metrics.d_bytes, canonical.len())?;
            let reference = references[index];
            layerfs_core::validate_bytes_identity(canonical, reference.object_id)?;
            add(&mut metrics.objects_authenticated, 1)?;
            add_len(&mut metrics.canonical_bytes_authenticated, canonical.len())?;
            callback(reference, canonical, metrics)?;
            index = index.checked_add(1).ok_or(CoreError::LengthOverflow)?;
        }
        if index != references.len() {
            return Err(CandidateError::MissingObject(references[index].object_id).into());
        }
        Ok(())
    }

    fn current_head(&self) -> AnyResult<Option<VisibleHead>> {
        self.current_head_accounted(&mut Metrics::default())
    }

    fn fresh_read_only_head(&self, metrics: &mut Metrics) -> AnyResult<Option<VisibleHead>> {
        let connection = Connection::open_with_flags(&self.path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        let meta: StoreMetaRow = connection.query_row(
            "SELECT profile_id, store_instance_id, validation_authority_id, integrity_epoch
             FROM wp4m_meta WHERE id = 1",
            [],
            |row| {
                let profile = fixed_blob(row.get_ref(0)?);
                let instance = fixed_blob(row.get_ref(1)?);
                let authority = fixed_blob(row.get_ref(2)?);
                let epoch = fixed_blob(row.get_ref(3)?);
                Ok((
                    profile.0,
                    instance.0,
                    authority.0,
                    epoch.0,
                    [profile.1, instance.1, authority.1, epoch.1],
                ))
            },
        )?;
        observe_query_call(metrics)?;
        observe_rows_returned(metrics, 1)?;
        observe_row_blobs(metrics, &meta.4)?;
        if meta.0 != Some(self.profile)
            || meta.1 != Some(self.store_instance_id)
            || meta.2 != Some(self.validation_authority_id)
            || meta.3 != Some(self.integrity_epoch.to_be_bytes())
        {
            return Err(CoreError::InvalidValidationReceipt.into());
        }
        let row: Option<VisibleHeadRow> = connection
            .query_row(
                "SELECT generation, child, transition, validation_receipt
                 FROM wp4m_visible_head WHERE id = 1",
                [],
                |row| {
                    let generation = fixed_blob(row.get_ref(0)?);
                    let child = fixed_blob(row.get_ref(1)?);
                    let transition = fixed_blob(row.get_ref(2)?);
                    let receipt = fixed_blob(row.get_ref(3)?);
                    Ok((
                        generation.0,
                        child.0,
                        transition.0,
                        receipt.0,
                        [generation.1, child.1, transition.1, receipt.1],
                    ))
                },
            )
            .optional()?;
        observe_query_call(metrics)?;
        observe_rows_returned(metrics, u64::from(row.is_some()))?;
        let Some((generation, child, transition, validation_receipt, lengths)) = row else {
            return Ok(None);
        };
        observe_row_blobs(metrics, &lengths)?;
        let generation = u64::from_be_bytes(generation.ok_or(CoreError::InvalidValidationReceipt)?);
        let child = ObjectId::from_bytes(&child.ok_or(CoreError::InvalidValidationReceipt)?)?;
        let transition =
            ObjectId::from_bytes(&transition.ok_or(CoreError::InvalidValidationReceipt)?)?;
        let validation_receipt = validation_receipt.ok_or(CoreError::InvalidValidationReceipt)?;
        let receipt = ValidatedSnapshotReceiptV1::decode(
            &validation_receipt,
            &self.validation_key,
            ObjectId::from_bytes(&self.profile)?,
            self.validation_authority_id,
        )?;
        if receipt.store_instance_id != self.store_instance_id
            || receipt.integrity_epoch != self.integrity_epoch
            || receipt.head_generation != generation
            || receipt.child_root_id != child
            || receipt.transition_id != transition
        {
            return Err(CoreError::InvalidValidationReceipt.into());
        }
        Ok(Some((generation, child, transition, validation_receipt)))
    }

    fn current_head_accounted(&self, metrics: &mut Metrics) -> AnyResult<Option<VisibleHead>> {
        let row: Option<VisibleHeadRow> = self
            .connection
            .query_row(
                "SELECT generation, child, transition, validation_receipt FROM wp4m_visible_head WHERE id = 1",
                [],
                |row| {
                    let generation = fixed_blob(row.get_ref(0)?);
                    let child = fixed_blob(row.get_ref(1)?);
                    let transition = fixed_blob(row.get_ref(2)?);
                    let receipt = fixed_blob(row.get_ref(3)?);
                    Ok((
                        generation.0,
                        child.0,
                        transition.0,
                        receipt.0,
                        [generation.1, child.1, transition.1, receipt.1],
                    ))
                },
            )
            .optional()?;
        observe_query_call(metrics)?;
        observe_rows_returned(metrics, u64::from(row.is_some()))?;
        let Some((generation, child, transition, validation_receipt, lengths)) = row else {
            return Ok(None);
        };
        observe_row_blobs(metrics, &lengths)?;
        let generation = u64::from_be_bytes(generation.ok_or(CoreError::InvalidValidationReceipt)?);
        let child = ObjectId::from_bytes(&child.ok_or(CoreError::InvalidValidationReceipt)?)?;
        let transition =
            ObjectId::from_bytes(&transition.ok_or(CoreError::InvalidValidationReceipt)?)?;
        let validation_receipt = validation_receipt.ok_or(CoreError::InvalidValidationReceipt)?;
        let receipt = ValidatedSnapshotReceiptV1::decode(
            &validation_receipt,
            &self.validation_key,
            ObjectId::from_bytes(&self.profile)?,
            self.validation_authority_id,
        )?;
        if receipt.store_instance_id != self.store_instance_id
            || receipt.integrity_epoch != self.integrity_epoch
            || receipt.head_generation != generation
            || receipt.child_root_id != child
            || receipt.transition_id != transition
        {
            return Err(CoreError::InvalidValidationReceipt.into());
        }
        Ok(Some((generation, child, transition, validation_receipt)))
    }

    fn publication_key(
        &self,
        prior: Option<&VisibleHead>,
        requested: &VisibleHead,
    ) -> CoreResult<[u8; 32]> {
        let mut hasher = Hasher::new();
        hasher.update(b"layerfs/publication-idempotency/v1\0");
        hasher.update(&self.store_instance_id);
        match prior {
            Some((generation, child, transition, receipt)) => {
                hasher.update(&[1]);
                hasher.update(&generation.to_be_bytes());
                hasher.update(child.as_bytes());
                hasher.update(transition.as_bytes());
                if receipt.len() != 216 {
                    return Err(CoreError::InvalidValidationReceipt);
                }
                hasher.update(receipt);
            }
            None => {
                hasher.update(&[0]);
            }
        }
        hasher.update(&requested.0.to_be_bytes());
        hasher.update(requested.1.as_bytes());
        hasher.update(requested.2.as_bytes());
        if requested.3.len() != 216 {
            return Err(CoreError::InvalidValidationReceipt);
        }
        hasher.update(&requested.3);
        Ok(*hasher.finalize().as_bytes())
    }

    #[cfg(test)]
    fn reconcile_publication(
        &self,
        prior: Option<&VisibleHead>,
        requested: &VisibleHead,
        request_key: [u8; 32],
    ) -> Reconciliation {
        self.reconcile_publication_accounted(prior, requested, request_key, &mut Metrics::default())
            .0
    }

    fn reconcile_publication_accounted(
        &self,
        prior: Option<&VisibleHead>,
        requested: &VisibleHead,
        request_key: [u8; 32],
        metrics: &mut Metrics,
    ) -> (Reconciliation, Option<FailureCause>) {
        let authoritative = match self.fresh_read_only_head(metrics) {
            Ok(authoritative) => authoritative,
            Err(error) => {
                return (
                    Reconciliation::Ambiguous,
                    Some(failure_cause(error.as_ref())),
                )
            }
        };
        if authoritative.as_ref() == Some(requested) {
            if self
                .publication_key(prior, requested)
                .is_ok_and(|key| key == request_key)
            {
                (Reconciliation::RequestedVisible, None)
            } else {
                (
                    Reconciliation::Ambiguous,
                    Some(FailureCause::Core(CoreError::InvalidValidationReceipt)),
                )
            }
        } else if authoritative.as_ref() == prior {
            (Reconciliation::PriorVisible, None)
        } else {
            (Reconciliation::DifferentHead, None)
        }
    }

    fn reconcile_publication_observed(
        &self,
        prior: Option<&VisibleHead>,
        requested: &VisibleHead,
        request_key: [u8; 32],
        next_reconciliation_calls: u64,
        metrics: &mut Metrics,
    ) -> (Reconciliation, Option<FailureCause>) {
        let started = Instant::now();
        let result = self.reconcile_publication_accounted(prior, requested, request_key, metrics);
        metrics.commit_reconciliation_calls = next_reconciliation_calls;
        metrics.commit_reconciliation_wall_ns = started.elapsed().as_nanos();
        result
    }

    #[cfg(test)]
    fn install_different_complete_head_after_commit(
        &self,
        requested: &VisibleHead,
    ) -> AnyResult<()> {
        let connection = Connection::open(&self.path)?;
        let generation = requested
            .0
            .checked_add(1)
            .ok_or(CoreError::LengthOverflow)?;
        let child = requested.1;
        let transition = requested.2;
        let receipt = ValidatedSnapshotReceiptV1 {
            store_instance_id: self.store_instance_id,
            validation_authority_id: self.validation_authority_id,
            integrity_epoch: self.integrity_epoch,
            head_generation: generation,
            child_root_id: child,
            transition_id: transition,
            mapping_profile_id: ObjectId::from_bytes(&self.profile)?,
        }
        .encode(&self.validation_key)?;
        connection.execute(
            "INSERT OR REPLACE INTO wp4m_visible_head
             (id, generation, child, transition, validation_receipt)
             VALUES (1, ?1, ?2, ?3, ?4)",
            params![
                generation.to_be_bytes().as_slice(),
                child.as_bytes().as_slice(),
                transition.as_bytes().as_slice(),
                receipt.as_slice(),
            ],
        )?;
        Ok(())
    }

    fn publish(
        &mut self,
        expected_head: Option<&VisibleHead>,
        child: ObjectId,
        transition: ObjectId,
        metrics: &mut Metrics,
    ) -> AnyResult<PublicationOutcome> {
        #[cfg(test)]
        let fault = self.next_publish_fault.take();
        #[cfg(not(test))]
        let fault = None;
        let started = Instant::now();
        let result = self.publish_with_fault(expected_head, child, transition, fault, metrics);
        metrics.commit_publish_call_wall_ns = started.elapsed().as_nanos();
        let provenance = result?;
        if provenance.dominant.is_some() {
            return Err(PublicationFailure(provenance).into());
        }
        if provenance.first.is_some()
            || provenance.cleanup_first.is_some()
            || provenance.reconciliation_error.is_some()
        {
            Ok(PublicationOutcome {
                status: PublicationStatus::RequestedVisible,
                diagnostic: Some(provenance),
            })
        } else {
            Ok(PublicationOutcome {
                status: PublicationStatus::Committed,
                diagnostic: None,
            })
        }
    }

    fn publish_with_fault(
        &mut self,
        expected_head: Option<&VisibleHead>,
        child: ObjectId,
        transition: ObjectId,
        fault: Option<PublishFault>,
        metrics: &mut Metrics,
    ) -> AnyResult<FailureProvenance> {
        if self.active_transaction.is_none() {
            return Err(CoreError::ValidationAuthorityUnavailable.into());
        }
        let _current_head_receipt_charge = expected_head
            .map(|_| charge_capacity(metrics, 216))
            .transpose()?;
        let current = self.current_head_accounted(metrics)?;
        if current.as_ref() != expected_head {
            return Err(CoreError::PublicationConflict.into());
        }
        let generation = current
            .as_ref()
            .map_or(0, |head| head.0)
            .checked_add(1)
            .ok_or(CoreError::LengthOverflow)?;
        let _requested_receipt_charge = charge_capacity(metrics, 216)?;
        let receipt_bytes = ValidatedSnapshotReceiptV1 {
            store_instance_id: self.store_instance_id,
            validation_authority_id: self.validation_authority_id,
            integrity_epoch: self.integrity_epoch,
            head_generation: generation,
            child_root_id: child,
            transition_id: transition,
            mapping_profile_id: ObjectId::from_bytes(&self.profile)?,
        }
        .encode(&self.validation_key)?;
        let requested = (generation, child, transition, receipt_bytes);
        let request_key = self.publication_key(current.as_ref(), &requested)?;
        let changed = match expected_head {
            None => self.connection.execute(
                "INSERT INTO wp4m_visible_head
                 (id, generation, child, transition, validation_receipt)
                 VALUES (1, ?1, ?2, ?3, ?4)",
                params![
                    generation.to_be_bytes().as_slice(),
                    child.as_bytes().as_slice(),
                    transition.as_bytes().as_slice(),
                    receipt_bytes.as_slice()
                ],
            )?,
            Some((prior_generation, prior_child, prior_transition, prior_receipt)) => {
                self.connection.execute(
                    "UPDATE wp4m_visible_head
                     SET generation=?1, child=?2, transition=?3, validation_receipt=?4
                     WHERE id=1 AND generation=?5 AND child=?6 AND transition=?7
                       AND validation_receipt=?8",
                    params![
                        generation.to_be_bytes().as_slice(),
                        child.as_bytes().as_slice(),
                        transition.as_bytes().as_slice(),
                        receipt_bytes.as_slice(),
                        prior_generation.to_be_bytes().as_slice(),
                        prior_child.as_bytes().as_slice(),
                        prior_transition.as_bytes().as_slice(),
                        prior_receipt.as_slice(),
                    ],
                )?
            }
        };
        if changed != 1 {
            return Err(CoreError::PublicationConflict.into());
        }
        observe_execute_call(metrics, changed)?;
        add(&mut metrics.row_blob_writes, 4)?;
        if fault == Some(PublishFault::BeforeCommit) {
            let cleanup_first = self
                .rollback(metrics)
                .err()
                .map(|error| failure_cause(error.as_ref()));
            return Ok(failure_provenance(
                Some(FailureCause::Core(CoreError::Io)),
                cleanup_first,
                Reconciliation::NotAttempted,
                None,
            ));
        }
        let next_commits = metrics
            .commits
            .checked_add(1)
            .ok_or(CoreError::LengthOverflow)?;
        let next_commit_returns = metrics
            .commit_returns
            .checked_add(1)
            .ok_or(CoreError::LengthOverflow)?;
        let next_commit_return_successes = metrics
            .commit_return_successes
            .checked_add(1)
            .ok_or(CoreError::LengthOverflow)?;
        let next_commit_return_errors = metrics
            .commit_return_errors
            .checked_add(1)
            .ok_or(CoreError::LengthOverflow)?;
        let next_reconciliation_calls = metrics
            .commit_reconciliation_calls
            .checked_add(1)
            .ok_or(CoreError::LengthOverflow)?;
        let next_sql_execute_calls = metrics
            .sql_execute_calls
            .checked_add(1)
            .ok_or(CoreError::LengthOverflow)?;
        let invalidated_authority_serial = self
            .same_open_authority_serial
            .checked_add(1)
            .ok_or(CoreError::LengthOverflow)?;
        metrics.sql_execute_calls = next_sql_execute_calls;
        metrics.commits = next_commits;
        metrics.sqlite_status_before_dispatch = self.sqlite_status_snapshot();
        metrics.commit_dispatch_filesystem = self.filesystem_snapshot();
        let commit_started = Instant::now();
        let commit_result = self.connection.execute_batch("COMMIT");
        metrics.commit_dispatch_to_return_wall_ns = commit_started.elapsed().as_nanos();
        metrics.commit_returns = next_commit_returns;
        metrics.sqlite_status_after_return = self.sqlite_status_snapshot();
        metrics.commit_return_filesystem = self.filesystem_snapshot();
        if commit_result.is_err() {
            metrics.commit_return_errors = next_commit_return_errors;
            self.active_transaction = None;
            self.same_open_authority_serial = invalidated_authority_serial;
            let (reconciliation, reconciliation_error) = self.reconcile_publication_observed(
                current.as_ref(),
                &requested,
                request_key,
                next_reconciliation_calls,
                metrics,
            );
            let cleanup_first = if reconciliation == Reconciliation::RequestedVisible {
                None
            } else {
                self.connection
                    .execute_batch("ROLLBACK")
                    .err()
                    .map(|error| failure_cause(&error))
            };
            return Ok(failure_provenance(
                Some(FailureCause::Core(CoreError::Io)),
                cleanup_first,
                reconciliation,
                reconciliation_error,
            ));
        }
        metrics.commit_return_successes = next_commit_return_successes;
        self.active_transaction = None;
        self.same_open_authority_serial = invalidated_authority_serial;
        #[allow(unused_mut)]
        let mut lost_ack = fault == Some(PublishFault::AfterCommitBeforeAck);
        #[cfg(test)]
        {
            lost_ack |= matches!(
                fault,
                Some(PublishFault::AfterCommitDifferentHead | PublishFault::AfterCommitUnavailable)
            );
        }
        if lost_ack {
            #[cfg(test)]
            if fault == Some(PublishFault::AfterCommitDifferentHead) {
                if let Err(error) = self.install_different_complete_head_after_commit(&requested) {
                    return Ok(failure_provenance(
                        Some(FailureCause::Core(CoreError::Io)),
                        None,
                        Reconciliation::Ambiguous,
                        Some(failure_cause(error.as_ref())),
                    ));
                }
            }
            #[cfg(test)]
            let unavailable_path = if fault == Some(PublishFault::AfterCommitUnavailable) {
                let mut hidden = self.path.as_os_str().to_os_string();
                hidden.push(".reconciliation-unavailable");
                let hidden = PathBuf::from(hidden);
                if let Err(error) = fs::rename(&self.path, &hidden) {
                    return Ok(failure_provenance(
                        Some(FailureCause::Core(CoreError::Io)),
                        None,
                        Reconciliation::Ambiguous,
                        Some(failure_cause(&error)),
                    ));
                }
                Some(hidden)
            } else {
                None
            };
            #[allow(unused_mut)]
            let (mut reconciliation, mut reconciliation_error) = self
                .reconcile_publication_observed(
                    current.as_ref(),
                    &requested,
                    request_key,
                    next_reconciliation_calls,
                    metrics,
                );
            #[cfg(test)]
            if let Some(hidden) = unavailable_path {
                if let Err(error) = fs::rename(hidden, &self.path) {
                    reconciliation = Reconciliation::Ambiguous;
                    reconciliation_error = Some(failure_cause(&error));
                }
            }
            return Ok(failure_provenance(
                Some(FailureCause::Core(CoreError::Io)),
                None,
                reconciliation,
                reconciliation_error,
            ));
        }
        Ok(failure_provenance(
            None,
            None,
            Reconciliation::RequestedVisible,
            None,
        ))
    }

    fn sqlite_db_status(&self, operation: i32, reset: bool) -> Result<u64, SqliteStatusError> {
        let mut current = 0_i32;
        let mut high_water = 0_i32;
        // SAFETY: this private benchmark is synchronous and single-threaded;
        // `self` owns a live Connection for the entire call, no concurrent
        // SQLite operation can close/mutate its handle, and both stack output
        // pointers remain valid until sqlite3_db_status returns.
        let result = unsafe {
            ffi::sqlite3_db_status(
                self.connection.handle(),
                operation,
                &mut current,
                &mut high_water,
                i32::from(reset),
            )
        };
        checked_sqlite_status_current(result, current)
    }

    fn sqlite_status_snapshot(&self) -> SqliteStatusSnapshot {
        let page_cache_used_bytes = self.sqlite_db_status(ffi::SQLITE_DBSTATUS_CACHE_USED, false);
        let cache_hits = self.sqlite_db_status(ffi::SQLITE_DBSTATUS_CACHE_HIT, false);
        let cache_misses = self.sqlite_db_status(ffi::SQLITE_DBSTATUS_CACHE_MISS, false);
        let dirty_pages_written = self.sqlite_db_status(ffi::SQLITE_DBSTATUS_CACHE_WRITE, false);
        let cache_spill_pages = self.sqlite_db_status(ffi::SQLITE_DBSTATUS_CACHE_SPILL, false);
        let errors = [
            &page_cache_used_bytes,
            &cache_hits,
            &cache_misses,
            &dirty_pages_written,
            &cache_spill_pages,
        ]
        .into_iter()
        .filter(|result| result.is_err())
        .count() as u64;
        SqliteStatusSnapshot {
            page_cache_used_bytes: page_cache_used_bytes.ok(),
            cache_hits: cache_hits.ok(),
            cache_misses: cache_misses.ok(),
            dirty_pages_written: dirty_pages_written.ok(),
            cache_spill_pages: cache_spill_pages.ok(),
            read_calls: 5,
            errors,
        }
    }

    fn start_sqlite_observations(&self, metrics: &mut Metrics) {
        let reset_results = [
            ffi::SQLITE_DBSTATUS_CACHE_HIT,
            ffi::SQLITE_DBSTATUS_CACHE_MISS,
            ffi::SQLITE_DBSTATUS_CACHE_WRITE,
            ffi::SQLITE_DBSTATUS_CACHE_SPILL,
        ]
        .map(|operation| self.sqlite_db_status(operation, true));
        metrics.measurement_status_reset_calls = 4;
        metrics.measurement_status_reset_errors = reset_results
            .iter()
            .filter(|result| result.is_err())
            .count() as u64;
        let page_size = self
            .connection
            .query_row("PRAGMA page_size", [], |row| row.get::<_, i64>(0))
            .ok();
        metrics.measurement_sql_queries = 1;
        metrics.measurement_sql_rows = u64::from(page_size.is_some());
        metrics.sqlite_page_size = page_size.and_then(|value| u64::try_from(value).ok());
        metrics.sqlite_status_before = self.sqlite_status_snapshot();
    }

    fn filesystem_snapshot(&self) -> PhysicalSnapshot {
        let mut journal = self.path.as_os_str().to_os_string();
        journal.push("-journal");
        let journal = PathBuf::from(journal);
        PhysicalSnapshot {
            apparent_database: apparent_file_bytes(&self.path),
            apparent_journal: apparent_file_bytes(&journal),
            apparent_authority: apparent_file_bytes(&self.authority_path),
            allocated_database: allocated_file_bytes(&self.path),
            allocated_journal: allocated_file_bytes(&journal),
            allocated_authority: allocated_file_bytes(&self.authority_path),
            ..PhysicalSnapshot::default()
        }
    }

    fn physical_snapshot(&self) -> PhysicalSnapshot {
        let page_count = self
            .connection
            .query_row("PRAGMA page_count", [], |row| row.get::<_, i64>(0))
            .ok();
        let page_size = page_count.and_then(|_| {
            self.connection
                .query_row("PRAGMA page_size", [], |row| row.get::<_, i64>(0))
                .ok()
        });
        let logical_database = page_count
            .zip(page_size)
            .and_then(|(pages, page_size)| pages.checked_mul(page_size))
            .and_then(|bytes| u64::try_from(bytes).ok());
        PhysicalSnapshot {
            logical_database,
            measurement_sql_queries: 1 + u64::from(page_count.is_some()),
            measurement_sql_rows: u64::from(page_count.is_some()) + u64::from(page_size.is_some()),
            ..self.filesystem_snapshot()
        }
    }
}

const Q_CONSTRUCTION_STATE_BYTES: usize = 4_096;
const Q_PUT_EVIDENCE_BYTES: usize = 80;

fn exact_vec_capacity<T>(capacity: usize) -> CoreResult<Vec<T>> {
    let mut values = Vec::new();
    values
        .try_reserve_exact(capacity)
        .map_err(|_| CoreError::AllocationFailed)?;
    if values.capacity() != capacity {
        return Err(CoreError::AllocationFailed);
    }
    Ok(values)
}

fn construction_frontier_bytes(
    candidate: Candidate,
    expected_references: u64,
) -> CoreResult<(usize, usize)> {
    let profile = file_codec::FileMappingProfile::new(candidate.k, candidate.f);
    let height = usize::from(file_codec::expected_file_level(
        expected_references,
        profile,
    )?);
    let leaf_count = if expected_references == 0 {
        0
    } else {
        expected_references
            .checked_add(u64::try_from(candidate.k - 1).map_err(|_| CoreError::LengthOverflow)?)
            .ok_or(CoreError::LengthOverflow)?
            / u64::try_from(candidate.k).map_err(|_| CoreError::LengthOverflow)?
    };
    let fanout = u64::try_from(candidate.f).map_err(|_| CoreError::LengthOverflow)?;
    let mut top_children = leaf_count;
    for _ in 0..height {
        top_children = top_children
            .checked_add(fanout - 1)
            .ok_or(CoreError::LengthOverflow)?
            / fanout;
    }
    let exact_full_topology = top_children == fanout;
    let levels = height
        .checked_add(1)
        .and_then(|value| value.checked_add(usize::from(exact_full_topology)))
        .ok_or(CoreError::LengthOverflow)?;
    let frontier_bytes = candidate
        .k
        .checked_mul(std::mem::size_of::<file_codec::FileReference>())
        .and_then(|value| {
            levels
                .checked_mul(
                    std::mem::size_of::<Vec<file_codec::FileChild>>()
                        .checked_add(
                            candidate
                                .f
                                .checked_mul(std::mem::size_of::<file_codec::FileChild>())?,
                        )?
                        .checked_add(std::mem::size_of::<u64>())?
                        .checked_add(std::mem::size_of::<Vec<ConstructionNodeProof>>())?
                        .checked_add(
                            candidate
                                .f
                                .checked_mul(std::mem::size_of::<ConstructionNodeProof>())?,
                        )?,
                )
                .and_then(|levels_bytes| value.checked_add(levels_bytes))
        })
        .ok_or(CoreError::LengthOverflow)?;
    Ok((levels, frontier_bytes))
}

struct ConstructionState {
    source_hasher: Hasher,
    sequence_hasher: Hasher,
    proof_levels: Vec<Vec<ConstructionNodeProof>>,
    store_instance_id: [u8; 16],
    validation_authority_id: [u8; 32],
    profile: [u8; 32],
    open_identity: u64,
    transaction_identity: u64,
    authority_serial: u64,
    integrity_epoch: u64,
    last_mutation_serial: u64,
    expected_references: u64,
    leaf_references: u64,
    leaf_total: u64,
    _fixed_charge: CapacityCharge,
    _frontier_charge: CapacityCharge,
    _evidence_slot_charge: CapacityCharge,
}

struct ConstructionScopeProof {
    store_instance_id: [u8; 16],
    validation_authority_id: [u8; 32],
    profile: [u8; 32],
    open_identity: u64,
    transaction_identity: u64,
    authority_serial: u64,
    integrity_epoch: u64,
    last_mutation_serial: u64,
    _charge: CapacityCharge,
}

struct FileConstructionProof {
    scope: ConstructionScopeProof,
    source_fingerprint: [u8; 32],
    cdc_sequence: [u8; 32],
    file: ConstructionNodeProof,
}

struct WorkspaceConstructionProof {
    file: FileConstructionProof,
    root: ObjectId,
}

struct FullCreateConstructionProof {
    workspace: WorkspaceConstructionProof,
    transition: ObjectId,
    expectation: Option<FullCreateExpectation>,
    consumed: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct FullCreateExpectation {
    source_fingerprint: [u8; 32],
    cdc_sequence: [u8; 32],
    references: u64,
    root: ObjectId,
    transition: ObjectId,
    closure: [u8; 32],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct FullCreateQualification {
    source_fingerprint: [u8; 32],
    cdc_sequence: [u8; 32],
    references: u64,
    total_raw: u64,
    root: ObjectId,
    transition: ObjectId,
    closure: [u8; 32],
}

fn full_create_expectation(
    base: Option<(ObjectId, ObjectId, [u8; 32])>,
    references: Option<u64>,
    fingerprint: Option<&str>,
    sequence: Option<&str>,
) -> AnyResult<FullCreateExpectation> {
    let (root, transition, closure) = base.ok_or(CoreError::PublicationConflict)?;
    Ok(FullCreateExpectation {
        source_fingerprint: ObjectId::from_bytes(&decode_hex(
            fingerprint.ok_or(CoreError::PublicationConflict)?,
        )?)?
        .to_bytes(),
        cdc_sequence: ObjectId::from_bytes(&decode_hex(
            sequence.ok_or(CoreError::PublicationConflict)?,
        )?)?
        .to_bytes(),
        references: references.ok_or(CoreError::PublicationConflict)?,
        root,
        transition,
        closure,
    })
}

enum RootConstructionCoverage {
    Children(Vec<ConstructionNodeProof>),
    Collapsed(ConstructionNodeProof),
}

impl ConstructionState {
    fn new(
        store: &Store,
        candidate: Candidate,
        expected_references: u64,
        metrics: &mut Metrics,
    ) -> AnyResult<(Self, usize)> {
        if std::mem::size_of::<PutEvidence>() != Q_PUT_EVIDENCE_BYTES
            || std::mem::size_of::<ConstructionNodeProof>() != 64
            || std::mem::size_of::<Self>() > Q_CONSTRUCTION_STATE_BYTES
        {
            return Err(CoreError::AllocationFailed.into());
        }
        let transaction_identity = store
            .active_transaction
            .ok_or(CoreError::ValidationAuthorityUnavailable)?
            .identity;
        let (levels, frontier_bytes) = construction_frontier_bytes(candidate, expected_references)?;
        let fixed_charge = charge_capacity(metrics, Q_CONSTRUCTION_STATE_BYTES)?;
        let frontier_charge = charge_capacity(metrics, frontier_bytes)?;
        let evidence_slot_charge = charge_capacity(metrics, Q_PUT_EVIDENCE_BYTES)?;
        let mut proof_levels = exact_vec_capacity(levels)?;
        for _ in 0..levels {
            proof_levels.push(exact_vec_capacity(candidate.f)?);
        }
        Ok((
            Self {
                source_hasher: Hasher::new(),
                sequence_hasher: Hasher::new(),
                proof_levels,
                store_instance_id: store.store_instance_id,
                validation_authority_id: store.validation_authority_id,
                profile: store.profile,
                open_identity: store.open_identity,
                transaction_identity,
                authority_serial: store.same_open_authority_serial,
                integrity_epoch: store.integrity_epoch,
                last_mutation_serial: store.mutation_serial,
                expected_references,
                leaf_references: 0,
                leaf_total: 0,
                _fixed_charge: fixed_charge,
                _frontier_charge: frontier_charge,
                _evidence_slot_charge: evidence_slot_charge,
            },
            levels,
        ))
    }

    fn accept_put(
        &mut self,
        evidence: PutEvidence,
        expected_id: ObjectId,
        expected_kind: ObjectKind,
        expected_len: usize,
        metrics: &mut Metrics,
    ) -> CoreResult<()> {
        let expected_mutation = self
            .last_mutation_serial
            .checked_add(1)
            .ok_or(CoreError::LengthOverflow)?;
        if evidence.object_id != expected_id
            || evidence.kind != expected_kind
            || evidence.canonical_len
                != u64::try_from(expected_len).map_err(|_| CoreError::LengthOverflow)?
            || evidence.open_identity != self.open_identity
            || evidence.transaction_identity != self.transaction_identity
            || evidence.authority_serial != self.authority_serial
            || evidence.mutation_serial != expected_mutation
        {
            return Err(CoreError::ValidationAuthorityUnavailable);
        }
        self.last_mutation_serial = evidence.mutation_serial;
        add(&mut metrics.construction_put_evidences, 1)
    }

    fn observe_chunk(
        &mut self,
        evidence: PutEvidence,
        reference: file_codec::FileReference,
        bytes: &[u8],
        metrics: &mut Metrics,
    ) -> CoreResult<()> {
        self.accept_put(
            evidence,
            reference.object_id,
            ObjectKind::Bytes,
            bytes
                .len()
                .checked_add(layerfs_core::object::HEADER_LEN + 4)
                .ok_or(CoreError::LengthOverflow)?,
            metrics,
        )?;
        if chunk_id(bytes) != reference.raw_id
            || u32::try_from(bytes.len()).map_err(|_| CoreError::LengthOverflow)?
                != reference.raw_length
        {
            return Err(CoreError::ChunkIdentityMismatch);
        }
        self.source_hasher.update(bytes);
        self.sequence_hasher
            .update(&reference.raw_length.to_be_bytes());
        self.sequence_hasher.update(reference.raw_id.as_bytes());
        self.leaf_references = self
            .leaf_references
            .checked_add(1)
            .ok_or(CoreError::LengthOverflow)?;
        self.leaf_total = self
            .leaf_total
            .checked_add(u64::from(reference.raw_length))
            .ok_or(CoreError::LengthOverflow)?;
        add_len(&mut metrics.construction_source_hash_bytes, bytes.len())?;
        add(&mut metrics.construction_cdc_entries, 1)
    }

    fn fold_leaf(
        &mut self,
        evidence: PutEvidence,
        id: ObjectId,
        canonical_len: usize,
        references: &[file_codec::FileReference],
        metrics: &mut Metrics,
    ) -> CoreResult<ConstructionNodeProof> {
        let references_count =
            u64::try_from(references.len()).map_err(|_| CoreError::LengthOverflow)?;
        let total = references.iter().try_fold(0_u64, |sum, reference| {
            sum.checked_add(u64::from(reference.raw_length))
                .ok_or(CoreError::LengthOverflow)
        })?;
        if references_count == 0
            || self.leaf_references != references_count
            || self.leaf_total != total
        {
            return Err(CoreError::NonCanonicalPagePartition);
        }
        self.accept_put(evidence, id, ObjectKind::Bytes, canonical_len, metrics)?;
        self.leaf_references = 0;
        self.leaf_total = 0;
        add(&mut metrics.construction_edges_covered, references_count)?;
        add(&mut metrics.construction_leaf_summaries, 1)?;
        Ok(ConstructionNodeProof {
            object_id: id,
            total_raw: total,
            references: references_count,
            transaction_identity: self.transaction_identity,
            level: 0,
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn fold_branch(
        &mut self,
        evidence: PutEvidence,
        id: ObjectId,
        canonical_len: usize,
        level: usize,
        children: &[file_codec::FileChild],
        proofs: Vec<ConstructionNodeProof>,
        metrics: &mut Metrics,
    ) -> CoreResult<ConstructionNodeProof> {
        if children.is_empty() || children.len() != proofs.len() {
            return Err(CoreError::NonCanonicalPagePartition);
        }
        let mut total = 0_u64;
        let mut references = 0_u64;
        for (child, proof) in children.iter().zip(proofs) {
            if proof.object_id != child.object_id
                || usize::from(proof.level) != level
                || proof.transaction_identity != self.transaction_identity
            {
                return Err(CoreError::WrongLogicalRole);
            }
            total = total
                .checked_add(proof.total_raw)
                .ok_or(CoreError::LengthOverflow)?;
            references = references
                .checked_add(proof.references)
                .ok_or(CoreError::LengthOverflow)?;
            if child.cumulative_end != total {
                return Err(CoreError::LengthMismatch {
                    expected: child.cumulative_end,
                    actual: total,
                });
            }
        }
        self.accept_put(evidence, id, ObjectKind::Bytes, canonical_len, metrics)?;
        add(
            &mut metrics.construction_edges_covered,
            u64::try_from(children.len()).map_err(|_| CoreError::LengthOverflow)?,
        )?;
        add(&mut metrics.construction_branch_summaries, 1)?;
        Ok(ConstructionNodeProof {
            object_id: id,
            total_raw: total,
            references,
            transaction_identity: self.transaction_identity,
            level: u8::try_from(level + 1).map_err(|_| CoreError::MappingDepthExceeded)?,
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn finish_file(
        mut self,
        evidence: PutEvidence,
        id: ObjectId,
        canonical_len: usize,
        level: usize,
        total_raw: u64,
        references: u64,
        children: &[file_codec::FileChild],
        coverage: RootConstructionCoverage,
        metrics: &mut Metrics,
    ) -> CoreResult<FileConstructionProof> {
        if self.leaf_references != 0
            || self.leaf_total != 0
            || references != self.expected_references
        {
            return Err(CoreError::NonCanonicalPagePartition);
        }
        match coverage {
            RootConstructionCoverage::Children(proofs) => {
                if children.len() != proofs.len() {
                    return Err(CoreError::NonCanonicalPagePartition);
                }
                let mut actual_total = 0_u64;
                let mut actual_references = 0_u64;
                for (child, proof) in children.iter().zip(proofs) {
                    if child.object_id != proof.object_id
                        || usize::from(proof.level) != level
                        || proof.transaction_identity != self.transaction_identity
                    {
                        return Err(CoreError::WrongLogicalRole);
                    }
                    actual_total = actual_total
                        .checked_add(proof.total_raw)
                        .ok_or(CoreError::LengthOverflow)?;
                    actual_references = actual_references
                        .checked_add(proof.references)
                        .ok_or(CoreError::LengthOverflow)?;
                    if child.cumulative_end != actual_total {
                        return Err(CoreError::LengthMismatch {
                            expected: child.cumulative_end,
                            actual: actual_total,
                        });
                    }
                }
                if actual_total != total_raw || actual_references != references {
                    return Err(CoreError::LengthMismatch {
                        expected: total_raw,
                        actual: actual_total,
                    });
                }
            }
            RootConstructionCoverage::Collapsed(proof) => {
                if proof.transaction_identity != self.transaction_identity
                    || proof.total_raw != total_raw
                    || proof.references != references
                    || usize::from(proof.level) <= level
                    || children.last().map_or(0, |child| child.cumulative_end) != total_raw
                {
                    return Err(CoreError::LengthMismatch {
                        expected: total_raw,
                        actual: proof.total_raw,
                    });
                }
            }
        }
        self.accept_put(evidence, id, ObjectKind::Bytes, canonical_len, metrics)?;
        add(
            &mut metrics.construction_edges_covered,
            u64::try_from(children.len()).map_err(|_| CoreError::LengthOverflow)?,
        )?;
        add(&mut metrics.construction_file_summaries, 1)?;
        add(&mut metrics.construction_source_hashes, 1)?;
        drop(self._frontier_charge);
        self._fixed_charge.absorb(self._evidence_slot_charge)?;
        Ok(FileConstructionProof {
            scope: ConstructionScopeProof {
                store_instance_id: self.store_instance_id,
                validation_authority_id: self.validation_authority_id,
                profile: self.profile,
                open_identity: self.open_identity,
                transaction_identity: self.transaction_identity,
                authority_serial: self.authority_serial,
                integrity_epoch: self.integrity_epoch,
                last_mutation_serial: self.last_mutation_serial,
                _charge: self._fixed_charge,
            },
            source_fingerprint: *self.source_hasher.finalize().as_bytes(),
            cdc_sequence: *self.sequence_hasher.finalize().as_bytes(),
            file: ConstructionNodeProof {
                object_id: id,
                total_raw,
                references,
                transaction_identity: self.transaction_identity,
                level: u8::try_from(level).map_err(|_| CoreError::MappingDepthExceeded)?,
            },
        })
    }
}

impl ConstructionScopeProof {
    fn accept_put(
        &mut self,
        evidence: PutEvidence,
        id: ObjectId,
        kind: ObjectKind,
        metrics: &mut Metrics,
    ) -> CoreResult<()> {
        let expected_mutation = self
            .last_mutation_serial
            .checked_add(1)
            .ok_or(CoreError::LengthOverflow)?;
        if evidence.object_id != id
            || evidence.kind != kind
            || evidence.open_identity != self.open_identity
            || evidence.transaction_identity != self.transaction_identity
            || evidence.authority_serial != self.authority_serial
            || evidence.mutation_serial != expected_mutation
        {
            return Err(CoreError::ValidationAuthorityUnavailable);
        }
        self.last_mutation_serial = evidence.mutation_serial;
        add(&mut metrics.construction_put_evidences, 1)
    }
}

impl FileConstructionProof {
    fn fold_workspace(
        mut self,
        evidence: PutEvidence,
        root: ObjectId,
        metrics: &mut Metrics,
    ) -> CoreResult<WorkspaceConstructionProof> {
        self.scope
            .accept_put(evidence, root, ObjectKind::Directory, metrics)?;
        add(&mut metrics.construction_edges_covered, 1)?;
        add(&mut metrics.construction_workspace_summaries, 1)?;
        Ok(WorkspaceConstructionProof { file: self, root })
    }
}

impl WorkspaceConstructionProof {
    fn fold_transition(
        mut self,
        evidence: PutEvidence,
        transition: ObjectId,
        metrics: &mut Metrics,
    ) -> CoreResult<FullCreateConstructionProof> {
        self.file
            .scope
            .accept_put(evidence, transition, ObjectKind::Bytes, metrics)?;
        add(&mut metrics.construction_edges_covered, 1)?;
        add(&mut metrics.construction_transition_summaries, 1)?;
        Ok(FullCreateConstructionProof {
            workspace: self,
            transition,
            expectation: None,
            consumed: false,
        })
    }
}

impl FullCreateConstructionProof {
    fn bind_expectation(&mut self, expected: FullCreateExpectation) -> CoreResult<()> {
        let file = &self.workspace.file;
        if self.expectation.is_some()
            || file.source_fingerprint != expected.source_fingerprint
            || file.cdc_sequence != expected.cdc_sequence
            || file.file.references != expected.references
            || self.workspace.root != expected.root
            || self.transition != expected.transition
        {
            return Err(CoreError::PublicationConflict);
        }
        self.expectation = Some(expected);
        Ok(())
    }

    fn consume(
        &mut self,
        store: &mut Store,
        expected: FullCreateExpectation,
        metrics: &mut Metrics,
    ) -> AnyResult<FullCreateQualification> {
        if self.consumed {
            return Err(CoreError::ValidationAuthorityUnavailable.into());
        }
        self.consumed = true;
        if self.expectation != Some(expected) {
            return Err(CoreError::PublicationConflict.into());
        }
        let scope = &self.workspace.file.scope;
        let transaction = store
            .active_transaction
            .as_mut()
            .ok_or(CoreError::ValidationAuthorityUnavailable)?;
        if !transaction.construction_proof_issued
            || transaction.construction_proof_consumed
            || transaction.identity != scope.transaction_identity
        {
            return Err(CoreError::ValidationAuthorityUnavailable.into());
        }
        transaction.construction_proof_consumed = true;
        if scope.open_identity != store.open_identity
            || scope.store_instance_id != store.store_instance_id
            || scope.validation_authority_id != store.validation_authority_id
            || scope.integrity_epoch != store.integrity_epoch
            || scope.profile != store.profile
            || scope.authority_serial != store.same_open_authority_serial
            || scope.last_mutation_serial != store.mutation_serial
        {
            return Err(CoreError::InvalidValidationReceipt.into());
        }
        if store.current_head_accounted(metrics)?.is_some() {
            return Err(CoreError::PublicationConflict.into());
        }
        let file = &self.workspace.file;
        if file.source_fingerprint != expected.source_fingerprint
            || file.cdc_sequence != expected.cdc_sequence
            || file.file.references != expected.references
            || self.workspace.root != expected.root
            || self.transition != expected.transition
        {
            return Err(CoreError::PublicationConflict.into());
        }
        add(&mut metrics.construction_proof_consumptions, 1)?;
        Ok(FullCreateQualification {
            source_fingerprint: file.source_fingerprint,
            cdc_sequence: file.cdc_sequence,
            references: file.file.references,
            total_raw: file.file.total_raw,
            root: self.workspace.root,
            transition: self.transition,
            closure: expected.closure,
        })
    }
}

struct FileBuilder {
    candidate: Candidate,
    leaf: Vec<file_codec::FileReference>,
    levels: Vec<Vec<file_codec::FileChild>>,
    level_totals: Vec<u64>,
    total_raw: u64,
    references: u64,
    construction: Option<ConstructionState>,
}

impl FileBuilder {
    fn new(candidate: Candidate) -> Self {
        Self {
            candidate,
            leaf: Vec::with_capacity(candidate.k),
            levels: Vec::new(),
            level_totals: Vec::new(),
            total_raw: 0,
            references: 0,
            construction: None,
        }
    }

    fn new_proving(
        candidate: Candidate,
        expected_references: u64,
        store: &Store,
        metrics: &mut Metrics,
    ) -> AnyResult<Self> {
        let (construction, level_count) =
            ConstructionState::new(store, candidate, expected_references, metrics)?;
        let leaf = exact_vec_capacity(candidate.k)?;
        let mut levels = exact_vec_capacity(level_count)?;
        let mut level_totals = exact_vec_capacity(level_count)?;
        for _ in 0..level_count {
            levels.push(exact_vec_capacity(candidate.f)?);
            level_totals.push(0);
        }
        Ok(Self {
            candidate,
            leaf,
            levels,
            level_totals,
            total_raw: 0,
            references: 0,
            construction: Some(construction),
        })
    }

    fn push_bytes(
        &mut self,
        store: &mut Store,
        bytes: &[u8],
        metrics: &mut Metrics,
    ) -> AnyResult<()> {
        add_len(&mut metrics.source_bytes_read, bytes.len())?;
        add_len(&mut metrics.source_cdc_bytes_read, bytes.len())?;
        add_len(&mut metrics.canonical_stage_source_bytes_read, bytes.len())?;
        let raw_length = u32::try_from(bytes.len()).map_err(|_| CoreError::LengthOverflow)?;
        let raw_id = chunk_id_accounted(bytes, metrics)?;
        add(&mut metrics.borrowed_source_encode_calls, 1)?;
        add_len(&mut metrics.borrowed_source_encode_input_bytes, bytes.len())?;
        let (object_id, evidence) = if self.construction.is_some() {
            let (object_id, _, evidence) =
                store.put_generated_bytes_with_evidence(bytes, metrics)?;
            (object_id, Some(evidence))
        } else {
            (store.put_generated_bytes(bytes, metrics)?.0, None)
        };
        let reference = file_codec::FileReference {
            raw_id,
            raw_length,
            object_id,
        };
        if let Some(construction) = self.construction.as_mut() {
            construction.observe_chunk(
                evidence.ok_or(CoreError::ValidationAuthorityUnavailable)?,
                reference,
                bytes,
                metrics,
            )?;
        }
        add(&mut metrics.chunks, 1)?;
        self.push_reference(store, reference, metrics)
    }

    fn push_reference(
        &mut self,
        store: &mut Store,
        reference: file_codec::FileReference,
        metrics: &mut Metrics,
    ) -> AnyResult<()> {
        self.total_raw = self
            .total_raw
            .checked_add(u64::from(reference.raw_length))
            .ok_or(CoreError::LengthOverflow)?;
        self.references = self
            .references
            .checked_add(1)
            .ok_or(CoreError::LengthOverflow)?;
        add(&mut metrics.references, 1)?;
        self.leaf.push(reference);
        observe_file_references(metrics, self.leaf.len())?;
        if self.leaf.len() == self.candidate.k {
            self.flush_leaf_with_store(store, metrics)?;
        }
        Ok(())
    }

    fn seed_reference(&mut self, reference: file_codec::FileReference) -> AnyResult<()> {
        if self.construction.is_some() {
            return Err(CoreError::ValidationAuthorityUnavailable.into());
        }
        if self.leaf.len() >= self.candidate.k {
            return Err(CoreError::NonCanonicalPagePartition.into());
        }
        self.total_raw = self
            .total_raw
            .checked_add(u64::from(reference.raw_length))
            .ok_or(CoreError::LengthOverflow)?;
        self.references = self
            .references
            .checked_add(1)
            .ok_or(CoreError::LengthOverflow)?;
        self.leaf.push(reference);
        Ok(())
    }

    fn seed_node(
        &mut self,
        level: usize,
        object_id: ObjectId,
        raw_length: u64,
        references: u64,
    ) -> AnyResult<()> {
        if self.construction.is_some() {
            return Err(CoreError::ValidationAuthorityUnavailable.into());
        }
        while self.levels.len() <= level {
            self.levels.push(Vec::with_capacity(self.candidate.f));
            self.level_totals.push(0);
        }
        if self.levels[level].len() >= self.candidate.f {
            return Err(CoreError::NonCanonicalPagePartition.into());
        }
        self.total_raw = self
            .total_raw
            .checked_add(raw_length)
            .ok_or(CoreError::LengthOverflow)?;
        self.references = self
            .references
            .checked_add(references)
            .ok_or(CoreError::LengthOverflow)?;
        self.level_totals[level] = self.level_totals[level]
            .checked_add(raw_length)
            .ok_or(CoreError::LengthOverflow)?;
        self.levels[level].push(file_codec::FileChild {
            object_id,
            cumulative_end: self.level_totals[level],
        });
        Ok(())
    }

    fn flush_level_with_store(
        &mut self,
        store: &mut Store,
        level: usize,
        metrics: &mut Metrics,
    ) -> AnyResult<()> {
        let children = std::mem::take(&mut self.levels[level]);
        let proofs = self
            .construction
            .as_mut()
            .map(|construction| std::mem::take(&mut construction.proof_levels[level]));
        self.level_totals[level] = 0;
        let branch_level = u8::try_from(level + 1).map_err(|_| CoreError::MappingDepthExceeded)?;
        let inner = encode_charged_file_branch(branch_level, &children, metrics)?;
        let (id, canonical_len, evidence) = if self.construction.is_some() {
            let (id, canonical_len, evidence) =
                store.put_generated_bytes_with_evidence(&inner, metrics)?;
            (id, canonical_len, Some(evidence))
        } else {
            let (id, canonical_len) = store.put_generated_bytes(&inner, metrics)?;
            (id, canonical_len, None)
        };
        let proof = match self.construction.as_mut() {
            Some(construction) => Some(construction.fold_branch(
                evidence.ok_or(CoreError::ValidationAuthorityUnavailable)?,
                id,
                canonical_len,
                level,
                &children,
                proofs.ok_or(CoreError::ValidationAuthorityUnavailable)?,
                metrics,
            )?),
            None => None,
        };
        add(&mut metrics.branches, 1)?;
        add_len(&mut metrics.mapping_bytes_rewritten, canonical_len)?;
        let end = children.last().map_or(0, |child| child.cumulative_end);
        self.push_node_with_store(
            store,
            level + 1,
            file_codec::FileChild {
                object_id: id,
                cumulative_end: end,
            },
            proof,
            metrics,
        )
    }

    fn push_node_with_store(
        &mut self,
        store: &mut Store,
        level: usize,
        child: file_codec::FileChild,
        proof: Option<ConstructionNodeProof>,
        metrics: &mut Metrics,
    ) -> AnyResult<()> {
        if self.construction.is_some() && level >= self.levels.len() {
            return Err(CoreError::NonCanonicalPagePartition.into());
        }
        while self.levels.len() <= level {
            self.levels.push(Vec::with_capacity(self.candidate.f));
            self.level_totals.push(0);
        }
        let cumulative_end = self.level_totals[level]
            .checked_add(child.cumulative_end)
            .ok_or(CoreError::LengthOverflow)?;
        self.levels[level].push(file_codec::FileChild {
            object_id: child.object_id,
            cumulative_end,
        });
        self.level_totals[level] = cumulative_end;
        match self.construction.as_mut() {
            Some(construction) => {
                let proof = proof.ok_or(CoreError::ValidationAuthorityUnavailable)?;
                if proof.object_id != child.object_id
                    || proof.total_raw != child.cumulative_end
                    || usize::from(proof.level) != level
                    || proof.transaction_identity != construction.transaction_identity
                {
                    return Err(CoreError::LengthMismatch {
                        expected: child.cumulative_end,
                        actual: proof.total_raw,
                    }
                    .into());
                }
                construction.proof_levels[level].push(proof);
            }
            None if proof.is_some() => return Err(CoreError::ValidationAuthorityUnavailable.into()),
            None => {}
        }
        if self.levels[level].len() == self.candidate.f {
            self.flush_level_with_store(store, level, metrics)?;
        }
        Ok(())
    }

    fn finish(self, store: &mut Store, metrics: &mut Metrics) -> AnyResult<ObjectId> {
        let (id, proof) = self.finish_inner(store, metrics)?;
        if proof.is_some() {
            return Err(CoreError::ValidationAuthorityUnavailable.into());
        }
        Ok(id)
    }

    fn finish_proven(
        self,
        store: &mut Store,
        metrics: &mut Metrics,
    ) -> AnyResult<(ObjectId, FileConstructionProof)> {
        let (id, proof) = self.finish_inner(store, metrics)?;
        Ok((id, proof.ok_or(CoreError::ValidationAuthorityUnavailable)?))
    }

    fn finish_inner(
        mut self,
        store: &mut Store,
        metrics: &mut Metrics,
    ) -> AnyResult<(ObjectId, Option<FileConstructionProof>)> {
        self.flush_leaf_with_store(store, metrics)?;
        loop {
            let nonempty: Vec<usize> = self
                .levels
                .iter()
                .enumerate()
                .filter_map(|(index, children)| (!children.is_empty()).then_some(index))
                .collect();
            if nonempty.len() <= 1 {
                break;
            }
            let level = nonempty[0];
            self.flush_level_with_store(store, level, metrics)?;
        }
        let mut level = self
            .levels
            .iter()
            .enumerate()
            .find_map(|(index, children)| (!children.is_empty()).then_some(index))
            .unwrap_or_default();
        let mut children = if self.levels.is_empty() {
            Vec::new()
        } else {
            std::mem::take(&mut self.levels[level])
        };
        let mut proofs = self
            .construction
            .as_mut()
            .map(|construction| std::mem::take(&mut construction.proof_levels[level]));
        let mut collapsed = None;
        while level > 0 && children.len() == 1 {
            if let Some(level_proofs) = proofs.take() {
                let mut level_proofs = level_proofs.into_iter();
                let proof = level_proofs
                    .next()
                    .ok_or(CoreError::NonCanonicalPagePartition)?;
                if level_proofs.next().is_some()
                    || proof.object_id != children[0].object_id
                    || usize::from(proof.level) != level
                {
                    return Err(CoreError::NonCanonicalPagePartition.into());
                }
                if collapsed.is_none() {
                    collapsed = Some(proof);
                }
            }
            let bytes = store.get_bytes(children[0].object_id, metrics)?;
            let payload = file_codec::decode_mapping(&bytes, file_codec::FILE_BRANCH_TAG)?;
            let _branch_children_charge = charge_decoded_file_children(payload, false, metrics)?;
            let (branch_level, branch_children) = file_codec::parse_file_children(payload, true)?;
            if usize::from(branch_level) != level {
                return Err(CoreError::NonCanonicalOrdering.into());
            }
            children = branch_children;
            level = level
                .checked_sub(1)
                .ok_or(CoreError::MappingDepthExceeded)?;
        }
        let inner = encode_charged_file_root(
            0,
            self.total_raw,
            self.references,
            u8::try_from(level).map_err(|_| CoreError::MappingDepthExceeded)?,
            &children,
            metrics,
        )?;
        let (id, canonical_len, evidence) = if self.construction.is_some() {
            let (id, canonical_len, evidence) =
                store.put_generated_bytes_with_evidence(&inner, metrics)?;
            (id, canonical_len, Some(evidence))
        } else {
            let (id, canonical_len) = store.put_generated_bytes(&inner, metrics)?;
            (id, canonical_len, None)
        };
        let proof = match self.construction.take() {
            Some(construction) => Some(construction.finish_file(
                evidence.ok_or(CoreError::ValidationAuthorityUnavailable)?,
                id,
                canonical_len,
                level,
                self.total_raw,
                self.references,
                &children,
                match collapsed {
                    Some(proof) => RootConstructionCoverage::Collapsed(proof),
                    None => RootConstructionCoverage::Children(
                        proofs.ok_or(CoreError::ValidationAuthorityUnavailable)?,
                    ),
                },
                metrics,
            )?),
            None => None,
        };
        add_len(&mut metrics.mapping_bytes_rewritten, canonical_len)?;
        Ok((id, proof))
    }

    fn flush_leaf_with_store(&mut self, store: &mut Store, metrics: &mut Metrics) -> AnyResult<()> {
        if self.leaf.is_empty() {
            return Ok(());
        }
        let inner = encode_charged_file_leaf(&self.leaf, metrics)?;
        let (id, canonical_len, evidence) = if self.construction.is_some() {
            let (id, canonical_len, evidence) =
                store.put_generated_bytes_with_evidence(&inner, metrics)?;
            (id, canonical_len, Some(evidence))
        } else {
            let (id, canonical_len) = store.put_generated_bytes(&inner, metrics)?;
            (id, canonical_len, None)
        };
        let proof = match self.construction.as_mut() {
            Some(construction) => Some(construction.fold_leaf(
                evidence.ok_or(CoreError::ValidationAuthorityUnavailable)?,
                id,
                canonical_len,
                &self.leaf,
                metrics,
            )?),
            None => None,
        };
        add(&mut metrics.pages, 1)?;
        add_len(&mut metrics.mapping_bytes_rewritten, canonical_len)?;
        let leaf_total = self.leaf.iter().try_fold(0_u64, |total, reference| {
            total
                .checked_add(u64::from(reference.raw_length))
                .ok_or(CoreError::LengthOverflow)
        })?;
        self.push_node_with_store(
            store,
            0,
            file_codec::FileChild {
                object_id: id,
                cumulative_end: leaf_total,
            },
            proof,
            metrics,
        )?;
        self.leaf.clear();
        Ok(())
    }
}

fn canonical_bytes(inner: Vec<u8>) -> AnyResult<(ObjectId, Vec<u8>)> {
    let canonical = encode_canonical_object(&Object::bytes(inner)?)?;
    let id = ObjectId::for_bytes(&canonical);
    Ok((id, canonical))
}

fn canonical_bytes_accounted(
    inner: ChargedVec<u8>,
    metrics: &mut Metrics,
) -> AnyResult<(ObjectId, ChargedVec<u8>)> {
    let canonical = encode_charged_bytes_object(&inner, metrics)?;
    let id = ObjectId::for_bytes(&canonical);
    Ok((id, canonical))
}

fn put_mapping(
    store: &mut Store,
    inner: ChargedVec<u8>,
    metrics: &mut Metrics,
) -> AnyResult<ObjectId> {
    let canonical = encode_charged_bytes_object(&inner, metrics)?;
    let id = object_id_accounted(&canonical, metrics)?;
    store.put(id, &canonical, metrics)?;
    add_len(&mut metrics.mapping_bytes_rewritten, canonical.len())?;
    Ok(id)
}

fn put_mapping_with_evidence(
    store: &mut Store,
    inner: ChargedVec<u8>,
    metrics: &mut Metrics,
) -> AnyResult<(ObjectId, PutEvidence)> {
    let canonical = encode_charged_bytes_object(&inner, metrics)?;
    let id = object_id_accounted(&canonical, metrics)?;
    let evidence = store.put_with_evidence(id, &canonical, metrics)?;
    add_len(&mut metrics.mapping_bytes_rewritten, canonical.len())?;
    Ok((id, evidence))
}

fn namespace_file_root(
    store: &mut Store,
    file_root: ObjectId,
    metrics: &mut Metrics,
) -> AnyResult<ObjectId> {
    let _directory_charge = charge_capacity(metrics, 256 + 256 + b"file".len())?;
    let name = CanonicalName::from_bytes(b"file")?;
    let object = Object::directory(vec![DirectoryEntry::new(
        name,
        ObjectReference::new(ObjectKind::Bytes, file_root),
    )])?;
    let canonical_len = 9 + 4 + 4 + b"file".len() + 1 + 32;
    let mut canonical = ChargedVec::with_capacity(canonical_len, metrics)?;
    layerfs_core::encode_object_to(&object, &mut *canonical)?;
    let id = object_id_accounted(&canonical, metrics)?;
    store.put(id, &canonical, metrics)?;
    add_len(&mut metrics.mapping_bytes_rewritten, canonical.len())?;
    Ok(id)
}

fn namespace_file_root_with_evidence(
    store: &mut Store,
    file_root: ObjectId,
    metrics: &mut Metrics,
) -> AnyResult<(ObjectId, PutEvidence)> {
    let _directory_charge = charge_capacity(metrics, 256 + 256 + b"file".len())?;
    let name = CanonicalName::from_bytes(b"file")?;
    let object = Object::directory(vec![DirectoryEntry::new(
        name,
        ObjectReference::new(ObjectKind::Bytes, file_root),
    )])?;
    let canonical_len = 9 + 4 + 4 + b"file".len() + 1 + 32;
    let mut canonical = ChargedVec::with_capacity(canonical_len, metrics)?;
    layerfs_core::encode_object_to(&object, &mut *canonical)?;
    let id = object_id_accounted(&canonical, metrics)?;
    let evidence = store.put_with_evidence(id, &canonical, metrics)?;
    add_len(&mut metrics.mapping_bytes_rewritten, canonical.len())?;
    Ok((id, evidence))
}

fn resolve_namespace_file_root(
    store: &Store,
    root: ObjectId,
    metrics: &mut Metrics,
) -> AnyResult<ObjectId> {
    let (_object_charge, object, _object_bytes) = store.get(root, metrics)?;
    let Object::Directory(entries) = object else {
        return Err(CoreError::WrongLogicalRole.into());
    };
    if entries.len() != 1
        || entries[0].name().as_bytes() != b"file"
        || entries[0].reference().kind() != ObjectKind::Bytes
    {
        return Err(CoreError::WrongLogicalRole.into());
    }
    let file_root = entries[0].reference().id();
    let bytes = store.get_bytes(file_root, metrics)?;
    file_codec::decode_mapping(&bytes, file_codec::FILE_ROOT_TAG)?;
    Ok(file_root)
}

fn namespace_entry_id(
    store: &Store,
    root: ObjectId,
    name: &[u8],
    metrics: &mut Metrics,
) -> AnyResult<ObjectId> {
    let (_object_charge, object, _object_bytes) = store.get(root, metrics)?;
    let Object::Directory(entries) = object else {
        return Err(CoreError::WrongLogicalRole.into());
    };
    entries
        .iter()
        .find(|entry| entry.name().as_bytes() == name)
        .map(|entry| entry.reference().id())
        .ok_or_else(|| CoreError::WrongLogicalRole.into())
}

fn source_path(root: &Path, size: u64) -> PathBuf {
    root.join(if size == SOURCE_100 {
        "S1-100.source"
    } else {
        "S1-512.source"
    })
}

fn fill_source(path: &Path, size: u64, seed: u64) -> AnyResult<()> {
    let mut file = File::create(path)?;
    let mut buffer = vec![0_u8; 1024 * 1024];
    let mut written = 0_u64;
    while written < size {
        fill_retained_buffer(
            &mut buffer,
            written,
            if seed == 0x51 { "S1-100" } else { "S1-512" },
        );
        let remaining = size.checked_sub(written).ok_or(CoreError::LengthOverflow)?;
        let take = usize::try_from(
            remaining.min(u64::try_from(buffer.len()).map_err(|_| CoreError::LengthOverflow)?),
        )
        .map_err(|_| CoreError::LengthOverflow)?;
        file.write_all(&buffer[..take])?;
        written = written
            .checked_add(u64::try_from(take).map_err(|_| CoreError::LengthOverflow)?)
            .ok_or(CoreError::LengthOverflow)?;
    }
    file.sync_all()?;
    Ok(())
}

fn fill_retained_buffer(buffer: &mut [u8], offset: u64, salt: &str) {
    let salt_hash = salt
        .bytes()
        .fold(0_u64, |value, byte| value.rotate_left(5) ^ u64::from(byte));
    let mut state = RETAINED_SEED ^ salt_hash ^ offset;
    for (index, byte) in buffer.iter_mut().enumerate() {
        state = state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        let position = offset.wrapping_add(index as u64);
        *byte = if (position / 8192) % 23 == 0 {
            (salt_hash as u8).wrapping_add((position / 8192) as u8)
        } else {
            (state >> 24) as u8
        };
    }
}

fn prepare_sources(root: &Path) -> AnyResult<()> {
    prepare_sources_for(root, &[SOURCE_100, SOURCE_512], false)
}

fn prepare_sources_for(root: &Path, sizes: &[u64], create_missing: bool) -> AnyResult<()> {
    fs::create_dir_all(root)?;
    let mut manifest = String::from(
        "{\"format\":1,\"fixture_origin\":\"phase2-deterministic-retained-generator\",\"fixtures\":[",
    );
    for &size in sizes {
        let path = source_path(root, size);
        if !path.exists() && create_missing {
            fill_source(&path, size, if size == SOURCE_100 { 0x51 } else { 0x52 })?;
        }
        if fs::metadata(&path).ok().map(|metadata| metadata.len()) != Some(size) {
            return Err(format!(
                "retained fixture {} is missing; generate/copy it outside the campaign first",
                path.display()
            )
            .into());
        }
        let expected = if size == SOURCE_100 {
            RETAINED_CDC_100
        } else {
            RETAINED_CDC_512
        };
        let expected_raw = if size == SOURCE_100 {
            RETAINED_RAW_100
        } else {
            RETAINED_RAW_512
        };
        let expected_sequence = if size == SOURCE_100 {
            RETAINED_CDC_SEQUENCE_100
        } else {
            RETAINED_CDC_SEQUENCE_512
        };
        let (actual_length, source_fingerprint) = source_hash(&path)?;
        let (chunks, sequence_fingerprint) = source_cdc_sequence(&path)?;
        if actual_length != size {
            return Err(CoreError::LengthMismatch {
                expected: size,
                actual: actual_length,
            }
            .into());
        }
        if chunks != expected {
            return Err(format!(
                "retained fixture {} has {chunks} CDC chunks, expected {expected}",
                path.display()
            )
            .into());
        }
        if source_fingerprint != expected_raw || sequence_fingerprint != expected_sequence {
            return Err(format!(
                "retained fixture {} fingerprint mismatch: raw={} sequence={}",
                path.display(),
                source_fingerprint,
                sequence_fingerprint
            )
            .into());
        }
        if !manifest.ends_with('[') {
            manifest.push(',');
        }
        manifest.push_str(&format!(
            "{{\"name\":\"{}\",\"size_bytes\":{},\"raw_fingerprint\":\"{}\",\"cdc_references\":{},\"cdc_sequence_fingerprint\":\"{}\"}}",
            path.file_name().and_then(|name| name.to_str()).unwrap_or("unknown"),
            size,
            source_fingerprint,
            chunks,
            sequence_fingerprint
        ));
    }
    manifest.push_str("]}\n");
    let manifest_path = root.join("wp4m-retained-fixture-manifest.json");
    if !manifest_path.exists() && create_missing {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&manifest_path)?;
        file.write_all(manifest.as_bytes())?;
        file.sync_all()?;
    }
    let retained_manifest = fs::read_to_string(&manifest_path).map_err(|error| {
        format!(
            "retained fixture manifest {} is missing; custody it outside the campaign: {error}",
            manifest_path.display()
        )
    })?;
    if retained_manifest != manifest {
        return Err(format!(
            "retained fixture manifest {} does not match the frozen raw/CDC fingerprints",
            manifest_path.display()
        )
        .into());
    }
    Ok(())
}

fn prepare_retained_fixtures(root: &Path) -> AnyResult<()> {
    prepare_sources_for(root, &[SOURCE_100, SOURCE_512], true)
}

fn source_hash(path: &Path) -> AnyResult<(u64, String)> {
    let mut file = File::open(path)?;
    let mut hasher = Hasher::new();
    let mut buffer = [0_u8; 1024 * 1024];
    let mut length = 0_u64;
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
        length = length
            .checked_add(u64::try_from(read).map_err(|_| CoreError::LengthOverflow)?)
            .ok_or(CoreError::LengthOverflow)?;
    }
    Ok((length, hasher.finalize().to_hex().to_string()))
}

fn source_cdc_sequence(path: &Path) -> AnyResult<(u64, String)> {
    let mut sequence_hasher = Hasher::new();
    let mut count = 0_u64;
    FastCdc::new().scan(File::open(path)?, |chunk| {
        sequence_hasher.update(
            &u32::try_from(chunk.len())
                .map_err(|_| CoreError::LengthOverflow)?
                .to_be_bytes(),
        );
        sequence_hasher.update(chunk_id(chunk).as_bytes());
        count = count.checked_add(1).ok_or(CoreError::LengthOverflow)?;
        Ok(())
    })?;
    Ok((count, sequence_hasher.finalize().to_hex().to_string()))
}

fn source_edit_point(source: &Path, operation: &str) -> AnyResult<(u64, u64, usize)> {
    let mut references = 0_u64;
    FastCdc::new().scan(File::open(source)?, |_| {
        references = references.checked_add(1).ok_or(CoreError::LengthOverflow)?;
        Ok(())
    })?;
    if references == 0 {
        return Err(CoreError::MissingObject.into());
    }
    let target = if operation.contains("early") {
        0
    } else {
        references / 2
    };
    let mut ordinal = 0_u64;
    let mut offset = 0_u64;
    let mut point = None;
    FastCdc::new().scan(File::open(source)?, |chunk| {
        if ordinal == target {
            point = Some((offset, chunk.len()));
        }
        ordinal = ordinal.checked_add(1).ok_or(CoreError::LengthOverflow)?;
        offset = offset
            .checked_add(u64::try_from(chunk.len()).map_err(|_| CoreError::LengthOverflow)?)
            .ok_or(CoreError::LengthOverflow)?;
        Ok(())
    })?;
    let (byte_offset, length) = point.ok_or(CoreError::MissingObject)?;
    Ok((references, byte_offset, length))
}

fn prepared_edit_point(source: &Path, operation: &str) -> AnyResult<EditPoint> {
    let (reference_count, byte_offset, replacement_length) = source_edit_point(source, operation)?;
    Ok(EditPoint {
        reference_count,
        position: if operation.contains("early") {
            0
        } else {
            reference_count / 2
        },
        byte_offset,
        replacement_length,
    })
}

fn edited_source_reader<'a>(
    source: &Path,
    offset: u64,
    removed_length: usize,
    inserted: &'a [u8],
) -> AnyResult<impl Read + 'a> {
    let prefix = File::open(source)?.take(offset);
    let mut suffix = File::open(source)?;
    suffix.seek(SeekFrom::Start(
        offset
            .checked_add(u64::try_from(removed_length).map_err(|_| CoreError::LengthOverflow)?)
            .ok_or(CoreError::LengthOverflow)?,
    ))?;
    Ok(prefix.chain(std::io::Cursor::new(inserted)).chain(suffix))
}

fn boundary_probe(
    label: &'static str,
    boundary: u64,
    length: u64,
) -> Option<(&'static str, std::ops::Range<u64>)> {
    (boundary > 0 && boundary < length).then(|| {
        (
            label,
            boundary.saturating_sub(1)..boundary.saturating_add(1).min(length),
        )
    })
}

#[allow(clippy::too_many_arguments)]
fn observe_expected_probe_segment(
    segment: &[u8],
    candidate: Candidate,
    final_length: u64,
    output_offset: &mut u64,
    output_references: &mut u64,
    cross_chunk: &mut bool,
    leaf_boundary: &mut bool,
    branch_boundary: &mut bool,
    probes: &mut Vec<(&'static str, std::ops::Range<u64>)>,
) -> CoreResult<()> {
    if !*cross_chunk {
        if let Some(probe) = boundary_probe("cross-chunk", *output_offset, final_length) {
            probes.push(probe);
            *cross_chunk = true;
        }
    }
    *output_offset = output_offset
        .checked_add(u64::try_from(segment.len()).map_err(|_| CoreError::LengthOverflow)?)
        .ok_or(CoreError::LengthOverflow)?;
    *output_references = output_references
        .checked_add(1)
        .ok_or(CoreError::LengthOverflow)?;
    if !*leaf_boundary
        && *output_references
            == u64::try_from(candidate.k).map_err(|_| CoreError::LengthOverflow)?
    {
        if let Some(probe) = boundary_probe("leaf-boundary", *output_offset, final_length) {
            probes.push(probe);
            *leaf_boundary = true;
        }
    }
    let branch_at = u64::try_from(candidate.k)
        .map_err(|_| CoreError::LengthOverflow)?
        .checked_mul(u64::try_from(candidate.f).map_err(|_| CoreError::LengthOverflow)?)
        .ok_or(CoreError::LengthOverflow)?;
    if !*branch_boundary && *output_references == branch_at {
        if let Some(probe) = boundary_probe("branch-boundary", *output_offset, final_length) {
            probes.push(probe);
            *branch_boundary = true;
        }
    }
    Ok(())
}

fn expected_range_probes(
    source: &Path,
    operation: &str,
    source_length: u64,
    candidate: Candidate,
) -> AnyResult<Vec<(&'static str, std::ops::Range<u64>)>> {
    let is_plus_one = operation.starts_with("plus1-");
    let (reference_count, byte_offset, replacement_length) = source_edit_point(
        source,
        if operation == "full" {
            "same-middle"
        } else {
            operation
        },
    )?;
    let position = if operation.contains("early") {
        0
    } else {
        reference_count / 2
    };
    let replacement = if operation == "same-middle" {
        same_middle_replacement(&read_source_segment(
            source,
            byte_offset,
            replacement_length,
        )?)
    } else {
        vec![0xa5]
    };
    let final_length = source_length
        .checked_add(u64::from(is_plus_one))
        .ok_or(CoreError::LengthOverflow)?;
    let mut probes = vec![("zero", 0..0), ("first-byte", 0..final_length.min(1))];
    let mut output_offset = 0_u64;
    let mut output_references = 0_u64;
    let mut cross_chunk = false;
    let mut leaf_boundary = false;
    let mut branch_boundary = false;
    let mut inserted = false;
    if operation == "same-middle" {
        let (_, byte_offset, _) = source_edit_point(source, operation)?;
        FastCdc::new().scan(
            edited_source_reader(source, byte_offset, replacement_length, &replacement)?,
            |chunk| {
                observe_expected_probe_segment(
                    chunk,
                    candidate,
                    final_length,
                    &mut output_offset,
                    &mut output_references,
                    &mut cross_chunk,
                    &mut leaf_boundary,
                    &mut branch_boundary,
                    &mut probes,
                )
            },
        )?;
    } else {
        FastCdc::new().scan(File::open(source)?, |chunk| {
            if is_plus_one && !inserted && output_references == position {
                observe_expected_probe_segment(
                    &replacement,
                    candidate,
                    final_length,
                    &mut output_offset,
                    &mut output_references,
                    &mut cross_chunk,
                    &mut leaf_boundary,
                    &mut branch_boundary,
                    &mut probes,
                )?;
                inserted = true;
            }
            observe_expected_probe_segment(
                chunk,
                candidate,
                final_length,
                &mut output_offset,
                &mut output_references,
                &mut cross_chunk,
                &mut leaf_boundary,
                &mut branch_boundary,
                &mut probes,
            )
        })?;
    }
    if is_plus_one && !inserted {
        let mut emit = |segment: &[u8]| -> CoreResult<()> {
            output_offset = output_offset
                .checked_add(u64::try_from(segment.len()).map_err(|_| CoreError::LengthOverflow)?)
                .ok_or(CoreError::LengthOverflow)?;
            output_references = output_references
                .checked_add(1)
                .ok_or(CoreError::LengthOverflow)?;
            Ok(())
        };
        emit(&replacement)?;
    }
    probes.push(("last-byte", final_length.saturating_sub(1)..final_length));
    probes.push(("eof", final_length..final_length));
    Ok(probes)
}

fn append_expected_segment(
    start: &mut u64,
    bytes: &[u8],
    probes: &[std::ops::Range<u64>],
    outputs: &mut [Vec<u8>],
    hasher: &mut Hasher,
    sequence_hasher: &mut Hasher,
) -> CoreResult<()> {
    let end = start
        .checked_add(u64::try_from(bytes.len()).map_err(|_| CoreError::LengthOverflow)?)
        .ok_or(CoreError::LengthOverflow)?;
    hasher.update(bytes);
    sequence_hasher.update(
        &u32::try_from(bytes.len())
            .map_err(|_| CoreError::LengthOverflow)?
            .to_be_bytes(),
    );
    sequence_hasher.update(chunk_id(bytes).as_bytes());
    for (index, probe) in probes.iter().enumerate() {
        let overlap_start = (*start).max(probe.start);
        let overlap_end = end.min(probe.end);
        if overlap_start < overlap_end {
            let from =
                usize::try_from(overlap_start - *start).map_err(|_| CoreError::LengthOverflow)?;
            let to =
                usize::try_from(overlap_end - *start).map_err(|_| CoreError::LengthOverflow)?;
            outputs[index].extend_from_slice(&bytes[from..to]);
        }
    }
    *start = end;
    Ok(())
}

fn expected_file_observations(
    source: &Path,
    operation: &str,
    source_length: u64,
    candidate: Candidate,
) -> AnyResult<FileObservations> {
    let is_plus_one = operation.starts_with("plus1-");
    let (reference_count, byte_offset, replacement_length) = source_edit_point(
        source,
        if operation == "full" {
            "same-middle"
        } else {
            operation
        },
    )?;
    let position = if operation.contains("early") {
        0
    } else {
        reference_count / 2
    };
    let probes = expected_range_probes(source, operation, source_length, candidate)?;
    let probe_ranges = probes
        .iter()
        .map(|(_, range)| range.clone())
        .collect::<Vec<_>>();
    let mut outputs = probe_ranges.iter().map(|_| Vec::new()).collect::<Vec<_>>();
    let mut hasher = Hasher::new();
    let mut sequence_hasher = Hasher::new();
    let mut output_offset = 0_u64;
    let mut ordinal = 0_u64;
    let mut inserted = false;
    let replacement = if operation == "same-middle" {
        same_middle_replacement(&read_source_segment(
            source,
            byte_offset,
            replacement_length,
        )?)
    } else {
        vec![0xa5]
    };
    if operation == "same-middle" {
        FastCdc::new().scan(
            edited_source_reader(source, byte_offset, replacement_length, &replacement)?,
            |chunk| {
                append_expected_segment(
                    &mut output_offset,
                    chunk,
                    &probe_ranges,
                    &mut outputs,
                    &mut hasher,
                    &mut sequence_hasher,
                )?;
                ordinal = ordinal.checked_add(1).ok_or(CoreError::LengthOverflow)?;
                Ok(())
            },
        )?;
    } else {
        FastCdc::new().scan(File::open(source)?, |chunk| {
            if is_plus_one && !inserted && ordinal == position {
                append_expected_segment(
                    &mut output_offset,
                    &replacement,
                    &probe_ranges,
                    &mut outputs,
                    &mut hasher,
                    &mut sequence_hasher,
                )?;
                ordinal = ordinal.checked_add(1).ok_or(CoreError::LengthOverflow)?;
                inserted = true;
            }
            append_expected_segment(
                &mut output_offset,
                chunk,
                &probe_ranges,
                &mut outputs,
                &mut hasher,
                &mut sequence_hasher,
            )?;
            ordinal = ordinal.checked_add(1).ok_or(CoreError::LengthOverflow)?;
            Ok(())
        })?;
    }
    if is_plus_one && !inserted {
        append_expected_segment(
            &mut output_offset,
            &replacement,
            &probe_ranges,
            &mut outputs,
            &mut hasher,
            &mut sequence_hasher,
        )?;
        ordinal = ordinal.checked_add(1).ok_or(CoreError::LengthOverflow)?;
    }
    let expected_references = ordinal;
    Ok((
        expected_references,
        hasher.finalize().to_hex().to_string(),
        sequence_hasher.finalize().to_hex().to_string(),
        outputs,
        probes,
    ))
}

fn read_source_segment(source: &Path, offset: u64, length: usize) -> AnyResult<Vec<u8>> {
    let mut file = File::open(source)?;
    file.seek(SeekFrom::Start(offset))?;
    let mut bytes = vec![0_u8; length];
    file.read_exact(&mut bytes)?;
    Ok(bytes)
}

fn read_source_segment_charged(
    source: &Path,
    offset: u64,
    length: usize,
    metrics: &mut Metrics,
) -> AnyResult<ChargedVec<u8>> {
    let mut bytes = ChargedVec::with_capacity(length, metrics)?;
    bytes.resize(length, 0);
    let mut file = File::open(source)?;
    file.seek(SeekFrom::Start(offset))?;
    file.read_exact(&mut bytes)?;
    Ok(bytes)
}

fn same_middle_replacement(removed: &[u8]) -> Vec<u8> {
    removed.iter().map(|byte| byte ^ 0x5a).collect()
}

fn is_same_middle_replacement(removed: &[u8], inserted: &[u8]) -> bool {
    removed.len() == inserted.len()
        && removed
            .iter()
            .zip(inserted)
            .all(|(before, after)| before ^ 0x5a == *after)
}

fn make_reference(
    store: &mut Store,
    bytes: &[u8],
    metrics: &mut Metrics,
) -> AnyResult<file_codec::FileReference> {
    add_len(&mut metrics.source_bytes_read, bytes.len())?;
    add_len(&mut metrics.canonical_stage_source_bytes_read, bytes.len())?;
    store_reference(store, bytes, metrics)
}

fn store_reference(
    store: &mut Store,
    bytes: &[u8],
    metrics: &mut Metrics,
) -> AnyResult<file_codec::FileReference> {
    let raw_length = u32::try_from(bytes.len()).map_err(|_| CoreError::LengthOverflow)?;
    let raw_id = chunk_id_accounted(bytes, metrics)?;
    let canonical = encode_charged_bytes_object(bytes, metrics)?;
    let object_id = object_id_accounted(&canonical, metrics)?;
    store.put(object_id, &canonical, metrics)?;
    add(&mut metrics.chunks, 1)?;
    Ok(file_codec::FileReference {
        raw_id,
        raw_length,
        object_id,
    })
}

#[derive(Clone, Copy, Debug)]
struct RejoinChunk {
    start: u64,
    raw_id: ObjectId,
    raw_length: u32,
}

fn rejoin_chunk_capacity(bytes: usize) -> CoreResult<usize> {
    bytes
        .checked_div(layerfs_core::cdc::MINIMUM_CHUNK_BYTES)
        .and_then(|count| count.checked_add(2))
        .ok_or(CoreError::LengthOverflow)
}

fn scan_rejoin_chunks(bytes: &[u8], metrics: &mut Metrics) -> AnyResult<ChargedVec<RejoinChunk>> {
    let mut chunks = ChargedVec::with_item_charge(
        rejoin_chunk_capacity(bytes.len())?,
        Q_FILE_REFERENCE_BYTES,
        metrics,
    )?;
    let mut start = 0_u64;
    FastCdc::new().scan(bytes, |chunk| {
        let raw_length = u32::try_from(chunk.len()).map_err(|_| CoreError::LengthOverflow)?;
        chunks.push(RejoinChunk {
            start,
            raw_id: chunk_id_accounted(chunk, metrics)?,
            raw_length,
        });
        start = start
            .checked_add(u64::from(raw_length))
            .ok_or(CoreError::LengthOverflow)?;
        Ok(())
    })?;
    Ok(chunks)
}

fn find_exact_rejoin(
    old: &[RejoinChunk],
    scanned: &[RejoinChunk],
    old_suffix_start: u64,
    changed_prefix: u64,
) -> Option<(usize, usize)> {
    for old_index in 2..old.len() {
        let old_relative = old[old_index].start.checked_sub(old_suffix_start)?;
        let expected_start = changed_prefix.checked_add(old_relative)?;
        let Some(scanned_index) = scanned
            .iter()
            .position(|chunk| chunk.start == expected_start)
        else {
            continue;
        };
        let confirmations = 2_usize.min(old.len() - old_index);
        if (0..confirmations).all(|offset| {
            let Some(scanned_chunk) = scanned.get(scanned_index + offset) else {
                return false;
            };
            let old_chunk = old[old_index + offset];
            scanned_chunk.raw_length == old_chunk.raw_length
                && scanned_chunk.raw_id == old_chunk.raw_id
        }) {
            return Some((old_index, scanned_index));
        }
    }
    None
}

fn tail_exact_rejoin(
    old: &[RejoinChunk],
    scanned: &[RejoinChunk],
    old_suffix_start: u64,
    changed_prefix: u64,
) -> Option<(usize, usize)> {
    let scanned_index = scanned.len().checked_sub(2)?;
    let new_start = scanned[scanned_index].start;
    let old_start = old_suffix_start.checked_add(new_start.checked_sub(changed_prefix)?)?;
    let old_index = old
        .binary_search_by_key(&old_start, |chunk| chunk.start)
        .ok()?;
    if old_index < 2 || old_index + 1 >= old.len() {
        return None;
    }
    ((0..2).all(|offset| {
        old[old_index + offset].raw_length == scanned[scanned_index + offset].raw_length
            && old[old_index + offset].raw_id == scanned[scanned_index + offset].raw_id
    }))
    .then_some((old_index, scanned_index))
}

fn file_reference_at_ordinal(
    store: &Store,
    file_root: ObjectId,
    candidate: Candidate,
    ordinal: u64,
    metrics: &mut Metrics,
) -> AnyResult<file_codec::FileReference> {
    let profile = file_codec::FileMappingProfile::new(candidate.k, candidate.f);
    let root_bytes = store.get_bytes(file_root, metrics)?;
    let payload = file_codec::decode_mapping(&root_bytes, file_codec::FILE_ROOT_TAG)?;
    let _children_charge = charge_decoded_file_children(payload, true, metrics)?;
    let (_, _, reference_count, mut level, children) = file_codec::parse_file_root(payload)?;
    if ordinal >= reference_count
        || level != file_codec::expected_file_level(reference_count, profile)?
    {
        return Err(CoreError::NonCanonicalPagePartition.into());
    }
    file_codec::validate_file_children(&children, profile, true)?;
    let mut local = ordinal;
    let (mut node, mut final_node) = {
        let capacity = subtree_reference_capacity(profile, level)?;
        let index = usize::try_from(local / capacity).map_err(|_| CoreError::LengthOverflow)?;
        local %= capacity;
        (
            children
                .get(index)
                .ok_or(CoreError::NonCanonicalPagePartition)?
                .object_id,
            index + 1 == children.len(),
        )
    };
    while level != 0 {
        let bytes = store.get_bytes(node, metrics)?;
        let payload = file_codec::decode_mapping(&bytes, file_codec::FILE_BRANCH_TAG)?;
        let _children_charge = charge_decoded_file_children(payload, false, metrics)?;
        let (branch_level, children) = file_codec::parse_file_children(payload, true)?;
        if branch_level != level {
            return Err(CoreError::NonCanonicalPagePartition.into());
        }
        file_codec::validate_file_children(&children, profile, final_node)?;
        level = level
            .checked_sub(1)
            .ok_or(CoreError::MappingDepthExceeded)?;
        let capacity = subtree_reference_capacity(profile, level)?;
        let index = usize::try_from(local / capacity).map_err(|_| CoreError::LengthOverflow)?;
        local %= capacity;
        final_node = final_node && index + 1 == children.len();
        node = children
            .get(index)
            .ok_or(CoreError::NonCanonicalPagePartition)?
            .object_id;
    }
    let bytes = store.get_bytes(node, metrics)?;
    let payload = file_codec::decode_mapping(&bytes, file_codec::FILE_LEAF_TAG)?;
    let _references_charge = charge_decoded_file_references(payload, metrics)?;
    let references = file_codec::parse_file_leaf(payload)?;
    file_codec::validate_file_leaf(&references, profile, final_node)?;
    references
        .get(usize::try_from(local).map_err(|_| CoreError::LengthOverflow)?)
        .copied()
        .ok_or_else(|| CoreError::NonCanonicalPagePartition.into())
}

fn same_middle_rejoin_references(
    store: &mut Store,
    file_root: ObjectId,
    candidate: Candidate,
    edit_point: EditPoint,
    replacement: &[u8],
    metrics: &mut Metrics,
) -> AnyResult<(u64, ChargedVec<file_codec::FileReference>)> {
    let (total, references) = {
        let root_bytes = store.get_bytes(file_root, metrics)?;
        let payload = file_codec::decode_mapping(&root_bytes, file_codec::FILE_ROOT_TAG)?;
        let root_children_charge = charge_decoded_file_children(payload, true, metrics)?;
        let (_, total, references, _, children) = file_codec::parse_file_root(payload)?;
        drop(children);
        drop(root_children_charge);
        drop(root_bytes);
        (total, references)
    };
    if references != edit_point.reference_count {
        return Err(CoreError::LengthMismatch {
            expected: edit_point.reference_count,
            actual: references,
        }
        .into());
    }
    let replacement_start = edit_point
        .position
        .checked_sub(1)
        .ok_or(CoreError::NonCanonicalPagePartition)?;
    let predecessor =
        file_reference_at_ordinal(store, file_root, candidate, replacement_start, metrics)?;
    let predecessor_length = u64::from(predecessor.raw_length);
    let scan_start = edit_point
        .byte_offset
        .checked_sub(predecessor_length)
        .ok_or(CoreError::LengthOverflow)?;
    let removed_length =
        u64::try_from(edit_point.replacement_length).map_err(|_| CoreError::LengthOverflow)?;
    if replacement.len() != edit_point.replacement_length {
        return Err(CoreError::LengthMismatch {
            expected: removed_length,
            actual: u64::try_from(replacement.len()).map_err(|_| CoreError::LengthOverflow)?,
        }
        .into());
    }
    let range_end = edit_point
        .byte_offset
        .checked_add(removed_length)
        .and_then(|end| end.checked_add(layerfs_core::MAX_REJOIN_WINDOW_BYTES))
        .ok_or(CoreError::LengthOverflow)?
        .min(total);
    metrics.q_cdc_base_live_bytes = q_current();
    let old_bytes = read_file_range(store, file_root, candidate, scan_start..range_end, metrics)?;
    metrics.q_cdc_old_window_bytes =
        u64::try_from(old_bytes.len()).map_err(|_| CoreError::LengthOverflow)?;
    let prefix_length =
        usize::try_from(predecessor_length).map_err(|_| CoreError::LengthOverflow)?;
    let replaced_end = prefix_length
        .checked_add(edit_point.replacement_length)
        .ok_or(CoreError::LengthOverflow)?;
    if old_bytes.len() < replaced_end {
        return Err(CoreError::UnexpectedEof.into());
    }
    let old_chunk_slots_bytes = rejoin_chunk_capacity(old_bytes.len())?
        .checked_mul(Q_FILE_REFERENCE_BYTES)
        .ok_or(CoreError::LengthOverflow)?;
    metrics.q_cdc_old_chunk_slots_bytes =
        u64::try_from(old_chunk_slots_bytes).map_err(|_| CoreError::LengthOverflow)?;
    let old_chunks = scan_rejoin_chunks(&old_bytes, metrics)?;
    if old_chunks.first().map(|chunk| chunk.raw_length) != Some(predecessor.raw_length)
        || old_chunks.get(1).map(|chunk| chunk.raw_length)
            != Some(
                u32::try_from(edit_point.replacement_length)
                    .map_err(|_| CoreError::LengthOverflow)?,
            )
    {
        return Err(CoreError::NonCanonicalPagePartition.into());
    }
    let mut scan_input = ChargedVec::with_capacity(old_bytes.len(), metrics)?;
    scan_input.extend_from_slice(&old_bytes[..prefix_length]);
    scan_input.extend_from_slice(replacement);
    scan_input.extend_from_slice(&old_bytes[replaced_end..]);
    metrics.q_cdc_scan_input_bytes =
        u64::try_from(scan_input.len()).map_err(|_| CoreError::LengthOverflow)?;
    metrics.q_cdc_overlap_current = q_current();
    let expected_overlap = metrics
        .q_cdc_base_live_bytes
        .checked_add(metrics.q_cdc_old_window_bytes)
        .and_then(|value| value.checked_add(metrics.q_cdc_old_chunk_slots_bytes))
        .and_then(|value| value.checked_add(metrics.q_cdc_scan_input_bytes))
        .ok_or(CoreError::LengthOverflow)?;
    if metrics.q_cdc_overlap_current != expected_overlap {
        return Err(CoreError::LengthMismatch {
            expected: expected_overlap,
            actual: metrics.q_cdc_overlap_current,
        }
        .into());
    }
    drop(old_bytes);
    add_len(&mut metrics.source_bytes_read, replacement.len())?;
    let old_suffix_start = predecessor_length
        .checked_add(removed_length)
        .ok_or(CoreError::LengthOverflow)?;
    let changed_prefix = predecessor_length
        .checked_add(removed_length)
        .ok_or(CoreError::LengthOverflow)?;
    let mut scanned = ChargedVec::with_item_charge(
        rejoin_chunk_capacity(scan_input.len())?,
        Q_FILE_REFERENCE_BYTES,
        metrics,
    )?;
    let mut scanned_end = 0_u64;
    let mut rejoin = None;
    let scan_result = FastCdc::new().scan(scan_input.as_slice(), |chunk| {
        let raw_length = u32::try_from(chunk.len()).map_err(|_| CoreError::LengthOverflow)?;
        scanned.push(RejoinChunk {
            start: scanned_end,
            raw_id: chunk_id_accounted(chunk, metrics)?,
            raw_length,
        });
        scanned_end = scanned_end
            .checked_add(u64::from(raw_length))
            .ok_or(CoreError::LengthOverflow)?;
        if let Some(found) =
            tail_exact_rejoin(&old_chunks, &scanned, old_suffix_start, changed_prefix)
        {
            rejoin = Some(found);
            return Err(CoreError::Io);
        }
        Ok(())
    });
    match scan_result {
        Err(_) if rejoin.is_some() => {}
        Ok(_) => {
            rejoin = find_exact_rejoin(&old_chunks, &scanned, old_suffix_start, changed_prefix);
        }
        Err(error) => return Err(error.into()),
    }
    add(&mut metrics.source_cdc_bytes_read, scanned_end)?;
    add(&mut metrics.canonical_stage_source_bytes_read, scanned_end)?;
    let (old_count, new_count) = rejoin.ok_or(CoreError::BoundedResynchronization {
        scanned: scanned_end,
        limit: layerfs_core::MAX_REJOIN_WINDOW_BYTES,
    })?;
    if old_count != new_count {
        return Err(CoreError::LengthMismatch {
            expected: u64::try_from(old_count).map_err(|_| CoreError::LengthOverflow)?,
            actual: u64::try_from(new_count).map_err(|_| CoreError::LengthOverflow)?,
        }
        .into());
    }
    drop(old_chunks);
    let mut replacements =
        ChargedVec::with_item_charge(new_count, Q_FILE_REFERENCE_BYTES, metrics)?;
    for chunk in scanned.iter().take(new_count) {
        let start = usize::try_from(chunk.start).map_err(|_| CoreError::LengthOverflow)?;
        let end = start
            .checked_add(usize::try_from(chunk.raw_length).map_err(|_| CoreError::LengthOverflow)?)
            .ok_or(CoreError::LengthOverflow)?;
        let bytes = scan_input.get(start..end).ok_or(CoreError::UnexpectedEof)?;
        let reference = store_reference(store, bytes, metrics)?;
        if reference.raw_id != chunk.raw_id || reference.raw_length != chunk.raw_length {
            return Err(CoreError::ChunkIdentityMismatch.into());
        }
        replacements.push(reference);
    }
    Ok((replacement_start, replacements))
}

fn publish_transition(
    store: &mut Store,
    parent: Option<ObjectId>,
    child: ObjectId,
    metrics: &mut Metrics,
) -> AnyResult<ObjectId> {
    publish_transition_with_operations(store, parent, child, &[], metrics)
}

fn publish_genesis_transition_with_evidence(
    store: &mut Store,
    child: ObjectId,
    metrics: &mut Metrics,
) -> AnyResult<(ObjectId, PutEvidence)> {
    let inner = encode_charged_transition(None, child, 0, &[], metrics)?;
    put_mapping_with_evidence(store, inner, metrics)
}

fn publish_transition_with_operations(
    store: &mut Store,
    parent: Option<ObjectId>,
    child: ObjectId,
    operations: &[delta_codec::TransitionOperation],
    metrics: &mut Metrics,
) -> AnyResult<ObjectId> {
    let _pages_charge = charge_capacity(
        metrics,
        usize::from(!operations.is_empty())
            .checked_mul(Q_TREE_NODE_BYTES)
            .ok_or(CoreError::LengthOverflow)?,
    )?;
    let pages = if operations.is_empty() {
        Vec::new()
    } else {
        vec![put_mapping(
            store,
            encode_charged_delta_page(operations, metrics)?,
            metrics,
        )?]
    };
    let entry_count = u32::try_from(operations.len()).map_err(|_| CoreError::LengthOverflow)?;
    let inner = encode_charged_transition(parent, child, entry_count, &pages, metrics)?;
    let transition = put_mapping(store, inner, metrics)?;
    Ok(transition)
}

fn verify_transition(
    store: &Store,
    transition: ObjectId,
    expected_parent: Option<ObjectId>,
    expected_child: ObjectId,
    expected_operations: Option<&[delta_codec::TransitionOperation]>,
    metrics: &mut Metrics,
) -> AnyResult<[u8; 32]> {
    let mut closure_hasher = Hasher::new();
    let bytes = store.get_bytes(transition, metrics)?;
    observe_closure(&mut closure_hasher, b"transition", transition, &bytes)?;
    let decoded_page_count = delta_codec::measure_mapping_transition_pages(&bytes)?;
    let _decoded_pages_charge = charge_capacity(
        metrics,
        decoded_page_count
            .checked_mul(Q_TREE_NODE_BYTES)
            .ok_or(CoreError::LengthOverflow)?,
    )?;
    let decoded = delta_codec::decode_mapping_transition(&bytes)?;
    if decoded.parent != expected_parent || decoded.child != expected_child {
        return Err(CoreError::PublicationConflict.into());
    }
    let operation_count =
        usize::try_from(decoded.entry_count).map_err(|_| CoreError::LengthOverflow)?;
    let mut operations_charge = charge_capacity(
        metrics,
        operation_count
            .checked_mul(256)
            .ok_or(CoreError::LengthOverflow)?,
    )?;
    let mut operations = Vec::with_capacity(operation_count);
    for page in &decoded.pages {
        let bytes = store.get_bytes(*page, metrics)?;
        observe_closure(&mut closure_hasher, b"transition-page", *page, &bytes)?;
        let (page_count, path_bytes) = delta_codec::measure_mapping_delta_page(&bytes)?;
        operations_charge.absorb(charge_capacity(metrics, path_bytes)?)?;
        let page_entries_charge = charge_capacity(
            metrics,
            page_count
                .checked_mul(256)
                .ok_or(CoreError::LengthOverflow)?,
        )?;
        let page_operations = delta_codec::decode_mapping_delta_page(&bytes)?;
        if page_operations.is_empty() {
            return Err(CoreError::NonCanonicalPagePartition.into());
        }
        operations.extend(page_operations);
        drop(page_entries_charge);
    }
    if u32::try_from(operations.len()).map_err(|_| CoreError::LengthOverflow)?
        != decoded.entry_count
    {
        return Err(CoreError::LengthMismatch {
            expected: u64::from(decoded.entry_count),
            actual: u64::try_from(operations.len()).map_err(|_| CoreError::LengthOverflow)?,
        }
        .into());
    }
    if expected_operations.is_some_and(|expected| operations.as_slice() != expected) {
        return Err(CoreError::PublicationConflict.into());
    }
    if expected_parent.is_none() && (!operations.is_empty() || !decoded.pages.is_empty()) {
        return Err(CoreError::PublicationConflict.into());
    }
    if let Some(parent) = expected_parent {
        replay_shadow_transition(
            store,
            &decoded,
            &operations,
            parent,
            expected_child,
            metrics,
        )?;
    }
    let (_object_charge, object, bytes) = store.get(expected_child, metrics)?;
    observe_closure(
        &mut closure_hasher,
        b"transition-child",
        expected_child,
        &bytes,
    )?;
    if !matches!(object, Object::Directory(_)) {
        return Err(CoreError::PublicationConflict.into());
    }
    Ok(*closure_hasher.finalize().as_bytes())
}

fn shadow_node(id: ObjectId) -> CoreResult<TreeNode> {
    let name = CanonicalName::from_bytes(format!("id-{id}").as_bytes())?;
    TreeNode::directory([(name, TreeNode::empty_directory())])
}

fn shadow_root(store: &Store, id: ObjectId, metrics: &mut Metrics) -> AnyResult<RootHandle> {
    let (_object_charge, _, bytes) = store.get(id, metrics)?;
    let Object::Directory(entries) = decode_object(&bytes)? else {
        return Err(CoreError::WrongLogicalRole.into());
    };
    let children = entries
        .into_iter()
        .map(|entry| Ok((entry.name().clone(), shadow_node(entry.reference().id())?)))
        .collect::<AnyResult<Vec<_>>>()?;
    Ok(RootHandle::from_entries(children)?)
}

fn replay_shadow_transition(
    store: &Store,
    transition: &delta_codec::DecodedTransition,
    operations: &[delta_codec::TransitionOperation],
    parent_id: ObjectId,
    child_id: ObjectId,
    metrics: &mut Metrics,
) -> AnyResult<()> {
    let [delta_codec::TransitionOperation::Replace {
        path,
        before,
        after,
    }] = operations
    else {
        return Err(CoreError::DeltaConflict.into());
    };
    let expected_tag = match path.as_slice() {
        b"file" => {
            let parent_file = resolve_namespace_file_root(store, parent_id, metrics)?;
            let child_file = resolve_namespace_file_root(store, child_id, metrics)?;
            if parent_file != *before || child_file != *after {
                return Err(CoreError::DeltaConflict.into());
            }
            return Ok(());
        }
        b"t" => file_codec::DIR_INDEX_TAG,
        _ => return Err(CoreError::DeltaConflict.into()),
    };
    let after_bytes = store.get_bytes(*after, metrics)?;
    file_codec::decode_mapping(&after_bytes, expected_tag)?;
    let after_node = shadow_node(*after)?;
    let parent = shadow_root(store, parent_id, metrics)?;
    let parent_tree_id = parent.node().identity();
    let before_tree_id = parent
        .lookup_required(&layerfs_core::CanonicalPath::from_bytes(path)?)?
        .identity();
    let after_tree_id = after_node.identity();
    let mut durable_ids = HashMap::from([
        (parent_tree_id, parent_id),
        (before_tree_id, *before),
        (after_tree_id, *after),
    ]);
    let replay = delta_codec::replay_durable_transition(
        transition,
        operations,
        &parent,
        parent_id,
        |id| {
            let bytes = store.get_bytes(id, metrics).map_err(|error| {
                match error.downcast_ref::<CoreError>() {
                    Some(error) => *error,
                    None => CoreError::Io,
                }
            })?;
            file_codec::decode_mapping(&bytes, expected_tag)?;
            shadow_node(id)
        },
        |node| {
            if let Some(id) = durable_ids.get(&node.identity()) {
                return Ok(*id);
            }
            if node.entries().is_some() {
                durable_ids.insert(node.identity(), child_id);
                return Ok(child_id);
            }
            Err(CoreError::MissingObject)
        },
    )?;
    let replayed = replay.apply(&parent)?;
    if replayed != shadow_root(store, child_id, metrics)? {
        return Err(CoreError::DeltaChildMismatch {
            expected: child_id,
            actual: child_id,
        }
        .into());
    }
    Ok(())
}

fn build_file(
    store: &mut Store,
    source: &Path,
    candidate: Candidate,
    metrics: &mut Metrics,
) -> AnyResult<(ObjectId, ObjectId)> {
    store.begin(metrics)?;
    let mut builder = FileBuilder::new(candidate);
    FastCdc::new().scan(File::open(source)?, |chunk| {
        builder
            .push_bytes(store, chunk, metrics)
            .map_err(|error| core_failure(error.as_ref()))
    })?;
    let file_root = builder.finish(store, metrics)?;
    let root = namespace_file_root(store, file_root, metrics)?;
    let transition = publish_transition(store, None, root, metrics)?;
    Ok((root, transition))
}

fn build_file_construction(
    store: &mut Store,
    source: &Path,
    candidate: Candidate,
    expected_references: u64,
    metrics: &mut Metrics,
) -> AnyResult<(ObjectId, ObjectId, FullCreateConstructionProof)> {
    store.begin(metrics)?;
    let mut builder = FileBuilder::new_proving(candidate, expected_references, store, metrics)?;
    FastCdc::new().scan(File::open(source)?, |chunk| {
        builder
            .push_bytes(store, chunk, metrics)
            .map_err(|error| core_failure(error.as_ref()))
    })?;
    let (file_root, file_proof) = builder.finish_proven(store, metrics)?;
    let (root, workspace_evidence) = namespace_file_root_with_evidence(store, file_root, metrics)?;
    let workspace_proof = file_proof.fold_workspace(workspace_evidence, root, metrics)?;
    let (transition, transition_evidence) =
        publish_genesis_transition_with_evidence(store, root, metrics)?;
    let proof = workspace_proof.fold_transition(transition_evidence, transition, metrics)?;
    Ok((root, transition, proof))
}

fn build_file_proven(
    store: &mut Store,
    source: &Path,
    candidate: Candidate,
    expected: FullCreateExpectation,
    metrics: &mut Metrics,
) -> AnyResult<(ObjectId, ObjectId, FullCreateConstructionProof)> {
    let (root, transition, mut proof) =
        build_file_construction(store, source, candidate, expected.references, metrics)?;
    proof.bind_expectation(expected)?;
    store.mark_construction_proof_issued(&proof)?;
    Ok((root, transition, proof))
}

fn prepare_same_middle_oracle(
    store: &mut Store,
    source: &Path,
    candidate: Candidate,
    edit_point: EditPoint,
    expected_fingerprint: &str,
    expected_sequence: &str,
) -> AnyResult<PreparedEditOracle> {
    if edit_point.replacement_length > MAX_EDIT_ORACLE_BYTES {
        return Err(CoreError::ObjectLimitExceeded.into());
    }
    let head = store
        .current_head()?
        .ok_or(CoreError::InvalidValidationReceipt)?;
    let before_file = resolve_namespace_file_root(store, head.1, &mut Metrics::default())?;
    let removed = read_source_segment(
        source,
        edit_point.byte_offset,
        edit_point.replacement_length,
    )?;
    let inserted = same_middle_replacement(&removed);
    let mut metrics = Metrics::default();
    store.begin(&mut metrics)?;
    let result: AnyResult<PreparedEditOracle> = (|| {
        let mut builder = FileBuilder::new(candidate);
        let mut ordinal = 0_u64;
        FastCdc::new().scan(
            edited_source_reader(
                source,
                edit_point.byte_offset,
                edit_point.replacement_length,
                &inserted,
            )?,
            |bytes| {
                builder
                    .push_bytes(store, bytes, &mut metrics)
                    .map_err(|error| core_failure(error.as_ref()))?;
                ordinal = ordinal.checked_add(1).ok_or(CoreError::LengthOverflow)?;
                Ok(())
            },
        )?;
        if ordinal != edit_point.reference_count {
            return Err(CoreError::LengthMismatch {
                expected: edit_point.reference_count,
                actual: ordinal,
            }
            .into());
        }
        let after_file = builder.finish(store, &mut metrics)?;
        let result_root = namespace_file_root(store, after_file, &mut metrics)?;
        let (operations, _operations_charge) =
            charged_replace_operation(b"file", before_file, after_file, &mut metrics)?;
        let result_transition = publish_transition_with_operations(
            store,
            Some(head.1),
            result_root,
            &operations,
            &mut metrics,
        )?;
        let transition_digest = verify_transition(
            store,
            result_transition,
            Some(head.1),
            result_root,
            Some(&operations),
            &mut metrics,
        )
        .map_err(|error| format!("oracle transition verification: {error}"))?;
        let content_digest = verify_file(
            store,
            result_root,
            candidate,
            Some(expected_fingerprint),
            Some(expected_sequence),
            &mut metrics,
        )
        .map_err(|error| format!("oracle file verification: {error}"))?
        .0;
        Ok(PreparedEditOracle {
            operation: "same-middle".to_string(),
            offset: edit_point.byte_offset,
            removed,
            inserted,
            before_file,
            after_file,
            result_root,
            result_transition,
            result_closure: combined_closure_digest(transition_digest, content_digest),
        })
    })();
    let rollback = store.rollback(&mut metrics);
    let oracle = result?;
    rollback?;
    if store.current_head()?.as_ref() != Some(&head) {
        return Err(CoreError::PublicationConflict.into());
    }
    Ok(oracle)
}

#[allow(clippy::too_many_arguments)]
fn rewrite_same_node_by_offset(
    store: &mut Store,
    id: ObjectId,
    level: u8,
    final_node: bool,
    profile: file_codec::FileMappingProfile,
    node_start: u64,
    target: u64,
    replacement: file_codec::FileReference,
    metrics: &mut Metrics,
) -> AnyResult<(ObjectId, u64, bool)> {
    if usize::from(level) > MAX_DEPTH {
        return Err(CoreError::MappingDepthExceeded.into());
    }
    let bytes = store.get_bytes(id, metrics)?;
    if level == 0 {
        let payload = file_codec::decode_mapping(&bytes, file_codec::FILE_LEAF_TAG)?;
        let _refs_charge = charge_decoded_file_references(payload, metrics)?;
        let mut refs = file_codec::parse_file_leaf(payload)?;
        observe_file_references(metrics, refs.len())?;
        file_codec::validate_file_leaf(&refs, profile, final_node)?;
        let mut offset = node_start;
        let mut changed = false;
        for reference in &mut refs {
            let end = offset
                .checked_add(u64::from(reference.raw_length))
                .ok_or(CoreError::LengthOverflow)?;
            if !changed && reference.raw_length != 0 && target >= offset && target < end {
                *reference = replacement;
                changed = true;
            }
            offset = end;
        }
        let total = offset
            .checked_sub(node_start)
            .ok_or(CoreError::LengthOverflow)?;
        if !changed {
            return Ok((id, total, false));
        }
        let (new_id, canonical) =
            canonical_bytes_accounted(encode_charged_file_leaf(&refs, metrics)?, metrics)?;
        store.put(new_id, &canonical, metrics)?;
        add(&mut metrics.pages, 1)?;
        add_len(&mut metrics.mapping_bytes_rewritten, canonical.len())?;
        return Ok((new_id, total, true));
    }
    let payload = file_codec::decode_mapping(&bytes, file_codec::FILE_BRANCH_TAG)?;
    let _children_charge = charge_decoded_file_children(payload, false, metrics)?;
    let (branch_level, mut children) = file_codec::parse_file_children(payload, true)?;
    if branch_level != level {
        return Err(CoreError::NonCanonicalOrdering.into());
    }
    file_codec::validate_file_children(&children, profile, final_node)?;
    let total = children.last().map_or(0, |child| child.cumulative_end);
    let mut previous = 0_u64;
    let mut changed = false;
    let child_count = children.len();
    for (index, child) in children.iter_mut().enumerate() {
        let child_start = node_start
            .checked_add(previous)
            .ok_or(CoreError::LengthOverflow)?;
        let child_end = node_start
            .checked_add(child.cumulative_end)
            .ok_or(CoreError::LengthOverflow)?;
        if !changed && target >= child_start && target < child_end {
            let (new_id, _, did_change) = rewrite_same_node_by_offset(
                store,
                child.object_id,
                level
                    .checked_sub(1)
                    .ok_or(CoreError::MappingDepthExceeded)?,
                final_node && index + 1 == child_count,
                profile,
                child_start,
                target,
                replacement,
                metrics,
            )?;
            if did_change {
                child.object_id = new_id;
                changed = true;
            }
        }
        previous = child.cumulative_end;
    }
    if !changed {
        return Ok((id, total, false));
    }
    let (new_id, canonical) = canonical_bytes_accounted(
        encode_charged_file_branch(level, &children, metrics)?,
        metrics,
    )?;
    store.put(new_id, &canonical, metrics)?;
    add(&mut metrics.branches, 1)?;
    add_len(&mut metrics.mapping_bytes_rewritten, canonical.len())?;
    Ok((new_id, total, true))
}

fn rewrite_same_root_by_offset(
    store: &mut Store,
    root: ObjectId,
    candidate: Candidate,
    target: u64,
    replacement: file_codec::FileReference,
    metrics: &mut Metrics,
) -> AnyResult<(ObjectId, bool)> {
    let bytes = store.get_bytes(root, metrics)?;
    let payload = file_codec::decode_mapping(&bytes, file_codec::FILE_ROOT_TAG)?;
    let _children_charge = charge_decoded_file_children(payload, true, metrics)?;
    let (mode, total_raw, reference_count, level, mut children) =
        file_codec::parse_file_root(payload)?;
    let profile = file_codec::FileMappingProfile::new(candidate.k, candidate.f);
    if level != file_codec::expected_file_level(reference_count, profile)? {
        return Err(CoreError::NonCanonicalPagePartition.into());
    }
    file_codec::validate_file_children(&children, profile, true)?;
    let mut previous = 0_u64;
    let mut changed = false;
    let child_count = children.len();
    for (index, child) in children.iter_mut().enumerate() {
        let child_start = previous;
        let child_end = child.cumulative_end;
        if !changed && target >= child_start && target < child_end {
            let (new_id, new_total, did_change) = rewrite_same_node_by_offset(
                store,
                child.object_id,
                level,
                index + 1 == child_count,
                profile,
                child_start,
                target,
                replacement,
                metrics,
            )?;
            if did_change {
                if new_total != child_end.saturating_sub(child_start) {
                    return Err(CoreError::LengthMismatch {
                        expected: child_end.saturating_sub(child_start),
                        actual: new_total,
                    }
                    .into());
                }
                child.object_id = new_id;
                changed = true;
            }
        }
        previous = child_end;
    }
    if !changed {
        return Ok((root, false));
    }
    let (new_id, canonical) = canonical_bytes_accounted(
        encode_charged_file_root(mode, total_raw, reference_count, level, &children, metrics)?,
        metrics,
    )?;
    store.put(new_id, &canonical, metrics)?;
    add_len(&mut metrics.mapping_bytes_rewritten, canonical.len())?;
    Ok((new_id, true))
}

#[allow(clippy::too_many_arguments)]
fn rewrite_same_node_by_ordinal(
    store: &mut Store,
    id: ObjectId,
    level: u8,
    final_node: bool,
    profile: file_codec::FileMappingProfile,
    node_start: u64,
    node_references: u64,
    replacement_start: u64,
    replacements: &[file_codec::FileReference],
    metrics: &mut Metrics,
) -> AnyResult<(ObjectId, u64, u64, bool)> {
    if usize::from(level) > MAX_DEPTH {
        return Err(CoreError::MappingDepthExceeded.into());
    }
    let replacement_count =
        u64::try_from(replacements.len()).map_err(|_| CoreError::LengthOverflow)?;
    let replacement_end = replacement_start
        .checked_add(replacement_count)
        .ok_or(CoreError::LengthOverflow)?;
    let node_end = node_start
        .checked_add(node_references)
        .ok_or(CoreError::LengthOverflow)?;
    let bytes = store.get_bytes(id, metrics)?;
    if level == 0 {
        let payload = file_codec::decode_mapping(&bytes, file_codec::FILE_LEAF_TAG)?;
        let _references_charge = charge_decoded_file_references(payload, metrics)?;
        let mut references = file_codec::parse_file_leaf(payload)?;
        let (actual_count, old_total) =
            file_codec::validate_file_leaf(&references, profile, final_node)?;
        if actual_count != node_references {
            return Err(CoreError::LengthMismatch {
                expected: node_references,
                actual: actual_count,
            }
            .into());
        }
        let overlap_start = node_start.max(replacement_start);
        let overlap_end = node_end.min(replacement_end);
        if overlap_start >= overlap_end {
            return Ok((id, old_total, old_total, false));
        }
        let local_start = usize::try_from(
            overlap_start
                .checked_sub(node_start)
                .ok_or(CoreError::LengthOverflow)?,
        )
        .map_err(|_| CoreError::LengthOverflow)?;
        let local_end = usize::try_from(
            overlap_end
                .checked_sub(node_start)
                .ok_or(CoreError::LengthOverflow)?,
        )
        .map_err(|_| CoreError::LengthOverflow)?;
        let replacement_local_start = usize::try_from(
            overlap_start
                .checked_sub(replacement_start)
                .ok_or(CoreError::LengthOverflow)?,
        )
        .map_err(|_| CoreError::LengthOverflow)?;
        let replacement_local_end = replacement_local_start
            .checked_add(local_end - local_start)
            .ok_or(CoreError::LengthOverflow)?;
        references[local_start..local_end]
            .copy_from_slice(&replacements[replacement_local_start..replacement_local_end]);
        let new_total = references.iter().try_fold(0_u64, |total, reference| {
            total
                .checked_add(u64::from(reference.raw_length))
                .ok_or(CoreError::LengthOverflow)
        })?;
        let (new_id, canonical) =
            canonical_bytes_accounted(encode_charged_file_leaf(&references, metrics)?, metrics)?;
        store.put(new_id, &canonical, metrics)?;
        add(&mut metrics.pages, 1)?;
        add_len(&mut metrics.mapping_bytes_rewritten, canonical.len())?;
        return Ok((new_id, old_total, new_total, true));
    }

    let payload = file_codec::decode_mapping(&bytes, file_codec::FILE_BRANCH_TAG)?;
    let _children_charge = charge_decoded_file_children(payload, false, metrics)?;
    let (branch_level, mut children) = file_codec::parse_file_children(payload, true)?;
    if branch_level != level {
        return Err(CoreError::NonCanonicalOrdering.into());
    }
    file_codec::validate_file_children(&children, profile, final_node)?;
    let child_level = level
        .checked_sub(1)
        .ok_or(CoreError::MappingDepthExceeded)?;
    let child_capacity = subtree_reference_capacity(profile, child_level)?;
    let mut old_previous = 0_u64;
    let mut new_previous = 0_u64;
    let mut consumed_references = 0_u64;
    let child_count = children.len();
    let mut changed = false;
    for (index, child) in children.iter_mut().enumerate() {
        let child_references = node_references
            .checked_sub(consumed_references)
            .ok_or(CoreError::LengthOverflow)?
            .min(child_capacity);
        let child_start = node_start
            .checked_add(consumed_references)
            .ok_or(CoreError::LengthOverflow)?;
        let child_end = child_start
            .checked_add(child_references)
            .ok_or(CoreError::LengthOverflow)?;
        let old_declared = child
            .cumulative_end
            .checked_sub(old_previous)
            .ok_or(CoreError::LengthOverflow)?;
        let overlaps = child_start < replacement_end && child_end > replacement_start;
        let (new_id, old_actual, new_actual, child_changed) = if overlaps {
            rewrite_same_node_by_ordinal(
                store,
                child.object_id,
                child_level,
                final_node && index + 1 == child_count,
                profile,
                child_start,
                child_references,
                replacement_start,
                replacements,
                metrics,
            )?
        } else {
            (child.object_id, old_declared, old_declared, false)
        };
        if old_actual != old_declared {
            return Err(CoreError::LengthMismatch {
                expected: old_declared,
                actual: old_actual,
            }
            .into());
        }
        new_previous = new_previous
            .checked_add(new_actual)
            .ok_or(CoreError::LengthOverflow)?;
        if child.object_id != new_id || child.cumulative_end != new_previous {
            changed = true;
            child.object_id = new_id;
            child.cumulative_end = new_previous;
        }
        changed |= child_changed;
        old_previous = old_previous
            .checked_add(old_declared)
            .ok_or(CoreError::LengthOverflow)?;
        consumed_references = consumed_references
            .checked_add(child_references)
            .ok_or(CoreError::LengthOverflow)?;
    }
    if consumed_references != node_references {
        return Err(CoreError::LengthMismatch {
            expected: node_references,
            actual: consumed_references,
        }
        .into());
    }
    if !changed {
        return Ok((id, old_previous, old_previous, false));
    }
    let (new_id, canonical) = canonical_bytes_accounted(
        encode_charged_file_branch(level, &children, metrics)?,
        metrics,
    )?;
    store.put(new_id, &canonical, metrics)?;
    add(&mut metrics.branches, 1)?;
    add_len(&mut metrics.mapping_bytes_rewritten, canonical.len())?;
    Ok((new_id, old_previous, new_previous, true))
}

fn rewrite_same_root_by_ordinal(
    store: &mut Store,
    root: ObjectId,
    candidate: Candidate,
    replacement_start: u64,
    replacements: &[file_codec::FileReference],
    metrics: &mut Metrics,
) -> AnyResult<ObjectId> {
    let bytes = store.get_bytes(root, metrics)?;
    let payload = file_codec::decode_mapping(&bytes, file_codec::FILE_ROOT_TAG)?;
    let _children_charge = charge_decoded_file_children(payload, true, metrics)?;
    let (mode, total_raw, reference_count, level, mut children) =
        file_codec::parse_file_root(payload)?;
    let replacement_count =
        u64::try_from(replacements.len()).map_err(|_| CoreError::LengthOverflow)?;
    let replacement_end = replacement_start
        .checked_add(replacement_count)
        .ok_or(CoreError::LengthOverflow)?;
    if replacement_count == 0 || replacement_end > reference_count {
        return Err(CoreError::NonCanonicalPagePartition.into());
    }
    let profile = file_codec::FileMappingProfile::new(candidate.k, candidate.f);
    if level != file_codec::expected_file_level(reference_count, profile)? {
        return Err(CoreError::NonCanonicalPagePartition.into());
    }
    file_codec::validate_file_children(&children, profile, true)?;
    let child_capacity = subtree_reference_capacity(profile, level)?;
    let mut old_previous = 0_u64;
    let mut new_previous = 0_u64;
    let mut consumed_references = 0_u64;
    let child_count = children.len();
    let mut changed = false;
    for (index, child) in children.iter_mut().enumerate() {
        let child_references = reference_count
            .checked_sub(consumed_references)
            .ok_or(CoreError::LengthOverflow)?
            .min(child_capacity);
        let child_start = consumed_references;
        let child_end = child_start
            .checked_add(child_references)
            .ok_or(CoreError::LengthOverflow)?;
        let old_declared = child
            .cumulative_end
            .checked_sub(old_previous)
            .ok_or(CoreError::LengthOverflow)?;
        let overlaps = child_start < replacement_end && child_end > replacement_start;
        let (new_id, old_actual, new_actual, child_changed) = if overlaps {
            rewrite_same_node_by_ordinal(
                store,
                child.object_id,
                level,
                index + 1 == child_count,
                profile,
                child_start,
                child_references,
                replacement_start,
                replacements,
                metrics,
            )?
        } else {
            (child.object_id, old_declared, old_declared, false)
        };
        if old_actual != old_declared {
            return Err(CoreError::LengthMismatch {
                expected: old_declared,
                actual: old_actual,
            }
            .into());
        }
        new_previous = new_previous
            .checked_add(new_actual)
            .ok_or(CoreError::LengthOverflow)?;
        if child.object_id != new_id || child.cumulative_end != new_previous {
            changed = true;
            child.object_id = new_id;
            child.cumulative_end = new_previous;
        }
        changed |= child_changed;
        old_previous = old_previous
            .checked_add(old_declared)
            .ok_or(CoreError::LengthOverflow)?;
        consumed_references = consumed_references
            .checked_add(child_references)
            .ok_or(CoreError::LengthOverflow)?;
    }
    if consumed_references != reference_count
        || old_previous != total_raw
        || new_previous != total_raw
    {
        return Err(CoreError::LengthMismatch {
            expected: total_raw,
            actual: new_previous,
        }
        .into());
    }
    if !changed {
        return Err(CoreError::DeltaConflict.into());
    }
    let (new_id, canonical) = canonical_bytes_accounted(
        encode_charged_file_root(mode, total_raw, reference_count, level, &children, metrics)?,
        metrics,
    )?;
    store.put(new_id, &canonical, metrics)?;
    add_len(&mut metrics.mapping_bytes_rewritten, canonical.len())?;
    Ok(new_id)
}

fn subtree_reference_capacity(
    profile: file_codec::FileMappingProfile,
    level: u8,
) -> CoreResult<u64> {
    let mut capacity =
        u64::try_from(profile.leaf_capacity).map_err(|_| CoreError::LengthOverflow)?;
    let fanout = u64::try_from(profile.branch_capacity).map_err(|_| CoreError::LengthOverflow)?;
    for _ in 0..level {
        capacity = capacity
            .checked_mul(fanout)
            .ok_or(CoreError::LengthOverflow)?;
    }
    Ok(capacity)
}

#[allow(clippy::too_many_arguments)]
fn rebuild_plus_one_suffix(
    store: &mut Store,
    id: ObjectId,
    level: u8,
    final_node: bool,
    local_position: u64,
    profile: file_codec::FileMappingProfile,
    inserted: file_codec::FileReference,
    builder: &mut FileBuilder,
    active: &mut Vec<ObjectId>,
    suffix_references: &mut u64,
    suffix_bytes: &mut u64,
    metrics: &mut Metrics,
) -> AnyResult<(u64, u64)> {
    if usize::from(level) > MAX_DEPTH {
        return Err(CoreError::MappingDepthExceeded.into());
    }
    if active.contains(&id) {
        return Err(CoreError::MappingCycle.into());
    }
    active.push(id);
    let result = (|| {
        let bytes = store.get_bytes(id, metrics)?;
        if level == 0 {
            let payload = file_codec::decode_mapping(&bytes, file_codec::FILE_LEAF_TAG)?;
            let _references_charge = charge_decoded_file_references(payload, metrics)?;
            let references = file_codec::parse_file_leaf(payload)?;
            observe_file_references(metrics, references.len())?;
            let (count, total) = file_codec::validate_file_leaf(&references, profile, final_node)?;
            let local = usize::try_from(local_position).map_err(|_| CoreError::LengthOverflow)?;
            if local >= references.len() {
                return Err(CoreError::NonCanonicalPagePartition.into());
            }
            for reference in references.iter().take(local).copied() {
                builder.seed_reference(reference)?;
            }
            builder.push_reference(store, inserted, metrics)?;
            for reference in references.iter().skip(local).copied() {
                builder.push_reference(store, reference, metrics)?;
                *suffix_references = suffix_references
                    .checked_add(1)
                    .ok_or(CoreError::LengthOverflow)?;
                *suffix_bytes = suffix_bytes
                    .checked_add(u64::from(reference.raw_length))
                    .ok_or(CoreError::LengthOverflow)?;
            }
            return Ok((total, count));
        }

        let payload = file_codec::decode_mapping(&bytes, file_codec::FILE_BRANCH_TAG)?;
        let _children_charge = charge_decoded_file_children(payload, false, metrics)?;
        let (branch_level, children) = file_codec::parse_file_children(payload, true)?;
        if branch_level != level {
            return Err(CoreError::NonCanonicalOrdering.into());
        }
        file_codec::validate_file_children(&children, profile, final_node)?;
        let child_level = level
            .checked_sub(1)
            .ok_or(CoreError::MappingDepthExceeded)?;
        let child_capacity = subtree_reference_capacity(profile, child_level)?;
        let selected = usize::try_from(local_position / child_capacity)
            .map_err(|_| CoreError::LengthOverflow)?;
        if selected >= children.len() {
            return Err(CoreError::NonCanonicalPagePartition.into());
        }
        let mut previous_end = 0_u64;
        let mut references = 0_u64;
        for (index, child) in children.iter().enumerate() {
            let child_length = child
                .cumulative_end
                .checked_sub(previous_end)
                .ok_or(CoreError::LengthOverflow)?;
            if index < selected {
                builder.seed_node(
                    usize::from(child_level),
                    child.object_id,
                    child_length,
                    child_capacity,
                )?;
                references = references
                    .checked_add(child_capacity)
                    .ok_or(CoreError::LengthOverflow)?;
            } else if index == selected {
                let (actual_length, actual_references) = rebuild_plus_one_suffix(
                    store,
                    child.object_id,
                    child_level,
                    final_node && index + 1 == children.len(),
                    local_position % child_capacity,
                    profile,
                    inserted,
                    builder,
                    active,
                    suffix_references,
                    suffix_bytes,
                    metrics,
                )?;
                if actual_length != child_length {
                    return Err(CoreError::LengthMismatch {
                        expected: child_length,
                        actual: actual_length,
                    }
                    .into());
                }
                references = references
                    .checked_add(actual_references)
                    .ok_or(CoreError::LengthOverflow)?;
            } else {
                let (actual_length, actual_references) = walk_file_references(
                    store,
                    child.object_id,
                    child_level,
                    final_node && index + 1 == children.len(),
                    profile,
                    active,
                    &mut |store, reference, metrics| {
                        builder.push_reference(store, reference, metrics)?;
                        *suffix_references = suffix_references
                            .checked_add(1)
                            .ok_or(CoreError::LengthOverflow)?;
                        *suffix_bytes = suffix_bytes
                            .checked_add(u64::from(reference.raw_length))
                            .ok_or(CoreError::LengthOverflow)?;
                        Ok(())
                    },
                    metrics,
                )?;
                if actual_length != child_length {
                    return Err(CoreError::LengthMismatch {
                        expected: child_length,
                        actual: actual_length,
                    }
                    .into());
                }
                references = references
                    .checked_add(actual_references)
                    .ok_or(CoreError::LengthOverflow)?;
            }
            previous_end = child.cumulative_end;
        }
        Ok((previous_end, references))
    })();
    active.pop();
    result
}

#[allow(clippy::too_many_arguments)]
fn rebuild_plus_one_root(
    store: &mut Store,
    root: ObjectId,
    candidate: Candidate,
    position: u64,
    inserted: file_codec::FileReference,
    builder: &mut FileBuilder,
    metrics: &mut Metrics,
) -> AnyResult<(u64, u64)> {
    let profile = file_codec::FileMappingProfile::new(candidate.k, candidate.f);
    let bytes = store.get_bytes(root, metrics)?;
    let payload = file_codec::decode_mapping(&bytes, file_codec::FILE_ROOT_TAG)?;
    let _children_charge = charge_decoded_file_children(payload, true, metrics)?;
    let (_, declared_total, declared_references, level, children) =
        file_codec::parse_file_root(payload)?;
    if position >= declared_references
        || level != file_codec::expected_file_level(declared_references, profile)?
    {
        return Err(CoreError::NonCanonicalPagePartition.into());
    }
    file_codec::validate_file_children(&children, profile, true)?;
    let child_capacity = subtree_reference_capacity(profile, level)?;
    let selected =
        usize::try_from(position / child_capacity).map_err(|_| CoreError::LengthOverflow)?;
    if selected >= children.len() {
        return Err(CoreError::NonCanonicalPagePartition.into());
    }
    let mut previous_end = 0_u64;
    let mut references = 0_u64;
    let mut suffix_references = 0_u64;
    let mut suffix_bytes = 0_u64;
    let suffix_objects_before = metrics.objects_authenticated;
    let mut active = vec![root];
    for (index, child) in children.iter().enumerate() {
        let child_length = child
            .cumulative_end
            .checked_sub(previous_end)
            .ok_or(CoreError::LengthOverflow)?;
        if index < selected {
            builder.seed_node(
                usize::from(level),
                child.object_id,
                child_length,
                child_capacity,
            )?;
            references = references
                .checked_add(child_capacity)
                .ok_or(CoreError::LengthOverflow)?;
        } else if index == selected {
            let (actual_length, actual_references) = rebuild_plus_one_suffix(
                store,
                child.object_id,
                level,
                index + 1 == children.len(),
                position % child_capacity,
                profile,
                inserted,
                builder,
                &mut active,
                &mut suffix_references,
                &mut suffix_bytes,
                metrics,
            )?;
            if actual_length != child_length {
                return Err(CoreError::LengthMismatch {
                    expected: child_length,
                    actual: actual_length,
                }
                .into());
            }
            references = references
                .checked_add(actual_references)
                .ok_or(CoreError::LengthOverflow)?;
        } else {
            let (actual_length, actual_references) = walk_file_references(
                store,
                child.object_id,
                level,
                index + 1 == children.len(),
                profile,
                &mut active,
                &mut |store, reference, metrics| {
                    builder.push_reference(store, reference, metrics)?;
                    suffix_references = suffix_references
                        .checked_add(1)
                        .ok_or(CoreError::LengthOverflow)?;
                    suffix_bytes = suffix_bytes
                        .checked_add(u64::from(reference.raw_length))
                        .ok_or(CoreError::LengthOverflow)?;
                    Ok(())
                },
                metrics,
            )?;
            if actual_length != child_length {
                return Err(CoreError::LengthMismatch {
                    expected: child_length,
                    actual: actual_length,
                }
                .into());
            }
            references = references
                .checked_add(actual_references)
                .ok_or(CoreError::LengthOverflow)?;
        }
        previous_end = child.cumulative_end;
    }
    file_codec::validate_file_root_summary(
        declared_total,
        declared_references,
        previous_end,
        references,
    )?;
    if suffix_references
        != declared_references
            .checked_sub(position)
            .ok_or(CoreError::LengthOverflow)?
    {
        return Err(CoreError::LengthMismatch {
            expected: declared_references
                .checked_sub(position)
                .ok_or(CoreError::LengthOverflow)?,
            actual: suffix_references,
        }
        .into());
    }
    metrics.suffix_references = suffix_references;
    metrics.suffix_bytes = suffix_bytes;
    metrics.suffix_objects = metrics
        .objects_authenticated
        .checked_sub(suffix_objects_before)
        .ok_or(CoreError::LengthOverflow)?;
    Ok((declared_total, declared_references))
}

fn edit_file(
    store: &mut Store,
    candidate: Candidate,
    operation: &str,
    edit_point: EditPoint,
    transaction_started: bool,
    metrics: &mut Metrics,
) -> AnyResult<(ObjectId, ObjectId)> {
    if transaction_started {
        if store.active_transaction.is_none() {
            return Err(CoreError::ValidationAuthorityUnavailable.into());
        }
    } else {
        store.begin(metrics)?;
    }
    let (_, parent, _, _) = store
        .current_head_accounted(metrics)?
        .ok_or(CoreError::MissingObject)?;
    let file_parent = resolve_namespace_file_root(store, parent, metrics)?;
    if operation == "same-middle" {
        let mut replacement = ChargedVec::with_capacity(edit_point.replacement_length, metrics)?;
        replacement.resize(edit_point.replacement_length, 0x5a);
        let replacement = make_reference(store, &replacement, metrics)?;
        let (file_root, changed) = rewrite_same_root_by_offset(
            store,
            file_parent,
            candidate,
            edit_point.byte_offset,
            replacement,
            metrics,
        )?;
        if !changed {
            return Err(CoreError::MissingObject.into());
        }
        let root = namespace_file_root(store, file_root, metrics)?;
        let (operations, _operations_charge) =
            charged_replace_operation(b"file", file_parent, file_root, metrics)?;
        let transition =
            publish_transition_with_operations(store, Some(parent), root, &operations, metrics)?;
        return Ok((root, transition));
    }
    let file_root_bytes = store.get_bytes(file_parent, metrics)?;
    let file_root_payload =
        file_codec::decode_mapping(&file_root_bytes, file_codec::FILE_ROOT_TAG)?;
    let _file_root_children_charge =
        charge_decoded_file_children(file_root_payload, true, metrics)?;
    let (_, _, reference_count, _, _file_root_children) =
        file_codec::parse_file_root(file_root_payload)?;
    if reference_count != edit_point.reference_count {
        return Err(CoreError::LengthMismatch {
            expected: edit_point.reference_count,
            actual: reference_count,
        }
        .into());
    }
    let position = edit_point.position;
    let replacement = vec![0xa5];
    let inserted = make_reference(store, &replacement, metrics)?;
    let mut builder = FileBuilder::new(candidate);
    let (_, rebuilt_references) = rebuild_plus_one_root(
        store,
        file_parent,
        candidate,
        position,
        inserted,
        &mut builder,
        metrics,
    )?;
    if rebuilt_references != reference_count {
        return Err(CoreError::LengthMismatch {
            expected: reference_count,
            actual: rebuilt_references,
        }
        .into());
    }
    let file_root = builder.finish(store, metrics)?;
    let root = namespace_file_root(store, file_root, metrics)?;
    let operation = delta_codec::TransitionOperation::Replace {
        path: b"file".to_vec(),
        before: file_parent,
        after: file_root,
    };
    let transition =
        publish_transition_with_operations(store, Some(parent), root, &[operation], metrics)?;
    Ok((root, transition))
}

fn edit_file_same_middle_cdc(
    store: &mut Store,
    candidate: Candidate,
    edit_point: EditPoint,
    replacement: &[u8],
    transaction_started: bool,
    metrics: &mut Metrics,
) -> AnyResult<(ObjectId, ObjectId)> {
    if transaction_started {
        if store.active_transaction.is_none() {
            return Err(CoreError::ValidationAuthorityUnavailable.into());
        }
    } else {
        store.begin(metrics)?;
    }
    let (_, parent, _, _) = store
        .current_head_accounted(metrics)?
        .ok_or(CoreError::MissingObject)?;
    let file_parent = resolve_namespace_file_root(store, parent, metrics)?;
    let (replacement_start, replacements) = same_middle_rejoin_references(
        store,
        file_parent,
        candidate,
        edit_point,
        replacement,
        metrics,
    )?;
    let file_root = rewrite_same_root_by_ordinal(
        store,
        file_parent,
        candidate,
        replacement_start,
        &replacements,
        metrics,
    )?;
    let root = namespace_file_root(store, file_root, metrics)?;
    let operation = delta_codec::TransitionOperation::Replace {
        path: b"file".to_vec(),
        before: file_parent,
        after: file_root,
    };
    let transition = publish_transition_with_operations(
        store,
        Some(parent),
        root,
        std::slice::from_ref(&operation),
        metrics,
    )?;
    Ok((root, transition))
}

#[allow(clippy::too_many_arguments)]
fn walk_file_root_references<F>(
    store: &mut Store,
    id: ObjectId,
    profile: file_codec::FileMappingProfile,
    active: &mut Vec<ObjectId>,
    callback: &mut F,
    metrics: &mut Metrics,
) -> AnyResult<(u64, u64)>
where
    F: FnMut(&mut Store, file_codec::FileReference, &mut Metrics) -> AnyResult<()>,
{
    if active.contains(&id) {
        return Err(CoreError::MappingCycle.into());
    }
    active.push(id);
    let result = (|| {
        let bytes = store.get_bytes(id, metrics)?;
        let payload = file_codec::decode_mapping(&bytes, file_codec::FILE_ROOT_TAG)?;
        let _children_charge = charge_decoded_file_children(payload, true, metrics)?;
        let (_, expected_length, expected_references, level, children) =
            file_codec::parse_file_root(payload)?;
        if level != file_codec::expected_file_level(expected_references, profile)? {
            return Err(CoreError::NonCanonicalPagePartition.into());
        }
        if expected_references == 0 {
            if expected_length != 0 || level != 0 || !children.is_empty() {
                return Err(CoreError::NonCanonicalPagePartition.into());
            }
            return Ok((0, 0));
        }
        file_codec::validate_file_children(&children, profile, true)?;
        let child_count = children.len();
        let mut length = 0_u64;
        let mut references = 0_u64;
        let mut previous_end = 0_u64;
        for (index, child) in children.into_iter().enumerate() {
            let (child_length, child_references) = walk_file_references(
                store,
                child.object_id,
                level,
                index + 1 == child_count,
                profile,
                active,
                callback,
                metrics,
            )?;
            let actual_end = previous_end
                .checked_add(child_length)
                .ok_or(CoreError::LengthOverflow)?;
            if child.cumulative_end != actual_end {
                return Err(CoreError::LengthMismatch {
                    expected: child.cumulative_end,
                    actual: actual_end,
                }
                .into());
            }
            length = actual_end;
            references = references
                .checked_add(child_references)
                .ok_or(CoreError::LengthOverflow)?;
            previous_end = child.cumulative_end;
        }
        file_codec::validate_file_root_summary(
            expected_length,
            expected_references,
            length,
            references,
        )?;
        Ok((length, references))
    })();
    active.pop();
    result
}

#[allow(clippy::too_many_arguments)]
fn walk_file_references<F>(
    store: &mut Store,
    id: ObjectId,
    level: u8,
    final_node: bool,
    profile: file_codec::FileMappingProfile,
    active: &mut Vec<ObjectId>,
    callback: &mut F,
    metrics: &mut Metrics,
) -> AnyResult<(u64, u64)>
where
    F: FnMut(&mut Store, file_codec::FileReference, &mut Metrics) -> AnyResult<()>,
{
    if usize::from(level) > MAX_DEPTH {
        return Err(CoreError::MappingDepthExceeded.into());
    }
    if active.contains(&id) {
        return Err(CoreError::MappingCycle.into());
    }
    active.push(id);
    let bytes = store.get_bytes(id, metrics)?;
    let result = if level == 0 {
        let payload = file_codec::decode_mapping(&bytes, file_codec::FILE_LEAF_TAG)?;
        let _references_charge = charge_decoded_file_references(payload, metrics)?;
        let references = file_codec::parse_file_leaf(payload)?;
        observe_file_references(metrics, references.len())?;
        file_codec::validate_file_leaf(&references, profile, final_node)?;
        let mut length = 0_u64;
        for reference in references {
            callback(store, reference, metrics)?;
            length = length
                .checked_add(u64::from(reference.raw_length))
                .ok_or(CoreError::LengthOverflow)?;
        }
        (
            length,
            u64::try_from(payload.len().saturating_sub(4) / file_codec::FILE_REF_BYTES)
                .map_err(|_| CoreError::LengthOverflow)?,
        )
    } else {
        let payload = file_codec::decode_mapping(&bytes, file_codec::FILE_BRANCH_TAG)?;
        let _children_charge = charge_decoded_file_children(payload, false, metrics)?;
        let (branch_level, children) = file_codec::parse_file_children(payload, true)?;
        if branch_level != level {
            return Err(CoreError::NonCanonicalOrdering.into());
        }
        file_codec::validate_file_children(&children, profile, final_node)?;
        let child_count = children.len();
        let mut length = 0_u64;
        let mut references = 0_u64;
        let mut previous_end = 0_u64;
        for (index, child) in children.into_iter().enumerate() {
            let (child_length, child_references) = walk_file_references(
                store,
                child.object_id,
                level
                    .checked_sub(1)
                    .ok_or(CoreError::MappingDepthExceeded)?,
                final_node && index + 1 == child_count,
                profile,
                active,
                callback,
                metrics,
            )?;
            let actual_end = previous_end
                .checked_add(child_length)
                .ok_or(CoreError::LengthOverflow)?;
            if child.cumulative_end != actual_end {
                return Err(CoreError::LengthMismatch {
                    expected: child.cumulative_end,
                    actual: actual_end,
                }
                .into());
            }
            length = actual_end;
            references = references
                .checked_add(child_references)
                .ok_or(CoreError::LengthOverflow)?;
            previous_end = child.cumulative_end;
        }
        (length, references)
    };
    active.pop();
    Ok(result)
}

#[allow(clippy::too_many_arguments)]
fn stream_file(
    store: &Store,
    id: ObjectId,
    level: u8,
    final_node: bool,
    profile: file_codec::FileMappingProfile,
    batch_leaf_reads: bool,
    active: &mut Vec<ObjectId>,
    hasher: &mut Hasher,
    closure_hasher: &mut Hasher,
    sequence_hasher: &mut Hasher,
    length: &mut u64,
    reference_count: &mut u64,
    metrics: &mut Metrics,
) -> AnyResult<()> {
    if usize::from(level) > MAX_DEPTH {
        return Err(CoreError::MappingDepthExceeded.into());
    }
    if active.contains(&id) {
        return Err(CoreError::MappingCycle.into());
    }
    active.push(id);
    let bytes = store.get_bytes(id, metrics)?;
    observe_closure(closure_hasher, b"file-mapping", id, &bytes)?;
    add(&mut metrics.closure_occurrences, 1)?;
    let payload = match level {
        0 => file_codec::decode_mapping(&bytes, file_codec::FILE_LEAF_TAG)?,
        _ => file_codec::decode_mapping(&bytes, file_codec::FILE_BRANCH_TAG)?,
    };
    if level == 0 {
        let _references_charge = charge_decoded_file_references(payload, metrics)?;
        let references = file_codec::parse_file_leaf(payload)?;
        observe_file_references(metrics, references.len())?;
        file_codec::validate_file_leaf(&references, profile, final_node)?;
        *reference_count = (*reference_count)
            .checked_add(u64::try_from(references.len()).map_err(|_| CoreError::LengthOverflow)?)
            .ok_or(CoreError::LengthOverflow)?;
        let mut consume = |reference: file_codec::FileReference,
                           canonical: &[u8],
                           metrics: &mut Metrics|
         -> AnyResult<()> {
            sequence_hasher.update(&reference.raw_length.to_be_bytes());
            sequence_hasher.update(reference.raw_id.as_bytes());
            add(&mut metrics.closure_occurrences, 1)?;
            observe_closure(
                closure_hasher,
                b"file-chunk",
                reference.object_id,
                canonical,
            )?;
            let raw = layerfs_core::decode_bytes_object(canonical)?;
            if u32::try_from(raw.len()).map_err(|_| CoreError::LengthOverflow)?
                != reference.raw_length
            {
                return Err(CoreError::ChunkLengthMismatch.into());
            }
            if chunk_id_accounted(raw, metrics)? != reference.raw_id {
                return Err(CoreError::ChunkIdentityMismatch.into());
            }
            hasher.update(raw);
            *length = length
                .checked_add(u64::try_from(raw.len()).map_err(|_| CoreError::LengthOverflow)?)
                .ok_or(CoreError::LengthOverflow)?;
            Ok(())
        };
        if batch_leaf_reads {
            store.for_each_leaf_bytes(&references, profile.leaf_capacity, metrics, &mut consume)?;
        } else {
            for reference in references {
                store.with_borrowed_bytes(reference.object_id, metrics, |canonical, metrics| {
                    consume(reference, canonical, metrics)
                })?;
            }
        }
        active.pop();
        return Ok(());
    }
    let _children_charge = charge_decoded_file_children(payload, false, metrics)?;
    let (branch_level, children) = file_codec::parse_file_children(payload, true)?;
    if branch_level != level {
        return Err(CoreError::NonCanonicalOrdering.into());
    }
    file_codec::validate_file_children(&children, profile, final_node)?;
    let child_count = children.len();
    let mut previous_end = 0_u64;
    for (index, child) in children.into_iter().enumerate() {
        let child_length_before = *length;
        let child_references_before = *reference_count;
        stream_file(
            store,
            child.object_id,
            level
                .checked_sub(1)
                .ok_or(CoreError::MappingDepthExceeded)?,
            final_node && index + 1 == child_count,
            profile,
            batch_leaf_reads,
            active,
            hasher,
            closure_hasher,
            sequence_hasher,
            length,
            reference_count,
            metrics,
        )?;
        let child_length = (*length)
            .checked_sub(child_length_before)
            .ok_or(CoreError::LengthOverflow)?;
        let child_references = (*reference_count)
            .checked_sub(child_references_before)
            .ok_or(CoreError::LengthOverflow)?;
        let actual_end = previous_end
            .checked_add(child_length)
            .ok_or(CoreError::LengthOverflow)?;
        if child.cumulative_end != actual_end {
            return Err(CoreError::LengthMismatch {
                expected: child.cumulative_end,
                actual: actual_end,
            }
            .into());
        }
        if child_references == 0 && child_length != 0 {
            return Err(CoreError::LengthMismatch {
                expected: 0,
                actual: child_length,
            }
            .into());
        }
        previous_end = child.cumulative_end;
    }
    active.pop();
    Ok(())
}

fn read_file_range(
    store: &Store,
    root: ObjectId,
    candidate: Candidate,
    range: std::ops::Range<u64>,
    metrics: &mut Metrics,
) -> AnyResult<ChargedVec<u8>> {
    if range.start > range.end {
        return Err(CoreError::InvalidRange {
            start: range.start,
            end: range.end,
            length: 0,
        }
        .into());
    }
    let bytes = store.get_bytes(root, metrics)?;
    add(&mut metrics.closure_occurrences, 1)?;
    let payload = file_codec::decode_mapping(&bytes, file_codec::FILE_ROOT_TAG)?;
    let _children_charge = charge_decoded_file_children(payload, true, metrics)?;
    let (_, total, references, level, children) = file_codec::parse_file_root(payload)?;
    let profile = file_codec::FileMappingProfile::new(candidate.k, candidate.f);
    if level != file_codec::expected_file_level(references, profile)? {
        return Err(CoreError::NonCanonicalPagePartition.into());
    }
    if references == 0 {
        if total != 0 || level != 0 || !children.is_empty() {
            return Err(CoreError::NonCanonicalPagePartition.into());
        }
    } else {
        file_codec::validate_file_children(&children, profile, true)?;
    }
    if range.end > total {
        return Err(CoreError::InvalidRange {
            start: range.start,
            end: range.end,
            length: total,
        }
        .into());
    }
    let requested = usize::try_from(
        range
            .end
            .checked_sub(range.start)
            .ok_or(CoreError::LengthOverflow)?,
    )
    .map_err(|_| CoreError::LengthOverflow)?;
    let mut output = ChargedVec::with_capacity(requested, metrics)?;
    let mut previous = 0_u64;
    let child_count = children.len();
    let active_capacity = usize::from(level)
        .checked_add(2)
        .ok_or(CoreError::LengthOverflow)?;
    let _active_charge = charge_dfs_frames(active_capacity, metrics)?;
    let mut active = Vec::with_capacity(active_capacity);
    active.push(root);
    for (index, child) in children.into_iter().enumerate() {
        let child_start = previous;
        previous = child.cumulative_end;
        if child.cumulative_end <= range.start || child_start >= range.end {
            continue;
        }
        route_file_range(
            store,
            child.object_id,
            level,
            index + 1 == child_count,
            profile,
            child_start,
            &range,
            &mut output,
            &mut active,
            metrics,
        )?;
    }
    Ok(output)
}

#[allow(clippy::too_many_arguments)]
fn route_file_range(
    store: &Store,
    id: ObjectId,
    level: u8,
    final_node: bool,
    profile: file_codec::FileMappingProfile,
    node_start: u64,
    range: &std::ops::Range<u64>,
    output: &mut Vec<u8>,
    active: &mut Vec<ObjectId>,
    metrics: &mut Metrics,
) -> AnyResult<()> {
    if usize::from(level) > MAX_DEPTH {
        return Err(CoreError::MappingDepthExceeded.into());
    }
    if active.contains(&id) {
        return Err(CoreError::MappingCycle.into());
    }
    active.push(id);
    let bytes = store.get_bytes(id, metrics)?;
    add(&mut metrics.closure_occurrences, 1)?;
    if level == 0 {
        let payload = file_codec::decode_mapping(&bytes, file_codec::FILE_LEAF_TAG)?;
        let _refs_charge = charge_decoded_file_references(payload, metrics)?;
        let refs = file_codec::parse_file_leaf(payload)?;
        let refs_len = refs.len();
        observe_file_references_and_output(metrics, refs_len, output.len())?;
        file_codec::validate_file_leaf(&refs, profile, final_node)?;
        let mut offset = node_start;
        for reference in refs {
            let end = offset
                .checked_add(u64::from(reference.raw_length))
                .ok_or(CoreError::LengthOverflow)?;
            if end > range.start && offset < range.end && reference.raw_length != 0 {
                store.with_borrowed_bytes(reference.object_id, metrics, |canonical, metrics| {
                    add(&mut metrics.closure_occurrences, 1)?;
                    let raw = layerfs_core::decode_bytes_object(canonical)?;
                    if chunk_id_accounted(raw, metrics)? != reference.raw_id
                        || u32::try_from(raw.len()).map_err(|_| CoreError::LengthOverflow)?
                            != reference.raw_length
                    {
                        return Err(CoreError::ChunkIdentityMismatch.into());
                    }
                    let start = usize::try_from(range.start.saturating_sub(offset))
                        .map_err(|_| CoreError::LengthOverflow)?;
                    let finish = usize::try_from(range.end.min(end) - offset)
                        .map_err(|_| CoreError::LengthOverflow)?;
                    output.extend_from_slice(&raw[start..finish]);
                    observe_file_references_and_output(metrics, refs_len, output.len())?;
                    Ok(())
                })?;
            }
            offset = end;
        }
        active.pop();
        return Ok(());
    }
    let payload = file_codec::decode_mapping(&bytes, file_codec::FILE_BRANCH_TAG)?;
    let _children_charge = charge_decoded_file_children(payload, false, metrics)?;
    let (branch_level, children) = file_codec::parse_file_children(payload, true)?;
    if branch_level != level {
        return Err(CoreError::NonCanonicalOrdering.into());
    }
    file_codec::validate_file_children(&children, profile, final_node)?;
    let mut previous = 0_u64;
    let child_count = children.len();
    for (index, child) in children.into_iter().enumerate() {
        let child_start = node_start
            .checked_add(previous)
            .ok_or(CoreError::LengthOverflow)?;
        let child_end = node_start
            .checked_add(child.cumulative_end)
            .ok_or(CoreError::LengthOverflow)?;
        previous = child.cumulative_end;
        if child_end > range.start && child_start < range.end {
            route_file_range(
                store,
                child.object_id,
                level
                    .checked_sub(1)
                    .ok_or(CoreError::MappingDepthExceeded)?,
                final_node && index + 1 == child_count,
                profile,
                child_start,
                range,
                output,
                active,
                metrics,
            )?;
        }
    }
    active.pop();
    Ok(())
}

#[derive(Clone, Copy)]
enum SpineSide {
    Prior,
    Replacement,
}

#[derive(Clone, Copy)]
struct ChangedFilePair {
    prior: ObjectId,
    replacement: ObjectId,
    prior_declared: u64,
    replacement_declared: u64,
    final_node: bool,
}

fn record_spine_authentication(
    metrics: &mut Metrics,
    side: SpineSide,
    canonical_len: usize,
) -> CoreResult<()> {
    match side {
        SpineSide::Prior => {
            add(
                &mut metrics.incremental_prior_spine_objects_authenticated,
                1,
            )?;
            add_len(
                &mut metrics.incremental_prior_spine_bytes_authenticated,
                canonical_len,
            )
        }
        SpineSide::Replacement => {
            add(
                &mut metrics.incremental_replacement_spine_objects_authenticated,
                1,
            )?;
            add_len(
                &mut metrics.incremental_replacement_spine_bytes_authenticated,
                canonical_len,
            )
        }
    }
}

fn load_spine_bytes(
    store: &Store,
    id: ObjectId,
    side: SpineSide,
    metrics: &mut Metrics,
) -> AnyResult<ChargedBytes> {
    let bytes = store.get_bytes(id, metrics)?;
    record_spine_authentication(metrics, side, bytes.len())?;
    Ok(bytes)
}

#[allow(clippy::too_many_arguments)]
fn verify_changed_file_pair(
    store: &Store,
    prior: ObjectId,
    replacement: ObjectId,
    level: u8,
    final_node: bool,
    profile: file_codec::FileMappingProfile,
    prior_active: &mut Vec<ObjectId>,
    replacement_active: &mut Vec<ObjectId>,
    metrics: &mut Metrics,
) -> AnyResult<(u64, u64)> {
    if usize::from(level) > MAX_DEPTH {
        return Err(CoreError::MappingDepthExceeded.into());
    }
    if prior_active.contains(&prior) || replacement_active.contains(&replacement) {
        return Err(CoreError::MappingCycle.into());
    }
    prior_active.push(prior);
    replacement_active.push(replacement);
    let result = (|| {
        let prior_bytes = load_spine_bytes(store, prior, SpineSide::Prior, metrics)?;
        let replacement_bytes =
            load_spine_bytes(store, replacement, SpineSide::Replacement, metrics)?;
        if level == 0 {
            let prior_payload =
                file_codec::decode_mapping(&prior_bytes, file_codec::FILE_LEAF_TAG)?;
            let replacement_payload =
                file_codec::decode_mapping(&replacement_bytes, file_codec::FILE_LEAF_TAG)?;
            let _prior_references_charge = charge_decoded_file_references(prior_payload, metrics)?;
            let _replacement_references_charge =
                charge_decoded_file_references(replacement_payload, metrics)?;
            let prior_references = file_codec::parse_file_leaf(prior_payload)?;
            let replacement_references = file_codec::parse_file_leaf(replacement_payload)?;
            observe_file_references(metrics, prior_references.len())?;
            observe_file_references(metrics, replacement_references.len())?;
            let (prior_count, prior_total) =
                file_codec::validate_file_leaf(&prior_references, profile, final_node)?;
            let (replacement_count, replacement_total) =
                file_codec::validate_file_leaf(&replacement_references, profile, final_node)?;
            if prior_count != replacement_count {
                return Err(CoreError::LengthMismatch {
                    expected: prior_count,
                    actual: replacement_count,
                }
                .into());
            }
            for (prior_reference, replacement_reference) in
                prior_references.iter().zip(&replacement_references)
            {
                if prior_reference == replacement_reference {
                    add(&mut metrics.incremental_receipt_covered_edges, 1)?;
                    continue;
                }
                if prior_reference.object_id == replacement_reference.object_id {
                    return Err(CoreError::ChunkIdentityMismatch.into());
                }
                add(&mut metrics.incremental_new_or_different_edges, 1)?;
                store.with_borrowed_bytes(
                    replacement_reference.object_id,
                    metrics,
                    |canonical, metrics| {
                        let raw = layerfs_core::decode_bytes_object(canonical)?;
                        if u32::try_from(raw.len()).map_err(|_| CoreError::LengthOverflow)?
                            != replacement_reference.raw_length
                        {
                            return Err(CoreError::ChunkLengthMismatch.into());
                        }
                        if chunk_id_accounted(raw, metrics)? != replacement_reference.raw_id {
                            return Err(CoreError::ChunkIdentityMismatch.into());
                        }
                        add(
                            &mut metrics.incremental_new_subtree_objects_authenticated,
                            1,
                        )?;
                        add_len(
                            &mut metrics.incremental_new_subtree_bytes_authenticated,
                            canonical.len(),
                        )?;
                        Ok(())
                    },
                )?;
            }
            return Ok((prior_total, replacement_total));
        }

        let prior_payload = file_codec::decode_mapping(&prior_bytes, file_codec::FILE_BRANCH_TAG)?;
        let replacement_payload =
            file_codec::decode_mapping(&replacement_bytes, file_codec::FILE_BRANCH_TAG)?;
        let _prior_children_charge = charge_decoded_file_children(prior_payload, false, metrics)?;
        let _replacement_children_charge =
            charge_decoded_file_children(replacement_payload, false, metrics)?;
        let (prior_level, prior_children) = file_codec::parse_file_children(prior_payload, true)?;
        let (replacement_level, replacement_children) =
            file_codec::parse_file_children(replacement_payload, true)?;
        if prior_level != level
            || replacement_level != level
            || prior_children.len() != replacement_children.len()
        {
            return Err(CoreError::NonCanonicalPagePartition.into());
        }
        file_codec::validate_file_children(&prior_children, profile, final_node)?;
        file_codec::validate_file_children(&replacement_children, profile, final_node)?;
        let changed_count = prior_children
            .iter()
            .zip(&replacement_children)
            .filter(|(prior, replacement)| prior.object_id != replacement.object_id)
            .count();
        let mut changed = ChargedVec::with_item_charge(
            changed_count,
            Q_TREE_NODE_BYTES
                .checked_mul(2)
                .ok_or(CoreError::LengthOverflow)?,
            metrics,
        )?;
        let mut prior_previous = 0_u64;
        let mut replacement_previous = 0_u64;
        for (index, (prior_child, replacement_child)) in
            prior_children.iter().zip(&replacement_children).enumerate()
        {
            let prior_declared = prior_child
                .cumulative_end
                .checked_sub(prior_previous)
                .ok_or(CoreError::LengthOverflow)?;
            let replacement_declared = replacement_child
                .cumulative_end
                .checked_sub(replacement_previous)
                .ok_or(CoreError::LengthOverflow)?;
            if prior_child.object_id == replacement_child.object_id {
                if prior_declared != replacement_declared {
                    return Err(CoreError::LengthMismatch {
                        expected: prior_declared,
                        actual: replacement_declared,
                    }
                    .into());
                }
                add(&mut metrics.incremental_receipt_covered_edges, 1)?;
            } else {
                add(&mut metrics.incremental_new_or_different_edges, 1)?;
                changed.push(ChangedFilePair {
                    prior: prior_child.object_id,
                    replacement: replacement_child.object_id,
                    prior_declared,
                    replacement_declared,
                    final_node: final_node && index + 1 == prior_children.len(),
                });
            }
            prior_previous = prior_child.cumulative_end;
            replacement_previous = replacement_child.cumulative_end;
        }
        drop(_replacement_children_charge);
        drop(_prior_children_charge);
        drop(replacement_children);
        drop(prior_children);
        drop(replacement_bytes);
        drop(prior_bytes);
        for pair in changed.iter() {
            let (prior_actual, replacement_actual) = verify_changed_file_pair(
                store,
                pair.prior,
                pair.replacement,
                level
                    .checked_sub(1)
                    .ok_or(CoreError::MappingDepthExceeded)?,
                pair.final_node,
                profile,
                prior_active,
                replacement_active,
                metrics,
            )?;
            if prior_actual != pair.prior_declared {
                return Err(CoreError::LengthMismatch {
                    expected: pair.prior_declared,
                    actual: prior_actual,
                }
                .into());
            }
            if replacement_actual != pair.replacement_declared {
                return Err(CoreError::LengthMismatch {
                    expected: pair.replacement_declared,
                    actual: replacement_actual,
                }
                .into());
            }
        }
        Ok((prior_previous, replacement_previous))
    })();
    prior_active.pop();
    replacement_active.pop();
    result
}

fn verify_same_count_changed_spine(
    store: &Store,
    permit: SameOpenValidationPermit,
    replacement_root: ObjectId,
    candidate: Candidate,
    metrics: &mut Metrics,
) -> AnyResult<()> {
    let _head_receipt_charge = charge_capacity(metrics, 216)?;
    let head = store
        .current_head_accounted(metrics)?
        .ok_or(CoreError::InvalidValidationReceipt)?;
    if !permit.covers(store, &head) {
        return Err(CoreError::ValidationAuthorityUnavailable.into());
    }
    add(&mut metrics.incremental_qualification_calls, 1)?;
    let (_prior_namespace_charge, prior_namespace, prior_namespace_bytes) =
        store.get(permit.root, metrics)?;
    record_spine_authentication(metrics, SpineSide::Prior, prior_namespace_bytes.len())?;
    let (_replacement_namespace_charge, replacement_namespace, replacement_namespace_bytes) =
        store.get(replacement_root, metrics)?;
    record_spine_authentication(
        metrics,
        SpineSide::Replacement,
        replacement_namespace_bytes.len(),
    )?;
    let (Object::Directory(prior_entries), Object::Directory(replacement_entries)) =
        (prior_namespace, replacement_namespace)
    else {
        return Err(CoreError::WrongLogicalRole.into());
    };
    if prior_entries.len() != 1
        || replacement_entries.len() != 1
        || prior_entries[0].name().as_bytes() != b"file"
        || replacement_entries[0].name().as_bytes() != b"file"
        || prior_entries[0].reference().kind() != ObjectKind::Bytes
        || replacement_entries[0].reference().kind() != ObjectKind::Bytes
    {
        return Err(CoreError::WrongLogicalRole.into());
    }
    let prior_file = prior_entries[0].reference().id();
    let replacement_file = replacement_entries[0].reference().id();
    if prior_file == replacement_file {
        return Err(CoreError::PublicationConflict.into());
    }
    add(&mut metrics.incremental_new_or_different_edges, 1)?;
    let prior_root_bytes = load_spine_bytes(store, prior_file, SpineSide::Prior, metrics)?;
    let replacement_root_bytes =
        load_spine_bytes(store, replacement_file, SpineSide::Replacement, metrics)?;
    let prior_payload = file_codec::decode_mapping(&prior_root_bytes, file_codec::FILE_ROOT_TAG)?;
    let replacement_payload =
        file_codec::decode_mapping(&replacement_root_bytes, file_codec::FILE_ROOT_TAG)?;
    let _prior_children_charge = charge_decoded_file_children(prior_payload, true, metrics)?;
    let _replacement_children_charge =
        charge_decoded_file_children(replacement_payload, true, metrics)?;
    let (prior_mode, prior_total, prior_references, prior_level, prior_children) =
        file_codec::parse_file_root(prior_payload)?;
    let (
        replacement_mode,
        replacement_total,
        replacement_references,
        replacement_level,
        replacement_children,
    ) = file_codec::parse_file_root(replacement_payload)?;
    let profile = file_codec::FileMappingProfile::new(candidate.k, candidate.f);
    if prior_mode != replacement_mode
        || prior_total != replacement_total
        || prior_references != replacement_references
        || prior_level != replacement_level
        || prior_children.len() != replacement_children.len()
        || prior_level != file_codec::expected_file_level(prior_references, profile)?
        || replacement_level != file_codec::expected_file_level(replacement_references, profile)?
    {
        return Err(CoreError::NonCanonicalPagePartition.into());
    }
    file_codec::validate_file_children(&prior_children, profile, true)?;
    file_codec::validate_file_children(&replacement_children, profile, true)?;
    let active_capacity = usize::from(prior_level)
        .checked_add(3)
        .ok_or(CoreError::LengthOverflow)?;
    let _prior_active_charge = charge_dfs_frames(active_capacity, metrics)?;
    let _replacement_active_charge = charge_dfs_frames(active_capacity, metrics)?;
    let mut prior_active = Vec::with_capacity(active_capacity);
    prior_active.extend([permit.root, prior_file]);
    let mut replacement_active = Vec::with_capacity(active_capacity);
    replacement_active.extend([replacement_root, replacement_file]);
    let mut prior_previous = 0_u64;
    let mut replacement_previous = 0_u64;
    for (index, (prior_child, replacement_child)) in
        prior_children.iter().zip(&replacement_children).enumerate()
    {
        let prior_declared = prior_child
            .cumulative_end
            .checked_sub(prior_previous)
            .ok_or(CoreError::LengthOverflow)?;
        let replacement_declared = replacement_child
            .cumulative_end
            .checked_sub(replacement_previous)
            .ok_or(CoreError::LengthOverflow)?;
        if prior_child.object_id == replacement_child.object_id {
            if prior_declared != replacement_declared {
                return Err(CoreError::LengthMismatch {
                    expected: prior_declared,
                    actual: replacement_declared,
                }
                .into());
            }
            add(&mut metrics.incremental_receipt_covered_edges, 1)?;
        } else {
            add(&mut metrics.incremental_new_or_different_edges, 1)?;
            let (prior_actual, replacement_actual) = verify_changed_file_pair(
                store,
                prior_child.object_id,
                replacement_child.object_id,
                prior_level,
                index + 1 == prior_children.len(),
                profile,
                &mut prior_active,
                &mut replacement_active,
                metrics,
            )?;
            if prior_actual != prior_declared {
                return Err(CoreError::LengthMismatch {
                    expected: prior_declared,
                    actual: prior_actual,
                }
                .into());
            }
            if replacement_actual != replacement_declared {
                return Err(CoreError::LengthMismatch {
                    expected: replacement_declared,
                    actual: replacement_actual,
                }
                .into());
            }
        }
        prior_previous = prior_child.cumulative_end;
        replacement_previous = replacement_child.cumulative_end;
    }
    if prior_previous != prior_total {
        return Err(CoreError::LengthMismatch {
            expected: prior_total,
            actual: prior_previous,
        }
        .into());
    }
    if replacement_previous != replacement_total {
        return Err(CoreError::LengthMismatch {
            expected: replacement_total,
            actual: replacement_previous,
        }
        .into());
    }
    Ok(())
}

fn validate_expected_edit_result(
    expected: ExpectedEditResult,
    root: ObjectId,
    transition: ObjectId,
    operations: &[delta_codec::TransitionOperation],
) -> CoreResult<()> {
    let [delta_codec::TransitionOperation::Replace {
        path,
        before,
        after,
    }] = operations
    else {
        return Err(CoreError::DeltaConflict);
    };
    if root != expected.root
        || transition != expected.transition
        || path.as_slice() != b"file"
        || *before != expected.before_file
        || *after != expected.after_file
    {
        return Err(CoreError::DeltaConflict);
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn qualify_same_middle_full_closure(
    store: &Store,
    prior_root: ObjectId,
    root: ObjectId,
    transition: ObjectId,
    operations: &[delta_codec::TransitionOperation],
    expected: ExpectedEditResult,
    candidate: Candidate,
    expected_fingerprint: &str,
    expected_sequence: &str,
    metrics: &mut Metrics,
) -> AnyResult<u64> {
    validate_expected_edit_result(expected, root, transition, operations)?;
    let transition_digest = verify_transition(
        store,
        transition,
        Some(prior_root),
        root,
        Some(operations),
        metrics,
    )?;
    let (content_digest, references, _) = verify_file(
        store,
        root,
        candidate,
        Some(expected_fingerprint),
        Some(expected_sequence),
        metrics,
    )?;
    if combined_closure_digest(transition_digest, content_digest) != expected.closure {
        return Err(CoreError::PublicationConflict.into());
    }
    Ok(references)
}

#[allow(clippy::too_many_arguments)]
fn qualify_same_middle_changed_spine(
    store: &Store,
    permit: SameOpenValidationPermit,
    prior_root: ObjectId,
    root: ObjectId,
    transition: ObjectId,
    operations: &[delta_codec::TransitionOperation],
    expected: ExpectedEditResult,
    candidate: Candidate,
    metrics: &mut Metrics,
) -> AnyResult<()> {
    validate_expected_edit_result(expected, root, transition, operations)?;
    verify_transition(
        store,
        transition,
        Some(prior_root),
        root,
        Some(operations),
        metrics,
    )?;
    verify_same_count_changed_spine(store, permit, root, candidate, metrics)
}

fn verify_file(
    store: &Store,
    root: ObjectId,
    candidate: Candidate,
    expected_fingerprint: Option<&str>,
    expected_sequence: Option<&str>,
    metrics: &mut Metrics,
) -> AnyResult<([u8; 32], u64, u64)> {
    verify_file_inner(
        store,
        root,
        candidate,
        expected_fingerprint,
        expected_sequence,
        false,
        metrics,
    )
}

fn reconstruct_file(
    store: &Store,
    root: ObjectId,
    candidate: Candidate,
    expected_fingerprint: Option<&str>,
    expected_sequence: Option<&str>,
    metrics: &mut Metrics,
) -> AnyResult<([u8; 32], u64, u64)> {
    verify_file_inner(
        store,
        root,
        candidate,
        expected_fingerprint,
        expected_sequence,
        true,
        metrics,
    )
}

#[allow(clippy::too_many_arguments)]
fn verify_file_inner(
    store: &Store,
    root: ObjectId,
    candidate: Candidate,
    expected_fingerprint: Option<&str>,
    expected_sequence: Option<&str>,
    batch_leaf_reads: bool,
    metrics: &mut Metrics,
) -> AnyResult<([u8; 32], u64, u64)> {
    let mut closure_hasher = Hasher::new();
    let (_namespace_charge, namespace, namespace_bytes) = store.get(root, metrics)?;
    observe_closure(
        &mut closure_hasher,
        b"namespace-root",
        root,
        &namespace_bytes,
    )?;
    let Object::Directory(entries) = namespace else {
        return Err(CoreError::WrongLogicalRole.into());
    };
    if entries.len() != 1
        || entries[0].name().as_bytes() != b"file"
        || entries[0].reference().kind() != ObjectKind::Bytes
    {
        return Err(CoreError::WrongLogicalRole.into());
    }
    let file_root = entries[0].reference().id();
    let root_bytes = store.get_bytes(file_root, metrics)?;
    observe_closure(&mut closure_hasher, b"file-root", file_root, &root_bytes)?;
    let payload = file_codec::decode_mapping(&root_bytes, file_codec::FILE_ROOT_TAG)?;
    let _root_children_charge = charge_decoded_file_children(payload, true, metrics)?;
    let (_, expected_length, expected_references, level, root_children) =
        file_codec::parse_file_root(payload)?;
    let expected_level = level;
    let profile = file_codec::FileMappingProfile::new(candidate.k, candidate.f);
    if expected_level != file_codec::expected_file_level(expected_references, profile)? {
        return Err(CoreError::NonCanonicalPagePartition.into());
    }
    if expected_references == 0 {
        if !root_children.is_empty() || level != 0 || expected_length != 0 {
            return Err(CoreError::NonCanonicalPagePartition.into());
        }
    } else {
        file_codec::validate_file_children(&root_children, profile, true)?;
    }
    let mut hasher = Hasher::new();
    let mut sequence_hasher = Hasher::new();
    let mut length = 0_u64;
    let root_payload = file_codec::decode_mapping(&root_bytes, file_codec::FILE_ROOT_TAG)?;
    let _children_charge = charge_decoded_file_children(root_payload, true, metrics)?;
    let (_, _, _, root_level, children) = file_codec::parse_file_root(root_payload)?;
    let active_capacity = usize::from(root_level)
        .checked_add(1)
        .ok_or(CoreError::LengthOverflow)?;
    let _active_charge = charge_dfs_frames(active_capacity, metrics)?;
    let mut active = Vec::with_capacity(active_capacity);
    let child_count = children.len();
    let mut reference_count = 0_u64;
    let mut previous_end = 0_u64;
    for (index, child) in children.into_iter().enumerate() {
        let child_length_before = length;
        let child_references_before = reference_count;
        stream_file(
            store,
            child.object_id,
            root_level,
            index + 1 == child_count,
            profile,
            batch_leaf_reads,
            &mut active,
            &mut hasher,
            &mut closure_hasher,
            &mut sequence_hasher,
            &mut length,
            &mut reference_count,
            metrics,
        )?;
        let child_length = length
            .checked_sub(child_length_before)
            .ok_or(CoreError::LengthOverflow)?;
        let child_references = reference_count
            .checked_sub(child_references_before)
            .ok_or(CoreError::LengthOverflow)?;
        let actual_end = previous_end
            .checked_add(child_length)
            .ok_or(CoreError::LengthOverflow)?;
        if child.cumulative_end != actual_end || (child_references == 0 && child_length != 0) {
            return Err(CoreError::LengthMismatch {
                expected: child.cumulative_end,
                actual: actual_end,
            }
            .into());
        }
        previous_end = child.cumulative_end;
    }
    let reconstructed_fingerprint = hasher.finalize().to_hex().to_string();
    let reconstructed_sequence = sequence_hasher.finalize().to_hex().to_string();
    file_codec::validate_file_root_summary(
        expected_length,
        expected_references,
        length,
        reference_count,
    )?;
    if length != expected_length
        || level != root_level
        || expected_fingerprint.is_some_and(|fingerprint| fingerprint != reconstructed_fingerprint)
        || expected_sequence.is_some_and(|sequence| sequence != reconstructed_sequence)
    {
        return Err(CoreError::LengthMismatch {
            expected: expected_length,
            actual: length,
        }
        .into());
    }
    Ok((
        *closure_hasher.finalize().as_bytes(),
        reference_count,
        length,
    ))
}

fn scrub_file(
    store: &mut Store,
    root: ObjectId,
    candidate: Candidate,
    metrics: &mut Metrics,
) -> AnyResult<(u64, u64)> {
    let file_root = resolve_namespace_file_root(store, root, metrics)?;
    let root_bytes = store.get_bytes(file_root, metrics)?;
    let payload = file_codec::decode_mapping(&root_bytes, file_codec::FILE_ROOT_TAG)?;
    let _root_children_charge = charge_decoded_file_children(payload, true, metrics)?;
    let (_, expected_length, expected_references, level, _root_children) =
        file_codec::parse_file_root(payload)?;
    let profile = file_codec::FileMappingProfile::new(candidate.k, candidate.f);
    if level != file_codec::expected_file_level(expected_references, profile)? {
        return Err(CoreError::NonCanonicalPagePartition.into());
    }
    let active_capacity = usize::from(level)
        .checked_add(2)
        .ok_or(CoreError::LengthOverflow)?;
    let _active_charge = charge_dfs_frames(active_capacity, metrics)?;
    let mut active = Vec::with_capacity(active_capacity);
    let mut callback = |store: &mut Store,
                        reference: file_codec::FileReference,
                        metrics: &mut Metrics|
     -> AnyResult<()> {
        store.with_borrowed_bytes(reference.object_id, metrics, |canonical, metrics| {
            let raw = layerfs_core::decode_bytes_object(canonical)?;
            if u32::try_from(raw.len()).map_err(|_| CoreError::LengthOverflow)?
                != reference.raw_length
                || chunk_id_accounted(raw, metrics)? != reference.raw_id
            {
                return Err(CoreError::ChunkIdentityMismatch.into());
            }
            Ok(())
        })
    };
    let (length, references) = walk_file_root_references(
        store,
        file_root,
        profile,
        &mut active,
        &mut callback,
        metrics,
    )?;
    file_codec::validate_file_root_summary(
        expected_length,
        expected_references,
        length,
        references,
    )?;
    Ok((length, references))
}

fn establish_same_open_file_witness(
    store: &mut Store,
    candidate: Candidate,
    expected_parent: Option<ObjectId>,
    expected_operations: Option<&[delta_codec::TransitionOperation]>,
    metrics: &mut Metrics,
) -> AnyResult<SameOpenValidationWitness> {
    if store.active_transaction.is_none() {
        return Err(CoreError::ValidationAuthorityUnavailable.into());
    }
    let _head_receipt_charge = charge_capacity(metrics, 216)?;
    let head = store
        .current_head_accounted(metrics)?
        .ok_or(CoreError::InvalidValidationReceipt)?;
    verify_transition(
        store,
        head.2,
        expected_parent,
        head.1,
        expected_operations,
        metrics,
    )?;
    scrub_file(store, head.1, candidate, metrics)?;
    if let Some(parent) = expected_parent {
        scrub_file(store, parent, candidate, metrics)?;
    }
    if store.current_head_accounted(metrics)?.as_ref() != Some(&head) {
        return Err(CoreError::PublicationConflict.into());
    }
    store.issue_same_open_witness(&head, metrics)
}

fn verify_ranges(
    store: &Store,
    file_root: ObjectId,
    candidate: Candidate,
    probes: &[(&'static str, std::ops::Range<u64>)],
    expected: &[Vec<u8>],
    metrics: &mut Metrics,
) -> AnyResult<ChargedVec<RangeMeasurement>> {
    if probes.len() != expected.len() {
        return Err(CoreError::LengthMismatch {
            expected: u64::try_from(probes.len()).map_err(|_| CoreError::LengthOverflow)?,
            actual: u64::try_from(expected.len()).map_err(|_| CoreError::LengthOverflow)?,
        }
        .into());
    }
    let mut measurements = ChargedVec::with_capacity(probes.len(), metrics)?;
    for ((label, range), expected) in probes.iter().zip(expected) {
        let authenticated_before = metrics.canonical_bytes_authenticated;
        let objects_before = metrics.objects_authenticated;
        let started = Instant::now();
        let actual = read_file_range(store, file_root, candidate, range.clone(), metrics)?;
        let wall_ns = started.elapsed().as_nanos();
        if actual.as_slice() != expected.as_slice() {
            return Err(CoreError::PublicationConflict.into());
        }
        measurements.push(RangeMeasurement {
            label,
            range: range.clone(),
            wall_ns,
            returned_bytes: actual.len(),
            canonical_bytes_authenticated: metrics
                .canonical_bytes_authenticated
                .checked_sub(authenticated_before)
                .ok_or(CoreError::LengthOverflow)?,
            objects_authenticated: metrics
                .objects_authenticated
                .checked_sub(objects_before)
                .ok_or(CoreError::LengthOverflow)?,
        });
    }
    Ok(measurements)
}

fn empty_file_root(store: &mut Store, metrics: &mut Metrics) -> AnyResult<ObjectId> {
    let inner = encode_charged_file_root(0, 0, 0, 0, &[], metrics)?;
    put_mapping(store, inner, metrics)
}

fn directory_name(number: usize) -> AnyResult<CanonicalName> {
    CanonicalName::from_bytes(format!("{number:08}-{}", "x".repeat(246)).as_bytes())
        .map_err(Into::into)
}

fn page_object(
    store: &mut Store,
    entries: &[DirectoryEntry],
    metrics: &mut Metrics,
) -> AnyResult<(ObjectId, Vec<u8>)> {
    let canonical = dir_codec::encode_directory_page(entries)?;
    let id = object_id_accounted(&canonical, metrics)?;
    store.put(id, &canonical, metrics)?;
    add(&mut metrics.pages, 1)?;
    add_len(&mut metrics.mapping_bytes_rewritten, canonical.len())?;
    Ok((id, canonical))
}

fn greedy_directory_entries(
    first: usize,
    last_number: usize,
    child: ObjectId,
    candidate: Candidate,
) -> AnyResult<Vec<DirectoryEntry>> {
    let end = last_number
        .checked_add(1)
        .ok_or(CoreError::LengthOverflow)?;
    if first >= end {
        return Err(CoreError::NonCanonicalPagePartition.into());
    }
    let mut entries = Vec::new();
    let mut encoded_size = 9_usize.checked_add(4).ok_or(CoreError::LengthOverflow)?;
    for number in first..end {
        let name = directory_name(number)?;
        let entry_size = 4_usize
            .checked_add(name.as_bytes().len())
            .and_then(|value| value.checked_add(1 + 32))
            .ok_or(CoreError::LengthOverflow)?;
        let next_size = encoded_size
            .checked_add(entry_size)
            .ok_or(CoreError::LengthOverflow)?;
        if !entries.is_empty() && next_size > candidate.directory_page {
            break;
        }
        entries.push(DirectoryEntry::new(
            name,
            ObjectReference::new(ObjectKind::Bytes, child),
        ));
        encoded_size = next_size;
    }
    if entries.is_empty() || encoded_size > candidate.directory_page {
        return Err(CoreError::NonCanonicalPagePartition.into());
    }
    Ok(entries)
}

fn build_directory(
    store: &mut Store,
    candidate: Candidate,
    leading: bool,
    replacement: bool,
    metrics: &mut Metrics,
) -> AnyResult<(ObjectId, ObjectId)> {
    store.begin(metrics)?;
    let child = empty_file_root(store, metrics)?;
    let replacement_child = if replacement {
        let inner = encode_charged_file_root(1, 0, 0, 0, &[], metrics)?;
        Some(put_mapping(store, inner, metrics)?)
    } else {
        None
    };
    let mut pages = Vec::new();
    let mut start = 1_usize;
    let total = if leading {
        DIRECTORY_ENTRIES
            .checked_add(1)
            .ok_or(CoreError::LengthOverflow)?
    } else {
        DIRECTORY_ENTRIES
    };
    let last_number = if leading { total - 1 } else { total };
    while start <= total {
        let first_number = if leading {
            start.saturating_sub(1)
        } else {
            start
        };
        let page_child = if replacement
            && start <= DIRECTORY_ENTRIES / 2
            && start.checked_add(1).ok_or(CoreError::LengthOverflow)? > DIRECTORY_ENTRIES / 2
        {
            replacement_child.ok_or(CoreError::MissingObject)?
        } else {
            child
        };
        let entries = greedy_directory_entries(first_number, last_number, page_child, candidate)?;
        let count = entries.len();
        let (id, _) = page_object(store, &entries, metrics)?;
        pages.push(dir_codec::DirectoryPageRef {
            count: u32::try_from(count).map_err(|_| CoreError::LengthOverflow)?,
            first_name: entries[0].name().as_bytes().to_vec(),
            object_id: id,
        });
        start = start.checked_add(count).ok_or(CoreError::LengthOverflow)?;
    }
    let metadata = put_mapping(
        store,
        encode_charged_directory_metadata(0, metrics)?,
        metrics,
    )?;
    let index = put_mapping(
        store,
        encode_charged_directory_index(
            u32::try_from(total).map_err(|_| CoreError::LengthOverflow)?,
            &pages,
            metrics,
        )?,
        metrics,
    )?;
    let wrapper = dir_codec::encode_directory_wrapper(metadata, index)?;
    let root = object_id_accounted(&wrapper, metrics)?;
    store.put(root, &wrapper, metrics)?;
    add_len(&mut metrics.mapping_bytes_rewritten, wrapper.len())?;
    let transition = publish_transition(store, None, root, metrics)?;
    Ok((root, transition))
}

fn directory_parts(
    store: &Store,
    root: ObjectId,
    metrics: &mut Metrics,
) -> AnyResult<Vec<dir_codec::DirectoryPageRef>> {
    let (_object_charge, object, _object_bytes) = store.get(root, metrics)?;
    let Object::Directory(entries) = object else {
        return Err(CoreError::WrongLogicalRole.into());
    };
    if entries.len() != 2
        || entries[0].name().as_bytes() != b"m"
        || entries[1].name().as_bytes() != b"t"
        || entries
            .iter()
            .any(|entry| entry.reference().kind() != ObjectKind::Bytes)
    {
        return Err(CoreError::WrongLogicalRole.into());
    }
    let (_metadata_charge, _, metadata_bytes) = store.get(entries[0].reference().id(), metrics)?;
    let metadata = file_codec::decode_mapping(&metadata_bytes, file_codec::DIR_METADATA_TAG)?;
    if metadata.len() != 4 {
        return Err(CoreError::NonCanonicalOrdering.into());
    }
    let index = entries[1].reference().id();
    let (_index_charge, _, bytes) = store.get(index, metrics)?;
    let payload = file_codec::decode_mapping(&bytes, file_codec::DIR_INDEX_TAG)?;
    Ok(dir_codec::parse_directory_index(payload)?)
}

fn directory_page_entries(
    store: &Store,
    id: ObjectId,
    metrics: &mut Metrics,
) -> AnyResult<Vec<DirectoryEntry>> {
    let (_object_charge, object, _object_bytes) = store.get(id, metrics)?;
    let Object::Directory(entries) = object else {
        return Err(CoreError::WrongLogicalRole.into());
    };
    Ok(entries)
}

fn edit_directory(
    store: &mut Store,
    candidate: Candidate,
    operation: &str,
    metrics: &mut Metrics,
) -> AnyResult<(ObjectId, ObjectId)> {
    let (_, parent, _, _) = store
        .current_head_accounted(metrics)?
        .ok_or(CoreError::MissingObject)?;
    let (_parent_charge, _, parent_bytes) = store.get(parent, metrics)?;
    let Object::Directory(parent_entries) = decode_object(&parent_bytes)? else {
        return Err(CoreError::WrongLogicalRole.into());
    };
    let before_index = parent_entries
        .iter()
        .find(|entry| entry.name().as_bytes() == b"t")
        .ok_or(CoreError::WrongLogicalRole)?
        .reference()
        .id();
    let old_pages = directory_parts(store, parent, metrics)?;
    store.begin(metrics)?;
    let child = if operation == "dir-replace" {
        put_mapping(
            store,
            encode_charged_file_root(1, 0, 0, 0, &[], metrics)?,
            metrics,
        )?
    } else {
        empty_file_root(store, metrics)?
    };
    let mut pages = Vec::new();
    if operation == "dir-replace" {
        let target = DIRECTORY_ENTRIES / 2;
        let mut seen = 0_usize;
        let (page_index, local) = old_pages
            .iter()
            .enumerate()
            .find_map(|(index, page)| {
                let count = usize::try_from(page.count).ok()?;
                let end = seen.checked_add(count)?;
                let result = (target >= seen && target < end).then_some((index, target - seen));
                seen = end;
                result
            })
            .ok_or(CoreError::NonCanonicalPagePartition)?;
        pages = old_pages.clone();
        let page = &old_pages[page_index];
        let mut entries = directory_page_entries(store, page.object_id, metrics)?;
        if local >= entries.len() {
            return Err(CoreError::NonCanonicalPagePartition.into());
        }
        entries[local] = DirectoryEntry::new(
            entries[local].name().clone(),
            ObjectReference::new(ObjectKind::Bytes, child),
        );
        let (id, _) = page_object(store, &entries, metrics)?;
        pages[page_index] = dir_codec::DirectoryPageRef {
            count: page.count,
            first_name: entries[0].name().as_bytes().to_vec(),
            object_id: id,
        };
    } else {
        let total = DIRECTORY_ENTRIES
            .checked_add(1)
            .ok_or(CoreError::LengthOverflow)?;
        let mut start = 0_usize;
        while start < total {
            let entries = greedy_directory_entries(
                start,
                total.checked_sub(1).ok_or(CoreError::LengthOverflow)?,
                child,
                candidate,
            )?;
            let count = entries.len();
            let (id, _) = page_object(store, &entries, metrics)?;
            pages.push(dir_codec::DirectoryPageRef {
                count: u32::try_from(count).map_err(|_| CoreError::LengthOverflow)?,
                first_name: entries[0].name().as_bytes().to_vec(),
                object_id: id,
            });
            start = start.checked_add(count).ok_or(CoreError::LengthOverflow)?;
        }
    }
    let metadata = put_mapping(
        store,
        encode_charged_directory_metadata(0, metrics)?,
        metrics,
    )?;
    let index = put_mapping(
        store,
        encode_charged_directory_index(
            u32::try_from(DIRECTORY_ENTRIES + usize::from(operation == "dir-leading"))
                .map_err(|_| CoreError::LengthOverflow)?,
            &pages,
            metrics,
        )?,
        metrics,
    )?;
    let wrapper = dir_codec::encode_directory_wrapper(metadata, index)?;
    let root = object_id_accounted(&wrapper, metrics)?;
    store.put(root, &wrapper, metrics)?;
    add_len(&mut metrics.mapping_bytes_rewritten, wrapper.len())?;
    let operation_record = delta_codec::TransitionOperation::Replace {
        path: b"t".to_vec(),
        before: before_index,
        after: index,
    };
    let transition = publish_transition_with_operations(
        store,
        Some(parent),
        root,
        &[operation_record],
        metrics,
    )?;
    Ok((root, transition))
}

fn verify_directory(
    store: &Store,
    root: ObjectId,
    candidate: Candidate,
    expected_entries: u64,
    expected_replacement: Option<(u64, ObjectId)>,
    metrics: &mut Metrics,
) -> AnyResult<[u8; 32]> {
    let mut closure_hasher = Hasher::new();
    let (_root_charge, object, root_bytes) = store.get(root, metrics)?;
    observe_closure(&mut closure_hasher, b"directory-root", root, &root_bytes)?;
    let Object::Directory(wrapper) = object else {
        return Err(CoreError::WrongLogicalRole.into());
    };
    if wrapper.len() != 2
        || wrapper[0].name().as_bytes() != b"m"
        || wrapper[1].name().as_bytes() != b"t"
        || wrapper
            .iter()
            .any(|entry| entry.reference().kind() != ObjectKind::Bytes)
    {
        return Err(CoreError::WrongLogicalRole.into());
    }
    let metadata_id = wrapper[0].reference().id();
    let (_metadata_charge, metadata_object, metadata_bytes) = store.get(metadata_id, metrics)?;
    observe_closure(
        &mut closure_hasher,
        b"directory-metadata",
        metadata_id,
        &metadata_bytes,
    )?;
    if !matches!(metadata_object, Object::Bytes(_))
        || file_codec::decode_mapping(&metadata_bytes, file_codec::DIR_METADATA_TAG)?.len() != 4
    {
        return Err(CoreError::WrongLogicalRole.into());
    }
    let index_id = wrapper[1].reference().id();
    let (_index_charge, _, index_bytes) = store.get(index_id, metrics)?;
    observe_closure(
        &mut closure_hasher,
        b"directory-index",
        index_id,
        &index_bytes,
    )?;
    let pages = dir_codec::parse_directory_index(file_codec::decode_mapping(
        &index_bytes,
        file_codec::DIR_INDEX_TAG,
    )?)?;
    let mut partition = dir_codec::DirectoryPartitionValidator::new(candidate.directory_page);
    let mut total = 0_u64;
    let mut replacement_seen = false;
    let mut expected_number = if expected_entries == DIRECTORY_ENTRIES as u64 + 1 {
        0
    } else {
        1
    };
    let mut previous_last: Option<Vec<u8>> = None;
    for page in &pages {
        let (_page_charge, page_object, page_bytes) = store.get(page.object_id, metrics)?;
        observe_closure(
            &mut closure_hasher,
            b"directory-page",
            page.object_id,
            &page_bytes,
        )?;
        let Object::Directory(entries) = page_object else {
            return Err(CoreError::WrongLogicalRole.into());
        };
        partition.push(&entries, page)?;
        let page_start = expected_number;
        let page_child = entries
            .first()
            .ok_or(CoreError::NonCanonicalPagePartition)?
            .reference()
            .id();
        let last_directory_number = if expected_entries == DIRECTORY_ENTRIES as u64 + 1 {
            expected_entries
                .checked_sub(1)
                .ok_or(CoreError::LengthOverflow)?
        } else {
            expected_entries
        };
        let greedy = greedy_directory_entries(
            page_start,
            usize::try_from(last_directory_number).map_err(|_| CoreError::LengthOverflow)?,
            page_child,
            candidate,
        )?;
        if entries.len() != usize::try_from(page.count).map_err(|_| CoreError::LengthOverflow)?
            || entries.first().map(|entry| entry.name().as_bytes())
                != Some(page.first_name.as_slice())
            || greedy.len() != entries.len()
        {
            return Err(CoreError::NonCanonicalOrdering.into());
        }
        for entry in &entries {
            if entry.name().as_bytes() != directory_name(expected_number)?.as_bytes() {
                return Err(CoreError::NonCanonicalOrdering.into());
            }
            if previous_last
                .as_ref()
                .is_some_and(|last| last.as_slice() >= entry.name().as_bytes())
            {
                return Err(CoreError::NonCanonicalPagePartition.into());
            }
            previous_last = Some(entry.name().as_bytes().to_vec());
            expected_number = expected_number
                .checked_add(1)
                .ok_or(CoreError::LengthOverflow)?;
        }
        for (index, entry) in entries.into_iter().enumerate() {
            let entry_number = page_start
                .checked_add(index)
                .ok_or(CoreError::LengthOverflow)?;
            let child_id = entry.reference().id();
            if let Some((expected_number, expected_id)) = expected_replacement {
                if u64::try_from(entry_number).map_err(|_| CoreError::LengthOverflow)?
                    == expected_number
                {
                    if child_id != expected_id {
                        return Err(CoreError::ChunkIdentityMismatch.into());
                    }
                    replacement_seen = true;
                } else if child_id == expected_id {
                    return Err(CoreError::NonCanonicalOrdering.into());
                }
            }
            let (_child_charge, child, child_bytes) = store.get(child_id, metrics)?;
            observe_closure(
                &mut closure_hasher,
                b"directory-target",
                child_id,
                &child_bytes,
            )?;
            if child.kind() != ObjectKind::Bytes {
                return Err(CoreError::WrongLogicalRole.into());
            }
            add(&mut metrics.closure_occurrences, 1)?;
            total = total.checked_add(1).ok_or(CoreError::LengthOverflow)?;
        }
    }
    partition.finish(u32::try_from(expected_entries).map_err(|_| CoreError::LengthOverflow)?)?;
    if total != expected_entries {
        return Err(CoreError::LengthMismatch {
            expected: expected_entries,
            actual: total,
        }
        .into());
    }
    if expected_replacement.is_some() && !replacement_seen {
        return Err(CoreError::NonCanonicalOrdering.into());
    }
    Ok(*closure_hasher.finalize().as_bytes())
}

fn lookup_directory_entry(
    store: &Store,
    root: ObjectId,
    candidate: Candidate,
    name: &CanonicalName,
    metrics: &mut Metrics,
) -> AnyResult<ObjectId> {
    let (_root_charge, root_object, _root_bytes) = store.get(root, metrics)?;
    let Object::Directory(wrapper) = root_object else {
        return Err(CoreError::WrongLogicalRole.into());
    };
    if wrapper.len() != 2
        || wrapper[0].name().as_bytes() != b"m"
        || wrapper[1].name().as_bytes() != b"t"
        || wrapper
            .iter()
            .any(|entry| entry.reference().kind() != ObjectKind::Bytes)
    {
        return Err(CoreError::WrongLogicalRole.into());
    }
    let (_metadata_charge, _, metadata_bytes) = store.get(wrapper[0].reference().id(), metrics)?;
    if file_codec::decode_mapping(&metadata_bytes, file_codec::DIR_METADATA_TAG)?.len() != 4 {
        return Err(CoreError::WrongLogicalRole.into());
    }
    let (_index_charge, _, index_bytes) = store.get(wrapper[1].reference().id(), metrics)?;
    let pages = dir_codec::parse_directory_index(file_codec::decode_mapping(
        &index_bytes,
        file_codec::DIR_INDEX_TAG,
    )?)?;
    let selected = pages
        .partition_point(|page| page.first_name.as_slice() <= name.as_bytes())
        .checked_sub(1)
        .ok_or(CoreError::PathNotFound)?;
    let descriptor = pages
        .get(selected)
        .ok_or(CoreError::NonCanonicalPagePartition)?;
    let (_page_charge, page_object, page_bytes) = store.get(descriptor.object_id, metrics)?;
    if page_bytes.len() > candidate.directory_page {
        return Err(CoreError::NonCanonicalPagePartition.into());
    }
    let Object::Directory(entries) = page_object else {
        return Err(CoreError::WrongLogicalRole.into());
    };
    if entries.len() != usize::try_from(descriptor.count).map_err(|_| CoreError::LengthOverflow)?
        || entries.first().map(|entry| entry.name().as_bytes())
            != Some(descriptor.first_name.as_slice())
    {
        return Err(CoreError::NonCanonicalPagePartition.into());
    }
    let entry = entries
        .binary_search_by(|entry| entry.name().cmp(name))
        .ok()
        .and_then(|index| entries.get(index))
        .ok_or(CoreError::PathNotFound)?;
    if entry.reference().kind() != ObjectKind::Bytes {
        return Err(CoreError::WrongLogicalRole.into());
    }
    let child = entry.reference().id();
    let (_child_charge, _, child_bytes) = store.get(child, metrics)?;
    file_codec::decode_mapping(&child_bytes, file_codec::FILE_ROOT_TAG)?;
    Ok(child)
}

fn verify_directory_lookups(
    store: &Store,
    root: ObjectId,
    candidate: Candidate,
    leading: bool,
    expected_replacement: Option<(u64, ObjectId)>,
    metrics: &mut Metrics,
) -> AnyResult<ChargedVec<RangeMeasurement>> {
    let first = usize::from(!leading);
    let middle = if leading {
        DIRECTORY_ENTRIES / 2
    } else {
        DIRECTORY_ENTRIES / 2 + 1
    };
    let unchanged = canonical_bytes(file_codec::encode_file_root(0, 0, 0, 0, &[])?)?.0;
    let mut measurements = ChargedVec::with_capacity(3, metrics)?;
    for (label, number) in [
        ("directory-lookup-first", first),
        ("directory-lookup-middle", middle),
        ("directory-lookup-last", DIRECTORY_ENTRIES),
    ] {
        let name = directory_name(number)?;
        let authenticated_before = metrics.canonical_bytes_authenticated;
        let objects_before = metrics.objects_authenticated;
        let started = Instant::now();
        let actual = lookup_directory_entry(store, root, candidate, &name, metrics)?;
        let wall_ns = started.elapsed().as_nanos();
        let number = u64::try_from(number).map_err(|_| CoreError::LengthOverflow)?;
        let expected = expected_replacement
            .filter(|(entry, _)| *entry == number)
            .map_or(unchanged, |(_, id)| id);
        if actual != expected {
            return Err(CoreError::ChunkIdentityMismatch.into());
        }
        measurements.push(RangeMeasurement {
            label,
            range: number..number,
            wall_ns,
            returned_bytes: 32,
            canonical_bytes_authenticated: metrics
                .canonical_bytes_authenticated
                .checked_sub(authenticated_before)
                .ok_or(CoreError::LengthOverflow)?,
            objects_authenticated: metrics
                .objects_authenticated
                .checked_sub(objects_before)
                .ok_or(CoreError::LengthOverflow)?,
        });
    }
    Ok(measurements)
}

fn apparent_file_bytes(path: &Path) -> Option<u64> {
    match fs::metadata(path) {
        Ok(metadata) => Some(metadata.len()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Some(0),
        Err(_) => None,
    }
}

#[cfg(target_os = "macos")]
fn allocated_file_bytes(path: &Path) -> Option<u64> {
    use std::os::macos::fs::MetadataExt;

    match fs::metadata(path) {
        Ok(metadata) => metadata.st_blocks().checked_mul(512),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Some(0),
        Err(_) => None,
    }
}

#[cfg(all(unix, not(target_os = "macos")))]
fn allocated_file_bytes(path: &Path) -> Option<u64> {
    use std::os::unix::fs::MetadataExt;

    match fs::metadata(path) {
        Ok(metadata) => metadata.blocks().checked_mul(512),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Some(0),
        Err(_) => None,
    }
}

#[cfg(not(unix))]
fn allocated_file_bytes(_path: &Path) -> Option<u64> {
    None
}

fn expectations_path(path: &Path) -> PathBuf {
    let mut value = path.as_os_str().to_os_string();
    value.push(".expectations");
    PathBuf::from(value)
}

fn oracle_database_path(path: &Path) -> PathBuf {
    let mut value = path.as_os_str().to_os_string();
    value.push(".oracle.sqlite");
    PathBuf::from(value)
}

fn hex_bytes(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn decode_hex(value: &str) -> AnyResult<Vec<u8>> {
    if value.len() % 2 != 0 {
        return Err(CoreError::InvalidIdentityText.into());
    }
    let mut decoded = Vec::new();
    decoded
        .try_reserve_exact(value.len() / 2)
        .map_err(|_| CoreError::AllocationFailed)?;
    for pair in value.as_bytes().chunks_exact(2) {
        let digit = |value| match value {
            b'0'..=b'9' => Ok(value - b'0'),
            b'a'..=b'f' => Ok(value - b'a' + 10),
            b'A'..=b'F' => Ok(value - b'A' + 10),
            _ => Err(CoreError::InvalidIdentityText),
        };
        decoded.push((digit(pair[0])? << 4) | digit(pair[1])?);
    }
    Ok(decoded)
}

fn probe_label(value: &str) -> AnyResult<&'static str> {
    match value {
        "zero" => Ok("zero"),
        "first-byte" => Ok("first-byte"),
        "cross-chunk" => Ok("cross-chunk"),
        "leaf-boundary" => Ok("leaf-boundary"),
        "branch-boundary" => Ok("branch-boundary"),
        "last-byte" => Ok("last-byte"),
        "eof" => Ok("eof"),
        _ => Err(format!("unknown prepared range label {value}").into()),
    }
}

fn expected_value<'a>(line: Option<&'a str>, prefix: &str) -> AnyResult<&'a str> {
    line.and_then(|line| line.strip_prefix(prefix))
        .ok_or_else(|| format!("malformed prepared expectation: missing {prefix}").into())
}

fn exact_owned(value: &str) -> CoreResult<String> {
    let mut owned = String::new();
    owned
        .try_reserve_exact(value.len())
        .map_err(|_| CoreError::AllocationFailed)?;
    owned.push_str(value);
    Ok(owned)
}

fn prepared_expectations_preflight_capacity(body: &str) -> AnyResult<(usize, usize)> {
    let mut lines = body.lines();
    if lines.next() != Some("LFS-WP4M-EXPECTATIONS-2") {
        return Err("prepared expectation version mismatch".into());
    }
    let _ = expected_value(lines.next(), "source_length=")?;
    let source_fingerprint = expected_value(lines.next(), "source_fingerprint=")?;
    let _ = expected_value(lines.next(), "edit=")?;
    let file = expected_value(lines.next(), "file=")?;
    let mut capacity = source_fingerprint.len();
    if file != "-" {
        let mut values = file.split(',');
        let _ = values.next().ok_or("missing expected references")?;
        capacity = capacity
            .checked_add(values.next().ok_or("missing expected fingerprint")?.len())
            .and_then(|value| {
                value.checked_add(values.next().ok_or("missing expected sequence").ok()?.len())
            })
            .ok_or(CoreError::LengthOverflow)?;
        if values.next().is_some() {
            return Err("malformed prepared file observations".into());
        }
    }
    let _ = expected_value(lines.next(), "base=")?;
    let oracle = expected_value(lines.next(), "oracle=")?;
    if oracle != "-" {
        let mut values = oracle.split(',');
        capacity = capacity
            .checked_add(values.next().ok_or("missing oracle operation")?.len())
            .ok_or(CoreError::LengthOverflow)?;
        let _ = values.next().ok_or("missing oracle offset")?;
        for label in ["removed", "inserted"] {
            let hex = values.next().ok_or(label)?;
            if hex.len() % 2 != 0 {
                return Err(CoreError::InvalidIdentityText.into());
            }
            capacity = capacity
                .checked_add(hex.len() / 2)
                .ok_or(CoreError::LengthOverflow)?;
        }
        for label in [
            "before file",
            "after file",
            "result root",
            "result transition",
            "result closure",
        ] {
            let _ = values.next().ok_or(label)?;
        }
        if values.next().is_some() {
            return Err("malformed prepared edit oracle".into());
        }
    }
    let mut ranges = 0usize;
    for line in lines {
        let value = line
            .strip_prefix("range=")
            .ok_or("malformed prepared range")?;
        let mut values = value.splitn(4, ',');
        let _ = values.next().ok_or("missing range label")?;
        let _ = values.next().ok_or("missing range start")?;
        let _ = values.next().ok_or("missing range end")?;
        let hex = values.next().ok_or("missing range bytes")?;
        if hex.len() % 2 != 0 {
            return Err(CoreError::InvalidIdentityText.into());
        }
        capacity = capacity
            .checked_add(hex.len() / 2)
            .ok_or(CoreError::LengthOverflow)?;
        ranges = ranges.checked_add(1).ok_or(CoreError::LengthOverflow)?;
    }
    capacity = capacity
        .checked_add(
            ranges
                .checked_mul(std::mem::size_of::<Vec<u8>>())
                .ok_or(CoreError::LengthOverflow)?,
        )
        .and_then(|value| {
            value.checked_add(
                ranges.checked_mul(std::mem::size_of::<(&'static str, std::ops::Range<u64>)>())?,
            )
        })
        .ok_or(CoreError::LengthOverflow)?;
    Ok((capacity, ranges))
}

fn write_prepared_expectations(path: &Path, expected: &PreparedExpectations) -> AnyResult<()> {
    let mut body = format!(
        "LFS-WP4M-EXPECTATIONS-2\nsource_length={}\nsource_fingerprint={}\n",
        expected.source_length, expected.source_fingerprint
    );
    if let Some(point) = expected.edit_point {
        body.push_str(&format!(
            "edit={},{},{},{}\n",
            point.reference_count, point.position, point.byte_offset, point.replacement_length
        ));
    } else {
        body.push_str("edit=-\n");
    }
    match (
        expected.expected_reference_count,
        expected.expected_fingerprint.as_deref(),
        expected.expected_sequence.as_deref(),
    ) {
        (Some(references), Some(fingerprint), Some(sequence)) => {
            body.push_str(&format!("file={references},{fingerprint},{sequence}\n"))
        }
        (None, None, None) => body.push_str("file=-\n"),
        _ => return Err("incomplete prepared file observations".into()),
    }
    if let Some((root, transition, closure)) = expected.base {
        body.push_str(&format!(
            "base={root},{transition},{}\n",
            hex_bytes(&closure)
        ));
    } else {
        body.push_str("base=-\n");
    }
    if let Some(oracle) = &expected.edit_oracle {
        body.push_str(&format!(
            "oracle={},{},{},{},{},{},{},{},{}\n",
            oracle.operation,
            oracle.offset,
            hex_bytes(&oracle.removed),
            hex_bytes(&oracle.inserted),
            oracle.before_file,
            oracle.after_file,
            oracle.result_root,
            oracle.result_transition,
            hex_bytes(&oracle.result_closure),
        ));
    } else {
        body.push_str("oracle=-\n");
    }
    if expected.expected_ranges.len() != expected.expected_probes.len() {
        return Err(CoreError::LengthMismatch {
            expected: u64::try_from(expected.expected_probes.len())
                .map_err(|_| CoreError::LengthOverflow)?,
            actual: u64::try_from(expected.expected_ranges.len())
                .map_err(|_| CoreError::LengthOverflow)?,
        }
        .into());
    }
    for ((label, range), bytes) in expected
        .expected_probes
        .iter()
        .zip(&expected.expected_ranges)
    {
        body.push_str(&format!(
            "range={label},{},{},{}\n",
            range.start,
            range.end,
            hex_bytes(bytes)
        ));
    }
    let contents = format!(
        "{body}manifest_blake3={}\n",
        blake3::hash(body.as_bytes()).to_hex()
    );
    let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
    file.write_all(contents.as_bytes())?;
    file.sync_all()?;
    Ok(())
}

fn read_prepared_expectations(
    path: &Path,
    metrics: &mut Metrics,
) -> AnyResult<ChargedPreparedExpectations> {
    let declared_length = fs::metadata(path)?.len();
    if declared_length > MAX_PREPARED_EXPECTATION_BYTES {
        return Err(CoreError::ObjectLimitExceeded.into());
    }
    let declared_length =
        usize::try_from(declared_length).map_err(|_| CoreError::LengthOverflow)?;
    let mut contents = ChargedVec::with_capacity(declared_length, metrics)?;
    contents.resize(declared_length, 0);
    let mut file = File::open(path)?;
    file.read_exact(&mut contents)?;
    let mut trailing = [0_u8; 1];
    if file.read(&mut trailing)? != 0 {
        return Err(CoreError::LengthMismatch {
            expected: u64::try_from(declared_length).map_err(|_| CoreError::LengthOverflow)?,
            actual: u64::try_from(declared_length + 1).map_err(|_| CoreError::LengthOverflow)?,
        }
        .into());
    }
    let contents = std::str::from_utf8(&contents)?;
    let checksum_offset = contents
        .rfind("manifest_blake3=")
        .ok_or("prepared expectation checksum missing")?;
    let body = &contents[..checksum_offset];
    let checksum = contents[checksum_offset..]
        .strip_prefix("manifest_blake3=")
        .and_then(|value| value.strip_suffix('\n'))
        .ok_or("malformed prepared expectation checksum")?;
    if checksum != blake3::hash(body.as_bytes()).to_hex().as_str() {
        return Err("prepared expectation checksum mismatch".into());
    }
    let (result_capacity, range_count) = prepared_expectations_preflight_capacity(body)?;
    let result_charge = charge_capacity(metrics, result_capacity)?;
    let mut lines = body.lines();
    if lines.next() != Some("LFS-WP4M-EXPECTATIONS-2") {
        return Err("prepared expectation version mismatch".into());
    }
    let source_length = expected_value(lines.next(), "source_length=")?.parse::<u64>()?;
    let source_fingerprint = exact_owned(expected_value(lines.next(), "source_fingerprint=")?)?;
    source_fingerprint.parse::<ObjectId>()?;
    let edit = expected_value(lines.next(), "edit=")?;
    let edit_point = if edit == "-" {
        None
    } else {
        let mut values = edit.split(',');
        let point = EditPoint {
            reference_count: values
                .next()
                .ok_or("missing edit reference count")?
                .parse()?,
            position: values.next().ok_or("missing edit position")?.parse()?,
            byte_offset: values.next().ok_or("missing edit byte offset")?.parse()?,
            replacement_length: values
                .next()
                .ok_or("missing edit replacement length")?
                .parse()?,
        };
        if values.next().is_some() || point.position >= point.reference_count {
            return Err("malformed prepared edit point".into());
        }
        Some(point)
    };
    let file = expected_value(lines.next(), "file=")?;
    let (expected_reference_count, expected_fingerprint, expected_sequence) = if file == "-" {
        (None, None, None)
    } else {
        let mut values = file.split(',');
        let references = values
            .next()
            .ok_or("missing expected references")?
            .parse()?;
        let fingerprint = exact_owned(values.next().ok_or("missing expected fingerprint")?)?;
        let sequence = exact_owned(values.next().ok_or("missing expected sequence")?)?;
        if values.next().is_some() {
            return Err("malformed prepared file observations".into());
        }
        fingerprint.parse::<ObjectId>()?;
        sequence.parse::<ObjectId>()?;
        (Some(references), Some(fingerprint), Some(sequence))
    };
    let base = expected_value(lines.next(), "base=")?;
    let base = if base == "-" {
        None
    } else {
        let mut values = base.split(',');
        let root = values.next().ok_or("missing prepared root")?.parse()?;
        let transition = values
            .next()
            .ok_or("missing prepared transition")?
            .parse()?;
        let closure = values
            .next()
            .ok_or("missing prepared closure")?
            .parse::<ObjectId>()?
            .to_bytes();
        if values.next().is_some() {
            return Err("malformed prepared base".into());
        }
        Some((root, transition, closure))
    };
    let oracle = expected_value(lines.next(), "oracle=")?;
    let edit_oracle = if oracle == "-" {
        None
    } else {
        let mut values = oracle.split(',');
        let oracle = PreparedEditOracle {
            operation: exact_owned(values.next().ok_or("missing oracle operation")?)?,
            offset: values.next().ok_or("missing oracle offset")?.parse()?,
            removed: decode_hex(values.next().ok_or("missing oracle removed bytes")?)?,
            inserted: decode_hex(values.next().ok_or("missing oracle inserted bytes")?)?,
            before_file: values.next().ok_or("missing oracle before file")?.parse()?,
            after_file: values.next().ok_or("missing oracle after file")?.parse()?,
            result_root: values.next().ok_or("missing oracle result root")?.parse()?,
            result_transition: values
                .next()
                .ok_or("missing oracle result transition")?
                .parse()?,
            result_closure: values
                .next()
                .ok_or("missing oracle result closure")?
                .parse::<ObjectId>()?
                .to_bytes(),
        };
        if values.next().is_some()
            || oracle.operation != "same-middle"
            || oracle.removed.len() != oracle.inserted.len()
            || oracle.removed.len() > MAX_EDIT_ORACLE_BYTES
        {
            return Err("malformed prepared edit oracle".into());
        }
        Some(oracle)
    };
    let mut expected_ranges = Vec::with_capacity(range_count);
    let mut expected_probes = Vec::with_capacity(range_count);
    for line in lines {
        if expected_ranges.len() == MAX_PREPARED_RANGE_PROBES {
            return Err(CoreError::ObjectLimitExceeded.into());
        }
        let value = line
            .strip_prefix("range=")
            .ok_or("malformed prepared range")?;
        let mut values = value.splitn(4, ',');
        let label = probe_label(values.next().ok_or("missing range label")?)?;
        let start = values.next().ok_or("missing range start")?.parse::<u64>()?;
        let end = values.next().ok_or("missing range end")?.parse::<u64>()?;
        if start > end {
            return Err("malformed prepared range bounds".into());
        }
        expected_probes.push((label, start..end));
        expected_ranges.push(decode_hex(values.next().ok_or("missing range bytes")?)?);
    }
    let value = PreparedExpectations {
        source_length,
        source_fingerprint,
        edit_point,
        expected_reference_count,
        expected_fingerprint,
        expected_sequence,
        expected_ranges,
        expected_probes,
        base,
        edit_oracle,
    };
    if prepared_expectations_capacity(&value)? != result_capacity {
        return Err(CoreError::LengthMismatch {
            expected: u64::try_from(result_capacity).map_err(|_| CoreError::LengthOverflow)?,
            actual: u64::try_from(prepared_expectations_capacity(&value)?)
                .map_err(|_| CoreError::LengthOverflow)?,
        }
        .into());
    }
    Ok(ChargedPreparedExpectations {
        value,
        _charge: result_charge,
    })
}

fn remove_sqlite_image(path: &Path) -> AnyResult<()> {
    if path.exists() {
        fs::remove_file(path)?;
    }
    let mut journal = path.as_os_str().to_os_string();
    journal.push("-journal");
    let journal = PathBuf::from(journal);
    if journal.exists() {
        fs::remove_file(journal)?;
    }
    let authority = authority_path(path);
    if authority.exists() {
        fs::remove_file(authority)?;
    }
    let expectations = expectations_path(path);
    if expectations.exists() {
        fs::remove_file(expectations)?;
    }
    Ok(())
}

fn row_database_path(
    root: &Path,
    candidate: Candidate,
    size: u64,
    operation: &str,
    iteration: usize,
) -> PathBuf {
    root.join(format!(
        "db-{}-{size}-{operation}-{iteration}.sqlite",
        candidate.name
    ))
}

fn prepare_row_database(
    root: &Path,
    candidate: Candidate,
    size: u64,
    operation: &str,
    iteration: usize,
) -> AnyResult<()> {
    let db_path = row_database_path(root, candidate, size, operation, iteration);
    remove_sqlite_image(&db_path)?;
    let source = source_path(root, size);
    let (source_length, source_fingerprint) = source_hash(&source)?;
    if source_length != size {
        return Err(CoreError::LengthMismatch {
            expected: size,
            actual: source_length,
        }
        .into());
    }
    let edit_point = matches!(operation, "same-middle" | "plus1-early" | "plus1-middle")
        .then(|| prepared_edit_point(&source, operation))
        .transpose()?;
    let observations = (!operation.starts_with("dir-"))
        .then(|| expected_file_observations(&source, operation, source_length, candidate))
        .transpose()?;
    let mut expected = match observations {
        Some((references, fingerprint, sequence, ranges, probes)) => PreparedExpectations {
            source_length,
            source_fingerprint,
            edit_point,
            expected_reference_count: Some(references),
            expected_fingerprint: Some(fingerprint),
            expected_sequence: Some(sequence),
            expected_ranges: ranges,
            expected_probes: probes,
            base: None,
            edit_oracle: None,
        },
        None => PreparedExpectations {
            source_length,
            source_fingerprint,
            edit_point,
            expected_reference_count: None,
            expected_fingerprint: None,
            expected_sequence: None,
            expected_ranges: Vec::new(),
            expected_probes: Vec::new(),
            base: None,
            edit_oracle: None,
        },
    };
    let mut store = Store::open(&db_path, candidate)?;
    let needs_file_base = matches!(operation, "same-middle" | "plus1-early" | "plus1-middle");
    let needs_directory_base = matches!(operation, "dir-lookup" | "dir-replace" | "dir-leading");
    if operation == "full" {
        drop(store);
        let oracle_path = root.join(format!(
            "f2-oracle-{}-{size}-{iteration}.sqlite",
            candidate.name
        ));
        if oracle_path.exists()
            || authority_path(&oracle_path).exists()
            || expectations_path(&oracle_path).exists()
        {
            return Err("F2 full-create oracle path already exists".into());
        }
        let mut oracle_store = Store::open(&oracle_path, candidate)?;
        let mut oracle_metrics = Metrics::default();
        let (root_id, transition_id) =
            build_file(&mut oracle_store, &source, candidate, &mut oracle_metrics)?;
        let transition_digest = verify_transition(
            &oracle_store,
            transition_id,
            None,
            root_id,
            None,
            &mut oracle_metrics,
        )?;
        let content_digest = verify_file(
            &oracle_store,
            root_id,
            candidate,
            expected.expected_fingerprint.as_deref(),
            expected.expected_sequence.as_deref(),
            &mut oracle_metrics,
        )?
        .0;
        expected.base = Some((
            root_id,
            transition_id,
            combined_closure_digest(transition_digest, content_digest),
        ));
        oracle_store.publish(None, root_id, transition_id, &mut oracle_metrics)?;
        drop(oracle_store);
        write_prepared_expectations(&expectations_path(&db_path), &expected)?;
        return Ok(());
    }
    if !needs_file_base && !needs_directory_base {
        drop(store);
        write_prepared_expectations(&expectations_path(&db_path), &expected)?;
        return Ok(());
    }

    let mut metrics = Metrics::default();
    let (root_id, transition_id) = if needs_file_base {
        build_file(
            &mut store,
            &source_path(root, size),
            candidate,
            &mut metrics,
        )?
    } else {
        build_directory(&mut store, candidate, false, false, &mut metrics)?
    };
    let transition_digest =
        verify_transition(&store, transition_id, None, root_id, None, &mut metrics)?;
    let content_digest = if needs_file_base {
        verify_file(&store, root_id, candidate, None, None, &mut metrics)?.0
    } else {
        verify_directory(
            &store,
            root_id,
            candidate,
            u64::try_from(DIRECTORY_ENTRIES).map_err(|_| CoreError::LengthOverflow)?,
            None,
            &mut metrics,
        )?
    };
    let expected_digest = combined_closure_digest(transition_digest, content_digest);
    store.publish(None, root_id, transition_id, &mut metrics)?;
    drop(store);

    let store = Store::open(&db_path, candidate)?;
    let head = store
        .current_head()?
        .ok_or(CoreError::InvalidValidationReceipt)?;
    if head.1 != root_id || head.2 != transition_id {
        return Err(CoreError::PublicationConflict.into());
    }
    let mut reopened_metrics = Metrics::default();
    let transition_digest = verify_transition(
        &store,
        transition_id,
        None,
        root_id,
        None,
        &mut reopened_metrics,
    )?;
    let content_digest = if needs_file_base {
        verify_file(
            &store,
            root_id,
            candidate,
            None,
            None,
            &mut reopened_metrics,
        )?
        .0
    } else {
        verify_directory(
            &store,
            root_id,
            candidate,
            u64::try_from(DIRECTORY_ENTRIES).map_err(|_| CoreError::LengthOverflow)?,
            None,
            &mut reopened_metrics,
        )?
    };
    if combined_closure_digest(transition_digest, content_digest) != expected_digest {
        return Err(CoreError::PublicationConflict.into());
    }
    expected.base = Some((root_id, transition_id, expected_digest));
    drop(store);
    if operation == "same-middle" {
        let oracle_path = oracle_database_path(&db_path);
        remove_sqlite_image(&oracle_path)?;
        let mut oracle_store = Store::open(&oracle_path, candidate)?;
        let mut oracle_metrics = Metrics::default();
        let (oracle_base_root, oracle_base_transition) =
            build_file(&mut oracle_store, &source, candidate, &mut oracle_metrics)?;
        if oracle_base_root != root_id || oracle_base_transition != transition_id {
            return Err(CoreError::PublicationConflict.into());
        }
        oracle_store.publish(
            None,
            oracle_base_root,
            oracle_base_transition,
            &mut oracle_metrics,
        )?;
        expected.edit_oracle = Some(prepare_same_middle_oracle(
            &mut oracle_store,
            &source,
            candidate,
            expected.edit_point.ok_or(CoreError::MissingObject)?,
            expected
                .expected_fingerprint
                .as_deref()
                .ok_or("missing edited fingerprint")?,
            expected
                .expected_sequence
                .as_deref()
                .ok_or("missing edited CDC sequence")?,
        )?);
        drop(oracle_store);
        remove_sqlite_image(&oracle_path)?;
    }
    require_amended_m45_expectations(candidate, size, operation, &expected)?;
    write_prepared_expectations(&expectations_path(&db_path), &expected)?;
    Ok(())
}

struct JsonOptional<T>(Option<T>);

impl<T: std::fmt::Display> std::fmt::Display for JsonOptional<T> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.0 {
            Some(value) => value.fmt(formatter),
            None => formatter.write_str("\"Unavailable\""),
        }
    }
}

struct JsonOptionalString<'a>(Option<&'a str>);

impl std::fmt::Display for JsonOptionalString<'_> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.0 {
            Some(value) => write!(formatter, "\"{value}\""),
            None => formatter.write_str("\"Unavailable\""),
        }
    }
}

struct HexBytes<'a>(&'a [u8]);

impl std::fmt::Display for HexBytes<'_> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for byte in self.0 {
            write!(formatter, "{byte:02x}")?;
        }
        Ok(())
    }
}

struct ErrorJson<'a>(Option<&'a str>);

impl std::fmt::Display for ErrorJson<'_> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let Some(value) = self.0 else {
            return formatter.write_str("null");
        };
        formatter.write_str("\"")?;
        for character in value.chars() {
            if character == '"' {
                formatter.write_str("'")?;
            } else {
                formatter.write_char(character)?;
            }
        }
        formatter.write_str("\"")
    }
}

struct MibPerSecond {
    bytes: u64,
    wall_ns: u128,
    available: bool,
}

impl std::fmt::Display for MibPerSecond {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if !self.available || self.wall_ns == 0 {
            return formatter.write_str("Unavailable");
        }
        write!(
            formatter,
            "{:.6}",
            (self.bytes as f64 / (1024.0 * 1024.0)) / (self.wall_ns as f64 / 1_000_000_000.0)
        )
    }
}

struct ProvenanceDisplay(Option<FailureProvenance>);

impl std::fmt::Display for ProvenanceDisplay {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let Some(value) = self.0 else {
            return formatter.write_str(
                "first=None;cleanup_first=None;reconciliation=RequestedVisible;reconciliation_error=None;dominant=None",
            );
        };
        write!(
            formatter,
            "first={:?};cleanup_first={:?};reconciliation={:?};reconciliation_error={:?};dominant={:?}",
            value.first,
            value.cleanup_first,
            value.reconciliation,
            value.reconciliation_error,
            value.dominant,
        )
    }
}

fn command_text(program: &str, arguments: &[&str]) -> AnyResult<String> {
    let output = std::process::Command::new(program)
        .args(arguments)
        .output()?;
    if !output.status.success() {
        return Err(format!("{program} {:?} failed", arguments).into());
    }
    Ok(String::from_utf8(output.stdout)?.trim().to_string())
}

fn executable_sha256(path: &Path) -> AnyResult<String> {
    let output = std::process::Command::new("shasum")
        .args(["-a", "256"])
        .arg(path)
        .output()?;
    if !output.status.success() {
        return Err("shasum -a 256 failed".into());
    }
    let digest = String::from_utf8(output.stdout)?
        .split_whitespace()
        .next()
        .ok_or("missing executable SHA-256")?
        .to_string();
    if digest.len() != 64 || !digest.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("invalid executable SHA-256".into());
    }
    Ok(digest)
}

fn json_escape(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t")
}

fn write_environment_record(root: &Path, executable: &Path) -> AnyResult<String> {
    let executable_sha256 = executable_sha256(executable)?;
    let rustc = command_text("rustc", &["-Vv"])?;
    let target = rustc
        .lines()
        .find_map(|line| line.strip_prefix("host: "))
        .ok_or("rustc host triple unavailable")?;
    let cargo = command_text("cargo", &["-V"])?;
    let commit = command_text("git", &["rev-parse", "HEAD"])?;
    let status = command_text("git", &["status", "--short"])?;
    let uname = command_text("uname", &["-a"])?;
    let cpu = command_text("sysctl", &["-n", "machdep.cpu.brand_string"])
        .unwrap_or_else(|_| "Unavailable".to_string());
    let memory =
        command_text("sysctl", &["-n", "hw.memsize"]).unwrap_or_else(|_| "Unavailable".to_string());
    let cores =
        command_text("sysctl", &["-n", "hw.ncpu"]).unwrap_or_else(|_| "Unavailable".to_string());
    let sqlite = Connection::open_in_memory()?
        .query_row("SELECT sqlite_version()", [], |row| row.get::<_, String>(0))?;
    let rustflags = env::var("RUSTFLAGS").unwrap_or_default();
    let encoded_rustflags = env::var("CARGO_ENCODED_RUSTFLAGS").unwrap_or_default();
    let mut record = File::create(root.join("wp4m-profile-selection-environment.json"))?;
    writeln!(
        record,
        "{{\"format\":1,\"build_command\":\"cargo build --release -p layerfs-engine --bin phase4_create_edit_benchmark\",\"build_profile\":\"release\",\"debug_assertions\":false,\"executable\":\"{}\",\"executable_sha256\":\"{executable_sha256}\",\"git_commit\":\"{commit}\",\"git_status\":\"{}\",\"rustc_vv\":\"{}\",\"cargo_version\":\"{}\",\"target_triple\":\"{target}\",\"sqlite_version\":\"{sqlite}\",\"rustflags\":\"{}\",\"cargo_encoded_rustflags\":\"{}\",\"os_uname\":\"{}\",\"cpu\":\"{}\",\"memory_bytes\":\"{}\",\"logical_cpu_count\":\"{}\"}}",
        json_escape(&executable.display().to_string()),
        json_escape(&status),
        json_escape(&rustc),
        json_escape(&cargo),
        json_escape(&rustflags),
        json_escape(&encoded_rustflags),
        json_escape(&uname),
        json_escape(&cpu),
        json_escape(&memory),
        json_escape(&cores),
    )?;
    record.sync_all()?;
    Ok(executable_sha256)
}

fn write_range_measurements(
    writer: &mut impl std::fmt::Write,
    measurements: &[RangeMeasurement],
) -> CoreResult<()> {
    for (index, measurement) in measurements.iter().enumerate() {
        if index != 0 {
            writer.write_char(',').map_err(|_| CoreError::Io)?;
        }
        write!(
            writer,
            "{{\"label\":\"{}\",\"start\":{},\"end\":{},\"wall_ns\":{},\"returned_bytes\":{},\"canonical_bytes_authenticated\":{},\"objects_authenticated\":{},\"throughput_mib_s\":{}}}",
            measurement.label,
            measurement.range.start,
            measurement.range.end,
            measurement.wall_ns,
            measurement.returned_bytes,
            measurement.canonical_bytes_authenticated,
            measurement.objects_authenticated,
            MibPerSecond {
                bytes: u64::try_from(measurement.returned_bytes)
                    .map_err(|_| CoreError::LengthOverflow)?,
                wall_ns: measurement.wall_ns,
                available: true,
            },
        )
        .map_err(|_| CoreError::Io)?;
    }
    Ok(())
}

fn range_measurements_json(
    measurements: &[RangeMeasurement],
    metrics: &mut Metrics,
) -> CoreResult<ChargedString> {
    let mut counter = CountingWriter(0);
    write_range_measurements(&mut counter, measurements)?;
    let mut output = ChargedString::with_capacity(counter.0, metrics)?;
    write_range_measurements(&mut output.value, measurements)?;
    Ok(output)
}

fn observed_delta(after: Option<u64>, before: Option<u64>) -> CoreResult<Option<u64>> {
    match (after, before) {
        (Some(after), Some(before)) => after
            .checked_sub(before)
            .map(Some)
            .ok_or(CoreError::LengthOverflow),
        _ => Ok(None),
    }
}

fn observed_product(left: Option<u64>, right: Option<u64>) -> CoreResult<Option<u64>> {
    match (left, right) {
        (Some(left), Some(right)) => left
            .checked_mul(right)
            .map(Some)
            .ok_or(CoreError::LengthOverflow),
        _ => Ok(None),
    }
}

fn observed_max(values: &[Option<u64>]) -> Option<u64> {
    values.iter().copied().flatten().max()
}

#[allow(clippy::too_many_arguments)]
fn row_json(
    candidate: Candidate,
    size: u64,
    operation: &str,
    iteration: usize,
    warmup: bool,
    source_fingerprint: &str,
    capture_ns: u128,
    verification_ns: u128,
    root: ObjectId,
    transition: ObjectId,
    expected_references: Option<u64>,
    expected_sequence: Option<&str>,
    actual_references: u64,
    closure_digest: [u8; 32],
    metrics: Metrics,
    physical_before: PhysicalSnapshot,
    physical_after: PhysicalSnapshot,
    phases: &PhaseTimes,
    phase_metrics: &[PhaseMetricInterval],
    ranges_json: &str,
    executable_sha256: &str,
    publication: Option<PublicationOutcome>,
    error: Option<&str>,
) -> AnyResult<ChargedString> {
    let _ = (F1_STATUS_CODES, F1_Q_EQUATION);
    let mut metrics = metrics;
    let pre_report_current = q_current();
    validate_metric_equations(metrics)?;
    let profile_bytes = profile_id(candidate)?;
    let profile = HexBytes(&profile_bytes);
    let status = if error.is_some() { "FAIL" } else { "PASS" };
    let error_json = ErrorJson(error);
    let allocated_store_delta = physical_before
        .allocated_store()
        .zip(physical_after.allocated_store())
        .map(|(before, after)| i128::from(after) - i128::from(before));
    let expected_references_json = JsonOptional(expected_references);
    let expected_sequence_json = JsonOptionalString(expected_sequence);
    let closure_digest = HexBytes(&closure_digest);
    let durable_sum = phases
        .canonical_cas_mapping_stage_ns
        .checked_add(phases.precommit_closure_validation_ns)
        .and_then(|value| value.checked_add(phases.sqlite_commit_durability_ns));
    let lifecycle_sum = durable_sum
        .and_then(|value| value.checked_add(phases.fresh_reopen_head_ns))
        .and_then(|value| value.checked_add(phases.fresh_full_scrub_ns))
        .and_then(|value| value.checked_add(phases.reconstruction_ns))
        .and_then(|value| value.checked_add(phases.range_verification_ns));
    let commit_pre_and_post_dispatch_wall_ns = metrics
        .commit_publish_call_wall_ns
        .checked_sub(metrics.commit_dispatch_to_return_wall_ns)
        .ok_or(CoreError::LengthOverflow)?;
    let commit_caller_wrapper_wall_ns = phases
        .sqlite_commit_durability_ns
        .checked_sub(metrics.commit_publish_call_wall_ns)
        .ok_or(CoreError::LengthOverflow)?;
    let commit_observation_sum_ns = metrics
        .commit_dispatch_to_return_wall_ns
        .checked_add(commit_pre_and_post_dispatch_wall_ns)
        .and_then(|value| value.checked_add(commit_caller_wrapper_wall_ns))
        .ok_or(CoreError::LengthOverflow)?;
    let commit_return_status = if metrics.commit_returns == 0 {
        "NotApplicable"
    } else if metrics.commit_return_successes == metrics.commit_returns {
        "ok"
    } else if metrics.commit_return_errors == metrics.commit_returns {
        "error"
    } else {
        "mixed"
    };
    let cache_hits = observed_delta(
        metrics.sqlite_status_after_return.cache_hits,
        metrics.sqlite_status_before.cache_hits,
    )?;
    let cache_misses = observed_delta(
        metrics.sqlite_status_after_return.cache_misses,
        metrics.sqlite_status_before.cache_misses,
    )?;
    let dirty_pages_written = observed_delta(
        metrics.sqlite_status_after_return.dirty_pages_written,
        metrics.sqlite_status_before.dirty_pages_written,
    )?;
    let cache_spill_pages = observed_delta(
        metrics.sqlite_status_after_return.cache_spill_pages,
        metrics.sqlite_status_before.cache_spill_pages,
    )?;
    let pager_write_bytes = observed_product(dirty_pages_written, metrics.sqlite_page_size)?;
    let page_cache_snapshot_max = observed_max(&[
        metrics.sqlite_status_before.page_cache_used_bytes,
        metrics.sqlite_status_before_dispatch.page_cache_used_bytes,
        metrics.sqlite_status_after_return.page_cache_used_bytes,
    ]);
    let journal_sampled_allocation_max = observed_max(&[
        physical_before.allocated_journal,
        metrics.commit_dispatch_filesystem.allocated_journal,
        metrics.commit_return_filesystem.allocated_journal,
        physical_after.allocated_journal,
    ]);
    let sqlite_status_classification = if metrics.measurement_status_reset_errors == 0
        && metrics.sqlite_status_before.page_cache_used_bytes.is_some()
        && metrics
            .sqlite_status_before_dispatch
            .page_cache_used_bytes
            .is_some()
        && metrics
            .sqlite_status_after_return
            .page_cache_used_bytes
            .is_some()
        && cache_hits.is_some()
        && cache_misses.is_some()
        && dirty_pages_written.is_some()
        && cache_spill_pages.is_some()
    {
        STATUS_OBSERVED
    } else {
        STATUS_UNAVAILABLE_STATUS_API
    };
    let measurement_sql_queries = physical_before
        .measurement_sql_queries
        .checked_add(metrics.measurement_sql_queries)
        .and_then(|value| value.checked_add(physical_after.measurement_sql_queries))
        .ok_or(CoreError::LengthOverflow)?;
    let measurement_sql_rows = physical_before
        .measurement_sql_rows
        .checked_add(metrics.measurement_sql_rows)
        .and_then(|value| value.checked_add(physical_after.measurement_sql_rows))
        .ok_or(CoreError::LengthOverflow)?;
    let measurement_status_read_calls = metrics
        .sqlite_status_before
        .read_calls
        .checked_add(metrics.sqlite_status_before_dispatch.read_calls)
        .and_then(|value| value.checked_add(metrics.sqlite_status_after_return.read_calls))
        .ok_or(CoreError::LengthOverflow)?;
    let measurement_status_errors = metrics
        .measurement_status_reset_errors
        .checked_add(metrics.sqlite_status_before.errors)
        .and_then(|value| value.checked_add(metrics.sqlite_status_before_dispatch.errors))
        .and_then(|value| value.checked_add(metrics.sqlite_status_after_return.errors))
        .ok_or(CoreError::LengthOverflow)?;
    let measurement_status_calls = metrics
        .measurement_status_reset_calls
        .checked_add(measurement_status_read_calls)
        .ok_or(CoreError::LengthOverflow)?;
    let phase_metrics_json = phase_metrics_json(phase_metrics, &mut metrics)?;
    let sql_calls = metrics
        .sql_query_calls
        .checked_add(metrics.sql_execute_calls)
        .ok_or(CoreError::LengthOverflow)?;
    let canonical_authenticated_nonnew_bytes = metrics
        .canonical_bytes_authenticated
        .checked_sub(metrics.canonical_bytes_written)
        .ok_or(CoreError::LengthOverflow)?;
    let canonical_authentication_hash_bytes = metrics
        .canonical_bytes_authenticated
        .checked_sub(metrics.reused_object_id_authentication_bytes)
        .ok_or(CoreError::LengthOverflow)?;
    let canonical_authentication_hashes = metrics
        .objects_authenticated
        .checked_sub(metrics.reused_object_id_authentications)
        .ok_or(CoreError::LengthOverflow)?;
    let identity_bytes_hashed = metrics
        .raw_bytes_hashed
        .checked_add(metrics.canonical_id_bytes_hashed)
        .and_then(|value| value.checked_add(canonical_authentication_hash_bytes))
        .ok_or(CoreError::LengthOverflow)?;
    let is_full_file = operation == "full";
    let capture_mib_s = MibPerSecond {
        bytes: size,
        wall_ns: phases.durable_capture_total_ns,
        available: is_full_file,
    };
    let complete_mib_s = MibPerSecond {
        bytes: size,
        wall_ns: phases.complete_lifecycle_total_ns,
        available: is_full_file,
    };
    let reconstruction_mib_s = MibPerSecond {
        bytes: size,
        wall_ns: phases.reconstruction_ns,
        available: true,
    };
    let source_cdc_nested = !operation.starts_with("dir-");
    let base_copy_method = match env::var("WP4M_BASE_COPY_METHOD").as_deref() {
        Ok("physical-byte-copy-identical-database-authority") => {
            "physical-byte-copy-identical-database-authority"
        }
        _ => "regenerated-isolated-database",
    };
    let base_database_sha256 =
        env::var("WP4M_BASE_DATABASE_SHA256").unwrap_or_else(|_| "Unavailable".to_string());
    let base_authority_sha256 =
        env::var("WP4M_BASE_AUTHORITY_SHA256").unwrap_or_else(|_| "Unavailable".to_string());
    let base_expectations_sha256 =
        env::var("WP4M_BASE_EXPECTATIONS_SHA256").unwrap_or_else(|_| "Unavailable".to_string());
    let precommit_reconstructs =
        operation != "dir-lookup" && operation != "same-middle" && operation != "full";
    let qualification_mode_label = if operation == "full" {
        "C1-construction-proof"
    } else if operation == "same-middle" {
        match qualification_mode()? {
            QualificationMode::FullClosure => "C0-full-closure",
            QualificationMode::ChangedSpine => "C1-changed-spine",
        }
    } else {
        "not-applicable"
    };
    let (purpose, milestone) = if operation == "full" {
        ("wp4m_f2_construction_proof", "F2")
    } else {
        ("wp4m_f1_commit_io_observability", "F1")
    };
    let (publication_status, publication_diagnostic) = match publication {
        Some(PublicationOutcome { status, diagnostic }) => {
            let status = match status {
                PublicationStatus::Committed => "Committed",
                PublicationStatus::RequestedVisible => "RequestedVisible",
            };
            (status, diagnostic)
        }
        None => ("Unavailable", None),
    };
    let receipt_provenance = ProvenanceDisplay(publication_diagnostic);
    macro_rules! render_row {
        ($writer:expr, $reported_q_high_water:expr, $report_output_bytes:expr) => {{
            let mut compact_writer = CompactStatusWriter($writer);
            let result = write!(
        &mut compact_writer,
        "{{\"qualification\":false,\"promotion\":false,\"rejection\":false,\"purpose\":\"{purpose}\",\"milestone\":\"{milestone}\",\"throughput_measurement_admissible\":false,\"status\":\"{status}\",\"candidate\":\"{}\",\"profile_id\":\"{profile}\",\"size_bytes\":{size},\"input_size_bytes\":{size},\"operation\":\"{operation}\",\"qualification_mode\":\"{qualification_mode_label}\",\"iteration\":{iteration},\"warmup\":{warmup},\"fixture\":\"{}\",\"fixture_manifest\":\"wp4m-retained-fixture-manifest.json\",\"source_fingerprint\":\"{source_fingerprint}\",\"expected_cdc_references\":{expected_references_json},\"expected_cdc_sequence_fingerprint\":{expected_sequence_json},\"actual_cdc_references\":{actual_references},\"ordered_closure_digest\":\"{closure_digest}\",\"root_id\":\"{}\",\"transition_id\":\"{}\",\"executable_sha256\":\"{executable_sha256}\",\"build_profile\":\"release\",\"debug_assertions\":false,\"base_preparation_in_measured_interval\":false,\"base_copy_method\":\"{base_copy_method}\",\"pre_edit_database_sha256\":\"{base_database_sha256}\",\"pre_edit_authority_sha256\":\"{base_authority_sha256}\",\"pre_edit_expectations_sha256\":\"{base_expectations_sha256}\",\"source_cache_state\":\"warm_or_unknown_after_manifest_preflight\",\"store_state\":\"fresh_logical_store_cache_unknown\",\"capture_publish_wall_ns\":{capture_ns},\"sqlite_qualification_wall_ns\":{verification_ns},\"elapsed_wall_ns\":{verification_ns},\"source_cdc_wall_ns\":\"NestedInCanonicalStage\",\"same_open_authority_establishment_wall_ns\":{authority_ns},\"canonical_cas_mapping_stage_wall_ns\":{canonical_stage_ns},\"precommit_closure_validation_wall_ns\":{precommit_ns},\"sqlite_commit_durability_wall_ns\":{commit_ns},\"commit_dispatches\":{commit_dispatches},\"commit_returns\":{commit_returns},\"commit_return_successes\":{commit_return_successes},\"commit_return_errors\":{commit_return_errors},\"commit_return_status\":\"{commit_return_status}\",\"commit_publish_call_wall_ns\":{commit_publish_call_wall_ns},\"commit_dispatch_to_return_wall_ns\":{commit_dispatch_to_return_wall_ns},\"commit_pre_and_post_dispatch_wall_ns\":{commit_pre_and_post_dispatch_wall_ns},\"commit_caller_wrapper_wall_ns\":{commit_caller_wrapper_wall_ns},\"commit_observation_sum_wall_ns\":{commit_observation_sum_ns},\"commit_timer_equation_matches\":{commit_timer_equation_matches},\"commit_reconciliation_calls\":{commit_reconciliation_calls},\"commit_reconciliation_wall_ns\":{commit_reconciliation_wall_ns},\"commit_reconciliation_timer_nested\":true,\"durable_capture_total_wall_ns\":{durable_ns},\"fresh_reopen_head_wall_ns\":{reopen_ns},\"fresh_full_scrub_wall_ns\":{scrub_ns},\"reconstruction_wall_ns\":{reconstruction_ns},\"range_verification_wall_ns\":{range_ns},\"complete_lifecycle_total_wall_ns\":{lifecycle_ns},\"durable_phase_sum_ns\":{durable_sum_ns},\"durable_phase_sum_matches\":{durable_matches},\"lifecycle_phase_sum_ns\":{lifecycle_sum_ns},\"lifecycle_phase_sum_matches\":{lifecycle_matches},\"source_cdc_nested_in_mapping_stage\":{source_cdc_nested},\"precommit_includes_reconstruction\":{precommit_reconstructs},\"phase_counters\":[{phase_metrics_json}],\"source_bytes_read\":{source_bytes_read},\"source_cdc_bytes_read\":{source_cdc_bytes_read},\"canonical_stage_source_bytes_read\":{canonical_stage_source_bytes_read},\"identity_bytes_hashed\":{identity_bytes_hashed},\"raw_bytes_hashed\":{raw_bytes_hashed},\"raw_hashes\":{raw_hashes},\"canonical_id_bytes_hashed\":{canonical_id_bytes_hashed},\"canonical_id_hashes\":{canonical_id_hashes},\"canonical_authentication_hash_bytes\":{canonical_authentication_hash_bytes},\"canonical_authentication_hashes\":{canonical_authentication_hashes},\"reused_object_id_authentications\":{reused_object_id_authentications},\"reused_object_id_authentication_bytes\":{reused_object_id_authentication_bytes},\"borrowed_bytes_encode_calls\":{borrowed_bytes_encode_calls},\"borrowed_bytes_encode_input_bytes\":{borrowed_bytes_encode_input_bytes},\"borrowed_source_encode_calls\":{borrowed_source_encode_calls},\"borrowed_source_encode_input_bytes\":{borrowed_source_encode_input_bytes},\"changed_work_bytes\":{source_bytes_read},\"capture_mib_s\":\"{capture_mib_s}\",\"complete_lifecycle_mib_s\":\"{complete_mib_s}\",\"scrub_authentication_mib_s\":\"Unavailable\",\"reconstruction_mib_s\":\"{reconstruction_mib_s}\",\"range_measurements\":[{ranges_json}],\"measurement_status\":{{\"phase_counters\":\"Observed\",\"identity_hash_bytes\":\"Observed\",\"borrowed_bytes_encoding\":\"Observed\",\"object_id_authentication_reuse\":\"Observed\",\"logical_q\":\"Observed\",\"w_d\":\"Unavailable: governing cumulative definitions are not implemented\",\"row_blob_copies\":\"Observed\",\"borrowed_row_blob_path\":\"Observed\",\"incremental_blob_api\":\"Observed\",\"cpu_rss\":\"Observed externally per child by /usr/bin/time -l\",\"other_heap_copy_bytes\":\"Unavailable\",\"sqlite_page_cache\":\"{sqlite_status_classification}\",\"sqlite_page_cache_true_high_water\":\"Unavailable: SQLITE_DBSTATUS_CACHE_USED high-water is always zero by API contract\",\"dirty_pages_current\":\"Unavailable: SQLite exposes dirty writes/spills but not current dirty-page count\",\"main_db_io_calls_bytes\":\"Unavailable: requires VFS xRead/xWrite or privileged syscall trace\",\"journal_io_calls_bytes\":\"Unavailable: requires VFS xRead/xWrite or privileged syscall trace\",\"sync_calls_wall\":\"Unavailable: VFS excluded; fs_usage/dtruss require unavailable privileges\",\"journal_true_peak\":\"Unavailable: DELETE journal can grow/disappear between snapshots\",\"temporary_file_peak\":\"Unavailable: no filename/peak API under temp_store=FILE\",\"host_physical_io_bytes\":\"Unavailable: not derived from logical/allocation/block-operation counters\",\"query_plans\":\"Unavailable\"}},\"sqlite_page_size_bytes\":{sqlite_page_size_bytes},\"sqlite_page_cache_used_bytes_before\":{sqlite_cache_used_before},\"sqlite_page_cache_used_bytes_before_dispatch\":{sqlite_cache_used_before_dispatch},\"sqlite_page_cache_used_bytes_after_return\":{sqlite_cache_used_after_return},\"sqlite_page_cache_snapshot_max_bytes\":{sqlite_page_cache_snapshot_max_bytes},\"sqlite_cache_hits\":{sqlite_cache_hits},\"sqlite_cache_misses\":{sqlite_cache_misses},\"sqlite_main_db_dirty_pages_written\":{sqlite_dirty_pages_written},\"sqlite_main_db_pager_write_bytes\":{sqlite_pager_write_bytes},\"sqlite_cache_spill_pages\":{sqlite_cache_spill_pages},\"sqlite_runtime_journal_mode\":\"delete\",\"sqlite_runtime_synchronous\":2,\"sqlite_runtime_temp_store\":1,\"sqlite_runtime_mmap_size\":0,\"sqlite_pre_logical_database_bytes\":{},\"sqlite_post_logical_database_bytes\":{},\"sqlite_pre_apparent_database_bytes\":{},\"sqlite_post_apparent_database_bytes\":{},\"sqlite_pre_allocated_database_bytes\":{},\"sqlite_post_allocated_database_bytes\":{},\"sqlite_pre_logical_store_bytes\":{},\"sqlite_post_logical_store_bytes\":{},\"sqlite_pre_apparent_store_bytes\":{},\"sqlite_post_apparent_store_bytes\":{},\"sqlite_pre_allocated_store_bytes\":{},\"sqlite_post_allocated_store_bytes\":{},\"allocated_store_delta_bytes\":{},\"commit_dispatch_db_apparent_bytes\":{commit_dispatch_db_apparent_bytes},\"commit_dispatch_journal_apparent_bytes\":{commit_dispatch_journal_apparent_bytes},\"commit_dispatch_authority_apparent_bytes\":{commit_dispatch_authority_apparent_bytes},\"commit_dispatch_db_allocated_bytes\":{commit_dispatch_db_allocated_bytes},\"commit_dispatch_journal_allocated_bytes\":{commit_dispatch_journal_allocated_bytes},\"commit_dispatch_authority_allocated_bytes\":{commit_dispatch_authority_allocated_bytes},\"commit_return_db_apparent_bytes\":{commit_return_db_apparent_bytes},\"commit_return_journal_apparent_bytes\":{commit_return_journal_apparent_bytes},\"commit_return_authority_apparent_bytes\":{commit_return_authority_apparent_bytes},\"commit_return_db_allocated_bytes\":{commit_return_db_allocated_bytes},\"commit_return_journal_allocated_bytes\":{commit_return_journal_allocated_bytes},\"commit_return_authority_allocated_bytes\":{commit_return_authority_allocated_bytes},\"journal_sampled_allocation_max_bytes\":{journal_sampled_allocation_max_bytes},\"physical_db_apparent_bytes\":{},\"physical_journal_apparent_bytes\":{},\"physical_authority_sidecar_apparent_bytes\":{},\"physical_db_allocated_bytes\":{},\"physical_journal_allocated_bytes\":{},\"physical_authority_sidecar_allocated_bytes\":{},\"physical_store_allocated_bytes\":{},\"peak_journal_bytes\":\"Unavailable\",\"peak_temporary_bytes\":\"Unavailable\",\"q_equation\":\"pre_admitted_checked_sum:canonical+decoded_nodes+file_refs+tree_nodes+dfs+cdc+sql+expectations+ranges+receipts+report\",\"q_high_water\":{logical_q_high_water},\"q_current\":{q_current},\"q_current_semantics\":\"after_report_output_drop\",\"q_report_output_bytes\":{report_output_bytes},\"q_cdc_base_live_bytes\":{q_cdc_base_live_bytes},\"q_cdc_old_window_bytes\":{q_cdc_old_window_bytes},\"q_cdc_scan_input_bytes\":{q_cdc_scan_input_bytes},\"q_cdc_overlap_current\":{q_cdc_overlap_current},\"q_fixed_envelope_removed\":true,\"leaf_batch_bound\":{},\"leaf_batch_queries\":{},\"leaf_batch_references\":{},\"leaf_batch_references_max\":{},\"leaf_batch_query_bytes_max\":{},\"w_bytes\":\"Unavailable\",\"d_bytes\":\"Unavailable\",\"canonical_new_write_bytes\":{canonical_new_write_bytes},\"canonical_authenticated_nonnew_bytes\":{canonical_authenticated_nonnew_bytes},\"canonical_rewrite_bytes\":{canonical_rewrite_bytes},\"statement_cache_acquisitions\":{statement_cache_acquisitions},\"native_sqlite_prepare_calls\":\"Unavailable\",\"sql_calls\":{},\"sql_rows_returned\":{},\"sql_query_calls\":{sql_query_calls},\"sql_execute_calls\":{sql_execute_calls},\"sql_rows_changed\":{sql_rows_changed},\"row_blob_reads\":{row_blob_reads},\"row_blob_writes\":{row_blob_writes},\"row_blob_copy_bytes\":{row_blob_copy_bytes},\"borrowed_row_blob_reads\":{borrowed_row_blob_reads},\"borrowed_row_blob_bytes\":{borrowed_row_blob_bytes},\"blob_api_status\":\"Observed\",\"blob_opens\":{},\"blob_reads\":{},\"blob_writes\":{},\"transactions\":{},\"commits\":{},\"sync_fsync_observations\":\"Unavailable\",\"query_plans\":\"Unavailable\",\"busy_events\":\"Unavailable\",\"locked_events\":\"Unavailable\",\"objects_created\":{},\"objects_reused\":{},\"objects_authenticated\":{},\"canonical_bytes_authenticated\":{},\"canonical_bytes_written\":{},\"mapping_bytes_rewritten\":{},\"covered_equal_edges\":{covered_equal_edges},\"new_or_different_edges\":{new_or_different_edges},\"fully_authenticated_new_objects\":{fully_authenticated_new_objects},\"fully_authenticated_new_bytes\":{fully_authenticated_new_bytes},\"construction_put_evidences\":{construction_put_evidences},\"construction_edges_covered\":{construction_edges_covered},\"construction_leaf_summaries\":{construction_leaf_summaries},\"construction_branch_summaries\":{construction_branch_summaries},\"construction_file_summaries\":{construction_file_summaries},\"construction_workspace_summaries\":{construction_workspace_summaries},\"construction_transition_summaries\":{construction_transition_summaries},\"construction_proof_consumptions\":{construction_proof_consumptions},\"construction_source_hash_bytes\":{construction_source_hash_bytes},\"construction_source_hashes\":{construction_source_hashes},\"construction_cdc_entries\":{construction_cdc_entries},\"closure_occurrences\":{},\"chunks\":{},\"references\":{},\"pages\":{},\"branches\":{},\"suffix_references\":{},\"suffix_bytes\":{},\"suffix_objects\":{},\"file_height\":\"Unavailable\",\"process_io\":\"Observed externally: separate user/system CPU and block-operation counts; byte-level physical I/O unavailable\",\"host_physical_io\":\"Unavailable\",\"physical_io_cache_sync_temp_journal_status\":\"Mixed: supported SQLite/filesystem snapshots observed; unsupported VFS/privileged facts unavailable with reasons\",\"publication_status\":\"{publication_status}\",\"receipt_provenance\":\"{receipt_provenance}\",\"error\":{error_json}}}",
        candidate.name,
        if size == SOURCE_100 { "S1-100" } else { "S1-512" },
        root,
        transition,
        JsonOptional(physical_before.logical_database),
        JsonOptional(physical_after.logical_database),
        JsonOptional(physical_before.apparent_database),
        JsonOptional(physical_after.apparent_database),
        JsonOptional(physical_before.allocated_database),
        JsonOptional(physical_after.allocated_database),
        JsonOptional(physical_before.logical_store()),
        JsonOptional(physical_after.logical_store()),
        JsonOptional(physical_before.apparent_store()),
        JsonOptional(physical_after.apparent_store()),
        JsonOptional(physical_before.allocated_store()),
        JsonOptional(physical_after.allocated_store()),
        JsonOptional(allocated_store_delta),
        JsonOptional(physical_after.apparent_database),
        JsonOptional(physical_after.apparent_journal),
        JsonOptional(physical_after.apparent_authority),
        JsonOptional(physical_after.allocated_database),
        JsonOptional(physical_after.allocated_journal),
        JsonOptional(physical_after.allocated_authority),
        JsonOptional(physical_after.allocated_store()),
        candidate.k,
        metrics.leaf_batch_queries,
        metrics.leaf_batch_references,
        metrics.leaf_batch_references_max,
        metrics.leaf_batch_query_bytes_max,
        sql_calls,
        metrics.sql_rows_returned,
        metrics.blob_opens,
        metrics.blob_reads,
        metrics.blob_writes,
        metrics.transactions,
        metrics.commits,
        metrics.objects_created,
        metrics.objects_reused,
        metrics.objects_authenticated,
        metrics.canonical_bytes_authenticated,
        metrics.canonical_bytes_written,
        metrics.mapping_bytes_rewritten,
        metrics.closure_occurrences,
        metrics.chunks,
        metrics.references,
        metrics.pages,
        metrics.branches,
        metrics.suffix_references,
        metrics.suffix_bytes,
        metrics.suffix_objects,
        purpose = purpose,
        milestone = milestone,
        qualification_mode_label = qualification_mode_label,
        authority_ns = phases.same_open_authority_establishment_ns,
        canonical_stage_ns = phases.canonical_cas_mapping_stage_ns,
        precommit_ns = phases.precommit_closure_validation_ns,
        commit_ns = phases.sqlite_commit_durability_ns,
        durable_ns = phases.durable_capture_total_ns,
        reopen_ns = phases.fresh_reopen_head_ns,
        scrub_ns = phases.fresh_full_scrub_ns,
        reconstruction_ns = phases.reconstruction_ns,
        range_ns = phases.range_verification_ns,
        lifecycle_ns = phases.complete_lifecycle_total_ns,
        durable_sum_ns = durable_sum.unwrap_or(0),
        durable_matches = durable_sum == Some(phases.durable_capture_total_ns),
        lifecycle_sum_ns = lifecycle_sum.unwrap_or(0),
        lifecycle_matches = lifecycle_sum == Some(phases.complete_lifecycle_total_ns),
        source_bytes_read = metrics.source_bytes_read,
        source_cdc_bytes_read = metrics.source_cdc_bytes_read,
        canonical_stage_source_bytes_read = metrics.canonical_stage_source_bytes_read,
        identity_bytes_hashed = identity_bytes_hashed,
        raw_bytes_hashed = metrics.raw_bytes_hashed,
        raw_hashes = metrics.raw_hashes,
        canonical_id_bytes_hashed = metrics.canonical_id_bytes_hashed,
        canonical_id_hashes = metrics.canonical_id_hashes,
        capture_mib_s = capture_mib_s,
        complete_mib_s = complete_mib_s,
        reconstruction_mib_s = reconstruction_mib_s,
        ranges_json = ranges_json,
        source_cdc_nested = source_cdc_nested,
        base_copy_method = base_copy_method,
        precommit_reconstructs = precommit_reconstructs,
        phase_metrics_json = phase_metrics_json,
        logical_q_high_water = $reported_q_high_water,
        q_current = 0,
        canonical_new_write_bytes = metrics.canonical_bytes_written,
        canonical_authenticated_nonnew_bytes = canonical_authenticated_nonnew_bytes,
        canonical_rewrite_bytes = metrics.mapping_bytes_rewritten,
        statement_cache_acquisitions = metrics.statement_cache_acquisitions,
        sql_query_calls = metrics.sql_query_calls,
        sql_execute_calls = metrics.sql_execute_calls,
        sql_rows_changed = metrics.sql_rows_changed,
        covered_equal_edges = metrics.incremental_receipt_covered_edges,
        new_or_different_edges = metrics.incremental_new_or_different_edges,
        fully_authenticated_new_objects = metrics
            .incremental_new_subtree_objects_authenticated,
        fully_authenticated_new_bytes = metrics.incremental_new_subtree_bytes_authenticated,
        construction_put_evidences = metrics.construction_put_evidences,
        construction_edges_covered = metrics.construction_edges_covered,
        construction_leaf_summaries = metrics.construction_leaf_summaries,
        construction_branch_summaries = metrics.construction_branch_summaries,
        construction_file_summaries = metrics.construction_file_summaries,
        construction_workspace_summaries = metrics.construction_workspace_summaries,
        construction_transition_summaries = metrics.construction_transition_summaries,
        construction_proof_consumptions = metrics.construction_proof_consumptions,
        construction_source_hash_bytes = metrics.construction_source_hash_bytes,
        construction_source_hashes = metrics.construction_source_hashes,
        construction_cdc_entries = metrics.construction_cdc_entries,
        row_blob_reads = metrics.row_blob_reads,
        row_blob_writes = metrics.row_blob_writes,
        row_blob_copy_bytes = metrics.row_blob_copy_bytes,
        borrowed_row_blob_reads = metrics.borrowed_row_blob_reads,
        borrowed_row_blob_bytes = metrics.borrowed_row_blob_bytes,
        reused_object_id_authentications = metrics.reused_object_id_authentications,
        reused_object_id_authentication_bytes = metrics.reused_object_id_authentication_bytes,
        borrowed_bytes_encode_calls = metrics.borrowed_bytes_encode_calls,
        borrowed_bytes_encode_input_bytes = metrics.borrowed_bytes_encode_input_bytes,
        borrowed_source_encode_calls = metrics.borrowed_source_encode_calls,
        borrowed_source_encode_input_bytes = metrics.borrowed_source_encode_input_bytes,
        base_database_sha256 = base_database_sha256,
        base_authority_sha256 = base_authority_sha256,
        base_expectations_sha256 = base_expectations_sha256,
        publication_status = publication_status,
        receipt_provenance = receipt_provenance,
        report_output_bytes = $report_output_bytes,
        q_cdc_base_live_bytes = metrics.q_cdc_base_live_bytes,
        q_cdc_old_window_bytes = metrics.q_cdc_old_window_bytes,
        q_cdc_scan_input_bytes = metrics.q_cdc_scan_input_bytes,
        q_cdc_overlap_current = metrics.q_cdc_overlap_current,
        commit_dispatches = metrics.commits,
        commit_returns = metrics.commit_returns,
        commit_return_successes = metrics.commit_return_successes,
        commit_return_errors = metrics.commit_return_errors,
        commit_return_status = commit_return_status,
        commit_publish_call_wall_ns = metrics.commit_publish_call_wall_ns,
        commit_dispatch_to_return_wall_ns = metrics.commit_dispatch_to_return_wall_ns,
        commit_pre_and_post_dispatch_wall_ns = commit_pre_and_post_dispatch_wall_ns,
        commit_caller_wrapper_wall_ns = commit_caller_wrapper_wall_ns,
        commit_observation_sum_ns = commit_observation_sum_ns,
        commit_timer_equation_matches =
            commit_observation_sum_ns == phases.sqlite_commit_durability_ns,
        commit_reconciliation_calls = metrics.commit_reconciliation_calls,
        commit_reconciliation_wall_ns = metrics.commit_reconciliation_wall_ns,
        sqlite_status_classification = sqlite_status_classification,
        sqlite_page_size_bytes = JsonOptional(metrics.sqlite_page_size),
        sqlite_cache_used_before =
            JsonOptional(metrics.sqlite_status_before.page_cache_used_bytes),
        sqlite_cache_used_before_dispatch = JsonOptional(
            metrics
                .sqlite_status_before_dispatch
                .page_cache_used_bytes,
        ),
        sqlite_cache_used_after_return =
            JsonOptional(metrics.sqlite_status_after_return.page_cache_used_bytes),
        sqlite_page_cache_snapshot_max_bytes = JsonOptional(page_cache_snapshot_max),
        sqlite_cache_hits = JsonOptional(cache_hits),
        sqlite_cache_misses = JsonOptional(cache_misses),
        sqlite_dirty_pages_written = JsonOptional(dirty_pages_written),
        sqlite_pager_write_bytes = JsonOptional(pager_write_bytes),
        sqlite_cache_spill_pages = JsonOptional(cache_spill_pages),
        commit_dispatch_db_apparent_bytes =
            JsonOptional(metrics.commit_dispatch_filesystem.apparent_database),
        commit_dispatch_journal_apparent_bytes =
            JsonOptional(metrics.commit_dispatch_filesystem.apparent_journal),
        commit_dispatch_authority_apparent_bytes =
            JsonOptional(metrics.commit_dispatch_filesystem.apparent_authority),
        commit_dispatch_db_allocated_bytes =
            JsonOptional(metrics.commit_dispatch_filesystem.allocated_database),
        commit_dispatch_journal_allocated_bytes =
            JsonOptional(metrics.commit_dispatch_filesystem.allocated_journal),
        commit_dispatch_authority_allocated_bytes =
            JsonOptional(metrics.commit_dispatch_filesystem.allocated_authority),
        commit_return_db_apparent_bytes =
            JsonOptional(metrics.commit_return_filesystem.apparent_database),
        commit_return_journal_apparent_bytes =
            JsonOptional(metrics.commit_return_filesystem.apparent_journal),
        commit_return_authority_apparent_bytes =
            JsonOptional(metrics.commit_return_filesystem.apparent_authority),
        commit_return_db_allocated_bytes =
            JsonOptional(metrics.commit_return_filesystem.allocated_database),
        commit_return_journal_allocated_bytes =
            JsonOptional(metrics.commit_return_filesystem.allocated_journal),
        commit_return_authority_allocated_bytes =
            JsonOptional(metrics.commit_return_filesystem.allocated_authority),
        journal_sampled_allocation_max_bytes =
            JsonOptional(journal_sampled_allocation_max),
    );
            result.and_then(|_| {
                compact_writer.0.remove_last_object_brace()?;
                write!(
                    &mut compact_writer,
                    ",\"q_cdc_old_chunk_slots_bytes\":{},\"measurement_status_schema\":\"{F1_STATUS_SCHEMA}\",\"instrumentation\":{{\"c\":\"{STATUS_OBSERVED}\",\"sql\":[{},{},{},{},{},{},{measurement_sql_queries},{measurement_sql_rows}],\"status\":[{},{},{},{},{measurement_status_calls},{measurement_status_errors}]}}}}",
                    metrics.q_cdc_old_chunk_slots_bytes,
                    physical_before.measurement_sql_queries,
                    physical_before.measurement_sql_rows,
                    metrics.measurement_sql_queries,
                    metrics.measurement_sql_rows,
                    physical_after.measurement_sql_queries,
                    physical_after.measurement_sql_rows,
                    metrics.measurement_status_reset_calls,
                    metrics.sqlite_status_before.read_calls,
                    metrics.sqlite_status_before_dispatch.read_calls,
                    metrics.sqlite_status_after_return.read_calls,
                )
            })
        }};
    }
    metrics.q_current = q_current();
    let report_scratch_current = pre_report_current
        .checked_add(
            u64::try_from(phase_metrics_json.len()).map_err(|_| CoreError::LengthOverflow)?,
        )
        .ok_or(CoreError::LengthOverflow)?;
    if metrics.q_current != report_scratch_current {
        return Err(CoreError::LengthMismatch {
            expected: report_scratch_current,
            actual: metrics.q_current,
        }
        .into());
    }
    let baseline_q_high_water = metrics.q_high_water;
    let mut reported_q_high_water = baseline_q_high_water;
    let mut reported_output_bytes = 0usize;
    let output_capacity = loop {
        let mut counter = CountingWriter(0);
        render_row!(&mut counter, reported_q_high_water, reported_output_bytes)
            .map_err(|_| CoreError::LengthOverflow)?;
        let prospective = baseline_q_high_water.max(
            report_scratch_current
                .checked_add(u64::try_from(counter.0).map_err(|_| CoreError::LengthOverflow)?)
                .ok_or(CoreError::LengthOverflow)?,
        );
        if prospective == reported_q_high_water && counter.0 == reported_output_bytes {
            break counter.0;
        }
        reported_q_high_water = prospective;
        reported_output_bytes = counter.0;
    };
    let mut output = ChargedString::with_capacity(output_capacity, &mut metrics)?;
    if metrics.q_high_water != reported_q_high_water {
        return Err(CoreError::LengthMismatch {
            expected: reported_q_high_water,
            actual: metrics.q_high_water,
        }
        .into());
    }
    render_row!(&mut output.value, reported_q_high_water, output_capacity)
        .map_err(|_| CoreError::Io)?;
    if output.value.len() != output_capacity {
        return Err(CoreError::LengthMismatch {
            expected: u64::try_from(output_capacity).map_err(|_| CoreError::LengthOverflow)?,
            actual: u64::try_from(output.value.len()).map_err(|_| CoreError::LengthOverflow)?,
        }
        .into());
    }
    Ok(output)
}

fn candidate_by_name(name: &str) -> AnyResult<Candidate> {
    FILE_CANDIDATES
        .iter()
        .chain(DIR_CANDIDATES.iter())
        .find(|candidate| candidate.name == name)
        .copied()
        .ok_or_else(|| format!("unknown candidate {name}").into())
}

fn require_optimized_benchmark() -> AnyResult<()> {
    if cfg!(debug_assertions) {
        return Err("throughput/campaign rows require an optimized --release build (debug_assertions=false)".into());
    }
    Ok(())
}

fn require_file_custody_hash(variable: &str, path: &Path) -> AnyResult<String> {
    let expected = env::var(variable).map_err(|_| format!("missing {variable}"))?;
    let actual = executable_sha256(path)?;
    if expected != actual {
        return Err(format!("{variable} mismatch: expected {expected}, actual {actual}").into());
    }
    Ok(actual)
}

struct CaptureOutcome {
    root_id: ObjectId,
    transition_id: ObjectId,
    expected_parent: Option<ObjectId>,
    expected_operations: Option<Vec<delta_codec::TransitionOperation>>,
    expected_operations_charge: Option<CapacityCharge>,
    closure_digest: [u8; 32],
    actual_references: u64,
    phases: PhaseTimes,
    capture_ns: u128,
    durable_capture_start: Instant,
    commit_end: Instant,
    authority_metrics_started: Metrics,
    authority_metrics_ended: Metrics,
    mapping_metrics_started: Metrics,
    mapping_metrics_ended: Metrics,
    precommit_metrics_started: Metrics,
    precommit_metrics_ended: Metrics,
    commit_metrics_started: Metrics,
    commit_metrics_ended: Metrics,
    publication: PublicationOutcome,
}

#[allow(clippy::too_many_arguments)]
fn capture_same_middle(
    store: &mut Store,
    candidate: Candidate,
    edit_point: EditPoint,
    replacement: &[u8],
    qualification_mode: QualificationMode,
    expected: ExpectedEditResult,
    expected_reference_count: u64,
    expected_fingerprint: &str,
    expected_sequence: &str,
    base_root: ObjectId,
    base_transition: ObjectId,
    metrics: &mut Metrics,
) -> AnyResult<CaptureOutcome> {
    let authority_metrics_started = *metrics;
    let authority_started = Instant::now();
    store.transaction_attempt(metrics, |store, metrics| {
        let mut witness = establish_same_open_file_witness(store, candidate, None, None, metrics)?;
        let permit = witness.consume(store, metrics)?;
        let authority_metrics_ended = *metrics;
        let authority_ns = authority_started.elapsed().as_nanos();

        store.start_sqlite_observations(metrics);
        let durable_capture_start = Instant::now();
        let mapping_metrics_started = *metrics;
        let _prior_head_receipt_charge = charge_capacity(metrics, 216)?;
        let prior_head = store
            .current_head_accounted(metrics)?
            .ok_or(CoreError::InvalidValidationReceipt)?;
        if prior_head.1 != base_root || prior_head.2 != base_transition {
            return Err(CoreError::PublicationConflict.into());
        }
        let (root_id, transition_id) =
            edit_file_same_middle_cdc(store, candidate, edit_point, replacement, true, metrics)?;
        let mapping_end = Instant::now();
        let mapping_metrics_ended = *metrics;

        let precommit_metrics_started = *metrics;
        let (expected_operations, expected_operations_charge) = charged_replace_operation(
            b"file",
            resolve_namespace_file_root(store, prior_head.1, metrics)?,
            resolve_namespace_file_root(store, root_id, metrics)?,
            metrics,
        )?;
        let references = match qualification_mode {
            QualificationMode::FullClosure => {
                let _current_receipt_charge = charge_capacity(metrics, 216)?;
                let current = store
                    .current_head_accounted(metrics)?
                    .ok_or(CoreError::InvalidValidationReceipt)?;
                if !permit.covers(store, &current) {
                    return Err(CoreError::ValidationAuthorityUnavailable.into());
                }
                qualify_same_middle_full_closure(
                    store,
                    prior_head.1,
                    root_id,
                    transition_id,
                    &expected_operations,
                    expected,
                    candidate,
                    expected_fingerprint,
                    expected_sequence,
                    metrics,
                )?
            }
            QualificationMode::ChangedSpine => {
                qualify_same_middle_changed_spine(
                    store,
                    permit,
                    prior_head.1,
                    root_id,
                    transition_id,
                    &expected_operations,
                    expected,
                    candidate,
                    metrics,
                )?;
                expected_reference_count
            }
        };
        let precommit_end = Instant::now();
        let precommit_metrics_ended = *metrics;

        let commit_metrics_started = *metrics;
        let publication = store.publish(Some(&prior_head), root_id, transition_id, metrics)?;
        let commit_end = Instant::now();
        let commit_metrics_ended = *metrics;
        let capture_ns = commit_end.duration_since(durable_capture_start).as_nanos();
        Ok(CaptureOutcome {
            root_id,
            transition_id,
            expected_parent: Some(base_root),
            expected_operations: Some(expected_operations),
            expected_operations_charge: Some(expected_operations_charge),
            closure_digest: expected.closure,
            actual_references: references,
            phases: PhaseTimes {
                same_open_authority_establishment_ns: authority_ns,
                canonical_cas_mapping_stage_ns: mapping_end
                    .duration_since(durable_capture_start)
                    .as_nanos(),
                precommit_closure_validation_ns: precommit_end
                    .duration_since(mapping_end)
                    .as_nanos(),
                sqlite_commit_durability_ns: commit_end.duration_since(precommit_end).as_nanos(),
                durable_capture_total_ns: capture_ns,
                ..PhaseTimes::default()
            },
            capture_ns,
            durable_capture_start,
            commit_end,
            authority_metrics_started,
            authority_metrics_ended,
            mapping_metrics_started,
            mapping_metrics_ended,
            precommit_metrics_started,
            precommit_metrics_ended,
            commit_metrics_started,
            commit_metrics_ended,
            publication,
        })
    })
}

fn run_row(
    root: &Path,
    candidate: Candidate,
    size: u64,
    operation: &str,
    iteration: usize,
    warmup: bool,
) -> AnyResult<ChargedString> {
    require_optimized_benchmark()?;
    let executable_sha256 = executable_sha256(&env::current_exe()?)?;
    if env::var("WP4M_EXECUTABLE_SHA256").is_ok_and(|expected| expected != executable_sha256) {
        return Err("running executable SHA-256 does not match campaign custody".into());
    }
    if executable_sha256.len() != 64
        || !executable_sha256
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
    {
        return Err("invalid campaign executable SHA-256".into());
    }
    let source = source_path(root, size);
    let db_path = row_database_path(root, candidate, size, operation, iteration);
    if !db_path.is_file()
        || !authority_path(&db_path).is_file()
        || !expectations_path(&db_path).is_file()
    {
        return Err("row database was not prepared outside the measured process".into());
    }
    if operation == "same-middle" {
        if env::var("WP4M_BASE_COPY_METHOD").as_deref()
            != Ok("physical-byte-copy-identical-database-authority")
        {
            return Err("same-middle row requires a physical byte-copied base".into());
        }
        require_file_custody_hash("WP4M_BASE_DATABASE_SHA256", &db_path)?;
        require_file_custody_hash("WP4M_BASE_AUTHORITY_SHA256", &authority_path(&db_path))?;
        require_file_custody_hash(
            "WP4M_BASE_EXPECTATIONS_SHA256",
            &expectations_path(&db_path),
        )?;
    }
    let mut metrics = Metrics::default();
    let prepared = read_prepared_expectations(&expectations_path(&db_path), &mut metrics)?;
    require_amended_m45_expectations(candidate, size, operation, &prepared.value)?;
    let ChargedPreparedExpectations {
        value:
            PreparedExpectations {
                source_length,
                source_fingerprint,
                edit_point,
                expected_reference_count,
                expected_fingerprint: expected_fingerprint_owned,
                expected_sequence: expected_sequence_owned,
                expected_ranges,
                expected_probes,
                base,
                edit_oracle,
            },
        _charge: _prepared_expectations_charge,
    } = prepared;
    if source_length != size {
        return Err(CoreError::LengthMismatch {
            expected: size,
            actual: source_length,
        }
        .into());
    }
    if operation.starts_with("dir-") {
        if expected_reference_count.is_some()
            || expected_fingerprint_owned.is_some()
            || expected_sequence_owned.is_some()
            || !expected_ranges.is_empty()
            || !expected_probes.is_empty()
        {
            return Err("directory row contains file expectations".into());
        }
    } else if expected_reference_count.is_none()
        || expected_fingerprint_owned.is_none()
        || expected_sequence_owned.is_none()
        || expected_ranges.is_empty()
        || expected_ranges.len() != expected_probes.len()
    {
        return Err("file row is missing prepared expectations".into());
    }
    let (expected_edit_result, expected_replacement) = match (operation, edit_oracle) {
        ("same-middle", Some(oracle)) => {
            let point = edit_point.ok_or(CoreError::MissingObject)?;
            let removed = read_source_segment_charged(
                &source,
                point.byte_offset,
                point.replacement_length,
                &mut metrics,
            )?;
            if oracle.operation != operation
                || oracle.offset != point.byte_offset
                || oracle.removed.as_slice() != removed.as_slice()
                || !is_same_middle_replacement(&oracle.removed, &oracle.inserted)
            {
                return Err(CoreError::PublicationConflict.into());
            }
            (Some(oracle.result()), Some(oracle.inserted))
        }
        ("same-middle", None) => return Err("same-middle row is missing its oracle".into()),
        (_, Some(_)) => return Err("non-same-middle row contains an edit oracle".into()),
        (_, None) => (None, None),
    };
    let full_create_expected = (operation == "full")
        .then(|| {
            full_create_expectation(
                base,
                expected_reference_count,
                expected_fingerprint_owned.as_deref(),
                expected_sequence_owned.as_deref(),
            )
        })
        .transpose()?;
    let qualification_mode = qualification_mode()?;
    let expected_dir_replacement = if operation == "dir-replace" {
        Some((
            u64::try_from(DIRECTORY_ENTRIES / 2 + 1).map_err(|_| CoreError::LengthOverflow)?,
            canonical_bytes(file_codec::encode_file_root(1, 0, 0, 0, &[])?)?.0,
        ))
    } else {
        None
    };
    let mut store = Store::open_measured(&db_path, candidate, &mut metrics)?;
    let physical_before = store.physical_snapshot();
    let mut phases = PhaseTimes::default();
    let authority_metrics_started = metrics;
    let authority_metrics_ended = metrics;
    if operation == "dir-lookup" {
        let (base_root, base_transition, closure_digest) =
            base.ok_or("prepared directory base is missing")?;
        let lookup_started = Instant::now();
        let lookup_metrics_started = metrics;
        let timed_head = store
            .current_head_accounted(&mut metrics)?
            .ok_or(CoreError::InvalidValidationReceipt)?;
        if timed_head.1 != base_root || timed_head.2 != base_transition {
            return Err(CoreError::PublicationConflict.into());
        }
        let range_measurements =
            verify_directory_lookups(&store, base_root, candidate, false, None, &mut metrics)?;
        let lookup_end = Instant::now();
        let phase_metrics = [("range_verification", lookup_metrics_started, metrics)];
        let elapsed = lookup_end.duration_since(lookup_started).as_nanos();
        phases = PhaseTimes {
            range_verification_ns: elapsed,
            complete_lifecycle_total_ns: elapsed,
            ..PhaseTimes::default()
        };
        let physical_after = store.physical_snapshot();
        drop(store);
        let ranges_json = range_measurements_json(&range_measurements, &mut metrics)?;
        drop(range_measurements);
        let report_source_fingerprint = ChargedString::from_str(&source_fingerprint, &mut metrics)?;
        drop(expected_ranges);
        drop(expected_probes);
        drop(expected_fingerprint_owned);
        drop(expected_sequence_owned);
        drop(expected_replacement);
        drop(source_fingerprint);
        drop(_prepared_expectations_charge);
        return row_json(
            candidate,
            size,
            operation,
            iteration,
            warmup,
            &report_source_fingerprint,
            0,
            elapsed,
            base_root,
            base_transition,
            None,
            None,
            0,
            closure_digest,
            metrics,
            physical_before,
            physical_after,
            &phases,
            &phase_metrics,
            &ranges_json,
            &executable_sha256,
            None,
            None,
        );
    }
    let capture_outcome = if operation == "same-middle" {
        let (base_root, base_transition, _) = base.ok_or("prepared edit base is missing")?;
        capture_same_middle(
            &mut store,
            candidate,
            edit_point.ok_or(CoreError::MissingObject)?,
            expected_replacement
                .as_deref()
                .ok_or("missing same-middle replacement")?,
            qualification_mode,
            expected_edit_result.ok_or("same-middle oracle disappeared")?,
            expected_reference_count.ok_or(CoreError::LengthOverflow)?,
            expected_fingerprint_owned
                .as_deref()
                .ok_or("missing edited fingerprint")?,
            expected_sequence_owned
                .as_deref()
                .ok_or("missing edited CDC sequence")?,
            base_root,
            base_transition,
            &mut metrics,
        )?
    } else {
        store.start_sqlite_observations(&mut metrics);
        let durable_capture_start = Instant::now();
        let mut durable_cursor = durable_capture_start;
        let mapping_metrics_started = metrics;
        let prior_head = if matches!(
            operation,
            "plus1-early" | "plus1-middle" | "dir-replace" | "dir-leading"
        ) {
            let (base_root, base_transition, _) = base.ok_or("prepared edit base is missing")?;
            let head = store
                .current_head_accounted(&mut metrics)?
                .ok_or(CoreError::InvalidValidationReceipt)?;
            if head.1 != base_root || head.2 != base_transition {
                return Err(CoreError::PublicationConflict.into());
            }
            Some(head)
        } else {
            None
        };
        let (root_id, transition_id, mut full_create_proof) = if operation == "full" {
            let stage_started = durable_cursor;
            let (root_id, transition_id, proof) = build_file_proven(
                &mut store,
                &source,
                candidate,
                full_create_expected.ok_or(CoreError::PublicationConflict)?,
                &mut metrics,
            )?;
            let stage_end = Instant::now();
            phases.canonical_cas_mapping_stage_ns =
                stage_end.duration_since(stage_started).as_nanos();
            durable_cursor = stage_end;
            (root_id, transition_id, Some(proof))
        } else if operation == "plus1-early" || operation == "plus1-middle" {
            let result = edit_file(
                &mut store,
                candidate,
                operation,
                edit_point.ok_or(CoreError::MissingObject)?,
                false,
                &mut metrics,
            )?;
            let stage_end = Instant::now();
            phases.canonical_cas_mapping_stage_ns =
                stage_end.duration_since(durable_cursor).as_nanos();
            durable_cursor = stage_end;
            (result.0, result.1, None)
        } else if operation == "dir-create" {
            let result = build_directory(&mut store, candidate, false, false, &mut metrics)?;
            let stage_end = Instant::now();
            phases.canonical_cas_mapping_stage_ns =
                stage_end.duration_since(durable_cursor).as_nanos();
            durable_cursor = stage_end;
            (result.0, result.1, None)
        } else if operation == "dir-replace" || operation == "dir-leading" {
            let result = edit_directory(&mut store, candidate, operation, &mut metrics)?;
            let stage_end = Instant::now();
            phases.canonical_cas_mapping_stage_ns =
                stage_end.duration_since(durable_cursor).as_nanos();
            durable_cursor = stage_end;
            (result.0, result.1, None)
        } else {
            return Err(format!("unknown operation {operation}").into());
        };
        let mapping_metrics_ended = metrics;
        let precommit_started = durable_cursor;
        let precommit_metrics_started = metrics;
        let expected_parent = prior_head.as_ref().map(|head| head.1);
        let (expected_operations, _expected_operations_charge) =
            if let Some(parent) = expected_parent {
                let (path, before, after) = if operation.starts_with("dir-") {
                    (
                        &b"t"[..],
                        namespace_entry_id(&store, parent, b"t", &mut metrics)?,
                        namespace_entry_id(&store, root_id, b"t", &mut metrics)?,
                    )
                } else {
                    (
                        &b"file"[..],
                        namespace_entry_id(&store, parent, b"file", &mut metrics)?,
                        namespace_entry_id(&store, root_id, b"file", &mut metrics)?,
                    )
                };
                let (operations, charge) =
                    charged_replace_operation(path, before, after, &mut metrics)?;
                (Some(operations), Some(charge))
            } else {
                (None, None)
            };
        let (closure_digest, actual_references) = if operation == "full" {
            let qualification = full_create_proof
                .as_mut()
                .ok_or(CoreError::ValidationAuthorityUnavailable)?
                .consume(
                    &mut store,
                    full_create_expected.ok_or(CoreError::PublicationConflict)?,
                    &mut metrics,
                )?;
            if qualification.root != root_id
                || qualification.transition != transition_id
                || qualification.total_raw != size
            {
                return Err(CoreError::PublicationConflict.into());
            }
            let result = (qualification.closure, qualification.references);
            drop(full_create_proof.take());
            result
        } else {
            let transition_digest = verify_transition(
                &store,
                transition_id,
                expected_parent,
                root_id,
                expected_operations.as_deref(),
                &mut metrics,
            )?;
            if operation.starts_with("dir-") {
                let digest = verify_directory(
                    &store,
                    root_id,
                    candidate,
                    u64::try_from(DIRECTORY_ENTRIES + usize::from(operation == "dir-leading"))
                        .map_err(|_| CoreError::LengthOverflow)?,
                    expected_dir_replacement,
                    &mut metrics,
                )?;
                (combined_closure_digest(transition_digest, digest), 0)
            } else {
                let (digest, references, _) = verify_file(
                    &store,
                    root_id,
                    candidate,
                    expected_fingerprint_owned.as_deref(),
                    expected_sequence_owned.as_deref(),
                    &mut metrics,
                )?;
                if expected_reference_count != Some(references) {
                    return Err(CoreError::LengthMismatch {
                        expected: expected_reference_count.unwrap_or(0),
                        actual: references,
                    }
                    .into());
                }
                (
                    combined_closure_digest(transition_digest, digest),
                    references,
                )
            }
        };
        let precommit_end = Instant::now();
        let precommit_metrics_ended = metrics;
        phases.precommit_closure_validation_ns =
            precommit_end.duration_since(precommit_started).as_nanos();
        let commit_started = precommit_end;
        let commit_metrics_started = metrics;
        let publication =
            store.publish(prior_head.as_ref(), root_id, transition_id, &mut metrics)?;
        let commit_end = Instant::now();
        let commit_metrics_ended = metrics;
        phases.sqlite_commit_durability_ns = commit_end.duration_since(commit_started).as_nanos();
        phases.durable_capture_total_ns =
            commit_end.duration_since(durable_capture_start).as_nanos();
        let capture_ns = phases.durable_capture_total_ns;
        CaptureOutcome {
            root_id,
            transition_id,
            expected_parent,
            expected_operations,
            expected_operations_charge: _expected_operations_charge,
            closure_digest,
            actual_references,
            phases,
            capture_ns,
            durable_capture_start,
            commit_end,
            authority_metrics_started,
            authority_metrics_ended,
            mapping_metrics_started,
            mapping_metrics_ended,
            precommit_metrics_started,
            precommit_metrics_ended,
            commit_metrics_started,
            commit_metrics_ended,
            publication,
        }
    };
    let CaptureOutcome {
        root_id,
        transition_id,
        expected_parent,
        expected_operations,
        expected_operations_charge: _expected_operations_charge,
        closure_digest,
        actual_references,
        phases: capture_phases,
        capture_ns,
        durable_capture_start,
        commit_end,
        authority_metrics_started,
        authority_metrics_ended,
        mapping_metrics_started,
        mapping_metrics_ended,
        precommit_metrics_started,
        precommit_metrics_ended,
        commit_metrics_started,
        commit_metrics_ended,
        publication,
    } = capture_outcome;
    phases = capture_phases;
    let postcommit = (move || -> AnyResult<ChargedString> {
        let reopen_started = commit_end;
        let reopen_metrics_started = metrics;
        drop(store);
        let mut store = Store::open_measured(&db_path, candidate, &mut metrics)?;
        {
            let _fresh_head_receipt_charge = charge_capacity(&mut metrics, 216)?;
            let head = store
                .current_head_accounted(&mut metrics)?
                .ok_or(CoreError::InvalidValidationReceipt)?;
            if head.1 != root_id || head.2 != transition_id {
                return Err(CoreError::PublicationConflict.into());
            }
        }
        let reopen_end = Instant::now();
        let reopen_metrics_ended = metrics;
        phases.fresh_reopen_head_ns = reopen_end.duration_since(reopen_started).as_nanos();
        let scrub_started = reopen_end;
        let scrub_metrics_started = metrics;
        let fresh_transition_digest = verify_transition(
            &store,
            transition_id,
            expected_parent,
            root_id,
            expected_operations.as_deref(),
            &mut metrics,
        )?;
        let fresh_references = if operation.starts_with("dir-") {
            let digest = verify_directory(
                &store,
                root_id,
                candidate,
                u64::try_from(DIRECTORY_ENTRIES + usize::from(operation == "dir-leading"))
                    .map_err(|_| CoreError::LengthOverflow)?,
                expected_dir_replacement,
                &mut metrics,
            )?;
            let _ = digest;
            0
        } else {
            let mut scrub_store = store;
            let (_, references) = scrub_file(&mut scrub_store, root_id, candidate, &mut metrics)?;
            store = scrub_store;
            references
        };
        let scrub_end = Instant::now();
        let scrub_metrics_ended = metrics;
        phases.fresh_full_scrub_ns = scrub_end.duration_since(scrub_started).as_nanos();
        let reconstruction_started = scrub_end;
        let reconstruction_metrics_started = metrics;
        let (fresh_content_digest, reconstructed_references) = if operation.starts_with("dir-") {
            let digest = verify_directory(
                &store,
                root_id,
                candidate,
                u64::try_from(DIRECTORY_ENTRIES + usize::from(operation == "dir-leading"))
                    .map_err(|_| CoreError::LengthOverflow)?,
                expected_dir_replacement,
                &mut metrics,
            )?;
            (digest, 0)
        } else {
            let (digest, references, _) = reconstruct_file(
                &store,
                root_id,
                candidate,
                expected_fingerprint_owned.as_deref(),
                expected_sequence_owned.as_deref(),
                &mut metrics,
            )?;
            (digest, references)
        };
        let reconstruction_end = Instant::now();
        let reconstruction_metrics_ended = metrics;
        phases.reconstruction_ns = reconstruction_end
            .duration_since(reconstruction_started)
            .as_nanos();
        let range_metrics_started = metrics;
        let range_measurements = if !operation.starts_with("dir-") {
            let file_root = namespace_entry_id(&store, root_id, b"file", &mut metrics)?;
            verify_ranges(
                &store,
                file_root,
                candidate,
                &expected_probes,
                &expected_ranges,
                &mut metrics,
            )?
        } else {
            verify_directory_lookups(
                &store,
                root_id,
                candidate,
                operation == "dir-leading",
                expected_dir_replacement,
                &mut metrics,
            )?
        };
        if reconstructed_references != fresh_references {
            return Err(CoreError::PublicationConflict.into());
        }
        if fresh_references != actual_references
            || combined_closure_digest(fresh_transition_digest, fresh_content_digest)
                != closure_digest
        {
            return Err(CoreError::PublicationConflict.into());
        }
        let lifecycle_end = Instant::now();
        let range_metrics_ended = metrics;
        phases.range_verification_ns = lifecycle_end.duration_since(reconstruction_end).as_nanos();
        phases.complete_lifecycle_total_ns = lifecycle_end
            .duration_since(durable_capture_start)
            .as_nanos();
        let physical_after = store.physical_snapshot();
        drop(store);
        let phase_metrics = [
            (
                "same_open_authority",
                authority_metrics_started,
                authority_metrics_ended,
            ),
            (
                "canonical_cas_mapping",
                mapping_metrics_started,
                mapping_metrics_ended,
            ),
            (
                "precommit_closure",
                precommit_metrics_started,
                precommit_metrics_ended,
            ),
            (
                "sqlite_commit",
                commit_metrics_started,
                commit_metrics_ended,
            ),
            (
                "fresh_reopen_head",
                reopen_metrics_started,
                reopen_metrics_ended,
            ),
            (
                "fresh_full_scrub",
                scrub_metrics_started,
                scrub_metrics_ended,
            ),
            (
                "reconstruction",
                reconstruction_metrics_started,
                reconstruction_metrics_ended,
            ),
            (
                "range_verification",
                range_metrics_started,
                range_metrics_ended,
            ),
        ];
        let qualification_ns = phases.complete_lifecycle_total_ns;
        let ranges_json = range_measurements_json(&range_measurements, &mut metrics)?;
        drop(range_measurements);
        drop(expected_operations);
        drop(_expected_operations_charge);
        let report_source_fingerprint = ChargedString::from_str(&source_fingerprint, &mut metrics)?;
        let report_expected_sequence = expected_sequence_owned
            .as_deref()
            .map(|value| ChargedString::from_str(value, &mut metrics))
            .transpose()?;
        drop(expected_ranges);
        drop(expected_probes);
        drop(expected_fingerprint_owned);
        drop(expected_sequence_owned);
        drop(expected_replacement);
        drop(source_fingerprint);
        drop(_prepared_expectations_charge);
        let output = row_json(
            candidate,
            size,
            operation,
            iteration,
            warmup,
            &report_source_fingerprint,
            capture_ns,
            qualification_ns,
            root_id,
            transition_id,
            expected_reference_count,
            report_expected_sequence.as_deref(),
            fresh_references,
            closure_digest,
            metrics,
            physical_before,
            physical_after,
            &phases,
            &phase_metrics,
            &ranges_json,
            &executable_sha256,
            Some(publication),
            None,
        )?;
        Ok(output)
    })();
    committed_result(root_id, transition_id, postcommit)
}

fn self_test(root: &Path) -> AnyResult<()> {
    fs::create_dir_all(root)?;
    let source = root.join("self-test.bin");
    fill_source(&source, 256 * 1024, 0x11)?;
    let candidate = FILE_CANDIDATES[0];
    let db = root.join("self-test.sqlite");
    if db.exists() {
        fs::remove_file(&db)?;
    }
    let mut metrics = Metrics::default();
    let mut store = Store::open(&db, candidate)?;
    let (root_id, transition_id) = build_file(&mut store, &source, candidate, &mut metrics)?;
    let (_, expected_fingerprint, expected_sequence, _expected_ranges, _) =
        expected_file_observations(&source, "full", 256 * 1024, candidate)?;
    let _ = verify_transition(&store, transition_id, None, root_id, None, &mut metrics)?;
    let _ = verify_file(
        &store,
        root_id,
        candidate,
        Some(&expected_fingerprint),
        Some(&expected_sequence),
        &mut metrics,
    )?;
    store.publish(None, root_id, transition_id, &mut metrics)?;
    store.begin(&mut metrics)?;
    let mut witness =
        establish_same_open_file_witness(&mut store, candidate, None, None, &mut metrics)?;
    let permit = witness.consume(&store, &mut metrics)?;
    let head_receipt_charge = charge_capacity(&mut metrics, 216)?;
    let head = store
        .current_head_accounted(&mut metrics)?
        .ok_or(CoreError::InvalidValidationReceipt)?;
    if !permit.covers(&store, &head) {
        return Err("same-open witness binding failed".into());
    }
    drop(head_receipt_charge);
    drop(permit);
    drop(witness);
    drop(store);
    let store = Store::open(&db, candidate)?;
    let _ = verify_file(
        &store,
        root_id,
        candidate,
        Some(&expected_fingerprint),
        Some(&expected_sequence),
        &mut metrics,
    )?;
    let mut malformed = vec![0_u8; 11];
    malformed[..8].copy_from_slice(b"LFS4MAP\0");
    malformed[8..10].copy_from_slice(&2_u16.to_be_bytes());
    if file_codec::decode_mapping(
        &encode_canonical_object(&Object::bytes(malformed)?)?,
        file_codec::FILE_ROOT_TAG,
    )
    .is_ok()
    {
        return Err("malformed mapping accepted".into());
    }
    store.connection.execute(
        "UPDATE wp4m_visible_head SET validation_receipt = zeroblob(215) WHERE id = 1",
        [],
    )?;
    if !matches!(
        store.current_head(),
        Err(error) if error.downcast_ref::<CoreError>() == Some(&CoreError::InvalidValidationReceipt)
    ) {
        return Err("invalid receipt accepted".into());
    }
    finish_q(&mut metrics)?;
    println!(
        "self-test PASS root={root_id} objects={} auth_bytes={}",
        metrics.objects_created, metrics.canonical_bytes_authenticated
    );
    Ok(())
}

struct JsonObject<'a> {
    fields: BTreeMap<&'a str, &'a str>,
}

impl<'a> JsonObject<'a> {
    fn parse(input: &'a str) -> AnyResult<Self> {
        let input = input.trim();
        let bytes = input.as_bytes();
        if bytes.first() != Some(&b'{') || bytes.last() != Some(&b'}') {
            return Err("expected one JSON object".into());
        }
        let mut fields = BTreeMap::new();
        let mut index = 1_usize;
        while index + 1 < bytes.len() {
            while bytes.get(index).is_some_and(u8::is_ascii_whitespace) {
                index += 1;
            }
            if bytes.get(index) == Some(&b'}') {
                break;
            }
            if bytes.get(index) != Some(&b'"') {
                return Err("expected JSON object key".into());
            }
            let key_start = index + 1;
            index = json_string_end(bytes, index)?;
            let key = &input[key_start..index - 1];
            while bytes.get(index).is_some_and(u8::is_ascii_whitespace) {
                index += 1;
            }
            if bytes.get(index) != Some(&b':') {
                return Err("expected JSON object colon".into());
            }
            index += 1;
            while bytes.get(index).is_some_and(u8::is_ascii_whitespace) {
                index += 1;
            }
            let value_start = index;
            let mut depth = 0_usize;
            while index < bytes.len() {
                match bytes[index] {
                    b'"' => index = json_string_end(bytes, index)?,
                    b'[' | b'{' => {
                        depth = depth.checked_add(1).ok_or(CoreError::LengthOverflow)?;
                        index += 1;
                    }
                    b']' | b'}' if depth != 0 => {
                        depth -= 1;
                        index += 1;
                    }
                    b',' | b'}' if depth == 0 => break,
                    _ => index += 1,
                }
            }
            let value = input[value_start..index].trim();
            if value.is_empty() || fields.insert(key, value).is_some() {
                return Err("empty or duplicate JSON object field".into());
            }
            if bytes.get(index) == Some(&b',') {
                index += 1;
            } else if bytes.get(index) != Some(&b'}') {
                return Err("unterminated JSON object".into());
            }
        }
        Ok(Self { fields })
    }

    fn string(&self, key: &str) -> Option<&'a str> {
        let value = self.fields.get(key)?;
        (value.starts_with('"') && value.ends_with('"') && value.len() >= 2)
            .then_some(&value[1..value.len() - 1])
    }

    fn u128(&self, key: &str) -> Option<u128> {
        self.fields.get(key)?.parse().ok()
    }

    fn usize(&self, key: &str) -> Option<usize> {
        self.u128(key)?.try_into().ok()
    }

    fn boolean(&self, key: &str) -> Option<bool> {
        match *self.fields.get(key)? {
            "true" => Some(true),
            "false" => Some(false),
            _ => None,
        }
    }

    fn numeric(&self, key: &str) -> bool {
        self.fields
            .get(key)
            .is_some_and(|value| value.parse::<i128>().is_ok())
    }
}

fn json_string_end(bytes: &[u8], start: usize) -> AnyResult<usize> {
    let mut index = start.checked_add(1).ok_or(CoreError::LengthOverflow)?;
    while index < bytes.len() {
        match bytes[index] {
            b'\\' => index = index.checked_add(2).ok_or(CoreError::LengthOverflow)?,
            b'"' => return Ok(index + 1),
            byte if byte < 0x20 => return Err("control byte in JSON string".into()),
            _ => index += 1,
        }
    }
    Err("unterminated JSON string".into())
}

fn decimal_seconds_to_ns(value: &str) -> Option<u128> {
    let (whole, fraction) = value.split_once('.').unwrap_or((value, ""));
    let seconds = whole.parse::<u128>().ok()?;
    let mut nanoseconds = 0_u128;
    for byte in fraction.as_bytes().iter().take(9) {
        nanoseconds = nanoseconds
            .checked_mul(10)?
            .checked_add(u128::from(byte.saturating_sub(b'0')))?;
    }
    for _ in fraction.len().min(9)..9 {
        nanoseconds = nanoseconds.checked_mul(10)?;
    }
    seconds
        .checked_mul(1_000_000_000)
        .and_then(|value| value.checked_add(nanoseconds))
}

fn external_resource_metrics(stderr: &str) -> (Option<u128>, Option<u64>) {
    let mut user_ns = None;
    let mut sys_ns = None;
    let mut rss_bytes = None;
    for line in stderr.lines() {
        let tokens: Vec<&str> = line.split_whitespace().collect();
        if let Some(index) = tokens.iter().position(|token| *token == "user") {
            if index > 0 {
                user_ns = decimal_seconds_to_ns(tokens[index - 1]);
            }
        }
        if let Some(index) = tokens.iter().position(|token| *token == "sys") {
            if index > 0 {
                sys_ns = decimal_seconds_to_ns(tokens[index - 1]);
            }
        }
        if line.contains("maximum resident set size") {
            rss_bytes = tokens
                .iter()
                .rev()
                .find_map(|token| token.parse::<u64>().ok());
        }
    }
    (
        user_ns.and_then(|user| sys_ns.and_then(|sys| user.checked_add(sys))),
        rss_bytes,
    )
}

fn add_external_resource_metrics(stdout: &str, stderr: &str) -> AnyResult<String> {
    let (cpu_ns, rss_bytes) = external_resource_metrics(stderr);
    let mut line = stdout.trim_end().to_string();
    JsonObject::parse(&line)?;
    if line.pop() != Some('}') {
        return Err("row JSON lost its object terminator".into());
    }
    writeln!(
        &mut line,
        ",\"cpu_ns\":{},\"rss_bytes\":{}}}",
        cpu_ns.map_or_else(|| "\"Unavailable\"".to_string(), |value| value.to_string()),
        rss_bytes.map_or_else(|| "\"Unavailable\"".to_string(), |value| value.to_string()),
    )
    .map_err(|_| CoreError::Io)?;
    Ok(line)
}

#[allow(clippy::too_many_arguments)]
fn invoke_campaign_row(
    root: &Path,
    candidate: Candidate,
    size: u64,
    operation: &str,
    iteration: usize,
    warmup: bool,
    output: &mut File,
    failures: &mut File,
    commands: &mut File,
    resources: &mut File,
    executable_sha256: &str,
) -> AnyResult<()> {
    let executable = env::current_exe()?;
    let preparation_args = vec![
        "--prepare-row".to_string(),
        root.to_str().ok_or("non-UTF8 campaign root")?.to_string(),
        candidate.name.to_string(),
        size.to_string(),
        operation.to_string(),
        iteration.to_string(),
    ];
    writeln!(commands, "{:?} {:?}", executable, preparation_args)?;
    let preparation = std::process::Command::new(&executable)
        .args(&preparation_args)
        .output()?;
    if !preparation.status.success() {
        return Err(format!(
            "row preparation failed {}: {}",
            candidate.name,
            String::from_utf8_lossy(&preparation.stderr)
        )
        .into());
    }
    let mut command = if Path::new("/usr/bin/time").is_file() {
        let mut command = std::process::Command::new("/usr/bin/time");
        command.arg("-l").arg(&executable);
        command
    } else {
        std::process::Command::new(&executable)
    };
    let args = vec![
        "--row".to_string(),
        root.to_str().ok_or("non-UTF8 campaign root")?.to_string(),
        candidate.name.to_string(),
        size.to_string(),
        operation.to_string(),
        iteration.to_string(),
        warmup.to_string(),
    ];
    command.args(&args);
    command.env("WP4M_EXECUTABLE_SHA256", executable_sha256);
    writeln!(
        commands,
        "WP4M_EXECUTABLE_SHA256={} {:?} {:?}",
        executable_sha256,
        command.get_program(),
        command.get_args().collect::<Vec<_>>()
    )?;
    let result = command.output()?;
    if !result.status.success() {
        let stderr = String::from_utf8_lossy(&result.stderr).replace('\n', " ");
        writeln!(
            failures,
            "{{\"candidate\":\"{}\",\"size_bytes\":{},\"operation\":\"{}\",\"iteration\":{},\"stderr\":\"{}\"}}",
            candidate.name,
            size,
            operation,
            iteration,
            stderr.replace('"', "'")
        )?;
        return Err(format!("row failed {}: {stderr}", candidate.name).into());
    }
    let stdout = String::from_utf8_lossy(&result.stdout);
    let stderr = String::from_utf8_lossy(&result.stderr);
    writeln!(
        resources,
        "candidate={} size={} operation={} iteration={} warmup={}",
        candidate.name, size, operation, iteration, warmup
    )?;
    resources.write_all(result.stderr.as_slice())?;
    if !result.stderr.ends_with(b"\n") {
        writeln!(resources)?;
    }
    output.write_all(add_external_resource_metrics(&stdout, &stderr)?.as_bytes())?;
    Ok(())
}

fn median(values: &[u128]) -> Option<u128> {
    let mut values = values.to_vec();
    values.sort_unstable();
    values.get(values.len() / 2).copied()
}

fn write_campaign_summary(root: &Path, jsonl: &Path, invocations: usize) -> AnyResult<()> {
    let raw = fs::read_to_string(jsonl)?;
    let mut warmup = 0_usize;
    let mut measured = 0_usize;
    let mut failures = 0_usize;
    let mut protected_metrics_available = true;
    let mut groups: BTreeMap<String, Vec<(usize, u128)>> = BTreeMap::new();
    let mut paired_modes: BTreeMap<(u128, String, usize), BTreeMap<String, u128>> = BTreeMap::new();
    let mut default_full_lifecycle = Vec::new();
    for line in raw.lines().filter(|line| !line.trim().is_empty()) {
        let object = JsonObject::parse(line)?;
        let is_warmup = object
            .boolean("warmup")
            .ok_or("row JSON warmup field is not boolean")?;
        if is_warmup {
            warmup = warmup.checked_add(1).ok_or(CoreError::LengthOverflow)?;
        } else {
            measured = measured.checked_add(1).ok_or(CoreError::LengthOverflow)?;
        }
        if object.string("status") != Some("PASS") {
            failures = failures.checked_add(1).ok_or(CoreError::LengthOverflow)?;
        }
        if [
            "cpu_ns",
            "rss_bytes",
            "allocated_store_delta_bytes",
            "q_high_water",
        ]
        .iter()
        .any(|key| !object.numeric(key))
        {
            protected_metrics_available = false;
        }
        let Some(candidate) = object.string("candidate") else {
            continue;
        };
        let Some(size) = object.u128("size_bytes") else {
            continue;
        };
        let Some(operation) = object.string("operation") else {
            continue;
        };
        let Some(qualification) = object.u128("sqlite_qualification_wall_ns") else {
            continue;
        };
        let iteration = object.usize("iteration").unwrap_or(0);
        if !is_warmup {
            if candidate == "K64-F64" && size == u128::from(SOURCE_100) && operation == "full" {
                default_full_lifecycle.push(qualification);
            }
            groups
                .entry(format!("{candidate}|{size}|{operation}"))
                .or_default()
                .push((iteration, qualification));
            if let Some(mode) = object.string("qualification_mode") {
                paired_modes
                    .entry((size, operation.to_string(), iteration))
                    .or_default()
                    .insert(mode.to_string(), qualification);
            }
        }
    }
    let mut rows = String::new();
    let mut first = true;
    for (key, values) in &groups {
        let mut samples: Vec<u128> = values.iter().map(|(_, value)| *value).collect();
        samples.sort_unstable();
        let min = samples.first().copied().unwrap_or(0);
        let max = samples.last().copied().unwrap_or(0);
        let spread = max.saturating_sub(min);
        if !first {
            rows.push(',');
        }
        first = false;
        rows.push_str(&format!(
            "{{\"group\":\"{key}\",\"samples\":{},\"median_sqlite_qualification_wall_ns\":{},\"min_ns\":{min},\"max_ns\":{max},\"spread_ns\":{spread}}}",
            samples.len(),
            median(&samples).unwrap_or(0)
        ));
    }
    let mut paired_effects = String::new();
    let mut paired_first = true;
    let mut causal_pairs = 0_usize;
    let mut causal_wins = 0_usize;
    for ((size, operation, iteration), modes) in &paired_modes {
        let (Some(c0), Some(c1)) = (modes.get("C0-full-closure"), modes.get("C1-changed-spine"))
        else {
            continue;
        };
        causal_pairs = causal_pairs
            .checked_add(1)
            .ok_or(CoreError::LengthOverflow)?;
        causal_wins = causal_wins
            .checked_add(usize::from(c1 < c0))
            .ok_or(CoreError::LengthOverflow)?;
        if !paired_first {
            paired_effects.push(',');
        }
        paired_first = false;
        let delta_ns = i128::try_from(*c1)
            .map_err(|_| CoreError::LengthOverflow)?
            .checked_sub(i128::try_from(*c0).map_err(|_| CoreError::LengthOverflow)?)
            .ok_or(CoreError::LengthOverflow)?;
        write!(
            &mut paired_effects,
            "{{\"size_bytes\":{size},\"operation\":\"{operation}\",\"iteration\":{iteration},\"c0_ns\":{c0},\"c1_ns\":{c1},\"paired_delta_ns\":{delta_ns},\"c1_win\":{}}}",
            c1 < c0,
        )
        .map_err(|_| CoreError::Io)?;
    }
    let gate = if invocations != 144 || warmup != 24 || measured != 120 || failures != 0 {
        "FAIL"
    } else {
        "INCONCLUSIVE"
    };
    let diagnostic_500ms = median(&default_full_lifecycle).map_or_else(
        || "{\"status\":\"INCONCLUSIVE\",\"median_complete_lifecycle_wall_ns\":\"Unavailable\",\"target_ns\":500000000}".to_string(),
        |value| {
            format!(
                "{{\"status\":\"{}\",\"median_complete_lifecycle_wall_ns\":{value},\"target_ns\":500000000}}",
                if value <= 500_000_000 { "PASS" } else { "FAIL" }
            )
        },
    );
    let summary_path = root.join("wp4m-profile-selection-summary.json");
    let mut summary = File::create(&summary_path)?;
    writeln!(
        summary,
        "{{\"format\":2,\"campaign_scope\":\"100MiB-mini-plus-wide-directory\",\"invocations\":{invocations},\"warmup\":{warmup},\"measured\":{measured},\"row_failures\":{failures},\"protected_metrics_available\":{protected_metrics_available},\"internal_500ms_diagnostic\":{diagnostic_500ms},\"sql_sensitivity\":\"INCONCLUSIVE-no-low-SQL-control\",\"paired_c0_c1\":{{\"pairs\":{causal_pairs},\"c1_wins\":{causal_wins},\"effects\":[{paired_effects}]}},\"admissibility\":\"{gate}\",\"reason\":\"512MiB scaling and profile selection are outside M4.5\",\"candidate_status\":{{\"K64-F64\":\"INCONCLUSIVE-default-not-promoted\",\"K59-F101\":\"INCONCLUSIVE\",\"K256-F256\":\"INCONCLUSIVE\",\"DIR64K\":\"INCONCLUSIVE\",\"DIR256K\":\"INCONCLUSIVE-default-not-promoted\",\"DIR1M\":\"INCONCLUSIVE\"}},\"rows\":[{rows}]}}"
    )?;
    summary.sync_all()?;
    Ok(())
}

#[cfg(test)]
#[allow(clippy::items_after_test_module)]
mod tests {
    use super::*;
    use layerfs_core::cas::InMemoryCas;
    use layerfs_core::content::{ChunkReference, LogicalFile};

    const COW_TEST_REFERENCES: u64 = 4_100;
    const DEEP_COW_TEST_REFERENCES: u64 = 64 * 64 * 64 + 1;

    #[test]
    fn live_capacity_sums_overlap_and_decharges_on_errors() {
        assert_eq!(q_current(), 0);
        let mut metrics = Metrics::default();
        let parent = ChargedVec::<file_codec::FileChild>::with_capacity(4, &mut metrics)
            .expect("parent allocation");
        let parent_bytes = parent.capacity() * std::mem::size_of::<file_codec::FileChild>();
        assert_eq!(q_current(), parent_bytes as u64);
        {
            let canonical =
                ChargedVec::<u8>::with_capacity(37, &mut metrics).expect("canonical allocation");
            let stack =
                ChargedVec::<ObjectId>::with_capacity(5, &mut metrics).expect("stack allocation");
            let stack_bytes = stack.capacity() * std::mem::size_of::<ObjectId>();
            assert_eq!(
                metrics.q_high_water,
                u64::try_from(parent_bytes + canonical.capacity() + stack_bytes)
                    .expect("expected Q"),
            );
        }
        assert_eq!(q_current(), parent_bytes as u64);
        drop(parent);
        assert_eq!(q_current(), 0);

        let failure: CoreResult<()> = (|| {
            let _output = ChargedVec::<u8>::with_capacity(91, &mut metrics)?;
            Err(CoreError::PublicationConflict)
        })();
        assert_eq!(failure, Err(CoreError::PublicationConflict));
        assert_eq!(q_current(), 0);
        finish_q(&mut metrics).expect("balanced Q");

        let boundary = charge_capacity(
            &mut metrics,
            usize::try_from(layerfs_core::limits::MAX_DURABLE_LIVE_ALLOCATION)
                .expect("live-allocation limit"),
        )
        .expect("exact 1-GiB boundary is admitted");
        assert_eq!(
            q_current(),
            layerfs_core::limits::MAX_DURABLE_LIVE_ALLOCATION
        );
        assert_eq!(
            charge_capacity(&mut metrics, 1).expect_err("one byte above boundary"),
            CoreError::AllocationBudgetExceeded
        );
        assert_eq!(
            q_current(),
            layerfs_core::limits::MAX_DURABLE_LIVE_ALLOCATION
        );
        drop(boundary);
        assert_eq!(q_current(), 0);

        let error = ChargedVec::<u8>::with_capacity(
            usize::try_from(layerfs_core::limits::MAX_DURABLE_LIVE_ALLOCATION)
                .expect("live-allocation limit")
                + 1,
            &mut metrics,
        )
        .expect_err("Q must reject above the durable live-allocation limit");
        assert_eq!(error, CoreError::AllocationBudgetExceeded);
        assert_eq!(q_current(), 0);
    }

    #[test]
    fn exact_builder_rejects_excess_capacity_and_cleans_q() {
        assert_eq!(q_current(), 0);
        let mut metrics = Metrics::default();
        let error = ChargedVec::<u8>::from_exact_builder(4, &mut metrics, || {
            let mut values = Vec::with_capacity(5);
            values.extend_from_slice(b"four");
            Ok(values)
        })
        .expect_err("unadmitted allocator capacity must be rejected");
        assert_eq!(error, CoreError::AllocationFailed);
        assert_eq!(q_current(), 0);
    }

    #[test]
    fn real_sqlite_read_precharges_canonical_and_decoded_overlap() {
        let database = test_path("real-q-read.sqlite");
        let mut store = Store::open(&database, FILE_CANDIDATES[0]).expect("open");
        let canonical =
            encode_canonical_object(&Object::bytes(b"payload".to_vec()).expect("object"))
                .expect("canonical");
        let id = ObjectId::for_bytes(&canonical);
        let mut setup_metrics = Metrics::default();
        store
            .put(id, &canonical, &mut setup_metrics)
            .expect("put canonical");
        assert_eq!(q_current(), 0);

        let mut metrics = Metrics::default();
        let loaded = store.get(id, &mut metrics).expect("charged real-path read");
        let expected = u64::try_from(canonical.len() + 256 + b"payload".len()).expect("expected Q");
        assert_eq!(q_current(), expected);
        assert_eq!(metrics.q_high_water, expected);
        assert_eq!(&*loaded.2, canonical.as_slice());
        drop(loaded);
        assert_eq!(q_current(), 0);
        finish_q(&mut metrics).expect("real-path read decharges");

        store
            .connection
            .execute(
                "UPDATE wp4m_objects SET canonical_bytes = ?1 WHERE object_id = ?2",
                params![b"bad".as_slice(), id.as_bytes().as_slice()],
            )
            .expect("corrupt canonical");
        let error = store
            .get(id, &mut metrics)
            .expect_err("corrupt real-path read must fail");
        assert_eq!(
            error.downcast_ref::<CoreError>(),
            Some(&CoreError::UnexpectedEof)
        );
        assert_eq!(q_current(), 0);
        drop(store);
        remove_sqlite_image(&database).expect("database cleanup");
    }

    #[test]
    fn campaign_json_parser_is_structural() {
        let row = r#"{"status":"PASS","warmup":false,"iteration":7,"nested":{"status":"FAIL","fake":"\"warmup\":true"},"values":[1,{"iteration":99}],"q_high_water":123}"#;
        let object = JsonObject::parse(row).expect("valid object");
        assert_eq!(object.string("status"), Some("PASS"));
        assert_eq!(object.boolean("warmup"), Some(false));
        assert_eq!(object.usize("iteration"), Some(7));
        assert_eq!(object.u128("q_high_water"), Some(123));
        assert!(object.numeric("q_high_water"));
        assert!(JsonObject::parse(r#"{"x":1,"x":2}"#).is_err());
    }

    #[test]
    fn f1_v2_status_code_dictionary_and_compaction_are_exact() {
        assert_eq!(F1_STATUS_SCHEMA, "f1-v2-status-codes-v1");
        assert_eq!(F1_Q_EQUATION, "Q1");
        assert_eq!(F1_STATUS_CODES.len(), 17);
        for (index, code) in F1_STATUS_CODES.iter().enumerate() {
            assert!(!code.is_empty());
            assert!(!F1_STATUS_CODES[..index].contains(code));
        }

        let verbose = F1_ROW_STATUS_REPLACEMENTS
            .iter()
            .map(|(from, _)| *from)
            .collect::<String>();
        let expected = F1_ROW_STATUS_REPLACEMENTS
            .iter()
            .map(|(_, to)| *to)
            .collect::<String>();
        let mut compact = String::new();
        write!(CompactStatusWriter(&mut compact), "{verbose}").expect("compact statuses");
        assert_eq!(compact, expected);
        assert!(verbose.len() > compact.len());
        for (_, to) in F1_ROW_STATUS_REPLACEMENTS {
            let code = to
                .rsplit_once(":\"")
                .and_then(|(_, value)| value.strip_suffix('\"'))
                .expect("replacement code");
            assert!(F1_STATUS_CODES.contains(&code) || code == F1_Q_EQUATION);
        }
    }

    #[test]
    fn row_json_reconciles_q_sql_and_changed_work_fields() {
        let metrics = Metrics {
            statement_cache_acquisitions: 3,
            sql_query_calls: 5,
            sql_execute_calls: 2,
            sql_rows_returned: 7,
            sql_rows_changed: 1,
            canonical_bytes_authenticated: 41,
            canonical_bytes_written: 11,
            objects_authenticated: 1,
            mapping_bytes_rewritten: 9,
            incremental_receipt_covered_edges: 127,
            incremental_new_or_different_edges: 4,
            incremental_new_subtree_objects_authenticated: 1,
            incremental_new_subtree_bytes_authenticated: 19,
            transactions: 1,
            commits: 1,
            commit_returns: 1,
            commit_return_successes: 1,
            commit_publish_call_wall_ns: 7,
            commit_dispatch_to_return_wall_ns: 4,
            q_high_water: 97,
            w_bytes: 101,
            d_bytes: 202,
            ..Metrics::default()
        };
        let mut invalid = metrics;
        invalid.commit_return_errors = 1;
        assert!(validate_metric_equations(invalid).is_err());
        let id = ObjectId::for_bytes(b"row-json-id");
        let json = row_json(
            FILE_CANDIDATES[0],
            SOURCE_100,
            "same-middle",
            1,
            false,
            RETAINED_RAW_100,
            1,
            2,
            id,
            id,
            Some(RETAINED_CDC_100),
            Some(RETAINED_CDC_SEQUENCE_100),
            RETAINED_CDC_100,
            [0_u8; 32],
            metrics,
            PhysicalSnapshot::default(),
            PhysicalSnapshot::default(),
            &PhaseTimes {
                sqlite_commit_durability_ns: 9,
                durable_capture_total_ns: 9,
                complete_lifecycle_total_ns: 9,
                ..PhaseTimes::default()
            },
            &[],
            "",
            &"0".repeat(64),
            Some(PublicationOutcome {
                status: PublicationStatus::RequestedVisible,
                diagnostic: Some(failure_provenance(
                    Some(FailureCause::Core(CoreError::Io)),
                    None,
                    Reconciliation::RequestedVisible,
                    None,
                )),
            }),
            None,
        )
        .expect("row JSON");
        let object = JsonObject::parse(&json).expect("structural row JSON");
        assert_eq!(object.string("milestone"), Some("F1"));
        assert_eq!(
            object.u128("q_high_water"),
            Some(u128::try_from(json.len()).expect("JSON length"))
        );
        assert_eq!(object.u128("q_current"), Some(0));
        assert_eq!(
            object.u128("q_report_output_bytes"),
            Some(u128::try_from(json.len()).expect("JSON length"))
        );
        assert_eq!(
            object.string("q_current_semantics"),
            Some("after_report_output_drop")
        );
        assert_eq!(object.u128("statement_cache_acquisitions"), Some(3));
        assert_eq!(
            object.string("native_sqlite_prepare_calls"),
            Some("U_NATIVE_PREP")
        );
        assert_eq!(object.u128("sql_query_calls"), Some(5));
        assert_eq!(object.u128("sql_execute_calls"), Some(2));
        assert_eq!(object.u128("sql_rows_returned"), Some(7));
        assert_eq!(object.u128("sql_rows_changed"), Some(1));
        assert_eq!(
            object.string("measurement_status_schema"),
            Some(F1_STATUS_SCHEMA)
        );
        assert_eq!(object.string("q_equation"), Some(F1_Q_EQUATION));
        assert_eq!(
            object.fields.get("instrumentation").copied(),
            Some(r#"{"c":"O","sql":[0,0,0,0,0,0,0,0],"status":[0,0,0,0,0,0]}"#)
        );
        let measurement_status = JsonObject::parse(
            object
                .fields
                .get("measurement_status")
                .expect("measurement status object"),
        )
        .expect("measurement status JSON");
        assert_eq!(
            measurement_status.string("phase_counters"),
            Some(STATUS_OBSERVED)
        );
        assert_eq!(measurement_status.string("w_d"), Some("U_WD"));
        assert_eq!(measurement_status.string("query_plans"), Some("U_PLAN"));
        assert_eq!(object.u128("commit_dispatches"), Some(1));
        assert_eq!(object.u128("commit_returns"), Some(1));
        assert_eq!(object.string("commit_return_status"), Some("ok"));
        assert_eq!(object.u128("commit_publish_call_wall_ns"), Some(7));
        assert_eq!(object.u128("commit_dispatch_to_return_wall_ns"), Some(4));
        assert_eq!(object.u128("commit_pre_and_post_dispatch_wall_ns"), Some(3));
        assert_eq!(object.u128("commit_caller_wrapper_wall_ns"), Some(2));
        assert_eq!(object.boolean("commit_timer_equation_matches"), Some(true));
        assert_eq!(object.u128("canonical_new_write_bytes"), Some(11));
        assert_eq!(
            object.u128("canonical_authenticated_nonnew_bytes"),
            Some(30)
        );
        assert_eq!(object.u128("canonical_rewrite_bytes"), Some(9));
        assert_eq!(
            object.string("physical_db_allocated_bytes"),
            Some("Unavailable")
        );
        assert_eq!(
            object.string("physical_journal_allocated_bytes"),
            Some("Unavailable")
        );
        assert_eq!(
            object.string("physical_authority_sidecar_allocated_bytes"),
            Some("Unavailable")
        );
        assert_eq!(object.string("w_bytes"), Some("U_WD"));
        assert_eq!(object.string("d_bytes"), Some("U_WD"));
        assert_eq!(object.u128("covered_equal_edges"), Some(127));
        assert_eq!(object.u128("new_or_different_edges"), Some(4));
        assert_eq!(
            object.string("publication_status"),
            Some("RequestedVisible")
        );
        assert_eq!(
            object.string("receipt_provenance"),
            Some("first=Some(Core(Io));cleanup_first=None;reconciliation=RequestedVisible;reconciliation_error=None;dominant=None")
        );
        assert_eq!(q_current(), json.len() as u64);
        drop(object);
        drop(json);
        assert_eq!(q_current(), 0);
    }

    fn test_path(label: &str) -> PathBuf {
        env::temp_dir().join(format!(
            "layerfs-wp4m-{label}-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ))
    }

    fn exact_same_middle_observations(source: &Path, point: EditPoint) -> (u64, String, String) {
        let removed = read_source_segment(source, point.byte_offset, point.replacement_length)
            .expect("removed bytes");
        let replacement = same_middle_replacement(&removed);
        let mut count = 0_u64;
        let mut fingerprint = Hasher::new();
        let mut sequence = Hasher::new();
        FastCdc::new()
            .scan(
                edited_source_reader(
                    source,
                    point.byte_offset,
                    point.replacement_length,
                    &replacement,
                )
                .expect("edited reader"),
                |chunk| {
                    count = count.checked_add(1).ok_or(CoreError::LengthOverflow)?;
                    fingerprint.update(chunk);
                    sequence.update(
                        &u32::try_from(chunk.len())
                            .map_err(|_| CoreError::LengthOverflow)?
                            .to_be_bytes(),
                    );
                    sequence.update(chunk_id(chunk).as_bytes());
                    Ok(())
                },
            )
            .expect("scan exact edited stream");
        (
            count,
            fingerprint.finalize().to_hex().to_string(),
            sequence.finalize().to_hex().to_string(),
        )
    }

    #[test]
    fn same_middle_expectations_use_the_exact_edited_cdc_stream() {
        let source = test_path("exact-edited-cdc.source");
        let size = 4 * 1024 * 1024_u64;
        fill_source(&source, size, 0x51).expect("write source");
        let point = prepared_edit_point(&source, "same-middle").expect("edit point");
        let expected = expected_file_observations(&source, "same-middle", size, FILE_CANDIDATES[0])
            .expect("prepared observations");
        let exact = exact_same_middle_observations(&source, point);
        assert_eq!(
            (expected.0, expected.1.as_str(), expected.2.as_str()),
            (exact.0, exact.1.as_str(), exact.2.as_str())
        );
        let removed = read_source_segment(&source, point.byte_offset, point.replacement_length)
            .expect("removed bytes");
        let replacement = same_middle_replacement(&removed);
        let mut ordinal = 0_u64;
        let mut old_sequence = Hasher::new();
        FastCdc::new()
            .scan(File::open(&source).expect("source"), |chunk| {
                let bytes = if ordinal == point.position {
                    replacement.as_slice()
                } else {
                    chunk
                };
                old_sequence.update(
                    &u32::try_from(bytes.len())
                        .map_err(|_| CoreError::LengthOverflow)?
                        .to_be_bytes(),
                );
                old_sequence.update(chunk_id(bytes).as_bytes());
                ordinal = ordinal.checked_add(1).ok_or(CoreError::LengthOverflow)?;
                Ok(())
            })
            .expect("old substitution scan");
        assert_ne!(expected.2, old_sequence.finalize().to_hex().to_string());
        fs::remove_file(source).expect("source cleanup");
    }

    #[test]
    fn amended_m45_fixture_gate_rejects_any_frozen_identity_drift() {
        let removed = vec![0_u8; AMENDED_M45_EDIT_LENGTH];
        let mut expected = PreparedExpectations {
            source_length: SOURCE_100,
            source_fingerprint: RETAINED_RAW_100.to_string(),
            edit_point: Some(EditPoint {
                reference_count: RETAINED_CDC_100,
                position: AMENDED_M45_EDIT_POSITION,
                byte_offset: AMENDED_M45_EDIT_OFFSET,
                replacement_length: AMENDED_M45_EDIT_LENGTH,
            }),
            expected_reference_count: Some(RETAINED_CDC_100),
            expected_fingerprint: Some(AMENDED_M45_EDITED_FINGERPRINT.to_string()),
            expected_sequence: Some(AMENDED_M45_CDC_SEQUENCE.to_string()),
            expected_ranges: Vec::new(),
            expected_probes: Vec::new(),
            base: Some((
                AMENDED_M45_BASE_ROOT.parse().expect("base root"),
                AMENDED_M45_BASE_TRANSITION
                    .parse()
                    .expect("base transition"),
                AMENDED_M45_BASE_CLOSURE
                    .parse::<ObjectId>()
                    .expect("base closure")
                    .to_bytes(),
            )),
            edit_oracle: Some(PreparedEditOracle {
                operation: "same-middle".to_string(),
                offset: AMENDED_M45_EDIT_OFFSET,
                inserted: same_middle_replacement(&removed),
                removed,
                before_file: AMENDED_M45_BEFORE_FILE.parse().expect("before file"),
                after_file: AMENDED_M45_AFTER_FILE.parse().expect("after file"),
                result_root: AMENDED_M45_RESULT_ROOT.parse().expect("result root"),
                result_transition: AMENDED_M45_RESULT_TRANSITION
                    .parse()
                    .expect("result transition"),
                result_closure: AMENDED_M45_RESULT_CLOSURE
                    .parse::<ObjectId>()
                    .expect("result closure")
                    .to_bytes(),
            }),
        };
        require_amended_m45_expectations(FILE_CANDIDATES[0], SOURCE_100, "same-middle", &expected)
            .expect("frozen amended row");
        expected.expected_sequence = Some("00".repeat(32));
        let error = require_amended_m45_expectations(
            FILE_CANDIDATES[0],
            SOURCE_100,
            "same-middle",
            &expected,
        )
        .expect_err("identity drift must fail");
        assert_eq!(
            error.downcast_ref::<CoreError>(),
            Some(&CoreError::PublicationConflict)
        );
    }

    #[test]
    fn oversized_prepared_expectations_fail_before_unbounded_read() {
        let path = test_path("oversized.expectations");
        fs::write(&path, vec![b'x'; 128 * 1024 + 1]).expect("oversized expectations");
        let error = read_prepared_expectations(&path, &mut Metrics::default())
            .err()
            .expect("oversized input must fail");
        assert_eq!(
            error.downcast_ref::<CoreError>(),
            Some(&CoreError::ObjectLimitExceeded)
        );
        fs::remove_file(path).expect("expectation cleanup");
    }

    fn uniform_file_observations(
        position: u64,
        replacement: u8,
        inserted: bool,
    ) -> (String, String) {
        let mut raw = Hasher::new();
        let mut sequence = Hasher::new();
        for ordinal in 0..COW_TEST_REFERENCES + u64::from(inserted) {
            let byte = if ordinal == position {
                replacement
            } else {
                b'x'
            };
            raw.update(&[byte]);
            sequence.update(&1_u32.to_be_bytes());
            sequence.update(chunk_id(&[byte]).as_bytes());
        }
        (
            raw.finalize().to_hex().to_string(),
            sequence.finalize().to_hex().to_string(),
        )
    }

    fn uniform_file_observations_for_changes(changes: &[(u64, u8)]) -> (String, String) {
        uniform_file_observations_for_reference_count(COW_TEST_REFERENCES, changes)
    }

    fn uniform_file_observations_for_reference_count(
        reference_count: u64,
        changes: &[(u64, u8)],
    ) -> (String, String) {
        let mut raw = Hasher::new();
        let mut sequence = Hasher::new();
        for ordinal in 0..reference_count {
            let byte = changes
                .iter()
                .find_map(|(position, byte)| (*position == ordinal).then_some(*byte))
                .unwrap_or(b'x');
            raw.update(&[byte]);
            sequence.update(&1_u32.to_be_bytes());
            sequence.update(chunk_id(&[byte]).as_bytes());
        }
        (
            raw.finalize().to_hex().to_string(),
            sequence.finalize().to_hex().to_string(),
        )
    }

    fn build_uniform_base(database: &Path, candidate: Candidate) -> (Store, ObjectId, ObjectId) {
        let mut store = Store::open(database, candidate).expect("open base");
        let mut metrics = Metrics::default();
        store.begin(&mut metrics).expect("begin base");
        let reference = make_reference(&mut store, b"x", &mut metrics).expect("reference");
        let mut builder = FileBuilder::new(candidate);
        for _ in 0..COW_TEST_REFERENCES {
            builder
                .push_reference(&mut store, reference, &mut metrics)
                .expect("base reference");
        }
        let file = builder.finish(&mut store, &mut metrics).expect("file base");
        let root = namespace_file_root(&mut store, file, &mut metrics).expect("namespace base");
        let transition =
            publish_transition(&mut store, None, root, &mut metrics).expect("base transition");
        store
            .publish(None, root, transition, &mut metrics)
            .expect("publish base");
        (store, root, file)
    }

    fn build_deep_uniform_base(
        database: &Path,
        candidate: Candidate,
    ) -> (Store, ObjectId, ObjectId) {
        assert_eq!((candidate.k, candidate.f), (64, 64));
        let mut store = Store::open(database, candidate).expect("open deep base");
        let mut metrics = Metrics::default();
        store.begin(&mut metrics).expect("begin deep base");
        let reference = make_reference(&mut store, b"x", &mut metrics).expect("deep reference");

        let full_leaf = put_mapping(
            &mut store,
            encode_charged_file_leaf(&vec![reference; candidate.k], &mut metrics)
                .expect("full deep leaf"),
            &mut metrics,
        )
        .expect("put full deep leaf");
        let partial_leaf = put_mapping(
            &mut store,
            encode_charged_file_leaf(&[reference], &mut metrics).expect("partial deep leaf"),
            &mut metrics,
        )
        .expect("put partial deep leaf");

        let leaf_span = u64::try_from(candidate.k).expect("leaf span");
        let level_one_children = (1..=candidate.f)
            .map(|ordinal| file_codec::FileChild {
                object_id: full_leaf,
                cumulative_end: u64::try_from(ordinal).expect("leaf ordinal") * leaf_span,
            })
            .collect::<Vec<_>>();
        let full_level_one = put_mapping(
            &mut store,
            encode_charged_file_branch(1, &level_one_children, &mut metrics)
                .expect("full level-one branch"),
            &mut metrics,
        )
        .expect("put full level-one branch");
        let partial_level_one = put_mapping(
            &mut store,
            encode_charged_file_branch(
                1,
                &[file_codec::FileChild {
                    object_id: partial_leaf,
                    cumulative_end: 1,
                }],
                &mut metrics,
            )
            .expect("partial level-one branch"),
            &mut metrics,
        )
        .expect("put partial level-one branch");

        let level_one_span = leaf_span * u64::try_from(candidate.f).expect("branch fanout");
        let level_two_children = (1..=candidate.f)
            .map(|ordinal| file_codec::FileChild {
                object_id: full_level_one,
                cumulative_end: u64::try_from(ordinal).expect("branch ordinal") * level_one_span,
            })
            .collect::<Vec<_>>();
        let full_level_two = put_mapping(
            &mut store,
            encode_charged_file_branch(2, &level_two_children, &mut metrics)
                .expect("full level-two branch"),
            &mut metrics,
        )
        .expect("put full level-two branch");
        let partial_level_two = put_mapping(
            &mut store,
            encode_charged_file_branch(
                2,
                &[file_codec::FileChild {
                    object_id: partial_level_one,
                    cumulative_end: 1,
                }],
                &mut metrics,
            )
            .expect("partial level-two branch"),
            &mut metrics,
        )
        .expect("put partial level-two branch");

        let full_span = level_one_span * u64::try_from(candidate.f).expect("root fanout");
        assert_eq!(full_span + 1, DEEP_COW_TEST_REFERENCES);
        let file = put_mapping(
            &mut store,
            encode_charged_file_root(
                0,
                DEEP_COW_TEST_REFERENCES,
                DEEP_COW_TEST_REFERENCES,
                2,
                &[
                    file_codec::FileChild {
                        object_id: full_level_two,
                        cumulative_end: full_span,
                    },
                    file_codec::FileChild {
                        object_id: partial_level_two,
                        cumulative_end: DEEP_COW_TEST_REFERENCES,
                    },
                ],
                &mut metrics,
            )
            .expect("deep file root"),
            &mut metrics,
        )
        .expect("put deep file root");
        let root = namespace_file_root(&mut store, file, &mut metrics).expect("deep namespace");
        let transition =
            publish_transition(&mut store, None, root, &mut metrics).expect("deep transition");
        store
            .publish(None, root, transition, &mut metrics)
            .expect("publish deep base");
        assert_eq!(q_current(), 0);
        (store, root, file)
    }

    fn file_root_children(store: &Store, file: ObjectId) -> (u8, Vec<file_codec::FileChild>) {
        let mut metrics = Metrics::default();
        let (_object_charge, _, bytes) = store.get(file, &mut metrics).expect("file root");
        let payload = file_codec::decode_mapping(&bytes, file_codec::FILE_ROOT_TAG)
            .expect("file root mapping");
        let (_, _, _, level, children) =
            file_codec::parse_file_root(payload).expect("file root body");
        (level, children)
    }

    fn consume_base_witness(
        store: &mut Store,
        candidate: Candidate,
        metrics: &mut Metrics,
    ) -> SameOpenValidationPermit {
        store.begin(metrics).expect("begin witness transaction");
        let mut witness = establish_same_open_file_witness(store, candidate, None, None, metrics)
            .expect("base full scrub witness");
        witness.consume(store, metrics).expect("consume witness")
    }

    fn digest_array(value: &str) -> [u8; 32] {
        decode_hex(value)
            .expect("hex digest")
            .try_into()
            .expect("32-byte digest")
    }

    fn build_proven_chunks(
        store: &mut Store,
        candidate: Candidate,
        chunks: &[Vec<u8>],
        metrics: &mut Metrics,
    ) -> (
        ObjectId,
        ObjectId,
        FullCreateConstructionProof,
        FullCreateExpectation,
    ) {
        store.begin(metrics).expect("begin construction");
        let mut builder = FileBuilder::new_proving(candidate, chunks.len() as u64, store, metrics)
            .expect("proving builder");
        let mut source = Hasher::new();
        let mut sequence = Hasher::new();
        for chunk in chunks {
            source.update(chunk);
            sequence.update(&(chunk.len() as u32).to_be_bytes());
            sequence.update(chunk_id(chunk).as_bytes());
            builder
                .push_bytes(store, chunk, metrics)
                .expect("push proven chunk");
        }
        let (file_root, file_proof) = builder
            .finish_proven(store, metrics)
            .expect("finish proven file");
        let (root, root_evidence) =
            namespace_file_root_with_evidence(store, file_root, metrics).expect("workspace");
        let workspace = file_proof
            .fold_workspace(root_evidence, root, metrics)
            .expect("workspace proof");
        let (transition, transition_evidence) =
            publish_genesis_transition_with_evidence(store, root, metrics).expect("transition");
        let mut proof = workspace
            .fold_transition(transition_evidence, transition, metrics)
            .expect("transition proof");
        let transition_digest = verify_transition(store, transition, None, root, None, metrics)
            .expect("transition shadow");
        let (content_digest, references, _) =
            verify_file(store, root, candidate, None, None, metrics).expect("file shadow");
        assert_eq!(references, chunks.len() as u64);
        let expectation = FullCreateExpectation {
            source_fingerprint: *source.finalize().as_bytes(),
            cdc_sequence: *sequence.finalize().as_bytes(),
            references,
            root,
            transition,
            closure: combined_closure_digest(transition_digest, content_digest),
        };
        proof
            .bind_expectation(expectation)
            .expect("bind expectation");
        store
            .mark_construction_proof_issued(&proof)
            .expect("proof issuance");
        (root, transition, proof, expectation)
    }

    #[test]
    fn f2_construction_proof_type_sizes_and_retained_q_are_exact() {
        assert_eq!(std::mem::size_of::<ObjectId>(), 32);
        assert_eq!(std::mem::size_of::<ObjectKind>(), 1);
        assert_eq!(std::mem::size_of::<Hasher>(), 1_920);
        assert_eq!(std::mem::size_of::<file_codec::FileReference>(), 68);
        assert_eq!(std::mem::size_of::<file_codec::FileChild>(), 40);
        assert_eq!(std::mem::size_of::<Vec<u8>>(), 24);
        assert_eq!(std::mem::size_of::<PutEvidence>(), 80);
        assert_eq!(std::mem::size_of::<ConstructionNodeProof>(), 64);
        assert!(std::mem::size_of::<ConstructionState>() <= Q_CONSTRUCTION_STATE_BYTES);
        let (levels, frontier) =
            construction_frontier_bytes(FILE_CANDIDATES[0], 5_284).expect("frontier");
        assert_eq!(levels, 2);
        assert_eq!(frontier, 17_776);
        assert_eq!(
            Q_CONSTRUCTION_STATE_BYTES + frontier + Q_PUT_EVIDENCE_BYTES,
            21_952
        );
    }

    #[test]
    fn f2_shadow_proof_matches_the_full_verifier_exactly() {
        let source = test_path("f2-shadow.source");
        fill_source(&source, 256 * 1024, 0x11).expect("source");
        let candidate = FILE_CANDIDATES[0];
        let (expected_references, fingerprint, sequence, _, _) =
            expected_file_observations(&source, "full", 256 * 1024, candidate)
                .expect("expectations");
        let database = test_path("f2-shadow.sqlite");
        let mut store = Store::open(&database, candidate).expect("store");
        let mut metrics = Metrics::default();
        let (root, transition, mut proof) = build_file_construction(
            &mut store,
            &source,
            candidate,
            expected_references,
            &mut metrics,
        )
        .expect("proven construction");
        let transition_digest =
            verify_transition(&store, transition, None, root, None, &mut metrics)
                .expect("full transition verifier");
        let (content_digest, references, total_raw) = verify_file(
            &store,
            root,
            candidate,
            Some(&fingerprint),
            Some(&sequence),
            &mut metrics,
        )
        .expect("full file verifier");
        let closure = combined_closure_digest(transition_digest, content_digest);
        let expectation = FullCreateExpectation {
            source_fingerprint: digest_array(&fingerprint),
            cdc_sequence: digest_array(&sequence),
            references: expected_references,
            root,
            transition,
            closure,
        };
        proof
            .bind_expectation(expectation)
            .expect("bind expectation");
        store
            .mark_construction_proof_issued(&proof)
            .expect("issue proof");
        let qualification = proof
            .consume(&mut store, expectation, &mut metrics)
            .expect("consume proof");
        assert_eq!(qualification.references, references);
        assert_eq!(qualification.total_raw, total_raw);
        assert_eq!(qualification.root, root);
        assert_eq!(qualification.transition, transition);
        assert_eq!(qualification.closure, closure);
        assert_eq!(qualification.source_fingerprint, digest_array(&fingerprint));
        assert_eq!(qualification.cdc_sequence, digest_array(&sequence));
        assert_eq!(metrics.construction_proof_consumptions, 1);
        assert_eq!(
            metrics.construction_put_evidences,
            metrics
                .chunks
                .checked_add(metrics.pages)
                .and_then(|value| value.checked_add(metrics.branches))
                .and_then(|value| value.checked_add(3))
                .expect("put equation")
        );
        assert_eq!(
            metrics.construction_edges_covered,
            metrics
                .references
                .checked_add(metrics.pages)
                .and_then(|value| value.checked_add(metrics.branches))
                .and_then(|value| value.checked_add(2))
                .expect("edge equation")
        );
        let error = proof
            .consume(&mut store, expectation, &mut metrics)
            .expect_err("proof is single-use");
        assert_eq!(
            error.downcast_ref::<CoreError>(),
            Some(&CoreError::ValidationAuthorityUnavailable)
        );
        drop(proof);
        store.rollback(&mut metrics).expect("rollback shadow row");
        finish_q(&mut metrics).expect("terminal Q");
        drop(store);
        remove_sqlite_image(&database).expect("database cleanup");
        fs::remove_file(source).expect("source cleanup");
    }

    #[test]
    fn f2_topology_boundaries_and_duplicate_reuse_stay_bounded() {
        let candidate = Candidate {
            name: "F2-K2-F2",
            k: 2,
            f: 2,
            directory_page: 256 * 1024,
        };
        for count in [0_usize, 1, 2, 3, 4, 5, 9] {
            let database = test_path(&format!("f2-topology-{count}.sqlite"));
            let mut store = Store::open(&database, candidate).expect("store");
            let mut metrics = Metrics::default();
            let chunks = (0..count)
                .map(|index| {
                    if index % 2 == 0 {
                        b"duplicate".to_vec()
                    } else {
                        format!("unique-{index}").into_bytes()
                    }
                })
                .collect::<Vec<_>>();
            let (_, _, mut proof, expectation) =
                build_proven_chunks(&mut store, candidate, &chunks, &mut metrics);
            let qualification = proof
                .consume(&mut store, expectation, &mut metrics)
                .expect("qualification");
            assert_eq!(qualification.references, count as u64);
            assert_eq!(
                qualification.total_raw,
                chunks.iter().map(|chunk| chunk.len() as u64).sum::<u64>()
            );
            assert_eq!(
                proof.workspace.file.file.level,
                file_codec::expected_file_level(
                    count as u64,
                    file_codec::FileMappingProfile::new(candidate.k, candidate.f),
                )
                .expect("expected level")
            );
            if count >= 3 {
                assert!(metrics.objects_reused > 0);
            }
            drop(proof);
            store.rollback(&mut metrics).expect("rollback");
            finish_q(&mut metrics).expect("terminal Q");
            drop(store);
            remove_sqlite_image(&database).expect("database cleanup");
        }
    }

    #[test]
    fn f2_proof_rejects_summary_namespace_transition_authority_and_mutation_drift() {
        let candidate = Candidate {
            name: "F2-ADVERSARY",
            k: 2,
            f: 2,
            directory_page: 256 * 1024,
        };

        let summary_database = test_path("f2-summary-drift.sqlite");
        let mut summary_store = Store::open(&summary_database, candidate).expect("store");
        let mut summary_metrics = Metrics::default();
        summary_store.begin(&mut summary_metrics).expect("begin");
        let mut builder =
            FileBuilder::new_proving(candidate, 1, &summary_store, &mut summary_metrics)
                .expect("builder");
        builder
            .push_bytes(&mut summary_store, b"summary", &mut summary_metrics)
            .expect("chunk");
        builder.total_raw += 1;
        let error = builder
            .finish_proven(&mut summary_store, &mut summary_metrics)
            .err()
            .expect("wrong root summary");
        assert!(matches!(
            error.downcast_ref::<CoreError>(),
            Some(CoreError::LengthMismatch { .. })
        ));
        summary_store
            .rollback(&mut summary_metrics)
            .expect("rollback summary");
        finish_q(&mut summary_metrics).expect("summary Q");
        drop(summary_store);
        remove_sqlite_image(&summary_database).expect("summary cleanup");

        for case in [
            "source",
            "sequence",
            "count",
            "root",
            "transition",
            "closure",
            "authority",
        ] {
            let database = test_path(&format!("f2-{case}-drift.sqlite"));
            let mut store = Store::open(&database, candidate).expect("store");
            let mut metrics = Metrics::default();
            let chunks = vec![b"a".to_vec(), b"b".to_vec(), b"c".to_vec()];
            let (_, _, mut proof, mut expectation) =
                build_proven_chunks(&mut store, candidate, &chunks, &mut metrics);
            match case {
                "source" => expectation.source_fingerprint[0] ^= 1,
                "sequence" => expectation.cdc_sequence[0] ^= 1,
                "count" => expectation.references += 1,
                "root" => expectation.root = ObjectId::for_bytes(b"wrong-root"),
                "transition" => expectation.transition = ObjectId::for_bytes(b"wrong-transition"),
                "closure" => expectation.closure[0] ^= 1,
                "authority" => store.validation_authority_id[0] ^= 1,
                _ => unreachable!(),
            }
            let error = proof
                .consume(&mut store, expectation, &mut metrics)
                .expect_err("drift must fail");
            assert!(matches!(
                error.downcast_ref::<CoreError>(),
                Some(CoreError::PublicationConflict | CoreError::InvalidValidationReceipt)
            ));
            drop(proof);
            store.rollback(&mut metrics).expect("rollback drift");
            finish_q(&mut metrics).expect("drift Q");
            drop(store);
            remove_sqlite_image(&database).expect("drift cleanup");
        }

        let mutation_database = test_path("f2-mutation.sqlite");
        let mut store = Store::open(&mutation_database, candidate).expect("store");
        let mut metrics = Metrics::default();
        let chunks = vec![b"mutation".to_vec()];
        let (_, _, mut proof, expectation) =
            build_proven_chunks(&mut store, candidate, &chunks, &mut metrics);
        let extra = encode_canonical_object(&Object::bytes(b"extra".to_vec()).expect("extra"))
            .expect("extra canonical");
        store
            .put(ObjectId::for_bytes(&extra), &extra, &mut metrics)
            .expect("later mutation");
        let error = proof
            .consume(&mut store, expectation, &mut metrics)
            .expect_err("mutation invalidates proof");
        assert_eq!(
            error.downcast_ref::<CoreError>(),
            Some(&CoreError::InvalidValidationReceipt)
        );
        drop(proof);
        store.rollback(&mut metrics).expect("rollback mutation");
        finish_q(&mut metrics).expect("mutation Q");
        drop(store);
        remove_sqlite_image(&mutation_database).expect("mutation cleanup");
    }

    #[test]
    fn f2_proof_is_invalid_after_rollback_commit_or_reopen() {
        let candidate = Candidate {
            name: "F2-LIFECYCLE",
            k: 2,
            f: 2,
            directory_page: 256 * 1024,
        };
        for case in ["rollback", "commit", "reopen"] {
            let database = test_path(&format!("f2-{case}.sqlite"));
            let mut store = Store::open(&database, candidate).expect("store");
            let mut metrics = Metrics::default();
            let chunks = vec![b"lifecycle".to_vec()];
            let (root, transition, mut proof, expectation) =
                build_proven_chunks(&mut store, candidate, &chunks, &mut metrics);
            match case {
                "rollback" => store.rollback(&mut metrics).expect("rollback"),
                "commit" => {
                    store
                        .publish(None, root, transition, &mut metrics)
                        .expect("commit");
                }
                "reopen" => {
                    store
                        .rollback(&mut metrics)
                        .expect("rollback before reopen");
                    drop(store);
                    store = Store::open(&database, candidate).expect("reopen");
                }
                _ => unreachable!(),
            }
            let error = proof
                .consume(&mut store, expectation, &mut metrics)
                .expect_err("lifecycle invalidation");
            assert_eq!(
                error.downcast_ref::<CoreError>(),
                Some(&CoreError::ValidationAuthorityUnavailable)
            );
            drop(proof);
            if store.active_transaction.is_some() {
                store.rollback(&mut metrics).expect("final rollback");
            }
            finish_q(&mut metrics).expect("terminal Q");
            drop(store);
            remove_sqlite_image(&database).expect("database cleanup");
        }
    }

    #[test]
    fn f2_evidence_rejects_wrong_role_missing_object_and_overflow() {
        let candidate = Candidate {
            name: "F2-ERRORS",
            k: 2,
            f: 2,
            directory_page: 256 * 1024,
        };

        let role_database = test_path("f2-wrong-role.sqlite");
        let mut role_store = Store::open(&role_database, candidate).expect("store");
        let mut role_metrics = Metrics::default();
        role_store.begin(&mut role_metrics).expect("begin");
        let (mut state, _) = ConstructionState::new(&role_store, candidate, 0, &mut role_metrics)
            .expect("construction state");
        let (id, len, evidence) = role_store
            .put_generated_bytes_with_evidence(b"role", &mut role_metrics)
            .expect("bytes evidence");
        let error = state
            .accept_put(evidence, id, ObjectKind::Directory, len, &mut role_metrics)
            .expect_err("wrong role");
        assert_eq!(error, CoreError::ValidationAuthorityUnavailable);
        drop(state);
        role_store
            .rollback(&mut role_metrics)
            .expect("role rollback");
        finish_q(&mut role_metrics).expect("role Q");
        drop(role_store);
        remove_sqlite_image(&role_database).expect("role cleanup");

        let missing_database = test_path("f2-missing.sqlite");
        let mut missing_store = Store::open(&missing_database, candidate).expect("store");
        let mut missing_metrics = Metrics::default();
        let chunks = vec![b"missing".to_vec()];
        let (root, _, mut proof, expectation) =
            build_proven_chunks(&mut missing_store, candidate, &chunks, &mut missing_metrics);
        let missing_id = canonical_bytes(chunks[0].clone()).expect("chunk id").0;
        missing_store
            .connection
            .execute(
                "DELETE FROM wp4m_objects WHERE object_id = ?1",
                params![missing_id.as_bytes().as_slice()],
            )
            .expect("delete injected object");
        missing_store.mutation_serial += 1;
        let verifier_error = verify_file(
            &missing_store,
            root,
            candidate,
            None,
            None,
            &mut missing_metrics,
        )
        .expect_err("full verifier rejects missing object");
        assert_eq!(
            verifier_error.downcast_ref::<CandidateError>(),
            Some(&CandidateError::MissingObject(missing_id))
        );
        let proof_error = proof
            .consume(&mut missing_store, expectation, &mut missing_metrics)
            .expect_err("mutation invalidates proof");
        assert_eq!(
            proof_error.downcast_ref::<CoreError>(),
            Some(&CoreError::InvalidValidationReceipt)
        );
        drop(proof);
        missing_store
            .rollback(&mut missing_metrics)
            .expect("missing rollback");
        finish_q(&mut missing_metrics).expect("missing Q");
        drop(missing_store);
        remove_sqlite_image(&missing_database).expect("missing cleanup");

        let overflow_database = test_path("f2-overflow.sqlite");
        let mut overflow_store = Store::open(&overflow_database, candidate).expect("store");
        let mut overflow_metrics = Metrics::default();
        overflow_store.begin(&mut overflow_metrics).expect("begin");
        overflow_store.mutation_serial = u64::MAX;
        let overflow = overflow_store
            .put_generated_bytes_with_evidence(b"overflow", &mut overflow_metrics)
            .expect_err("serial overflow");
        assert_eq!(
            overflow.downcast_ref::<CoreError>(),
            Some(&CoreError::LengthOverflow)
        );
        overflow_store.mutation_serial = 0;
        overflow_store
            .rollback(&mut overflow_metrics)
            .expect("overflow rollback");
        finish_q(&mut overflow_metrics).expect("overflow Q");
        assert_eq!(
            construction_frontier_bytes(
                Candidate {
                    name: "F2-OVERFLOW",
                    k: usize::MAX,
                    f: 2,
                    directory_page: 256 * 1024,
                },
                1,
            ),
            Err(CoreError::LengthOverflow)
        );
        drop(overflow_store);
        remove_sqlite_image(&overflow_database).expect("overflow cleanup");
    }

    #[test]
    fn f2_candidate_publishes_once_and_fresh_verification_recomputes_closure() {
        let source = test_path("f2-publish.source");
        fill_source(&source, 256 * 1024, 0x11).expect("source");
        let candidate = FILE_CANDIDATES[0];
        let (references, fingerprint, sequence, _, _) =
            expected_file_observations(&source, "full", 256 * 1024, candidate)
                .expect("expectations");

        let oracle_database = test_path("f2-publish-oracle.sqlite");
        let mut oracle = Store::open(&oracle_database, candidate).expect("oracle");
        let mut oracle_metrics = Metrics::default();
        let (root, transition) =
            build_file(&mut oracle, &source, candidate, &mut oracle_metrics).expect("oracle build");
        let transition_digest =
            verify_transition(&oracle, transition, None, root, None, &mut oracle_metrics)
                .expect("oracle transition");
        let content_digest = verify_file(
            &oracle,
            root,
            candidate,
            Some(&fingerprint),
            Some(&sequence),
            &mut oracle_metrics,
        )
        .expect("oracle file")
        .0;
        let expectation = FullCreateExpectation {
            source_fingerprint: digest_array(&fingerprint),
            cdc_sequence: digest_array(&sequence),
            references,
            root,
            transition,
            closure: combined_closure_digest(transition_digest, content_digest),
        };
        oracle
            .rollback(&mut oracle_metrics)
            .expect("oracle rollback");
        drop(oracle);

        let database = test_path("f2-publish.sqlite");
        let mut store = Store::open(&database, candidate).expect("store");
        let mut metrics = Metrics::default();
        let (actual_root, actual_transition, mut proof) =
            build_file_proven(&mut store, &source, candidate, expectation, &mut metrics)
                .expect("candidate build");
        let precommit = metrics;
        let qualification = proof
            .consume(&mut store, expectation, &mut metrics)
            .expect("candidate qualification");
        assert_eq!(qualification.root, root);
        assert_eq!(qualification.transition, transition);
        assert_eq!(
            metric_delta(metrics.sql_query_calls, precommit.sql_query_calls),
            Ok(1)
        );
        assert_eq!(
            metric_delta(metrics.sql_rows_returned, precommit.sql_rows_returned),
            Ok(0)
        );
        assert_eq!(
            metric_delta(metrics.row_blob_reads, precommit.row_blob_reads),
            Ok(0)
        );
        assert_eq!(
            metric_delta(
                metrics.objects_authenticated,
                precommit.objects_authenticated
            ),
            Ok(0)
        );
        drop(proof);
        store
            .publish(None, actual_root, actual_transition, &mut metrics)
            .expect("one COMMIT");
        assert_eq!(metrics.transactions, 1);
        assert_eq!(metrics.commits, 1);
        drop(store);

        let mut fresh = Store::open(&database, candidate).expect("fresh reopen");
        let head = fresh.current_head().expect("head").expect("visible head");
        assert_eq!((head.1, head.2), (root, transition));
        let fresh_transition =
            verify_transition(&fresh, transition, None, root, None, &mut metrics)
                .expect("fresh transition");
        let (_, fresh_references) =
            scrub_file(&mut fresh, root, candidate, &mut metrics).expect("fresh scrub");
        let (fresh_content, reconstructed_references, total_raw) = reconstruct_file(
            &fresh,
            root,
            candidate,
            Some(&fingerprint),
            Some(&sequence),
            &mut metrics,
        )
        .expect("fresh reconstruction");
        assert_eq!(fresh_references, references);
        assert_eq!(reconstructed_references, references);
        assert_eq!(total_raw, 256 * 1024);
        assert_eq!(
            combined_closure_digest(fresh_transition, fresh_content),
            expectation.closure
        );
        drop(fresh);
        finish_q(&mut metrics).expect("terminal Q");
        remove_sqlite_image(&oracle_database).expect("oracle cleanup");
        remove_sqlite_image(&database).expect("database cleanup");
        fs::remove_file(source).expect("source cleanup");
    }

    #[test]
    fn candidate_store_rebinds_cached_object_statements_without_weakening_immutable_handoff() {
        let database = test_path("cached-object-statements.sqlite");
        let mut store = Store::open(&database, FILE_CANDIDATES[0]).expect("open");
        let canonical_a = encode_canonical_object(&Object::bytes(b"a".to_vec()).expect("object a"))
            .expect("canonical a");
        let canonical_b = encode_canonical_object(&Object::bytes(b"b".to_vec()).expect("object b"))
            .expect("canonical b");
        let id_a = ObjectId::for_bytes(&canonical_a);
        let id_b = ObjectId::for_bytes(&canonical_b);
        let mut metrics = Metrics::default();

        store.put(id_a, &canonical_a, &mut metrics).expect("put a");
        store.put(id_b, &canonical_b, &mut metrics).expect("put b");
        store
            .put(id_a, &canonical_a, &mut metrics)
            .expect("reuse a");
        assert_eq!(metrics.objects_created, 2);
        assert_eq!(metrics.objects_reused, 1);
        assert_eq!(store.get(id_a, &mut metrics).expect("get a").2, canonical_a);
        assert_eq!(
            store.get_bytes(id_b, &mut metrics).expect("get bytes b"),
            canonical_b
        );

        store
            .connection
            .execute(
                "UPDATE wp4m_objects SET canonical_bytes = ?1 WHERE object_id = ?2",
                params![canonical_b.as_slice(), id_a.as_bytes().as_slice()],
            )
            .expect("corrupt incumbent");
        let error = store
            .get_bytes(id_a, &mut metrics)
            .expect_err("corrupt bytes must fail");
        assert_eq!(
            error.downcast_ref::<CoreError>(),
            Some(&CoreError::IdentityMismatch)
        );
        let error = store
            .put(id_a, &canonical_a, &mut metrics)
            .expect_err("corrupt incumbent must fail");
        assert_eq!(
            error.downcast_ref::<CoreError>(),
            Some(&CoreError::IdentityMismatch)
        );

        drop(store);
        remove_sqlite_image(&database).expect("database cleanup");
    }

    #[test]
    fn candidate_borrowed_bytes_preserve_typed_failures_and_callback_errors() {
        let database = test_path("borrowed-bytes-errors.sqlite");
        let mut store = Store::open(&database, FILE_CANDIDATES[0]).expect("open");
        let canonical =
            encode_canonical_object(&Object::bytes(b"payload".to_vec()).expect("object"))
                .expect("canonical");
        let id = ObjectId::for_bytes(&canonical);
        let missing = ObjectId::for_bytes(b"missing");
        let mut metrics = Metrics::default();
        store.put(id, &canonical, &mut metrics).expect("put");

        let raw = store
            .with_borrowed_bytes(id, &mut metrics, |canonical, _| {
                Ok(layerfs_core::decode_bytes_object(canonical)?.to_vec())
            })
            .expect("borrowed read");
        assert_eq!(raw, b"payload");
        assert_eq!(metrics.borrowed_row_blob_reads, 1);
        assert_eq!(metrics.borrowed_row_blob_bytes, canonical.len() as u64);

        let error = store
            .with_borrowed_bytes(id, &mut metrics, |_, _| {
                Err::<(), _>(CoreError::PublicationConflict.into())
            })
            .expect_err("callback error");
        assert_eq!(
            error.downcast_ref::<CoreError>(),
            Some(&CoreError::PublicationConflict)
        );

        let error = store
            .with_borrowed_bytes(missing, &mut metrics, |_, _| Ok(()))
            .expect_err("missing row");
        assert_eq!(
            error.downcast_ref::<CandidateError>(),
            Some(&CandidateError::MissingObject(missing))
        );

        store
            .connection
            .execute(
                "UPDATE wp4m_objects SET canonical_bytes = ?1 WHERE object_id = ?2",
                params![b"tampered".as_slice(), id.as_bytes().as_slice()],
            )
            .expect("tamper row");
        let error = store
            .with_borrowed_bytes(id, &mut metrics, |_, _| Ok(()))
            .expect_err("tampered row");
        assert_eq!(
            error.downcast_ref::<CoreError>(),
            Some(&CoreError::IdentityMismatch)
        );

        store
            .connection
            .execute(
                "UPDATE wp4m_objects SET canonical_bytes = 'wrong-type' WHERE object_id = ?1",
                params![id.as_bytes().as_slice()],
            )
            .expect("wrong-type row");
        let error = store
            .with_borrowed_bytes(id, &mut metrics, |_, _| Ok(()))
            .expect_err("wrong-type row");
        assert!(matches!(
            error.downcast_ref::<rusqlite::types::FromSqlError>(),
            Some(rusqlite::types::FromSqlError::InvalidType)
        ));

        drop(store);
        remove_sqlite_image(&database).expect("database cleanup");
    }

    #[test]
    fn candidate_generated_bytes_preserve_identity_and_external_authentication() {
        let database = test_path("generated-bytes-identity.sqlite");
        let mut store = Store::open(&database, FILE_CANDIDATES[0]).expect("open");
        let mut metrics = Metrics::default();
        let value = b"generated payload";
        let expected =
            encode_canonical_object(&Object::bytes(value.to_vec()).expect("owned object"))
                .expect("owned canonical");
        let expected_id = ObjectId::for_bytes(&expected);

        let (actual_id, actual_len) = store
            .put_generated_bytes(value, &mut metrics)
            .expect("generated put");
        assert_eq!(actual_id, expected_id);
        assert_eq!(actual_len, expected.len());
        let stored: Vec<u8> = store
            .connection
            .query_row(
                "SELECT canonical_bytes FROM wp4m_objects WHERE object_id = ?1",
                params![actual_id.as_bytes().as_slice()],
                |row| row.get(0),
            )
            .expect("stored canonical");
        assert_eq!(stored, expected);
        assert_eq!(metrics.borrowed_bytes_encode_calls, 1);
        assert_eq!(
            metrics.borrowed_bytes_encode_input_bytes,
            value.len() as u64
        );
        assert_eq!(metrics.reused_object_id_authentications, 1);
        assert_eq!(
            metrics.reused_object_id_authentication_bytes,
            expected.len() as u64
        );
        assert_eq!(metrics.objects_authenticated, 1);
        assert_eq!(metrics.canonical_id_hashes, 1);
        assert_eq!(metrics.statement_cache_acquisitions, 1);
        assert_eq!(metrics.sql_execute_calls, 1);
        assert_eq!(metrics.sql_query_calls, 0);
        assert_eq!(metrics.sql_rows_changed, 1);
        assert_eq!(metrics.sql_rows_returned, 0);
        assert_eq!(q_current(), 0);

        let wrong_id = ObjectId::for_bytes(b"wrong identity");
        let error = store
            .put(wrong_id, &expected, &mut metrics)
            .expect_err("externally supplied identity must still authenticate");
        assert_eq!(
            error.downcast_ref::<CoreError>(),
            Some(&CoreError::IdentityMismatch)
        );

        drop(store);
        remove_sqlite_image(&database).expect("database cleanup");
    }

    #[test]
    fn candidate_leaf_batches_preserve_duplicates_and_reject_missing_or_mismatched_rows() {
        let database = test_path("leaf-batches.sqlite");
        let candidate = FILE_CANDIDATES[0];
        let mut store = Store::open(&database, candidate).expect("open");
        let mut metrics = Metrics::default();
        let x = make_reference(&mut store, b"x", &mut metrics).expect("x reference");
        let y = make_reference(&mut store, b"y", &mut metrics).expect("y reference");
        let references = [x, y, x];
        let mut observed = Vec::new();
        store
            .for_each_leaf_bytes(
                &references,
                candidate.k,
                &mut metrics,
                &mut |reference, canonical, _| {
                    let raw = layerfs_core::decode_bytes_object(canonical)?;
                    assert_eq!(chunk_id(raw), reference.raw_id);
                    observed.extend_from_slice(raw);
                    Ok(())
                },
            )
            .expect("ordered duplicate batch");
        assert_eq!(observed, b"xyx");
        assert_eq!(metrics.leaf_batch_queries, 1);
        assert_eq!(metrics.leaf_batch_references, 3);
        assert_eq!(metrics.leaf_batch_references_max, 3);
        assert!(metrics.leaf_batch_query_bytes_max > 0);

        let canonical_y = encode_canonical_object(&Object::bytes(b"y".to_vec()).expect("y object"))
            .expect("y canonical");
        store
            .connection
            .execute(
                "UPDATE wp4m_objects SET canonical_bytes = ?1 WHERE object_id = ?2",
                params![canonical_y, x.object_id.as_bytes().as_slice()],
            )
            .expect("mismatch row");
        let error = store
            .for_each_leaf_bytes(
                &references,
                candidate.k,
                &mut metrics,
                &mut |_, _, _| Ok(()),
            )
            .expect_err("mismatched row must fail");
        assert_eq!(
            error.downcast_ref::<CoreError>(),
            Some(&CoreError::IdentityMismatch)
        );

        store
            .connection
            .execute(
                "DELETE FROM wp4m_objects WHERE object_id = ?1",
                params![y.object_id.as_bytes().as_slice()],
            )
            .expect("delete row");
        let error = store
            .for_each_leaf_bytes(
                std::slice::from_ref(&y),
                candidate.k,
                &mut metrics,
                &mut |_, _, _| Ok(()),
            )
            .expect_err("missing row must fail");
        assert_eq!(
            error.downcast_ref::<CandidateError>(),
            Some(&CandidateError::MissingObject(y.object_id))
        );

        drop(store);
        remove_sqlite_image(&database).expect("database cleanup");
    }

    #[test]
    fn candidate_sqlite_matches_memory_range_and_reopens() {
        let raw = b"candidate-shadow-memory-parity";
        let source = test_path("parity-source");
        let database = test_path("parity-db.sqlite");
        fs::write(&source, raw).expect("source");

        let mut cas = InMemoryCas::new();
        let (chunk, _) = cas.put_chunk(raw).expect("memory chunk");
        let logical =
            LogicalFile::from_chunks(&cas, vec![ChunkReference::new(chunk, raw.len() as u64)])
                .expect("memory file");
        let memory = logical.read_range(&cas, 4..24).expect("memory range");

        let candidate = FILE_CANDIDATES[0];
        let mut metrics = Metrics::default();
        let (root, transition) = {
            let mut store = Store::open(&database, candidate).expect("candidate open");
            let (root, transition) =
                build_file(&mut store, &source, candidate, &mut metrics).expect("candidate build");
            let file_root =
                namespace_entry_id(&store, root, b"file", &mut metrics).expect("file root");
            let sqlite = read_file_range(&store, file_root, candidate, 4..24, &mut metrics)
                .expect("sqlite range");
            assert_eq!(sqlite.as_slice(), memory.bytes());
            store
                .publish(None, root, transition, &mut metrics)
                .expect("candidate publish");
            (root, transition)
        };

        let reopened = Store::open(&database, candidate).expect("candidate reopen");
        assert_eq!(
            reopened
                .current_head()
                .expect("head")
                .map(|head| (head.1, head.2)),
            Some((root, transition))
        );
        drop(reopened);
        fs::remove_file(source).expect("source cleanup");
        remove_sqlite_image(&database).expect("database cleanup");

        let boundary_database = test_path("parity-boundaries.sqlite");
        let (store, _, file_root) = build_uniform_base(&boundary_database, candidate);
        let mut cas = InMemoryCas::new();
        let (chunk, _) = cas.put_chunk(b"x").expect("memory chunk");
        let memory = LogicalFile::from_chunks(
            &cas,
            vec![ChunkReference::new(chunk, 1); COW_TEST_REFERENCES as usize],
        )
        .expect("memory boundary file");
        let mut metrics = Metrics::default();
        for range in [
            0..0,
            0..1,
            63..65,
            4_095..4_097,
            4_099..4_100,
            4_100..4_100,
            0..4_100,
        ] {
            assert_eq!(
                read_file_range(&store, file_root, candidate, range.clone(), &mut metrics)
                    .expect("candidate boundary range")
                    .as_slice(),
                memory
                    .read_range(&cas, range)
                    .expect("memory boundary range")
                    .bytes()
            );
        }
        drop(store);
        let store = Store::open(&boundary_database, candidate).expect("boundary reopen");
        assert_eq!(
            read_file_range(&store, file_root, candidate, 4_095..4_097, &mut metrics)
                .expect("reopened branch boundary")
                .as_slice(),
            b"xx"
        );
        drop(store);
        remove_sqlite_image(&boundary_database).expect("boundary cleanup");
    }

    #[test]
    fn measured_edit_starts_from_an_already_published_base() {
        let root = test_path("prepared-edit");
        fs::create_dir_all(&root).expect("root");
        let source = source_path(&root, 256 * 1024);
        let size = (1_u64..=16)
            .map(|multiple| multiple * 256 * 1024)
            .find(|size| {
                fill_source(&source, *size, 0x31).expect("candidate source");
                let point = prepared_edit_point(&source, "same-middle").expect("candidate point");
                exact_same_middle_observations(&source, point).0 == point.reference_count
            })
            .expect("small deterministic same-count fixture");
        let candidate = FILE_CANDIDATES[0];
        prepare_row_database(&root, candidate, size, "same-middle", 0)
            .expect("untimed preparation");
        let database = row_database_path(&root, candidate, size, "same-middle", 0);
        let mut store = Store::open(&database, candidate).expect("prepared store");
        assert_eq!(
            store
                .current_head()
                .expect("prepared head")
                .map(|head| head.0),
            Some(1)
        );

        let expectation_path = expectations_path(&database);
        let expectation_bytes = fs::metadata(&expectation_path)
            .expect("expectation metadata")
            .len();
        let mut prepared_metrics = Metrics::default();
        let ChargedPreparedExpectations {
            value: prepared,
            _charge: prepared_charge,
        } = read_prepared_expectations(&expectation_path, &mut prepared_metrics)
            .expect("prepared expectations");
        let prepared_capacity =
            prepared_expectations_capacity(&prepared).expect("prepared expectation capacity");
        assert_eq!(
            prepared_metrics.q_high_water,
            expectation_bytes
                .checked_add(u64::try_from(prepared_capacity).expect("prepared capacity"))
                .expect("prepared overlap")
        );
        assert_eq!(
            q_current(),
            u64::try_from(prepared_capacity).expect("prepared current")
        );
        let point = prepared.edit_point.expect("prepared edit point");
        let (base_root, base_transition, _) = prepared.base.expect("prepared base");
        let expected_reference_count = prepared
            .expected_reference_count
            .expect("expected reference count");
        let expected_fingerprint = prepared
            .expected_fingerprint
            .as_deref()
            .expect("expected fingerprint");
        let expected_sequence = prepared
            .expected_sequence
            .as_deref()
            .expect("expected sequence");
        let oracle = prepared.edit_oracle.expect("prepared full-rebuild oracle");
        assert_eq!(oracle.operation, "same-middle");
        assert_eq!(oracle.offset, point.byte_offset);
        assert_eq!(oracle.removed.len(), point.replacement_length);
        assert_eq!(oracle.inserted, same_middle_replacement(&oracle.removed));
        fs::remove_file(&source).expect("remove source before measured mutation");
        let mut measured = Metrics::default();
        let expected = oracle.result();
        let capture = capture_same_middle(
            &mut store,
            candidate,
            point,
            &oracle.inserted,
            QualificationMode::ChangedSpine,
            expected,
            expected_reference_count,
            expected_fingerprint,
            expected_sequence,
            base_root,
            base_transition,
            &mut measured,
        )
        .expect("measured transaction");
        let after_file = resolve_namespace_file_root(&store, capture.root_id, &mut measured)
            .expect("result file");
        assert_eq!(
            (capture.root_id, capture.transition_id),
            (oracle.result_root, oracle.result_transition)
        );
        assert_eq!(after_file, oracle.after_file);
        assert_eq!(capture.publication.status, PublicationStatus::Committed);
        assert_eq!(capture.publication.diagnostic, None);
        assert_eq!(measured.transactions, 1);
        assert_eq!(measured.commits, 1);
        assert_eq!(
            measured.q_cdc_overlap_current,
            measured.q_cdc_base_live_bytes
                + measured.q_cdc_old_window_bytes
                + measured.q_cdc_old_chunk_slots_bytes
                + measured.q_cdc_scan_input_bytes
        );
        assert_eq!(measured.q_high_water, measured.q_cdc_overlap_current);
        assert!(
            measured.q_high_water
                > u64::try_from(prepared_capacity).expect("prepared capacity in u64")
        );
        assert_eq!(
            store
                .current_head()
                .expect("result visible")
                .map(|head| head.0),
            Some(2)
        );
        drop(capture);
        drop(prepared_charge);
        assert_eq!(q_current(), 0);
        drop(store);
        remove_sqlite_image(&database).expect("database cleanup");
        fs::remove_dir(root).expect("root cleanup");
    }

    #[test]
    fn cow_edits_reuse_the_unchanged_tree_and_account_for_the_exact_suffix() {
        let candidate = FILE_CANDIDATES[0];

        let same_database = test_path("same-count-cow.sqlite");
        let (mut same_store, parent, before_file) = build_uniform_base(&same_database, candidate);
        let (level, before_root_children) = file_root_children(&same_store, before_file);
        assert_eq!(level, 1);
        let mut before_metrics = Metrics::default();
        let (_before_charge, _, before_branch_bytes) = same_store
            .get(before_root_children[0].object_id, &mut before_metrics)
            .expect("before branch");
        let before_branch = file_codec::parse_file_children(
            file_codec::decode_mapping(&before_branch_bytes, file_codec::FILE_BRANCH_TAG)
                .expect("before branch mapping"),
            true,
        )
        .expect("before branch body")
        .1;
        let mut same_metrics = Metrics::default();
        let (same_root, same_transition) = edit_file(
            &mut same_store,
            candidate,
            "same-middle",
            EditPoint {
                reference_count: COW_TEST_REFERENCES,
                position: COW_TEST_REFERENCES / 2,
                byte_offset: COW_TEST_REFERENCES / 2,
                replacement_length: 1,
            },
            false,
            &mut same_metrics,
        )
        .expect("same-count edit");
        let same_file = namespace_entry_id(&same_store, same_root, b"file", &mut same_metrics)
            .expect("same file");
        let (_, after_root_children) = file_root_children(&same_store, same_file);
        assert_eq!(before_root_children[1], after_root_children[1]);
        let mut after_metrics = Metrics::default();
        let (_after_charge, _, after_branch_bytes) = same_store
            .get(after_root_children[0].object_id, &mut after_metrics)
            .expect("after branch");
        let after_branch = file_codec::parse_file_children(
            file_codec::decode_mapping(&after_branch_bytes, file_codec::FILE_BRANCH_TAG)
                .expect("after branch mapping"),
            true,
        )
        .expect("after branch body")
        .1;
        assert_eq!(
            before_branch
                .iter()
                .zip(&after_branch)
                .filter(|(before, after)| before.object_id != after.object_id)
                .count(),
            1
        );
        assert_eq!(same_metrics.pages, 1);
        assert_eq!(same_metrics.branches, 1);
        let operation = delta_codec::TransitionOperation::Replace {
            path: b"file".to_vec(),
            before: before_file,
            after: same_file,
        };
        verify_transition(
            &same_store,
            same_transition,
            Some(parent),
            same_root,
            Some(std::slice::from_ref(&operation)),
            &mut same_metrics,
        )
        .expect("same transition replay");
        let (fingerprint, sequence) =
            uniform_file_observations(COW_TEST_REFERENCES / 2, 0x5a, false);
        assert_eq!(
            verify_file(
                &same_store,
                same_root,
                candidate,
                Some(&fingerprint),
                Some(&sequence),
                &mut same_metrics,
            )
            .expect("same-count content")
            .1,
            COW_TEST_REFERENCES
        );
        drop(same_store);
        remove_sqlite_image(&same_database).expect("same cleanup");

        for (label, position) in [("early", 0), ("middle", COW_TEST_REFERENCES / 2)] {
            let database = test_path(&format!("plus-one-{label}.sqlite"));
            let (mut store, parent, before_file) = build_uniform_base(&database, candidate);
            let mut metrics = Metrics::default();
            let (root, transition) = edit_file(
                &mut store,
                candidate,
                if position == 0 {
                    "plus1-early"
                } else {
                    "plus1-middle"
                },
                EditPoint {
                    reference_count: COW_TEST_REFERENCES,
                    position,
                    byte_offset: position,
                    replacement_length: 1,
                },
                false,
                &mut metrics,
            )
            .expect("plus-one edit");
            assert_eq!(metrics.suffix_references, COW_TEST_REFERENCES - position);
            assert_eq!(metrics.suffix_bytes, COW_TEST_REFERENCES - position);
            assert_eq!(metrics.references, COW_TEST_REFERENCES - position + 1);
            let (fingerprint, sequence) = uniform_file_observations(position, 0xa5, true);
            assert_eq!(
                verify_file(
                    &store,
                    root,
                    candidate,
                    Some(&fingerprint),
                    Some(&sequence),
                    &mut metrics,
                )
                .expect("plus-one content")
                .1,
                COW_TEST_REFERENCES + 1
            );
            let after_file =
                namespace_entry_id(&store, root, b"file", &mut metrics).expect("after file");
            let operation = delta_codec::TransitionOperation::Replace {
                path: b"file".to_vec(),
                before: before_file,
                after: after_file,
            };
            verify_transition(
                &store,
                transition,
                Some(parent),
                root,
                Some(std::slice::from_ref(&operation)),
                &mut metrics,
            )
            .expect("plus-one transition replay");
            drop(store);
            remove_sqlite_image(&database).expect("plus-one cleanup");
        }
    }

    #[test]
    fn cow_edits_reject_a_corrupt_parent_mapping_before_rewrite() {
        let candidate = FILE_CANDIDATES[0];
        for operation in ["same-middle", "plus1-middle"] {
            let database = test_path(&format!("corrupt-parent-{operation}.sqlite"));
            let (mut store, _, file) = build_uniform_base(&database, candidate);
            store
                .connection
                .execute(
                    "UPDATE wp4m_objects SET canonical_bytes = zeroblob(20) WHERE object_id = ?1",
                    params![file.as_bytes().as_slice()],
                )
                .expect("corrupt parent mapping");
            let mut metrics = Metrics::default();
            let error = edit_file(
                &mut store,
                candidate,
                operation,
                EditPoint {
                    reference_count: COW_TEST_REFERENCES,
                    position: COW_TEST_REFERENCES / 2,
                    byte_offset: COW_TEST_REFERENCES / 2,
                    replacement_length: 1,
                },
                false,
                &mut metrics,
            )
            .expect_err("corrupt parent must fail");
            assert_eq!(
                error.downcast_ref::<CoreError>(),
                Some(&CoreError::IdentityMismatch)
            );
            drop(store);
            remove_sqlite_image(&database).expect("corrupt parent cleanup");
        }
    }

    #[test]
    fn witnessed_changed_spine_authenticates_all_differences_before_commit() {
        let candidate = FILE_CANDIDATES[0];

        let database = test_path("witnessed-spine.sqlite");
        let (mut store, parent, before_file) = build_uniform_base(&database, candidate);
        let mut metrics = Metrics::default();
        let permit = consume_base_witness(&mut store, candidate, &mut metrics);
        let (root, transition) = edit_file(
            &mut store,
            candidate,
            "same-middle",
            EditPoint {
                reference_count: COW_TEST_REFERENCES,
                position: COW_TEST_REFERENCES / 2,
                byte_offset: COW_TEST_REFERENCES / 2,
                replacement_length: 1,
            },
            true,
            &mut metrics,
        )
        .expect("same-count edit");
        let after_file =
            namespace_entry_id(&store, root, b"file", &mut metrics).expect("after file root");
        let operation = delta_codec::TransitionOperation::Replace {
            path: b"file".to_vec(),
            before: before_file,
            after: after_file,
        };
        let (fingerprint, sequence) =
            uniform_file_observations(COW_TEST_REFERENCES / 2, 0x5a, false);
        let transition_digest = verify_transition(
            &store,
            transition,
            Some(parent),
            root,
            Some(std::slice::from_ref(&operation)),
            &mut metrics,
        )
        .expect("full transition oracle");
        let content_digest = verify_file(
            &store,
            root,
            candidate,
            Some(&fingerprint),
            Some(&sequence),
            &mut metrics,
        )
        .expect("full content oracle")
        .0;
        let expected = ExpectedEditResult {
            before_file,
            after_file,
            root,
            transition,
            closure: combined_closure_digest(transition_digest, content_digest),
        };
        let storage_before_qualification = store.physical_snapshot();
        assert_eq!(
            qualify_same_middle_full_closure(
                &store,
                parent,
                root,
                transition,
                std::slice::from_ref(&operation),
                expected,
                candidate,
                &fingerprint,
                &sequence,
                &mut metrics,
            )
            .expect("C0 full qualification"),
            COW_TEST_REFERENCES
        );
        qualify_same_middle_changed_spine(
            &store,
            permit,
            parent,
            root,
            transition,
            std::slice::from_ref(&operation),
            expected,
            candidate,
            &mut metrics,
        )
        .expect("C1 changed-spine qualification");
        assert_eq!(metrics.incremental_qualification_calls, 1);
        assert_eq!(metrics.incremental_prior_spine_objects_authenticated, 4);
        assert_eq!(
            metrics.incremental_replacement_spine_objects_authenticated,
            4
        );
        assert_eq!(metrics.incremental_receipt_covered_edges, 127);
        assert_eq!(metrics.incremental_new_or_different_edges, 4);
        assert_eq!(metrics.incremental_new_subtree_objects_authenticated, 1);
        assert!(metrics.incremental_new_subtree_bytes_authenticated > 0);
        assert_eq!(q_current(), 0);
        assert_eq!(store.physical_snapshot(), storage_before_qualification);
        assert_eq!(metrics.commits, 0);
        assert_eq!(
            store.current_head().expect("prior head").map(|head| head.1),
            Some(parent)
        );
        store
            .connection
            .execute_batch("ROLLBACK")
            .expect("rollback test edit");
        drop(store);
        remove_sqlite_image(&database).expect("spine cleanup");

        let missing_database = test_path("witnessed-spine-missing.sqlite");
        let (mut store, parent, before_file) = build_uniform_base(&missing_database, candidate);
        let mut metrics = Metrics::default();
        let permit = consume_base_witness(&mut store, candidate, &mut metrics);
        let (root, transition) = edit_file(
            &mut store,
            candidate,
            "same-middle",
            EditPoint {
                reference_count: COW_TEST_REFERENCES,
                position: COW_TEST_REFERENCES / 2,
                byte_offset: COW_TEST_REFERENCES / 2,
                replacement_length: 1,
            },
            true,
            &mut metrics,
        )
        .expect("same-count missing edit");
        let after_file =
            namespace_entry_id(&store, root, b"file", &mut metrics).expect("missing after file");
        let operation = delta_codec::TransitionOperation::Replace {
            path: b"file".to_vec(),
            before: before_file,
            after: after_file,
        };
        let (fingerprint, sequence) =
            uniform_file_observations(COW_TEST_REFERENCES / 2, 0x5a, false);
        let transition_digest = verify_transition(
            &store,
            transition,
            Some(parent),
            root,
            Some(std::slice::from_ref(&operation)),
            &mut metrics,
        )
        .expect("missing transition oracle");
        let content_digest = verify_file(
            &store,
            root,
            candidate,
            Some(&fingerprint),
            Some(&sequence),
            &mut metrics,
        )
        .expect("missing content oracle")
        .0;
        let expected = ExpectedEditResult {
            before_file,
            after_file,
            root,
            transition,
            closure: combined_closure_digest(transition_digest, content_digest),
        };
        let replacement_id = canonical_bytes(vec![0x5a])
            .expect("replacement canonical")
            .0;
        store
            .connection
            .execute(
                "DELETE FROM wp4m_objects WHERE object_id = ?1",
                params![replacement_id.as_bytes().as_slice()],
            )
            .expect("delete new chunk");
        let c0 = qualify_same_middle_full_closure(
            &store,
            parent,
            root,
            transition,
            std::slice::from_ref(&operation),
            expected,
            candidate,
            &fingerprint,
            &sequence,
            &mut metrics,
        )
        .expect_err("C0 missing new subtree must fail");
        let error = qualify_same_middle_changed_spine(
            &store,
            permit,
            parent,
            root,
            transition,
            std::slice::from_ref(&operation),
            expected,
            candidate,
            &mut metrics,
        )
        .expect_err("C1 missing new subtree must fail");
        assert_eq!(c0.downcast_ref::<CandidateError>(), error.downcast_ref());
        assert_eq!(
            error.downcast_ref::<CandidateError>(),
            Some(&CandidateError::MissingObject(replacement_id))
        );
        assert_eq!(q_current(), 0);
        assert_eq!(metrics.commits, 0);
        assert_eq!(
            store.current_head().expect("prior head").map(|head| head.1),
            Some(parent)
        );
        store
            .connection
            .execute_batch("ROLLBACK")
            .expect("rollback missing edit");
        drop(store);
        remove_sqlite_image(&missing_database).expect("missing cleanup");
    }

    #[test]
    fn witnessed_spine_handles_multiple_children_final_partial_leaf_and_mode() {
        let candidate = FILE_CANDIDATES[0];
        let database = test_path("witnessed-spine-multiple.sqlite");
        let (mut store, parent, before_file) = build_uniform_base(&database, candidate);
        let mut metrics = Metrics::default();
        let permit = consume_base_witness(&mut store, candidate, &mut metrics);
        let first = make_reference(&mut store, b"a", &mut metrics).expect("first replacement");
        let (once, changed) =
            rewrite_same_root_by_offset(&mut store, before_file, candidate, 0, first, &mut metrics)
                .expect("first rewrite");
        assert!(changed);
        let final_replacement =
            make_reference(&mut store, b"b", &mut metrics).expect("final replacement");
        let (after_file, changed) = rewrite_same_root_by_offset(
            &mut store,
            once,
            candidate,
            COW_TEST_REFERENCES - 1,
            final_replacement,
            &mut metrics,
        )
        .expect("final partial rewrite");
        assert!(changed);
        let root = namespace_file_root(&mut store, after_file, &mut metrics).expect("namespace");
        let operation = delta_codec::TransitionOperation::Replace {
            path: b"file".to_vec(),
            before: before_file,
            after: after_file,
        };
        let transition = publish_transition_with_operations(
            &mut store,
            Some(parent),
            root,
            std::slice::from_ref(&operation),
            &mut metrics,
        )
        .expect("multiple transition");
        let transition_digest = verify_transition(
            &store,
            transition,
            Some(parent),
            root,
            Some(std::slice::from_ref(&operation)),
            &mut metrics,
        )
        .expect("multiple transition proof");
        let (fingerprint, sequence) =
            uniform_file_observations_for_changes(&[(0, b'a'), (COW_TEST_REFERENCES - 1, b'b')]);
        let (content_digest, full_references, _) = verify_file(
            &store,
            root,
            candidate,
            Some(&fingerprint),
            Some(&sequence),
            &mut metrics,
        )
        .expect("multiple full oracle");
        let expected = ExpectedEditResult {
            before_file,
            after_file,
            root,
            transition,
            closure: combined_closure_digest(transition_digest, content_digest),
        };
        assert_eq!(
            qualify_same_middle_full_closure(
                &store,
                parent,
                root,
                transition,
                std::slice::from_ref(&operation),
                expected,
                candidate,
                &fingerprint,
                &sequence,
                &mut metrics,
            )
            .expect("multiple C0 qualification"),
            COW_TEST_REFERENCES
        );
        qualify_same_middle_changed_spine(
            &store,
            permit,
            parent,
            root,
            transition,
            std::slice::from_ref(&operation),
            expected,
            candidate,
            &mut metrics,
        )
        .expect("multiple C1 qualification");
        assert_eq!(metrics.incremental_prior_spine_objects_authenticated, 6);
        assert_eq!(
            metrics.incremental_replacement_spine_objects_authenticated,
            6
        );
        assert_eq!(metrics.incremental_new_or_different_edges, 7);
        assert_eq!(metrics.incremental_new_subtree_objects_authenticated, 2);
        assert_eq!(full_references, COW_TEST_REFERENCES);
        assert_eq!(metrics.commits, 0);
        store
            .connection
            .execute_batch("ROLLBACK")
            .expect("rollback multiple edit");
        drop(store);
        remove_sqlite_image(&database).expect("multiple cleanup");

        let mode_database = test_path("witnessed-spine-mode.sqlite");
        let (mut store, parent, before_file) = build_uniform_base(&mode_database, candidate);
        let mut metrics = Metrics::default();
        let permit = consume_base_witness(&mut store, candidate, &mut metrics);
        let replacement = make_reference(&mut store, b"z", &mut metrics).expect("mode replacement");
        let (after_file, changed) = rewrite_same_root_by_offset(
            &mut store,
            before_file,
            candidate,
            0,
            replacement,
            &mut metrics,
        )
        .expect("mode base rewrite");
        assert!(changed);
        let expected_root =
            namespace_file_root(&mut store, after_file, &mut metrics).expect("expected namespace");
        let expected_operation = delta_codec::TransitionOperation::Replace {
            path: b"file".to_vec(),
            before: before_file,
            after: after_file,
        };
        let expected_transition = publish_transition_with_operations(
            &mut store,
            Some(parent),
            expected_root,
            std::slice::from_ref(&expected_operation),
            &mut metrics,
        )
        .expect("expected transition");
        let expected = ExpectedEditResult {
            before_file,
            after_file,
            root: expected_root,
            transition: expected_transition,
            closure: [0_u8; 32],
        };
        let bytes = store
            .get_bytes(after_file, &mut metrics)
            .expect("after file root");
        let payload =
            file_codec::decode_mapping(&bytes, file_codec::FILE_ROOT_TAG).expect("root mapping");
        let (_, total, references, level, children) =
            file_codec::parse_file_root(payload).expect("root body");
        let mode_file = put_mapping(
            &mut store,
            encode_charged_file_root(1, total, references, level, &children, &mut metrics)
                .expect("mode root"),
            &mut metrics,
        )
        .expect("put mode root");
        let root =
            namespace_file_root(&mut store, mode_file, &mut metrics).expect("mode namespace");
        let error = verify_same_count_changed_spine(&store, permit, root, candidate, &mut metrics)
            .expect_err("mode mismatch must fail before commit");
        let c0 = qualify_same_middle_full_closure(
            &store,
            parent,
            root,
            expected_transition,
            std::slice::from_ref(&expected_operation),
            expected,
            candidate,
            "unused-after-root-mismatch",
            "unused-after-root-mismatch",
            &mut metrics,
        )
        .expect_err("C0 oracle must reject forged mode result");
        assert!(c0.downcast_ref::<CoreError>().is_some());
        assert_eq!(
            error.downcast_ref::<CoreError>(),
            Some(&CoreError::NonCanonicalPagePartition)
        );
        let second = make_reference(&mut store, b"q", &mut metrics).expect("mode-preserving ref");
        let (preserved_mode_file, changed) =
            rewrite_same_root_by_offset(&mut store, mode_file, candidate, 1, second, &mut metrics)
                .expect("mode-preserving rewrite");
        assert!(changed);
        let preserved = store
            .get_bytes(preserved_mode_file, &mut metrics)
            .expect("preserved mode root");
        let preserved = file_codec::decode_mapping(&preserved, file_codec::FILE_ROOT_TAG)
            .expect("preserved mode mapping");
        assert_eq!(
            file_codec::parse_file_root(preserved)
                .expect("preserved mode body")
                .0,
            1
        );
        assert_eq!(metrics.commits, 0);
        assert_eq!(
            store.current_head().expect("prior head").map(|head| head.1),
            Some(parent)
        );
        store
            .connection
            .execute_batch("ROLLBACK")
            .expect("rollback mode edit");
        drop(store);
        remove_sqlite_image(&mode_database).expect("mode cleanup");
    }

    #[test]
    fn changed_spine_accepts_same_count_length_redistribution() {
        let candidate = FILE_CANDIDATES[0];
        let database = test_path("witnessed-spine-redistributed-lengths.sqlite");
        let (mut store, parent, before_file) = build_uniform_base(&database, candidate);
        let mut metrics = Metrics::default();
        let permit = consume_base_witness(&mut store, candidate, &mut metrics);
        let longer = make_reference(&mut store, b"yy", &mut metrics).expect("longer chunk");
        let empty = make_reference(&mut store, b"", &mut metrics).expect("empty chunk");
        let replacements = [longer, empty];
        let after_file = rewrite_same_root_by_ordinal(
            &mut store,
            before_file,
            candidate,
            63,
            &replacements,
            &mut metrics,
        )
        .expect("redistributed same-count rewrite");
        let root = namespace_file_root(&mut store, after_file, &mut metrics).expect("namespace");
        let operation = delta_codec::TransitionOperation::Replace {
            path: b"file".to_vec(),
            before: before_file,
            after: after_file,
        };
        let transition = publish_transition_with_operations(
            &mut store,
            Some(parent),
            root,
            std::slice::from_ref(&operation),
            &mut metrics,
        )
        .expect("transition");
        let mut fingerprint = Hasher::new();
        let mut sequence = Hasher::new();
        for ordinal in 0..COW_TEST_REFERENCES {
            let bytes = match ordinal {
                63 => &b"yy"[..],
                64 => &b""[..],
                _ => &b"x"[..],
            };
            fingerprint.update(bytes);
            sequence.update(
                &u32::try_from(bytes.len())
                    .expect("segment length")
                    .to_be_bytes(),
            );
            sequence.update(chunk_id(bytes).as_bytes());
        }
        let fingerprint = fingerprint.finalize().to_hex().to_string();
        let sequence = sequence.finalize().to_hex().to_string();
        let transition_digest = verify_transition(
            &store,
            transition,
            Some(parent),
            root,
            Some(std::slice::from_ref(&operation)),
            &mut metrics,
        )
        .expect("transition proof");
        let content_digest = verify_file(
            &store,
            root,
            candidate,
            Some(&fingerprint),
            Some(&sequence),
            &mut metrics,
        )
        .expect("full content proof")
        .0;
        let expected = ExpectedEditResult {
            before_file,
            after_file,
            root,
            transition,
            closure: combined_closure_digest(transition_digest, content_digest),
        };
        qualify_same_middle_changed_spine(
            &store,
            permit,
            parent,
            root,
            transition,
            std::slice::from_ref(&operation),
            expected,
            candidate,
            &mut metrics,
        )
        .expect("changed spine must authenticate redistributed lengths");
        store.rollback(&mut metrics).expect("rollback");
        drop(store);
        remove_sqlite_image(&database).expect("database cleanup");
    }

    #[test]
    fn deep_changed_spine_proves_height_union_and_bounded_qualification() {
        let candidate = FILE_CANDIDATES[0];
        let database = test_path("witnessed-spine-deep.sqlite");
        let (mut store, parent, before_file) = build_deep_uniform_base(&database, candidate);
        let (height, root_children) = file_root_children(&store, before_file);
        assert_eq!(height, 2);
        assert_eq!(root_children.len(), 2);
        assert_eq!(
            file_codec::expected_file_level(
                DEEP_COW_TEST_REFERENCES,
                file_codec::FileMappingProfile::new(candidate.k, candidate.f),
            ),
            Ok(height)
        );

        let mut setup_metrics = Metrics::default();
        let permit = consume_base_witness(&mut store, candidate, &mut setup_metrics);
        let replacements = [
            make_reference(&mut store, b"a", &mut setup_metrics).expect("replacement a"),
            make_reference(&mut store, b"b", &mut setup_metrics).expect("replacement b"),
        ];
        let after_crossing_leaf = rewrite_same_root_by_ordinal(
            &mut store,
            before_file,
            candidate,
            63,
            &replacements,
            &mut setup_metrics,
        )
        .expect("cross-leaf rewrite");
        let first_inner_leaf = make_reference(&mut store, b"c", &mut setup_metrics)
            .expect("first inner-leaf replacement");
        let after_inner_branch = rewrite_same_root_by_ordinal(
            &mut store,
            after_crossing_leaf,
            candidate,
            64 * 64,
            &[first_inner_leaf],
            &mut setup_metrics,
        )
        .expect("first leaf of second inner branch");
        let final_partial = make_reference(&mut store, b"d", &mut setup_metrics)
            .expect("final partial-leaf replacement");
        let after_file = rewrite_same_root_by_ordinal(
            &mut store,
            after_inner_branch,
            candidate,
            DEEP_COW_TEST_REFERENCES - 1,
            &[final_partial],
            &mut setup_metrics,
        )
        .expect("final partial-leaf rewrite");
        let root = namespace_file_root(&mut store, after_file, &mut setup_metrics)
            .expect("deep replacement namespace");
        let operation = delta_codec::TransitionOperation::Replace {
            path: b"file".to_vec(),
            before: before_file,
            after: after_file,
        };
        let transition = publish_transition_with_operations(
            &mut store,
            Some(parent),
            root,
            std::slice::from_ref(&operation),
            &mut setup_metrics,
        )
        .expect("deep replacement transition");
        assert_eq!(q_current(), 216);

        let (fingerprint, sequence) = uniform_file_observations_for_reference_count(
            DEEP_COW_TEST_REFERENCES,
            &[
                (63, b'a'),
                (64, b'b'),
                (64 * 64, b'c'),
                (DEEP_COW_TEST_REFERENCES - 1, b'd'),
            ],
        );
        let mut oracle_metrics = Metrics::default();
        let transition_digest = verify_transition(
            &store,
            transition,
            Some(parent),
            root,
            Some(std::slice::from_ref(&operation)),
            &mut oracle_metrics,
        )
        .expect("deep transition oracle");
        let (content_digest, references, total) = verify_file(
            &store,
            root,
            candidate,
            Some(&fingerprint),
            Some(&sequence),
            &mut oracle_metrics,
        )
        .expect("deep full oracle");
        assert_eq!(
            (references, total),
            (DEEP_COW_TEST_REFERENCES, DEEP_COW_TEST_REFERENCES)
        );
        assert_eq!(q_current(), 216);
        let expected = ExpectedEditResult {
            before_file,
            after_file,
            root,
            transition,
            closure: combined_closure_digest(transition_digest, content_digest),
        };

        let mut c0_metrics = Metrics::default();
        assert_eq!(
            qualify_same_middle_full_closure(
                &store,
                parent,
                root,
                transition,
                std::slice::from_ref(&operation),
                expected,
                candidate,
                &fingerprint,
                &sequence,
                &mut c0_metrics,
            )
            .expect("deep C0 qualification"),
            DEEP_COW_TEST_REFERENCES,
        );
        assert_eq!(c0_metrics.closure_occurrences, 266_309);
        assert!(c0_metrics.sql_query_calls > DEEP_COW_TEST_REFERENCES);
        assert_eq!(q_current(), 216);

        let mut c1_metrics = Metrics::default();
        qualify_same_middle_changed_spine(
            &store,
            permit,
            parent,
            root,
            transition,
            std::slice::from_ref(&operation),
            expected,
            candidate,
            &mut c1_metrics,
        )
        .expect("deep C1 qualification");
        const CHANGED_LEAF_UNION: u64 = 4;
        const CHANGED_BRANCH_UNION: u64 = 5;
        assert_eq!(
            c1_metrics.incremental_prior_spine_objects_authenticated,
            2 + CHANGED_BRANCH_UNION + CHANGED_LEAF_UNION,
        );
        assert_eq!(
            c1_metrics.incremental_replacement_spine_objects_authenticated,
            2 + CHANGED_BRANCH_UNION + CHANGED_LEAF_UNION,
        );
        assert_eq!(c1_metrics.incremental_receipt_covered_edges, 376);
        assert_eq!(c1_metrics.incremental_new_or_different_edges, 14);
        assert_eq!(c1_metrics.incremental_new_subtree_objects_authenticated, 4);
        assert_eq!(c1_metrics.closure_occurrences, 0);
        assert_eq!(c1_metrics.leaf_batch_queries, 0);
        assert!(c1_metrics.sql_query_calls < 100);
        assert_eq!((usize::from(height) + 3) * Q_DFS_FRAME_BYTES * 2, 640,);
        assert_eq!(c1_metrics.q_high_water, 43_488);
        println!(
            "deep H={height} leaves={CHANGED_LEAF_UNION} branches={CHANGED_BRANCH_UNION} covered={} different={} q_high_water={} c0_queries={} c1_queries={}",
            c1_metrics.incremental_receipt_covered_edges,
            c1_metrics.incremental_new_or_different_edges,
            c1_metrics.q_high_water,
            c0_metrics.sql_query_calls,
            c1_metrics.sql_query_calls,
        );
        assert_eq!(q_current(), 0);
        finish_q(&mut c1_metrics).expect("deep C1 terminal Q");
        store
            .rollback(&mut setup_metrics)
            .expect("rollback deep valid edit");

        let mut malformed_setup = Metrics::default();
        let malformed_permit = consume_base_witness(&mut store, candidate, &mut malformed_setup);
        let malformed_file = {
            let root_bytes = store
                .get_bytes(before_file, &mut malformed_setup)
                .expect("deep base root bytes");
            let root_payload = file_codec::decode_mapping(&root_bytes, file_codec::FILE_ROOT_TAG)
                .expect("deep base root mapping");
            let (mode, total, references, level, mut root_children) =
                file_codec::parse_file_root(root_payload).expect("deep base root body");
            let level_two_bytes = store
                .get_bytes(root_children[0].object_id, &mut malformed_setup)
                .expect("level-two bytes");
            let level_two_payload =
                file_codec::decode_mapping(&level_two_bytes, file_codec::FILE_BRANCH_TAG)
                    .expect("level-two mapping");
            let (_, mut level_two_children) =
                file_codec::parse_file_children(level_two_payload, true).expect("level-two body");
            let level_one_bytes = store
                .get_bytes(level_two_children[0].object_id, &mut malformed_setup)
                .expect("level-one bytes");
            let level_one_payload =
                file_codec::decode_mapping(&level_one_bytes, file_codec::FILE_BRANCH_TAG)
                    .expect("level-one mapping");
            let (_, mut level_one_children) =
                file_codec::parse_file_children(level_one_payload, true).expect("level-one body");
            level_one_children
                .last_mut()
                .expect("last level-one child")
                .cumulative_end -= 1;
            let malformed_level_one = put_mapping(
                &mut store,
                encode_charged_file_branch(1, &level_one_children, &mut malformed_setup)
                    .expect("malformed level-one encoding"),
                &mut malformed_setup,
            )
            .expect("put malformed level-one branch");
            level_two_children[0].object_id = malformed_level_one;
            let malformed_level_two = put_mapping(
                &mut store,
                encode_charged_file_branch(2, &level_two_children, &mut malformed_setup)
                    .expect("malformed level-two encoding"),
                &mut malformed_setup,
            )
            .expect("put malformed level-two branch");
            root_children[0].object_id = malformed_level_two;
            put_mapping(
                &mut store,
                encode_charged_file_root(
                    mode,
                    total,
                    references,
                    level,
                    &root_children,
                    &mut malformed_setup,
                )
                .expect("malformed deep root encoding"),
                &mut malformed_setup,
            )
            .expect("put malformed deep root")
        };
        assert_eq!(q_current(), 216);
        let malformed_root = namespace_file_root(&mut store, malformed_file, &mut malformed_setup)
            .expect("malformed deep namespace");
        let malformed_operation = delta_codec::TransitionOperation::Replace {
            path: b"file".to_vec(),
            before: before_file,
            after: malformed_file,
        };
        let malformed_transition = publish_transition_with_operations(
            &mut store,
            Some(parent),
            malformed_root,
            std::slice::from_ref(&malformed_operation),
            &mut malformed_setup,
        )
        .expect("malformed deep transition");
        let malformed_expected = ExpectedEditResult {
            before_file,
            after_file: malformed_file,
            root: malformed_root,
            transition: malformed_transition,
            closure: [0_u8; 32],
        };
        let c0_error = qualify_same_middle_full_closure(
            &store,
            parent,
            malformed_root,
            malformed_transition,
            std::slice::from_ref(&malformed_operation),
            malformed_expected,
            candidate,
            "unused-before-deep-summary-failure",
            "unused-before-deep-summary-failure",
            &mut Metrics::default(),
        )
        .expect_err("deep C0 must reject malformed cumulative summary");
        let c1_error = qualify_same_middle_changed_spine(
            &store,
            malformed_permit,
            parent,
            malformed_root,
            malformed_transition,
            std::slice::from_ref(&malformed_operation),
            malformed_expected,
            candidate,
            &mut Metrics::default(),
        )
        .expect_err("deep C1 must reject malformed cumulative summary");
        assert!(matches!(
            c0_error.downcast_ref::<CoreError>(),
            Some(CoreError::LengthMismatch { .. })
        ));
        assert!(matches!(
            c1_error.downcast_ref::<CoreError>(),
            Some(CoreError::LengthMismatch { .. })
        ));
        assert_eq!(q_current(), 0);
        assert_eq!(
            store
                .current_head()
                .expect("deep prior head")
                .map(|head| head.1),
            Some(parent)
        );
        store
            .rollback(&mut malformed_setup)
            .expect("rollback malformed deep edit");
        drop(store);
        remove_sqlite_image(&database).expect("deep database cleanup");
    }

    #[test]
    fn full_and_incremental_shadow_both_reject_malformed_summary() {
        let candidate = FILE_CANDIDATES[0];
        let database = test_path("witnessed-spine-malformed.sqlite");
        let (mut store, parent, before_file) = build_uniform_base(&database, candidate);
        let mut metrics = Metrics::default();
        let permit = consume_base_witness(&mut store, candidate, &mut metrics);
        let (valid_root, _) = edit_file(
            &mut store,
            candidate,
            "same-middle",
            EditPoint {
                reference_count: COW_TEST_REFERENCES,
                position: COW_TEST_REFERENCES / 2,
                byte_offset: COW_TEST_REFERENCES / 2,
                replacement_length: 1,
            },
            true,
            &mut metrics,
        )
        .expect("valid edit");
        let valid_file =
            namespace_entry_id(&store, valid_root, b"file", &mut metrics).expect("valid file");
        let bytes = store
            .get_bytes(valid_file, &mut metrics)
            .expect("valid file bytes");
        let payload =
            file_codec::decode_mapping(&bytes, file_codec::FILE_ROOT_TAG).expect("valid mapping");
        let (mode, total, references, level, mut children) =
            file_codec::parse_file_root(payload).expect("valid root");
        children.last_mut().expect("root child").cumulative_end =
            total.checked_sub(1).expect("malformed cumulative end");
        let malformed_file = put_mapping(
            &mut store,
            encode_charged_file_root(mode, total, references, level, &children, &mut metrics)
                .expect("malformed root bytes"),
            &mut metrics,
        )
        .expect("put malformed root");
        let malformed_root = namespace_file_root(&mut store, malformed_file, &mut metrics)
            .expect("malformed namespace");
        let c0 = verify_file(&store, malformed_root, candidate, None, None, &mut metrics)
            .expect_err("C0 must reject malformed cumulative summary");
        let c1 = verify_same_count_changed_spine(
            &store,
            permit,
            malformed_root,
            candidate,
            &mut metrics,
        )
        .expect_err("C1 must reject malformed cumulative summary");
        assert!(c0.downcast_ref::<CoreError>().is_some());
        assert!(c1.downcast_ref::<CoreError>().is_some());
        assert_eq!(metrics.commits, 0);
        assert_eq!(
            store.current_head().expect("prior head").map(|head| head.1),
            Some(parent)
        );
        assert_ne!(before_file, malformed_file);
        store
            .connection
            .execute_batch("ROLLBACK")
            .expect("rollback malformed edit");
        drop(store);
        remove_sqlite_image(&database).expect("malformed cleanup");
    }

    #[test]
    fn same_open_witness_requires_full_scrub_and_is_exactly_single_use() {
        let candidate = FILE_CANDIDATES[0];
        let database = test_path("same-open-witness.sqlite");
        let (mut store, root, file) = build_uniform_base(&database, candidate);
        let head = store.current_head().expect("head").expect("visible head");
        let mut metrics = Metrics::default();

        let error =
            establish_same_open_file_witness(&mut store, candidate, None, None, &mut metrics)
                .expect_err("Store-open state without a writer transaction is insufficient");
        assert_eq!(
            error.downcast_ref::<CoreError>(),
            Some(&CoreError::ValidationAuthorityUnavailable)
        );
        store.begin(&mut metrics).expect("begin exact witness");
        let mut witness =
            establish_same_open_file_witness(&mut store, candidate, None, None, &mut metrics)
                .expect("same-open full scrub");
        let permit = witness
            .consume(&store, &mut metrics)
            .expect("exact witness consumption");
        assert_eq!(permit.open_identity, store.open_identity);
        assert_eq!(permit.store_instance_id, store.store_instance_id);
        assert_eq!(
            permit.validation_authority_id,
            store.validation_authority_id
        );
        assert_eq!(permit.integrity_epoch, store.integrity_epoch);
        assert_eq!(permit.profile, store.profile);
        assert_eq!(permit.generation, head.0);
        assert_eq!(permit.root, head.1);
        assert_eq!(permit.transition, head.2);
        assert_eq!(permit.receipt.as_slice(), head.3.as_slice());
        assert_eq!(permit.authority_serial, store.same_open_authority_serial);
        assert_eq!(
            Some(permit.transaction_identity),
            store
                .active_transaction
                .map(|transaction| transaction.identity)
        );
        let error = witness
            .consume(&store, &mut metrics)
            .expect_err("witness reuse must fail");
        assert_eq!(
            error.downcast_ref::<CoreError>(),
            Some(&CoreError::ValidationAuthorityUnavailable)
        );
        store
            .rollback(&mut metrics)
            .expect("rollback exact witness");

        store.begin(&mut metrics).expect("begin reopen witness");
        let mut reopened_witness =
            establish_same_open_file_witness(&mut store, candidate, None, None, &mut metrics)
                .expect("witness before reopen");
        drop(store);
        let mut store = Store::open(&database, candidate).expect("reopen");
        let error = reopened_witness
            .consume(&store, &mut metrics)
            .expect_err("cross-reopen witness must fail");
        assert_eq!(
            error.downcast_ref::<CoreError>(),
            Some(&CoreError::ValidationAuthorityUnavailable)
        );

        for mismatch in 0..7 {
            store.begin(&mut metrics).expect("begin mismatch witness");
            let mut witness =
                establish_same_open_file_witness(&mut store, candidate, None, None, &mut metrics)
                    .expect("fresh exact witness");
            match mismatch {
                0 => witness.store_instance_id[0] ^= 1,
                1 => witness.validation_authority_id[0] ^= 1,
                2 => {
                    witness.integrity_epoch = witness.integrity_epoch.checked_add(1).expect("epoch")
                }
                3 => witness.profile[0] ^= 1,
                4 => witness.generation = witness.generation.checked_add(1).expect("generation"),
                5 => witness.root = ObjectId::for_bytes(b"wrong-root"),
                6 => witness.receipt[215] ^= 1,
                _ => unreachable!(),
            }
            let error = witness
                .consume(&store, &mut metrics)
                .expect_err("tuple mismatch must fail");
            assert_eq!(
                error.downcast_ref::<CoreError>(),
                Some(&CoreError::InvalidValidationReceipt)
            );
            let error = witness
                .consume(&store, &mut metrics)
                .expect_err("mismatched witness must stay invalid");
            assert_eq!(
                error.downcast_ref::<CoreError>(),
                Some(&CoreError::ValidationAuthorityUnavailable)
            );
            store
                .rollback(&mut metrics)
                .expect("rollback mismatch witness");
        }

        store
            .begin(&mut metrics)
            .expect("begin invalidated witness");
        let mut invalidated =
            establish_same_open_file_witness(&mut store, candidate, None, None, &mut metrics)
                .expect("witness before authority mutation");
        store.same_open_authority_serial = store
            .same_open_authority_serial
            .checked_add(1)
            .expect("invalidate authority");
        let error = invalidated
            .consume(&store, &mut metrics)
            .expect_err("authority mutation must invalidate witness");
        assert_eq!(
            error.downcast_ref::<CoreError>(),
            Some(&CoreError::ValidationAuthorityUnavailable)
        );
        store
            .rollback(&mut metrics)
            .expect("rollback invalidated witness");

        store
            .connection
            .execute(
                "DELETE FROM wp4m_objects WHERE object_id = ?1",
                params![file.as_bytes().as_slice()],
            )
            .expect("delete unchanged mapping before scrub");
        store.begin(&mut metrics).expect("begin failed scrub");
        let error =
            establish_same_open_file_witness(&mut store, candidate, None, None, &mut metrics)
                .expect_err("incomplete closure must prevent witness issuance");
        assert_eq!(
            error.downcast_ref::<CandidateError>(),
            Some(&CandidateError::MissingObject(file))
        );
        assert_eq!(
            store
                .current_head()
                .expect("head remains readable")
                .map(|head| head.1),
            Some(root)
        );
        store.rollback(&mut metrics).expect("rollback failed scrub");

        drop(store);
        remove_sqlite_image(&database).expect("database cleanup");
    }

    #[test]
    fn witness_requires_the_exact_complete_namespace_closure() {
        let candidate = FILE_CANDIDATES[0];
        let database = test_path("witness-complete-namespace.sqlite");
        let mut store = Store::open(&database, candidate).expect("open");
        let mut metrics = Metrics::default();
        store.begin(&mut metrics).expect("begin malformed base");
        let file = empty_file_root(&mut store, &mut metrics).expect("empty file");
        let missing = ObjectId::for_bytes(b"missing-extra-namespace-edge");
        let entries = vec![
            DirectoryEntry::new(
                CanonicalName::from_bytes(b"extra").expect("extra name"),
                ObjectReference::new(ObjectKind::Bytes, missing),
            ),
            DirectoryEntry::new(
                CanonicalName::from_bytes(b"file").expect("file name"),
                ObjectReference::new(ObjectKind::Bytes, file),
            ),
        ];
        let canonical = encode_canonical_object(
            &Object::directory(entries).expect("malformed namespace object"),
        )
        .expect("namespace canonical");
        let root = ObjectId::for_bytes(&canonical);
        store
            .put(root, &canonical, &mut metrics)
            .expect("store malformed namespace");
        let transition =
            publish_transition(&mut store, None, root, &mut metrics).expect("transition");
        store
            .publish(None, root, transition, &mut metrics)
            .expect("publish malformed namespace fixture");

        store.begin(&mut metrics).expect("begin witness attempt");
        let error =
            establish_same_open_file_witness(&mut store, candidate, None, None, &mut metrics)
                .expect_err("extra namespace edge must prevent witness issuance");
        assert_eq!(
            error.downcast_ref::<CoreError>(),
            Some(&CoreError::WrongLogicalRole)
        );
        store
            .rollback(&mut metrics)
            .expect("rollback witness attempt");
        drop(store);
        remove_sqlite_image(&database).expect("database cleanup");
    }

    #[test]
    fn failed_rollback_invalidates_an_unconsumed_witness() {
        let candidate = FILE_CANDIDATES[0];
        let database = test_path("failed-rollback-witness.sqlite");
        let (mut store, _, _) = build_uniform_base(&database, candidate);
        let mut metrics = Metrics::default();
        store
            .begin(&mut metrics)
            .expect("begin witness transaction");
        let mut witness =
            establish_same_open_file_witness(&mut store, candidate, None, None, &mut metrics)
                .expect("issue witness");
        store
            .connection
            .execute_batch("COMMIT")
            .expect("end SQLite transaction behind cleanup owner");
        store
            .rollback(&mut metrics)
            .expect_err("ROLLBACK without an SQLite transaction must fail");
        assert!(store.active_transaction.is_none());
        let error = witness
            .consume(&store, &mut metrics)
            .expect_err("failed rollback must invalidate transaction authority");
        assert_eq!(
            error.downcast_ref::<CoreError>(),
            Some(&CoreError::ValidationAuthorityUnavailable)
        );
        drop(store);
        remove_sqlite_image(&database).expect("database cleanup");
    }

    #[test]
    fn publication_faults_record_reconciliation_and_require_private_authority() {
        let source = test_path("fault-source");
        fs::write(&source, b"publication-fault-test").expect("source");
        let candidate = FILE_CANDIDATES[0];

        let before_database = test_path("fault-before.sqlite");
        {
            let mut store = Store::open(&before_database, candidate).expect("open");
            let mut metrics = Metrics::default();
            let result = build_file(&mut store, &source, candidate, &mut metrics).expect("build");
            let provenance = store
                .publish_with_fault(
                    None,
                    result.0,
                    result.1,
                    Some(PublishFault::BeforeCommit),
                    &mut metrics,
                )
                .expect("fault provenance");
            assert_eq!(provenance.first, Some(FailureCause::Core(CoreError::Io)));
            assert_eq!(provenance.reconciliation, Reconciliation::NotAttempted);
            assert_eq!(provenance.cleanup_first, None);
            assert_eq!(provenance.dominant, Some(FailureCause::Core(CoreError::Io)));
        }
        let reopened = Store::open(&before_database, candidate).expect("reopen");
        assert!(reopened.current_head().expect("head").is_none());
        drop(reopened);
        remove_sqlite_image(&before_database).expect("before cleanup");

        let after_database = test_path("fault-after.sqlite");
        let authority_key;
        {
            let mut store = Store::open(&after_database, candidate).expect("open");
            let mut metrics = Metrics::default();
            let result = build_file(&mut store, &source, candidate, &mut metrics).expect("build");
            let provenance = store
                .publish_with_fault(
                    None,
                    result.0,
                    result.1,
                    Some(PublishFault::AfterCommitBeforeAck),
                    &mut metrics,
                )
                .expect("fault provenance");
            assert_eq!(provenance.first, Some(FailureCause::Core(CoreError::Io)));
            assert_eq!(provenance.reconciliation, Reconciliation::RequestedVisible);
            assert_eq!(provenance.dominant, None);

            let requested = store.current_head().expect("head").expect("visible head");
            let requested_key = store
                .publication_key(None, &requested)
                .expect("requested key");
            assert_eq!(
                store.reconcile_publication(None, &requested, requested_key),
                Reconciliation::RequestedVisible
            );
            let mut wrong_requested_key = requested_key;
            wrong_requested_key[0] ^= 1;
            assert_eq!(
                store.reconcile_publication(None, &requested, wrong_requested_key),
                Reconciliation::Ambiguous
            );
            let (wrong_key_reconciliation, wrong_key_error) = store
                .reconcile_publication_accounted(
                    None,
                    &requested,
                    wrong_requested_key,
                    &mut metrics,
                );
            assert_eq!(wrong_key_reconciliation, Reconciliation::Ambiguous);
            assert_eq!(
                wrong_key_error,
                Some(FailureCause::Core(CoreError::InvalidValidationReceipt))
            );

            let next_child = ObjectId::for_bytes(b"different-child");
            let next_transition = ObjectId::for_bytes(b"different-transition");
            let next_generation = requested.0.checked_add(1).expect("generation");
            let next_receipt = ValidatedSnapshotReceiptV1 {
                store_instance_id: store.store_instance_id,
                validation_authority_id: store.validation_authority_id,
                integrity_epoch: store.integrity_epoch,
                head_generation: next_generation,
                child_root_id: next_child,
                transition_id: next_transition,
                mapping_profile_id: ObjectId::from_bytes(&store.profile).expect("profile"),
            }
            .encode(&store.validation_key)
            .expect("next receipt");
            assert_eq!(next_receipt.len(), 216);
            let next = (next_generation, next_child, next_transition, next_receipt);
            let next_key = store
                .publication_key(Some(&requested), &next)
                .expect("next key");
            assert_eq!(
                store.reconcile_publication(Some(&requested), &next, next_key),
                Reconciliation::PriorVisible
            );
            assert_eq!(
                store.reconcile_publication(
                    None,
                    &next,
                    store.publication_key(None, &next).expect("different key")
                ),
                Reconciliation::DifferentHead
            );

            for malformed in [vec![0_u8; 215], vec![0_u8; 217]] {
                store
                    .connection
                    .execute(
                        "UPDATE wp4m_visible_head SET validation_receipt = ?1 WHERE id = 1",
                        params![malformed],
                    )
                    .expect("malformed receipt");
                let error = store.current_head().expect_err("receipt length must fail");
                assert_eq!(
                    error.downcast_ref::<CoreError>(),
                    Some(&CoreError::InvalidValidationReceipt)
                );
            }

            let mut corrupt_receipt = requested.3;
            corrupt_receipt[215] ^= 1;
            store
                .connection
                .execute(
                    "UPDATE wp4m_visible_head SET validation_receipt = ?1 WHERE id = 1",
                    params![corrupt_receipt.as_slice()],
                )
                .expect("corrupt receipt");
            let error = store.current_head().expect_err("corrupt MAC must fail");
            assert_eq!(
                error.downcast_ref::<CoreError>(),
                Some(&CoreError::InvalidValidationReceipt)
            );

            store
                .connection
                .execute(
                    "UPDATE wp4m_visible_head SET validation_receipt = ?1 WHERE id = 1",
                    params![next.3.as_slice()],
                )
                .expect("mismatched receipt");
            let error = store
                .current_head()
                .expect_err("mismatched receipt must fail");
            assert_eq!(
                error.downcast_ref::<CoreError>(),
                Some(&CoreError::InvalidValidationReceipt)
            );
            assert_eq!(
                store.reconcile_publication(None, &requested, requested_key),
                Reconciliation::Ambiguous
            );
            authority_key = store.validation_key;
        }
        let authority = authority_path(&after_database);
        fs::remove_file(&authority).expect("authority removal");
        let error = Store::open(&after_database, candidate)
            .err()
            .expect("missing authority must fail");
        assert_eq!(
            error.downcast_ref::<CoreError>(),
            Some(&CoreError::ValidationAuthorityUnavailable)
        );
        fs::write(&authority, [0_u8; 32]).expect("corrupt authority");
        let error = Store::open(&after_database, candidate)
            .err()
            .expect("corrupt authority must fail");
        assert_eq!(
            error.downcast_ref::<CoreError>(),
            Some(&CoreError::InvalidValidationReceipt)
        );
        fs::write(&authority, authority_key).expect("restore authority");
        remove_sqlite_image(&after_database).expect("after cleanup");

        for (reconciliation, dominant) in [
            (
                Reconciliation::NotAttempted,
                Some(FailureCause::Core(CoreError::Io)),
            ),
            (Reconciliation::RequestedVisible, None),
            (
                Reconciliation::PriorVisible,
                Some(FailureCause::Core(CoreError::Io)),
            ),
            (
                Reconciliation::DifferentHead,
                Some(FailureCause::Core(CoreError::PublicationConflict)),
            ),
            (
                Reconciliation::Ambiguous,
                Some(FailureCause::Core(CoreError::AmbiguousDurability)),
            ),
        ] {
            let provenance = failure_provenance(
                Some(FailureCause::Core(CoreError::Io)),
                Some(FailureCause::Core(CoreError::LengthOverflow)),
                reconciliation,
                None,
            );
            assert_eq!(provenance.first, Some(FailureCause::Core(CoreError::Io)));
            assert_eq!(
                provenance.cleanup_first,
                Some(FailureCause::Core(CoreError::LengthOverflow))
            );
            assert_eq!(provenance.reconciliation, reconciliation);
            assert_eq!(provenance.reconciliation_error, None);
            assert_eq!(provenance.dominant, dominant);
        }
        fs::remove_file(source).expect("source cleanup");
    }

    #[test]
    fn f1_commit_observations_separate_dispatch_return_and_reconciliation() {
        let source = test_path("f1-commit-observation-source");
        let database = test_path("f1-commit-observation.sqlite");
        fs::write(&source, b"f1 commit observation").expect("source");
        let candidate = FILE_CANDIDATES[0];
        let mut store = Store::open(&database, candidate).expect("open");
        let mut metrics = Metrics::default();
        let physical_before = store.physical_snapshot();
        assert_eq!(physical_before.measurement_sql_queries, 2);
        assert_eq!(physical_before.measurement_sql_rows, 2);
        assert_eq!(metrics.sql_query_calls, 0);
        assert_eq!(metrics.statement_cache_acquisitions, 0);
        store.start_sqlite_observations(&mut metrics);
        assert_eq!(metrics.measurement_sql_queries, 1);
        assert_eq!(metrics.measurement_sql_rows, 1);
        assert_eq!(metrics.measurement_status_reset_calls, 4);
        assert_eq!(metrics.measurement_status_reset_errors, 0);
        assert_eq!(metrics.sqlite_status_before.read_calls, 5);
        assert_eq!(metrics.sqlite_status_before.errors, 0);
        let (root, transition) =
            build_file(&mut store, &source, candidate, &mut metrics).expect("build");
        let publication = store
            .publish(None, root, transition, &mut metrics)
            .expect("publish");
        assert_eq!(publication.status, PublicationStatus::Committed);
        assert_eq!(metrics.transactions, 1);
        assert_eq!(metrics.commits, 1);
        assert_eq!(metrics.commit_returns, 1);
        assert_eq!(metrics.commit_return_successes, 1);
        assert_eq!(metrics.commit_return_errors, 0);
        assert_eq!(metrics.commit_reconciliation_calls, 0);
        assert_eq!(metrics.commit_reconciliation_wall_ns, 0);
        assert!(metrics.commit_publish_call_wall_ns >= metrics.commit_dispatch_to_return_wall_ns);
        assert!(metrics.sqlite_status_before.page_cache_used_bytes.is_some());
        assert!(metrics
            .sqlite_status_before_dispatch
            .page_cache_used_bytes
            .is_some());
        assert!(metrics
            .sqlite_status_after_return
            .dirty_pages_written
            .is_some());
        assert_eq!(metrics.sqlite_status_before_dispatch.read_calls, 5);
        assert_eq!(metrics.sqlite_status_before_dispatch.errors, 0);
        assert_eq!(metrics.sqlite_status_after_return.read_calls, 5);
        assert_eq!(metrics.sqlite_status_after_return.errors, 0);
        assert!(metrics
            .commit_dispatch_filesystem
            .apparent_journal
            .is_some());
        assert!(metrics.commit_return_filesystem.apparent_journal.is_some());
        let workload_queries = metrics.sql_query_calls;
        let workload_acquisitions = metrics.statement_cache_acquisitions;
        let physical_after = store.physical_snapshot();
        assert_eq!(physical_after.measurement_sql_queries, 2);
        assert_eq!(physical_after.measurement_sql_rows, 2);
        assert_eq!(metrics.sql_query_calls, workload_queries);
        assert_eq!(metrics.statement_cache_acquisitions, workload_acquisitions);
        assert_eq!(
            physical_before.measurement_sql_queries
                + metrics.measurement_sql_queries
                + physical_after.measurement_sql_queries,
            5
        );
        assert_eq!(
            physical_before.measurement_sql_rows
                + metrics.measurement_sql_rows
                + physical_after.measurement_sql_rows,
            5
        );
        assert_eq!(
            metrics.measurement_status_reset_calls
                + metrics.sqlite_status_before.read_calls
                + metrics.sqlite_status_before_dispatch.read_calls
                + metrics.sqlite_status_after_return.read_calls,
            19
        );
        assert_eq!(
            checked_sqlite_status_current(ffi::SQLITE_ERROR, 0),
            Err(SqliteStatusError::Sqlite(ffi::SQLITE_ERROR))
        );
        assert_eq!(
            checked_sqlite_status_current(ffi::SQLITE_OK, -1),
            Err(SqliteStatusError::CurrentOutOfRange(-1))
        );
        validate_metric_equations(metrics).expect("observation equations");
        assert_eq!(q_current(), 0);

        let mut predispatch = Metrics::default();
        store
            .begin(&mut predispatch)
            .expect("begin rejected publish");
        let error = store
            .publish(None, root, transition, &mut predispatch)
            .expect_err("duplicate genesis must fail before COMMIT");
        assert_eq!(
            error.downcast_ref::<CoreError>(),
            Some(&CoreError::PublicationConflict)
        );
        assert_eq!(predispatch.commits, 0);
        assert_eq!(predispatch.commit_returns, 0);
        assert_eq!(predispatch.commit_reconciliation_calls, 0);
        store.rollback(&mut predispatch).expect("rollback");
        assert_eq!(q_current(), 0);

        assert!(observed_delta(Some(1), Some(2)).is_err());
        assert!(observed_product(Some(u64::MAX), Some(2)).is_err());
        drop(store);
        fs::remove_file(source).expect("source cleanup");
        remove_sqlite_image(&database).expect("database cleanup");
    }

    #[test]
    fn actual_commit_error_uses_fresh_reconciliation() {
        let source = test_path("commit-error-source");
        fs::write(&source, b"actual sqlite commit error").expect("source");
        let database = test_path("commit-error.sqlite");
        let candidate = FILE_CANDIDATES[0];
        {
            let mut store = Store::open(&database, candidate).expect("open");
            let mut metrics = Metrics::default();
            let result = build_file(&mut store, &source, candidate, &mut metrics).expect("build");
            store
                .connection
                .commit_hook(Some(|| true))
                .expect("install commit rejection");
            let provenance = store
                .publish_with_fault(None, result.0, result.1, None, &mut metrics)
                .expect("commit failure provenance");
            assert_eq!(provenance.first, Some(FailureCause::Core(CoreError::Io)));
            assert_eq!(provenance.reconciliation, Reconciliation::PriorVisible);
            assert_eq!(provenance.reconciliation_error, None);
            assert_eq!(provenance.dominant, Some(FailureCause::Core(CoreError::Io)));
            assert_eq!(metrics.commits, 1);
            assert_eq!(metrics.commit_returns, 1);
            assert_eq!(metrics.commit_return_successes, 0);
            assert_eq!(metrics.commit_return_errors, 1);
            assert_eq!(metrics.commit_reconciliation_calls, 1);
            assert!(store.active_transaction.is_none());
            assert!(store.current_head().expect("head after rollback").is_none());
            store
                .connection
                .commit_hook(None::<fn() -> bool>)
                .expect("remove commit rejection");
        }
        let reopened = Store::open(&database, candidate).expect("reopen");
        assert!(reopened.current_head().expect("reopened head").is_none());
        drop(reopened);
        remove_sqlite_image(&database).expect("database cleanup");
        fs::remove_file(source).expect("source cleanup");
    }

    #[test]
    fn normal_publish_retains_requested_visible_diagnostic() {
        let source = test_path("publish-diagnostic-source");
        let database = test_path("publish-diagnostic.sqlite");
        fs::write(&source, b"publish diagnostic").expect("source");
        let mut store = Store::open(&database, FILE_CANDIDATES[0]).expect("open");
        let mut metrics = Metrics::default();
        let (root, transition) =
            build_file(&mut store, &source, FILE_CANDIDATES[0], &mut metrics).expect("build");
        store.next_publish_fault = Some(PublishFault::AfterCommitBeforeAck);
        let publication = store
            .publish(None, root, transition, &mut metrics)
            .expect("requested head reconciles as committed");
        assert_eq!(publication.status, PublicationStatus::RequestedVisible);
        let diagnostic = publication.diagnostic.expect("lost-ack diagnostic");
        assert_eq!(diagnostic.first, Some(FailureCause::Core(CoreError::Io)));
        assert_eq!(diagnostic.reconciliation, Reconciliation::RequestedVisible);
        assert_eq!(diagnostic.dominant, None);
        assert_eq!(metrics.commits, 1);
        assert_eq!(
            store
                .current_head()
                .expect("head")
                .map(|head| (head.1, head.2)),
            Some((root, transition))
        );
        drop(store);
        fs::remove_file(source).expect("source cleanup");
        remove_sqlite_image(&database).expect("database cleanup");
    }

    #[test]
    fn real_commit_dispatch_boundaries_cover_requested_different_and_ambiguous() {
        let source = test_path("commit-boundary-matrix-source");
        fs::write(&source, b"real sqlite commit dispatch matrix").expect("source");
        for (label, fault, reconciliation, dominant, reconciliation_error) in [
            (
                "requested",
                PublishFault::AfterCommitBeforeAck,
                Reconciliation::RequestedVisible,
                None,
                None,
            ),
            (
                "different",
                PublishFault::AfterCommitDifferentHead,
                Reconciliation::DifferentHead,
                Some(FailureCause::Core(CoreError::PublicationConflict)),
                None,
            ),
            (
                "ambiguous",
                PublishFault::AfterCommitUnavailable,
                Reconciliation::Ambiguous,
                Some(FailureCause::Core(CoreError::AmbiguousDurability)),
                Some(FailureCause::Core(CoreError::Io)),
            ),
        ] {
            let database = test_path(&format!("commit-boundary-{label}.sqlite"));
            let mut store = Store::open(&database, FILE_CANDIDATES[0]).expect("open");
            let mut metrics = Metrics::default();
            let (root, transition) =
                build_file(&mut store, &source, FILE_CANDIDATES[0], &mut metrics).expect("build");
            let provenance = store
                .publish_with_fault(None, root, transition, Some(fault), &mut metrics)
                .expect("lost acknowledgement provenance");
            assert_eq!(provenance.first, Some(FailureCause::Core(CoreError::Io)));
            assert_eq!(provenance.reconciliation, reconciliation);
            assert_eq!(provenance.reconciliation_error, reconciliation_error);
            assert_eq!(provenance.dominant, dominant);
            assert_eq!(metrics.commits, 1);
            assert_eq!(metrics.commit_returns, 1);
            assert_eq!(metrics.commit_return_successes, 1);
            assert_eq!(metrics.commit_return_errors, 0);
            assert_eq!(metrics.commit_reconciliation_calls, 1);
            assert!(store.active_transaction.is_none());
            if reconciliation == Reconciliation::RequestedVisible {
                assert_eq!(
                    store
                        .current_head()
                        .expect("requested head")
                        .map(|head| (head.1, head.2)),
                    Some((root, transition))
                );
            }
            drop(store);
            remove_sqlite_image(&database).expect("database cleanup");
        }
        fs::remove_file(source).expect("source cleanup");
    }

    #[test]
    fn failure_provenance_preserves_exact_missing_object() {
        let missing = ObjectId::for_bytes(b"missing provenance object");
        let provenance = failure_provenance(
            Some(FailureCause::MissingObject(missing)),
            None,
            Reconciliation::NotAttempted,
            None,
        );
        assert_eq!(provenance.first, Some(FailureCause::MissingObject(missing)));
        assert_eq!(provenance.dominant, provenance.first);
    }

    #[test]
    fn begin_counter_overflow_precedes_sql_and_leaves_connection_usable() {
        let database = test_path("begin-overflow.sqlite");
        let mut store = Store::open(&database, FILE_CANDIDATES[0]).expect("open");
        let mut metrics = Metrics {
            sql_execute_calls: u64::MAX,
            ..Metrics::default()
        };
        let error = store
            .transaction_attempt(&mut metrics, |_, _| Ok(()))
            .expect_err("counter overflow must reject before BEGIN");
        assert_eq!(
            error.downcast_ref::<CoreError>(),
            Some(&CoreError::LengthOverflow)
        );
        assert!(store.active_transaction.is_none());
        assert_eq!(metrics.transactions, 0);
        assert_eq!(metrics.sql_execute_calls, u64::MAX);
        assert!(store.current_head().expect("prior head").is_none());

        metrics.sql_execute_calls = 0;
        store
            .begin(&mut metrics)
            .expect("connection remains usable");
        store.rollback(&mut metrics).expect("cleanup usable writer");
        assert!(store.current_head().expect("unchanged head").is_none());
        drop(store);
        remove_sqlite_image(&database).expect("database cleanup");
    }

    #[test]
    fn transaction_attempt_rolls_back_with_exact_first_cause() {
        let database = test_path("transaction-attempt.sqlite");
        let mut store = Store::open(&database, FILE_CANDIDATES[0]).expect("open");
        let mut metrics = Metrics::default();
        let missing = ObjectId::for_bytes(b"missing transaction object");
        let error = store
            .transaction_attempt(&mut metrics, |store, _| {
                assert!(store.active_transaction.is_some());
                Err::<(), _>(CandidateError::MissingObject(missing).into())
            })
            .expect_err("pre-COMMIT failure");
        let provenance = error
            .downcast_ref::<PublicationFailure>()
            .expect("publication provenance")
            .0;
        assert_eq!(provenance.first, Some(FailureCause::MissingObject(missing)));
        assert_eq!(provenance.cleanup_first, None);
        assert_eq!(provenance.reconciliation, Reconciliation::NotAttempted);
        assert_eq!(provenance.dominant, provenance.first);
        assert!(store.active_transaction.is_none());
        assert!(store.current_head().expect("head").is_none());
        drop(store);
        remove_sqlite_image(&database).expect("database cleanup");
    }

    #[test]
    fn postcommit_failure_preserves_committed_publication_and_exact_cause() {
        let source = test_path("postcommit-source");
        let database = test_path("postcommit.sqlite");
        fs::write(&source, b"postcommit publication").expect("source");
        let mut store = Store::open(&database, FILE_CANDIDATES[0]).expect("open");
        let mut metrics = Metrics::default();
        let (root, transition) =
            build_file(&mut store, &source, FILE_CANDIDATES[0], &mut metrics).expect("build");
        store
            .publish(None, root, transition, &mut metrics)
            .expect("publish");
        let missing = ObjectId::for_bytes(b"postcommit missing object");
        let error = committed_result::<()>(
            root,
            transition,
            Err(CandidateError::MissingObject(missing).into()),
        )
        .expect_err("postcommit verification failure");
        assert_eq!(
            error.downcast_ref::<CommittedPublicationFailure>(),
            Some(&CommittedPublicationFailure {
                root,
                transition,
                cause: FailureCause::MissingObject(missing),
            })
        );
        assert_eq!(
            store
                .current_head()
                .expect("head")
                .map(|head| (head.1, head.2)),
            Some((root, transition))
        );
        drop(store);
        fs::remove_file(source).expect("source cleanup");
        remove_sqlite_image(&database).expect("database cleanup");
    }

    #[test]
    fn complete_head_compare_prevents_genesis_overwrite_and_aba() {
        let source = test_path("complete-head-source");
        let database = test_path("complete-head.sqlite");
        fs::write(&source, b"complete-head-test").expect("source");
        let candidate = FILE_CANDIDATES[0];
        let mut store = Store::open(&database, candidate).expect("open");
        let mut metrics = Metrics::default();
        let (root, transition) =
            build_file(&mut store, &source, candidate, &mut metrics).expect("build");
        store
            .publish(None, root, transition, &mut metrics)
            .expect("genesis insert");
        let prior = store.current_head().expect("head").expect("prior head");

        store.begin(&mut metrics).expect("begin duplicate genesis");
        let error = store
            .publish(None, root, transition, &mut metrics)
            .expect_err("genesis must be insert-only");
        assert_eq!(
            error.downcast_ref::<CoreError>(),
            Some(&CoreError::PublicationConflict)
        );
        store
            .rollback(&mut metrics)
            .expect("rollback duplicate genesis");

        let mut wrong_prior = prior;
        wrong_prior.0 = wrong_prior.0.checked_add(1).expect("wrong generation");
        store.begin(&mut metrics).expect("begin ABA attempt");
        let error = store
            .publish(Some(&wrong_prior), root, transition, &mut metrics)
            .expect_err("root-only equality must not authorize update");
        assert_eq!(
            error.downcast_ref::<CoreError>(),
            Some(&CoreError::PublicationConflict)
        );
        store.rollback(&mut metrics).expect("rollback ABA attempt");
        assert_eq!(store.current_head().expect("unchanged head"), Some(prior));

        drop(store);
        fs::remove_file(source).expect("source cleanup");
        remove_sqlite_image(&database).expect("database cleanup");
    }
}

fn run_campaign(root: &Path) -> AnyResult<()> {
    require_optimized_benchmark()?;
    fs::create_dir_all(root)?;
    let executable = env::current_exe()?;
    let executable_sha256 = write_environment_record(root, &executable)?;
    prepare_sources(root)?;
    let jsonl = root.join("wp4m-profile-selection.jsonl");
    let mut output = File::create(&jsonl)?;
    let mut failures = File::create(root.join("wp4m-profile-selection-failures.jsonl"))?;
    let mut commands = File::create(root.join("wp4m-profile-selection-commands.txt"))?;
    let mut resources = File::create(root.join("wp4m-profile-selection-resources.stderr"))?;
    let mut invocations = 0_usize;
    for iteration in 0..6 {
        let warmup = iteration == 0;
        let mut candidates = FILE_CANDIDATES.to_vec();
        let candidate_count = candidates.len();
        candidates.rotate_left(iteration % candidate_count);
        for candidate in candidates {
            for operation in ["full", "same-middle", "plus1-early", "plus1-middle"] {
                invoke_campaign_row(
                    root,
                    candidate,
                    SOURCE_100,
                    operation,
                    iteration,
                    warmup,
                    &mut output,
                    &mut failures,
                    &mut commands,
                    &mut resources,
                    &executable_sha256,
                )?;
                invocations = invocations
                    .checked_add(1)
                    .ok_or(CoreError::LengthOverflow)?;
            }
        }
        let mut directories = DIR_CANDIDATES.to_vec();
        let directory_count = directories.len();
        directories.rotate_left(iteration % directory_count);
        for candidate in directories {
            for operation in ["dir-create", "dir-lookup", "dir-replace", "dir-leading"] {
                invoke_campaign_row(
                    root,
                    candidate,
                    SOURCE_100,
                    operation,
                    iteration,
                    warmup,
                    &mut output,
                    &mut failures,
                    &mut commands,
                    &mut resources,
                    &executable_sha256,
                )?;
                invocations = invocations
                    .checked_add(1)
                    .ok_or(CoreError::LengthOverflow)?;
            }
        }
    }
    output.sync_all()?;
    if invocations != 144 {
        return Err(format!("campaign invocation count {invocations}, expected 144").into());
    }
    failures.sync_all()?;
    commands.sync_all()?;
    resources.sync_all()?;
    write_campaign_summary(root, &jsonl, invocations)?;
    println!(
        "campaign COMPLETE invocations={invocations} jsonl={} summary={}",
        jsonl.display(),
        root.join("wp4m-profile-selection-summary.json").display()
    );
    Ok(())
}

fn main() -> AnyResult<()> {
    let args: Vec<String> = env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("--self-test") => self_test(Path::new(args.get(2).ok_or("missing self-test root")?)),
        Some("--prepare-fixtures") => {
            prepare_retained_fixtures(Path::new(args.get(2).ok_or("missing fixture root")?))
        }
        Some("--campaign") => run_campaign(Path::new(args.get(2).ok_or("missing campaign root")?)),
        Some("--prepare-row") => {
            let root = Path::new(args.get(2).ok_or("missing row root")?);
            let candidate = candidate_by_name(args.get(3).ok_or("missing candidate")?)?;
            let size = args.get(4).ok_or("missing size")?.parse::<u64>()?;
            let operation = args.get(5).ok_or("missing operation")?;
            let iteration = args.get(6).ok_or("missing iteration")?.parse::<usize>()?;
            prepare_row_database(root, candidate, size, operation, iteration)
        }
        Some("--row") => {
            let root = Path::new(args.get(2).ok_or("missing row root")?);
            let candidate = candidate_by_name(args.get(3).ok_or("missing candidate")?)?;
            let size = args.get(4).ok_or("missing size")?.parse::<u64>()?;
            let operation = args.get(5).ok_or("missing operation")?;
            let iteration = args.get(6).ok_or("missing iteration")?.parse::<usize>()?;
            let warmup = args.get(7).ok_or("missing warmup")?.parse::<bool>()?;
            println!("{}", run_row(root, candidate, size, operation, iteration, warmup)?);
            Ok(())
        }
        _ => Err("usage: --self-test ROOT | --prepare-fixtures ROOT | --campaign ROOT | --prepare-row ROOT CANDIDATE SIZE OP ITERATION | --row ROOT CANDIDATE SIZE OP ITERATION WARMUP".into()),
    }
}

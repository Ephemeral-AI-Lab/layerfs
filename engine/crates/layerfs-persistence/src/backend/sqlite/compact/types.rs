//! §7/§11 compaction types, sidecar constants and the error classification.

use crate::backend::sqlite::session::rows;
use crate::KeptWaste;
use layerfs_content::ObjectId;
use layerfs_history::error::HistoryError;
use layerfs_storage::port::PersistenceError;
use std::path::Path;

/// The sidecar record suffix (§11: SQLite keeps its phase outside the sealed
/// database).
pub(crate) const OP_SUFFIX: &str = ".compact-op";

/// The fresh rebuild Store beside the live one.
pub(crate) const COMPACT_SUFFIX: &str = ".compact";

/// The §6.3 visited scratch database.
pub(crate) const VISITED_SUFFIX: &str = ".compact-visited";

/// The full SQLite artifact set of §9, without the variable `-mj*` journals:
/// the fixed staged fresh file and visited artifact plus the incomplete op
/// record (`.compact-op.tmp`), which an op write that died before its rename
/// leaves. The fresh file is built with `journal_mode=OFF`, so a crash leaves a
/// partial `.compact` with no rollback journal; the settle deletes it.
pub(crate) const ARTIFACT_SUFFIXES: [&str; 7] = [
    COMPACT_SUFFIX,
    ".compact-journal",
    ".compact-wal",
    ".compact-shm",
    VISITED_SUFFIX,
    ".compact-visited-journal",
    ".compact-op.tmp",
];

/// The mark tuple a compaction records before its first checkpoint (§8 Phase M
/// on PostgreSQL; the same first-statement tuple in the SQLite sidecar here).
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct MarkTuple {
    pub(crate) catalog_id: Vec<u8>,
    pub(crate) generation: i64,
    pub(crate) next_history_serial: i64,
    pub(crate) next_pack_id: i64,
    pub(crate) next_ordinal: i64,
    pub(crate) logical: i64,
    pub(crate) canonical: i64,
    pub(crate) stored: i64,
    pub(crate) metadata: i64,
    pub(crate) stamp: i64,
    pub(crate) history_rows: i64,
}

/// Which §3 path a compaction took.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CompactPath {
    /// Below the 10% wholly-dead threshold: mark only, nothing written.
    None,
    /// A rebuild installed at `G → G+1`.
    Rebuild,
}

impl CompactPath {
    /// The report's `path` text.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Rebuild => "rebuild",
        }
    }
}

/// A test-only stop point of the run, so a crash between phases can be
/// exercised. The run leaves the state a crash would leave and reports an
/// unknown outcome; the next `store.compact` must recover it (§9).
#[doc(hidden)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CompactFailpoint {
    /// Before the seal, while the open session, the writer lane and the
    /// admission intent are still held.
    BeforeSeal,
    /// After the seal returns and before the sealed file is read back.
    BeforeSealRead,
    /// After the seal and the sealed-state comparison, before any artifact.
    AfterSeal,
    /// Inside the build, before the visited scratch or the copy is written.
    BeforeCopy,
    /// Inside the build, immediately before the fresh file's first write.
    /// The fresh file does not exist yet; the real-kill arm stops here with
    /// the visited scratch present and no staged name on disk. The in-process
    /// `DuringCopy` arm keeps the torn-target state.
    BeforeLoad,
    /// Inside the build, after the fresh file's schema is created and before
    /// the retained rows are copied: the state a crash inside the bulk load
    /// leaves.
    DuringCopy,
    /// After the fresh `<store>.compact` is built, indexed and switched to WAL.
    AfterBuild,
    /// The fresh Store's first write meets a bounded filesystem (ENOSPC).
    CapacityAtBuild,
    /// After the staged file is switched to WAL and checkpointed.
    AfterWalSwitch,
    /// Inside the verification, after the schema and row-count checks.
    DuringVerify,
    /// After the immutable verification passes.
    AfterVerify,
    /// Immediately before the rename would run.
    BeforeInstall,
    /// After the rename and before the directory fsync (rename/journal).
    DuringRename,
    /// After the rename and directory fsync.
    AfterInstall,
    /// Inside the settle, after the artifact set is removed and before the op
    /// record.
    DuringSettle,
    /// PostgreSQL: after Phase M wrote the op and the dead set, before Phase A.
    PostgresAfterMark,
    /// PostgreSQL: after Phase A built and verified the scratch tables.
    PostgresAfterBuild,
    /// PostgreSQL: after Phase B committed the swap, before Phase C.
    PostgresAfterSwap,
    /// PostgreSQL: after Phase C settled the op.
    PostgresAfterCleanup,
}

impl CompactFailpoint {
    /// The phase the sidecar records for this stop point.
    pub(crate) const fn phase(self) -> &'static str {
        match self {
            Self::BeforeSeal | Self::BeforeSealRead | Self::AfterSeal => "mark",
            Self::BeforeCopy
            | Self::BeforeLoad
            | Self::DuringCopy
            | Self::AfterBuild
            | Self::CapacityAtBuild
            | Self::AfterWalSwitch => "build",
            Self::DuringVerify | Self::AfterVerify | Self::BeforeInstall => "verify",
            Self::DuringRename | Self::AfterInstall => "install",
            // §11: the settle is its own `cleanup` phase, so a crash after the
            // install and before the op is settled reads `cleanup`.
            Self::DuringSettle => "cleanup",
            Self::PostgresAfterMark => "mark",
            Self::PostgresAfterBuild => "build",
            Self::PostgresAfterSwap => "swap",
            Self::PostgresAfterCleanup => "cleanup",
        }
    }
}

/// Inputs one `store.compact` attempt uses.
#[derive(Clone, Debug, Default)]
pub struct CompactRequest {
    /// 16-byte operation identity, written as the admission intent's `op_id`.
    pub op_id: [u8; 16],
    /// Owning runtime token, for the op and intent records.
    pub owner_runtime: Option<String>,
    /// Owning boot token, for the op and intent records.
    pub owner_boot: Option<String>,
    /// The owning process id, so `store.status` can tell a live run from a
    /// leftover (§11). The kernel start time and boot id, when known, keep a
    /// reused id from passing as the owner.
    pub owner_pid: Option<u32>,
    /// The owner's kernel start time (the lease's second field).
    pub owner_pid_start: Option<u64>,
    /// The owner's kernel boot id (the lease's third field).
    pub owner_pid_boot: Option<u128>,
    /// A test-only stop point; production callers leave it `None`.
    pub failpoint: Option<CompactFailpoint>,
    /// A test-only hook called after verification and immediately before the
    /// §10 install fence, so a test can replace the Store under the sealed
    /// handle. Production callers leave it `None`.
    #[doc(hidden)]
    pub before_install_hook: Option<fn(&Path)>,
}

/// The report keeps at most this many kept-waste delta-base identities (§11,
/// S7): a rebuild of a Store with millions of bases must not serialise a
/// multi-megabyte reference list into its receipt. The bytes still count the
/// whole kept set.
pub const DELTA_BASE_REFERENCES_CAP: usize = 1024;

/// The `keptWaste` §11 part of a compaction report.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CompactKeptWaste {
    /// Bytes of unreferenced records inside packs the mark retained.
    pub dead_in_retained_packs_bytes: u64,
    /// Bytes of objects reached only as a delta base.
    pub delta_bases_bytes: u64,
    /// The objects counted by [`Self::delta_bases_bytes`], in ascending order,
    /// capped at [`DELTA_BASE_REFERENCES_CAP`] entries.
    pub delta_base_references: Vec<[u8; 32]>,
    /// Whether [`Self::delta_base_references`] was cut at the cap; the wire
    /// renders a capped list as `{value, cap}` (S7).
    pub delta_base_references_capped: bool,
    /// Bytes of `inode_value_block` rows, which 2.0 keeps in full.
    pub inode_blocks_bytes: u64,
}

impl CompactKeptWaste {
    /// One mark's kept-waste accounting, capped at
    /// [`DELTA_BASE_REFERENCES_CAP`] identities. Both backends project their
    /// receipt through this constructor so the cap cannot drift.
    pub fn from_waste(waste: &KeptWaste) -> Self {
        let (references, cut) = cap_delta_references(waste.delta_base_references());
        Self {
            dead_in_retained_packs_bytes: waste.dead_in_retained_packs_bytes(),
            delta_bases_bytes: waste.delta_bases_bytes(),
            delta_base_references: references,
            // The mark already bounded its own list, so its recorded cut is the
            // authority; `cut` covers a cap that ever becomes tighter than the
            // mark's bound.
            delta_base_references_capped: waste.delta_base_references_capped() || cut,
            inode_blocks_bytes: waste.inode_blocks_bytes(),
        }
    }

    /// Whether every part is zero.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.dead_in_retained_packs_bytes == 0
            && self.delta_bases_bytes == 0
            && self.inode_blocks_bytes == 0
    }
}

/// The report's `deltaBases.references`: the first [`DELTA_BASE_REFERENCES_CAP`]
/// identities in ascending order, and whether the list was cut.
pub fn cap_delta_references(ids: &[ObjectId]) -> (Vec<[u8; 32]>, bool) {
    (
        ids.iter()
            .take(DELTA_BASE_REFERENCES_CAP)
            .map(|id| *id.as_bytes())
            .collect(),
        ids.len() > DELTA_BASE_REFERENCES_CAP,
    )
}

/// The `residue` §11 part of a compaction report.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CompactResidue {
    /// Bytes left behind by the artifact set after settle.
    pub bytes: u64,
    /// The artifact file names that remain.
    pub artifacts: Vec<String>,
    /// Why anything remains, when it does.
    pub reason: Option<String>,
}

/// One zero report field and its §11 `zeroReason`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CompactZeroReason {
    /// The report field the reason explains.
    pub field: &'static str,
    /// One word of the §11 vocabulary. The #711 report-semantics fix adds
    /// `no-free-pages` (a rebuild whose source Store had no reusable pages) and
    /// `no-shrink` (a rebuild with dead bytes but no physical file delta).
    pub reason: &'static str,
}

impl CompactZeroReason {
    /// The §10 wire key this reason names: the §11 logical name rendered
    /// snake_case, the same mapping the report serializer uses.
    #[must_use]
    pub fn wire_field(self) -> &'static str {
        match self.field {
            "deadBytesBound" => "dead_bytes_bound",
            "deadBytesRefined" => "dead_bytes_refined",
            "logicalDiscarded" => "logical_discarded",
            "backendReusable" => "backend_reusable",
            "physicalShrink" => "physical_shrink",
            "keptWaste" => "kept_waste",
            "peakTempBytes" => "peak_temp_bytes",
            other => other,
        }
    }
}

/// The §11 report `store.compact` returns.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompactReceipt {
    /// `none` or `rebuild`.
    pub path: CompactPath,
    /// The generation the mark ran at.
    pub generation_before: u64,
    /// The installed generation: equal on `none`.
    pub generation_after: u64,
    /// Bytes in wholly dead packs after the metadata walk; the path's input.
    pub dead_bytes_bound: u64,
    /// The same bytes after the payload sweep (§7 step 4). The mark resolves the
    /// whole retained set and no second sweep runs, so this equals
    /// `dead_bytes_bound` on every path (#42 A10).
    pub dead_bytes_refined: u64,
    /// Collected history rows' logical bytes.
    pub logical_discarded: u64,
    /// SQLite free pages of the new (rebuild) or live (`none`) Store.
    pub backend_reusable: u64,
    /// Old file size minus new.
    pub physical_shrink: u64,
    /// The bytes the mark keeps that no retained root needs.
    pub kept_waste: CompactKeptWaste,
    /// Peak temporary bytes the run needed.
    pub peak_temp_bytes: u64,
    /// The artifact set left after settle; empty on success.
    pub residue: CompactResidue,
    /// §13: the report is an estimate, not a measurement.
    pub estimate: bool,
    /// One `zeroReason` per zero report field.
    pub zero_reasons: Vec<CompactZeroReason>,
}

/// §7 step 3's capacity classification: `ENOSPC`, `EDQUOT` and `EFBIG` are
/// a `Capacity` refusal; every other I/O failure stays `Integrity`.
pub fn io_error(error: std::io::Error) -> HistoryError {
    match error.raw_os_error() {
        Some(28) | Some(122) | Some(27) | Some(69) => HistoryError::Capacity("disk space"),
        _ => HistoryError::Integrity("Store compaction I/O"),
    }
}

pub(crate) fn db_error(error: rusqlite::Error) -> HistoryError {
    rows::error(error).into()
}

pub(crate) fn seal_error(error: PersistenceError) -> HistoryError {
    match error {
        PersistenceError::Busy => HistoryError::Busy,
        PersistenceError::Refused { status } if status == "Capacity" => {
            HistoryError::Capacity("Store compaction")
        }
        PersistenceError::Refused { status } if status == "Store sidecar remains after seal" => {
            HistoryError::Conflict("Store sidecar remains after seal")
        }
        PersistenceError::Refused { .. } | PersistenceError::Malformed => {
            HistoryError::Integrity("Store seal")
        }
        PersistenceError::Uncertain => HistoryError::UnknownOutcome,
        PersistenceError::Missing | PersistenceError::BackendUnavailable => {
            HistoryError::Unsupported("SQLite Store compaction")
        }
    }
}

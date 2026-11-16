//! §7 SQLite Store compaction: mark, rebuild, immutable verification and the
//! atomic install.
//!
//! One call takes the Store's writer lane before it registers its admission
//! intent, runs the T7.2 mark in the same exclusive window, seals the file
//! (checkpoint, checked close, sidecar check and `(size, mtime, change
//! counter)` capture), and then either stops on the §3 `none` path or builds
//! `<store>.compact`, verifies it through an `immutable=1` URI, and renames it
//! into place at `G → G+1`. The run's phase and mark tuple are kept beside the
//! Store in `<store>.compact-op`, the SQLite equivalent of the PostgreSQL
//! `gc_op` row (§11); the next `store.compact` judges a leftover by the Store
//! generation (`G` not installed, `G+1` installed; §9) and removes the artifact
//! set it names.
//!
//! The admission intent lives through the build and the install and is cleared
//! at the `cleanup` settle, after the artifact set is removed (§5.1, §9). The
//! transient `kept_node` working table is emptied before every `store.compact`
//! replies on both paths: a `none` run must leave the live Store exactly as it
//! found it. §6.3 has both paths spill the closure to
//! `<store>.compact-visited`, so `none` creates the scratch and deletes it
//! before replying, and the rebuild keeps and reuses it.
//!
//! The T7.2 mark already resolves every logical reference and delta base, so
//! the bytes of packs that hold no reached object are the refined dead bytes:
//! §7's step-4 payload sweep cannot discover another live object on SQLite.
//! `deadBytesRefined` therefore equals `deadBytesBound`, as §11 requires for
//! the `none` path.

use super::lifecycle::seal::SealedFacts;
use super::schema;
use super::session::connection::{Session, SqlWork};
use super::session::rows;
use crate::backend::records::BackendError;
use crate::compact_metrics::{
    self, CompactOutcome, CompactPhase, CompactRefusal, CompactRunObservation,
};
use crate::{KeptWaste, MarkResult, SqliteHandles};
use layerfs_content::ObjectId;
use layerfs_history::error::{HistoryError, HistoryResult};
use layerfs_history::records::{GcIntent, GcIntentKind, GcScopeKind};
use layerfs_history::HistoryCatalog;
use layerfs_storage::port::PersistenceError;
use rusqlite::{params, Connection, OpenFlags};
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

/// The sidecar record suffix (§11: SQLite keeps its phase outside the sealed
/// database).
const OP_SUFFIX: &str = ".compact-op";
/// The staged rebuild Store beside the live one.
const COMPACT_SUFFIX: &str = ".compact";
/// The §6.3 visited scratch database.
const VISITED_SUFFIX: &str = ".compact-visited";
/// Upper bound on the rollback-journal header: SQLite pads it to the
/// filesystem's sector size and writes a header for each transaction. Two
/// sectors cover the prune and rewrite headers.
const JOURNAL_HEADER_BOUND: u64 = 8192;

/// The full SQLite artifact set of §9, without the variable `-mj*` journals:
/// the fixed staged and visited artifacts plus the incomplete op record
/// (`.compact-op.tmp`), which an op write that died before its rename leaves.
/// `.compact-vacuum` is not part of it: SQLite's `VACUUM` writes its rewrite to
/// an unlinked `etilqs_*` temp database, never beside the Store.
const ARTIFACT_SUFFIXES: [&str; 7] = [
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
struct MarkTuple {
    catalog_id: Vec<u8>,
    generation: i64,
    next_history_serial: i64,
    next_pack_id: i64,
    next_ordinal: i64,
    logical: i64,
    canonical: i64,
    stored: i64,
    metadata: i64,
    stamp: i64,
    history_rows: i64,
}

/// The first-statement tuple, in the column order every reader decodes.
const TUPLE_SQL: &str = "SELECT h.catalog_id,h.catalog_incarnation,h.next_history_serial,\
p.next_pack_id,p.next_ordinal,t.logical,t.canonical,t.stored,t.metadata,\
(SELECT COALESCE(MAX(stamp),0) FROM content_signature),\
(SELECT COUNT(*) FROM \"commit\")+(SELECT COUNT(*) FROM layer) \
FROM history_meta h, store_policy p, store_totals t \
WHERE h.id=1 AND p.id=1 AND t.id=1";

/// The §6.4 live-heads digest input, in identity order.
const LIVE_STACKS: &str = "SELECT s.layer_stack_id,s.head_layer_id FROM layer_stack s \
WHERE NOT EXISTS (SELECT 1 FROM layer_stack_retired r WHERE r.layer_stack_id=s.layer_stack_id) \
ORDER BY s.layer_stack_id";
/// The marker-free Branches, in identity order.
const LIVE_BRANCHES: &str = "SELECT b.branch_id,b.head_commit_id,b.base_layer_id FROM branch b \
WHERE NOT EXISTS (SELECT 1 FROM branch_retired r WHERE r.branch_id=b.branch_id) \
AND NOT EXISTS (SELECT 1 FROM layer_stack_retired s WHERE s.layer_stack_id=b.layer_stack_id) \
ORDER BY b.branch_id";

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
    /// Inside the build, immediately before `VACUUM INTO` starts the copy.
    /// SQLite runs that copy as one uninterruptible call, so a real-process
    /// `SIGKILL` cannot land inside it; the real-kill arm stops here with the
    /// visited scratch present and the staged file not yet created. The
    /// in-process `DuringCopy` arm keeps the torn-target state.
    BeforeVacuum,
    /// Inside the build, after `VACUUM INTO` produced `<store>.compact` and
    /// before the copy is pruned: the state a crash during the copy leaves.
    DuringCopy,
    /// After the staged `<store>.compact` is built and compacted.
    AfterBuild,
    /// The staged Store's `VACUUM INTO` meets a bounded filesystem (ENOSPC).
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
    const fn phase(self) -> &'static str {
        match self {
            Self::BeforeSeal | Self::BeforeSealRead | Self::AfterSeal => "mark",
            Self::BeforeCopy
            | Self::BeforeVacuum
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
        let (references, capped) = cap_delta_references(waste.delta_base_references());
        Self {
            dead_in_retained_packs_bytes: waste.dead_in_retained_packs_bytes(),
            delta_bases_bytes: waste.delta_bases_bytes(),
            delta_base_references: references,
            delta_base_references_capped: capped,
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
fn cap_delta_references(ids: &[ObjectId]) -> (Vec<[u8; 32]>, bool) {
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
    /// One word of the §11 closed vocabulary.
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
    /// The same bytes after the payload sweep.
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

/// One recorded compaction run beside the Store.
#[derive(Clone, Debug)]
struct RunOp {
    op_id: [u8; 16],
    generation: i64,
    phase: String,
    started_ms: i64,
    owner_pid: Option<u32>,
    owner_pid_start: Option<u64>,
    owner_pid_boot: Option<u128>,
    tuple: MarkTuple,
    live_heads_digest: [u8; 32],
}

impl RunOp {
    fn encode(&self) -> String {
        format!(
            "version=1\nop_id={}\ngeneration={}\nphase={}\nstarted_ms={}\n\
owner_pid={}\nowner_pid_start={}\nowner_pid_boot={}\n\
catalog_id={}\nnext_history_serial={}\nnext_pack_id={}\nnext_ordinal={}\n\
mark_serial={}\nmark_pack_id={}\nmark_digest={}\nmark_totals={},{},{},{}\nmark_stamp={}\nhistory_rows={}\n\
live_heads_digest={}\n",
            hex(&self.op_id),
            self.generation,
            self.phase,
            self.started_ms,
            self.owner_pid.map_or_else(String::new, |pid| pid.to_string()),
            self.owner_pid_start.map_or_else(String::new, |start| start.to_string()),
            self.owner_pid_boot.map_or_else(String::new, |boot| format!("{boot:032x}")),
            hex(&self.tuple.catalog_id),
            self.tuple.next_history_serial,
            self.tuple.next_pack_id,
            self.tuple.next_ordinal,
            self.tuple.next_history_serial.saturating_sub(1),
            self.tuple.next_pack_id,
            hex(&self.live_heads_digest),
            self.tuple.logical,
            self.tuple.canonical,
            self.tuple.stored,
            self.tuple.metadata,
            self.tuple.stamp,
            self.tuple.history_rows,
            hex(&self.live_heads_digest),
        )
    }

    fn write(&self, store: &Path) -> HistoryResult<()> {
        let path = op_path(store);
        let temporary = op_temporary_path(store);
        std::fs::write(&temporary, self.encode()).map_err(io_error)?;
        if let Ok(file) = std::fs::File::open(&temporary) {
            let _ = file.sync_all();
        }
        std::fs::rename(&temporary, &path).map_err(io_error)?;
        if let Some(parent) = path.parent() {
            let _ = std::fs::File::open(parent).and_then(|directory| directory.sync_all());
        }
        Ok(())
    }

    fn read(store: &Path) -> Option<Self> {
        let text = std::fs::read_to_string(op_path(store)).ok()?;
        let mut values = BTreeMap::new();
        for line in text.lines() {
            let (key, value) = line.split_once('=')?;
            values.insert(key, value);
        }
        if values.get("version") != Some(&"1") {
            return None;
        }
        let op_id = unhex::<16>(values.get("op_id")?)?;
        let generation: i64 = values.get("generation")?.parse().ok()?;
        let phase = (*values.get("phase")?).to_owned();
        let started_ms: i64 = values.get("started_ms")?.parse().ok()?;
        let owner_pid = values
            .get("owner_pid")
            .filter(|text| !text.is_empty())
            .and_then(|text| text.parse().ok());
        let owner_pid_start = values
            .get("owner_pid_start")
            .filter(|text| !text.is_empty())
            .and_then(|text| text.parse().ok());
        let owner_pid_boot = values
            .get("owner_pid_boot")
            .filter(|text| !text.is_empty())
            .and_then(|text| u128::from_str_radix(text, 16).ok());
        let catalog_id = unhex_vec(values.get("catalog_id")?)?;
        let tuple = MarkTuple {
            catalog_id,
            generation,
            next_history_serial: values.get("next_history_serial")?.parse().ok()?,
            next_pack_id: values.get("next_pack_id")?.parse().ok()?,
            next_ordinal: values.get("next_ordinal")?.parse().ok()?,
            logical: values.get("mark_totals")?.split(',').next()?.parse().ok()?,
            canonical: values.get("mark_totals")?.split(',').nth(1)?.parse().ok()?,
            stored: values.get("mark_totals")?.split(',').nth(2)?.parse().ok()?,
            metadata: values.get("mark_totals")?.split(',').nth(3)?.parse().ok()?,
            stamp: values.get("mark_stamp")?.parse().ok()?,
            history_rows: values.get("history_rows")?.parse().ok()?,
        };
        let live_heads_digest = unhex::<32>(values.get("live_heads_digest")?)?;
        Some(Self {
            op_id,
            generation,
            phase,
            started_ms,
            owner_pid,
            owner_pid_start,
            owner_pid_boot,
            tuple,
            live_heads_digest,
        })
    }
}

/// The operation a Store's `<store>.compact-op` records, as `store.status`
/// (T10.1) projects it: the operation identity, the generation the run started
/// at, its phase, its owner and whether its outcome is known. A settled run
/// removes the record, so [`compact_op_status`] returns `None`.
#[doc(hidden)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompactOpStatus {
    /// The 16-byte operation identity.
    pub op_id: [u8; 16],
    /// The generation the run started at.
    pub generation: i64,
    /// The recorded phase: `admission`, `mark`, `build`, `verify` or `install`.
    pub phase: String,
    /// When the run started, in Unix milliseconds.
    pub started_ms: i64,
    /// The owning process id, when the run recorded one.
    pub owner_pid: Option<u32>,
    /// The owner's kernel start time, when recorded.
    pub owner_pid_start: Option<u64>,
    /// The owner's kernel boot id, when recorded.
    pub owner_pid_boot: Option<u128>,
    /// True while the record remains: the run did not settle, so its outcome
    /// is unknown. `store.status` reports `busy` while the owner lives and
    /// `unresolved` once it is gone.
    pub uncertain: bool,
}

impl CompactOpStatus {
    /// The `store.status` projection of this operation; `alive` says whether
    /// the owning process is still running. A live owner is `busy`, a gone one
    /// left an uncertain `unresolved` outcome. `uncertain` stays the record's
    /// own fact: it is true for every unsettled run, live or leftover.
    #[must_use]
    pub fn status(&self, alive: bool) -> crate::report::CompactStatus {
        // A read in the admission window still projects a §11 phase: the run
        // is about to mark, and `admission` is not one of the four.
        let phase = if self.phase == "admission" {
            "mark".to_owned()
        } else {
            self.phase.clone()
        };
        crate::report::CompactStatus {
            id: format!("op-{}", hex(&self.op_id)),
            name: "store.compact",
            phase,
            started: self.started_ms,
            uncertain: self.uncertain,
            state: if alive { "busy" } else { "unresolved" },
            phases: &crate::report::SQLITE_COMPACT_PHASES,
        }
    }
}

/// Reads the operation `<store>.compact-op` records, or `None` when the Store
/// has no compaction record. `store.status` (T10.1) uses this to show the
/// operation, its phase and its uncertain outcome; a settled run removes the
/// record (§9).
#[doc(hidden)]
#[must_use]
pub fn compact_op_status(store: &Path) -> Option<CompactOpStatus> {
    RunOp::read(store).map(|op| CompactOpStatus {
        op_id: op.op_id,
        generation: op.generation,
        phase: op.phase,
        started_ms: op.started_ms,
        owner_pid: op.owner_pid,
        owner_pid_start: op.owner_pid_start,
        owner_pid_boot: op.owner_pid_boot,
        uncertain: true,
    })
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| {
            i64::try_from(elapsed.as_millis()).unwrap_or(i64::MAX)
        })
}

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        let _ = write!(out, "{byte:02x}");
    }
    out
}

fn unhex_vec(text: &str) -> Option<Vec<u8>> {
    if text.len() % 2 != 0 {
        return None;
    }
    (0..text.len())
        .step_by(2)
        .map(|index| u8::from_str_radix(&text[index..index + 2], 16).ok())
        .collect()
}

fn unhex<const N: usize>(text: &str) -> Option<[u8; N]> {
    let bytes = unhex_vec(text)?;
    bytes.try_into().ok()
}

fn with_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.as_os_str().to_os_string();
    name.push(suffix);
    PathBuf::from(name)
}

fn compact_path(store: &Path) -> PathBuf {
    with_suffix(store, COMPACT_SUFFIX)
}

fn visited_path(store: &Path) -> PathBuf {
    with_suffix(store, VISITED_SUFFIX)
}

fn op_path(store: &Path) -> PathBuf {
    with_suffix(store, OP_SUFFIX)
}

/// The incomplete op record an op write leaves when it dies before its rename.
fn op_temporary_path(store: &Path) -> PathBuf {
    with_suffix(&op_path(store), ".tmp")
}

/// The §9 SQLite artifact set: the fixed suffixes plus every multi-database
/// super-journal the staged file may have left (`<store>.compact-mj*`).
fn artifact_paths(store: &Path) -> Vec<PathBuf> {
    let mut paths: Vec<PathBuf> = ARTIFACT_SUFFIXES
        .iter()
        .map(|suffix| with_suffix(store, suffix))
        .collect();
    let name = store.file_name().map_or_else(
        || store.as_os_str().to_os_string(),
        |name| name.to_os_string(),
    );
    let mut prefix = name;
    prefix.push(".compact-mj");
    let prefix = prefix.to_string_lossy().into_owned();
    if let Some(parent) = store.parent() {
        if let Ok(entries) = std::fs::read_dir(parent) {
            for entry in entries.flatten() {
                if entry.file_name().to_string_lossy().starts_with(&prefix) {
                    paths.push(entry.path());
                }
            }
        }
    }
    paths
}

fn io_error(error: std::io::Error) -> HistoryError {
    match error.raw_os_error() {
        Some(28) | Some(122) | Some(27) | Some(69) => HistoryError::Capacity("disk space"),
        _ => HistoryError::Integrity("Store compaction I/O"),
    }
}

fn db_error(error: rusqlite::Error) -> HistoryError {
    rows::error(error).into()
}

fn seal_error(error: PersistenceError) -> HistoryError {
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

/// The read-only URI of one database file that must not be changed while it is
/// read. Path bytes are percent-encoded so `?`, `#` and `%` do not misdirect
/// the URI (§7 step 6).
#[cfg(unix)]
fn immutable_uri(path: &Path) -> String {
    use std::os::unix::ffi::OsStrExt;
    let mut uri = String::from("file:");
    for byte in path.as_os_str().as_bytes() {
        match byte {
            b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b'/' => {
                uri.push(char::from(*byte));
            }
            _ => {
                use std::fmt::Write as _;
                let _ = write!(uri, "%{byte:02X}");
            }
        }
    }
    uri.push_str("?immutable=1");
    uri
}

fn open_immutable(path: &Path) -> HistoryResult<Connection> {
    #[cfg(unix)]
    {
        let connection = Connection::open_with_flags(
            immutable_uri(path),
            OpenFlags::SQLITE_OPEN_READ_ONLY
                | OpenFlags::SQLITE_OPEN_URI
                | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
        .map_err(db_error)?;
        connection
            .busy_timeout(std::time::Duration::ZERO)
            .map_err(db_error)?;
        Ok(connection)
    }
    #[cfg(not(unix))]
    {
        let _ = path;
        Err(HistoryError::Unsupported("SQLite Store compaction"))
    }
}

fn live_tuple(session: &Arc<Session>) -> HistoryResult<MarkTuple> {
    session
        .run::<_, BackendError>(false, |tx| {
            let rows = tx.query(TUPLE_SQL, vec![])?;
            let row = rows.first().ok_or(BackendError::Integrity)?;
            Ok(MarkTuple {
                catalog_id: row.get(0)?,
                generation: row.get(1)?,
                next_history_serial: row.get(2)?,
                next_pack_id: row.get(3)?,
                next_ordinal: row.get(4)?,
                logical: row.get(5)?,
                canonical: row.get(6)?,
                stored: row.get(7)?,
                metadata: row.get(8)?,
                stamp: row.get(9)?,
                history_rows: row.get(10)?,
            })
        })
        .map_err(Into::into)
}

fn file_tuple(connection: &Connection) -> HistoryResult<MarkTuple> {
    connection
        .query_row(TUPLE_SQL, params![], |row| {
            Ok(MarkTuple {
                catalog_id: row.get(0)?,
                generation: row.get(1)?,
                next_history_serial: row.get(2)?,
                next_pack_id: row.get(3)?,
                next_ordinal: row.get(4)?,
                logical: row.get(5)?,
                canonical: row.get(6)?,
                stored: row.get(7)?,
                metadata: row.get(8)?,
                stamp: row.get(9)?,
                history_rows: row.get(10)?,
            })
        })
        .map_err(db_error)
}

fn pack_lengths(session: &Arc<Session>) -> HistoryResult<BTreeMap<i64, u64>> {
    session
        .run::<_, BackendError>(false, |tx| {
            let rows = tx.query("SELECT pack_id,length FROM pack", vec![])?;
            let mut lengths = BTreeMap::new();
            for row in &rows {
                let id: i64 = row.get(0)?;
                let length: i64 = row.get(1)?;
                lengths.insert(
                    id,
                    u64::try_from(length).map_err(|_| BackendError::Integrity)?,
                );
            }
            Ok(lengths)
        })
        .map_err(Into::into)
}

fn live_page_metrics(session: &Arc<Session>) -> HistoryResult<(u64, u64)> {
    session
        .run::<_, BackendError>(false, |tx| {
            let free = tx.query("PRAGMA freelist_count", vec![])?;
            let page = tx.query("PRAGMA page_size", vec![])?;
            let free = free.first().ok_or(BackendError::Integrity)?.get::<i64>(0)?;
            let page = page.first().ok_or(BackendError::Integrity)?.get::<i64>(0)?;
            Ok((
                u64::try_from(free).map_err(|_| BackendError::Integrity)?,
                u64::try_from(page).map_err(|_| BackendError::Integrity)?,
            ))
        })
        .map_err(Into::into)
}

/// The rollback-journal upper bound of one transaction on a `pages`-page file:
/// one record per modified page (`page_size + 8` bytes: page number, page image
/// and checksum) plus the header allowance. A page is journaled at most once
/// per transaction, so the staged copy's page count bounds both the prune and
/// the rewrite journal, and the bound never under-reports the observed file.
const fn journal_bound_bytes(pages: u64, page_size: u64) -> u64 {
    pages
        .saturating_mul(page_size.saturating_add(8))
        .saturating_add(JOURNAL_HEADER_BOUND)
}

/// The staged file's page count and page size, before the prune frees pages.
fn file_page_extent(connection: &Connection) -> HistoryResult<(u64, u64)> {
    let pages: i64 = connection
        .query_row("PRAGMA page_count", params![], |row| row.get(0))
        .map_err(db_error)?;
    let page: i64 = connection
        .query_row("PRAGMA page_size", params![], |row| row.get(0))
        .map_err(db_error)?;
    Ok((
        u64::try_from(pages).map_err(|_| HistoryError::Integrity("page count"))?,
        u64::try_from(page).map_err(|_| HistoryError::Integrity("page size"))?,
    ))
}

fn file_page_metrics(connection: &Connection) -> HistoryResult<(u64, u64)> {
    let free: i64 = connection
        .query_row("PRAGMA freelist_count", params![], |row| row.get(0))
        .map_err(db_error)?;
    let page: i64 = connection
        .query_row("PRAGMA page_size", params![], |row| row.get(0))
        .map_err(db_error)?;
    Ok((
        u64::try_from(free).map_err(|_| HistoryError::Integrity("freelist count"))?,
        u64::try_from(page).map_err(|_| HistoryError::Integrity("page size"))?,
    ))
}

fn file_heads_digest(connection: &Connection) -> HistoryResult<[u8; 32]> {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"layerfs/live-heads/v1\0");
    let mut stacks = connection.prepare(LIVE_STACKS).map_err(db_error)?;
    let mut rows = stacks.query(params![]).map_err(db_error)?;
    while let Some(row) = rows.next().map_err(db_error)? {
        let id: Vec<u8> = row.get(0).map_err(db_error)?;
        let head: Vec<u8> = row.get(1).map_err(db_error)?;
        if id.len() != 17 || head.len() != 33 {
            return Err(HistoryError::Integrity("live-heads stack width"));
        }
        hasher.update(&id);
        hasher.update(&head);
    }
    drop(rows);
    drop(stacks);
    let mut branches = connection.prepare(LIVE_BRANCHES).map_err(db_error)?;
    let mut rows = branches.query(params![]).map_err(db_error)?;
    while let Some(row) = rows.next().map_err(db_error)? {
        let id: Vec<u8> = row.get(0).map_err(db_error)?;
        let head: Option<Vec<u8>> = row.get(1).map_err(db_error)?;
        let base: Vec<u8> = row.get(2).map_err(db_error)?;
        if id.len() != 17 || base.len() != 33 {
            return Err(HistoryError::Integrity("live-heads branch width"));
        }
        hasher.update(&id);
        match head {
            Some(head) if head.len() == 33 => hasher.update(&head),
            Some(_) => return Err(HistoryError::Integrity("live-heads commit width")),
            None => hasher.update(&[0u8; 33]),
        };
        hasher.update(&base);
    }
    Ok(*hasher.finalize().as_bytes())
}

/// Writes the §6.3 visited set and the retained pack set to the scratch
/// database the §7 copy joins against. The scratch runs `journal_mode=OFF`, so
/// it leaves no journal of its own.
fn write_visited(scratch: &Path, mark: &MarkResult) -> HistoryResult<u64> {
    if std::fs::symlink_metadata(scratch).is_ok() {
        return Err(HistoryError::Exists("staged compaction artifact"));
    }
    let connection = Connection::open_with_flags(
        scratch,
        OpenFlags::SQLITE_OPEN_READ_WRITE
            | OpenFlags::SQLITE_OPEN_CREATE
            | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(db_error)?;
    connection
        .busy_timeout(std::time::Duration::ZERO)
        .map_err(db_error)?;
    connection
        .execute_batch(
            "PRAGMA journal_mode=OFF; PRAGMA synchronous=OFF;\
CREATE TABLE visited (\
 object_id BLOB PRIMARY KEY CHECK(length(object_id)=32),\
 pack_id INTEGER NOT NULL\
) STRICT, WITHOUT ROWID;\
CREATE TABLE live_pack (pack_id INTEGER PRIMARY KEY) STRICT, WITHOUT ROWID;",
        )
        .map_err(db_error)?;
    connection
        .execute_batch("BEGIN IMMEDIATE")
        .map_err(db_error)?;
    {
        let mut visited = connection
            .prepare("INSERT OR IGNORE INTO visited(object_id,pack_id) VALUES(?1,?2)")
            .map_err(db_error)?;
        let mut live = connection
            .prepare("INSERT OR IGNORE INTO live_pack(pack_id) VALUES(?1)")
            .map_err(db_error)?;
        for pack in mark.objects().packs() {
            live.execute(params![pack]).map_err(db_error)?;
        }
        // The mark already resolved each reached object's winning locator, so
        // the scratch joins against it without a second `object_location` load.
        let locators = mark.objects().locators();
        for id in mark.objects().objects() {
            let pack = locators
                .get(id)
                .ok_or(HistoryError::Integrity("mark object locator"))?;
            visited
                .execute(params![id.as_bytes().as_slice(), pack])
                .map_err(db_error)?;
        }
    }
    connection.execute_batch("COMMIT").map_err(db_error)?;
    connection.close().map_err(|(_, error)| db_error(error))?;
    for suffix in ["-journal", "-wal", "-shm"] {
        let mut sidecar = scratch.as_os_str().to_os_string();
        sidecar.push(suffix);
        if std::fs::symlink_metadata(sidecar).is_ok() {
            return Err(HistoryError::Integrity("visited scratch sidecar"));
        }
    }
    // §7 step 7 fsyncs the staged artifact it installs; the scratch is part of
    // the recorded set too, and `journal_mode=OFF` skips SQLite's own sync.
    let bytes = std::fs::metadata(scratch).map_err(io_error)?.len();
    std::fs::File::open(scratch)
        .map_err(io_error)?
        .sync_all()
        .map_err(io_error)?;
    Ok(bytes)
}

/// What the staged rebuild produced, for the report and the install fence.
struct Built {
    target: PathBuf,
    target_bytes: u64,
    /// The staged copy's extent right after `VACUUM INTO`, before the prune.
    staged_bytes: u64,
    /// The rollback-journal upper bound the staged page count implies.
    journal_bound_bytes: u64,
    visited_bytes: u64,
    logical: u64,
    canonical: u64,
    stored: u64,
    metadata: u64,
    free_bytes: u64,
}

impl Built {
    /// §11 peak temporary bytes: the run's whole artifact footprint below the
    /// rename, as a documented upper bound rather than a sampled peak.
    ///
    /// The staged copy is the whole sealed Store (`VACUUM INTO`), so it keeps
    /// the source extent until the rewrite's final truncate. The rewrite then
    /// writes the new Store into SQLite's temp database (the process temp
    /// directory, unlinked) while rewriting the staged file in place, and both
    /// the prune and the rewrite journal the pages they overwrite. The journal
    /// term bounds either journal by one record per staged page
    /// (`page_size + 8` bytes each: page number, image and checksum) plus the
    /// header allowance, so it never under-reports the observed journal. The
    /// sum over-reports the measured 50 GB peak by a few percent; §13 already
    /// marks the report an estimate.
    fn peak_temp_bytes(&self) -> u64 {
        self.staged_bytes
            .saturating_add(self.journal_bound_bytes)
            .saturating_add(self.target_bytes)
            .saturating_add(self.visited_bytes)
    }
}

/// Escaped SQL text literal for an `ATTACH` path (the statement cannot bind).
fn quote(text: &str) -> String {
    text.replace('\'', "''")
}

fn table_present(connection: &Connection, name: &str) -> HistoryResult<bool> {
    let found: i64 = connection
        .query_row(
            "SELECT count(*) FROM sqlite_schema WHERE type='table' AND name=?1",
            params![name],
            |row| row.get(0),
        )
        .map_err(db_error)?;
    Ok(found == 1)
}

/// The sealed Store and the mark a staged build is measured against.
struct BuildInput<'a> {
    facts: &'a SealedFacts,
    mark: &'a MarkResult,
    tuple: &'a MarkTuple,
    layout: crate::SqlitePackLayout,
    synchronous: i64,
}

/// The §7 steps 2–5: build the staged Store, copy only retained rows, compact
/// it, and switch it to WAL.
fn build(
    input: BuildInput<'_>,
    capacity: bool,
    request: &CompactRequest,
    op: &RunOp,
) -> HistoryResult<Built> {
    let BuildInput {
        facts,
        mark,
        tuple,
        layout,
        synchronous,
    } = input;
    let store = &facts.path;
    let target = compact_path(store);
    let visited = visited_path(store);
    for staged in [&target, &visited] {
        if std::fs::symlink_metadata(staged).is_ok() {
            return Err(HistoryError::Exists("staged compaction artifact"));
        }
    }
    stop_at(request, CompactFailpoint::BeforeCopy, op, store)?;
    let visited_bytes = write_visited(&visited, mark)?;
    if capacity {
        // §7 step 3 on a bounded filesystem: `VACUUM INTO` has already created
        // the staged file when the write meets ENOSPC. The abort path removes
        // it and the live Store is unchanged.
        std::fs::write(&target, b"staged").map_err(io_error)?;
        return Err(io_error(std::io::Error::from_raw_os_error(28)));
    }

    // A real-process crash arm stops here: the visited scratch is written and
    // `VACUUM INTO` has not started. See `BeforeVacuum`.
    stop_at(request, CompactFailpoint::BeforeVacuum, op, store)?;
    let source = open_immutable(store)?;
    let target_text = target
        .to_str()
        .ok_or(HistoryError::InvalidInput("compaction path"))?;
    source
        .execute("VACUUM INTO ?1", params![target_text])
        .map_err(db_error)?;
    source.close().map_err(|(_, error)| db_error(error))?;
    // The copy produced the staged file but has not pruned or verified it.
    stop_at(request, CompactFailpoint::DuringCopy, op, store)?;
    // The staged copy holds the source's whole extent until the final
    // `VACUUM` truncates it; §11 counts it in the peak temp.
    let staged_bytes = std::fs::metadata(&target).map_err(io_error)?.len();

    let connection = Connection::open_with_flags(
        &target,
        OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(db_error)?;
    connection
        .busy_timeout(std::time::Duration::ZERO)
        .map_err(db_error)?;
    connection
        .execute_batch(&format!(
            "PRAGMA journal_mode=DELETE; PRAGMA synchronous={synchronous};"
        ))
        .map_err(db_error)?;
    // The prune and the rewrite journal the pages they overwrite; one record
    // per staged page bounds either journal before the staged file shrinks.
    let (staged_pages, page_size) = file_page_extent(&connection)?;
    let journal_bound_bytes = journal_bound_bytes(staged_pages, page_size);
    // The copied published-pack rows may be deleted in the artifact; the live
    // immutability triggers are restored before the file is verified.
    // §7 step 4 asks for `foreign_keys=ON` outside the transaction (inside it
    // the pragma is a no-op). The pruning keeps `foreign_keys=OFF` — an
    // explicit, tested deviation: with enforcement on, SQLite's deferred
    // constraint counter survives this delete order and the COMMIT is refused
    // `SQLITE_CONSTRAINT_FOREIGNKEY` even though `pragma_foreign_key_check`
    // reports the final row set clean. The verification's
    // `pragma_foreign_key_check` proves that same final state, so the pragma is
    // not the only guard.
    // `sqlite_sequence` is the acquisition operation counter: the copy keeps
    // every row the source had, and the sequence never falls (§4.1, §7).
    let sequences: Vec<(String, i64)> = if table_present(&connection, "sqlite_sequence")? {
        let mut statement = connection
            .prepare("SELECT name,seq FROM sqlite_sequence")
            .map_err(db_error)?;
        let mut rows = statement.query(params![]).map_err(db_error)?;
        let mut found = Vec::new();
        while let Some(row) = rows.next().map_err(db_error)? {
            found.push((
                row.get::<_, String>(0).map_err(db_error)?,
                row.get::<_, i64>(1).map_err(db_error)?,
            ));
        }
        found
    } else {
        Vec::new()
    };
    let triggers: Vec<String> = {
        let mut statement = connection
            .prepare(
                "SELECT sql FROM sqlite_schema \
WHERE type='trigger' AND name LIKE '%immutable%' AND sql IS NOT NULL",
            )
            .map_err(db_error)?;
        let mut rows = statement.query(params![]).map_err(db_error)?;
        let mut found = Vec::new();
        while let Some(row) = rows.next().map_err(db_error)? {
            found.push(row.get::<_, String>(0).map_err(db_error)?);
        }
        found
    };
    connection
        .execute_batch(
            "DROP TRIGGER IF EXISTS pack_immutable_update;\
DROP TRIGGER IF EXISTS pack_immutable_delete;\
DROP TRIGGER IF EXISTS pack_unit_immutable_update;\
DROP TRIGGER IF EXISTS pack_unit_immutable_delete;\
PRAGMA foreign_keys=OFF;\
PRAGMA defer_foreign_keys=ON;",
        )
        .map_err(db_error)?;
    connection
        .execute_batch(&format!(
            "ATTACH DATABASE '{}' AS scratch;",
            quote(
                visited
                    .to_str()
                    .ok_or(HistoryError::InvalidInput("compaction path"))?
            )
        ))
        .map_err(db_error)?;

    let units = layout.uses_units();
    let acquisition_tables = table_present(&connection, "init_operation")?;
    connection
        .execute_batch("BEGIN IMMEDIATE")
        .map_err(db_error)?;
    let staged = (|| -> HistoryResult<()> {
        // §7 step 4: the totals trigger invariant holds before the copy.
        let before: (i64, i64) = connection
            .query_row(
                "SELECT (SELECT logical FROM store_totals WHERE id=1),\
(SELECT COALESCE(SUM(logical),0) FROM \"commit\")+\
(SELECT COALESCE(SUM(logical),0) FROM layer)",
                params![],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .map_err(db_error)?;
        if before.0 != before.1 {
            return Err(HistoryError::Integrity(
                "Store totals disagree with the history rows",
            ));
        }
        let mut deletes = format!(
            "DELETE FROM object_location WHERE object_id NOT IN \
(SELECT object_id FROM scratch.visited);\
DELETE FROM content_signature WHERE object_id NOT IN \
(SELECT object_id FROM scratch.visited);\
DELETE FROM file_digest WHERE root NOT IN (SELECT object_id FROM scratch.visited);\
DELETE FROM binding_hint_block;\
DELETE FROM \"commit\" WHERE commit_id NOT IN \
(SELECT node_id FROM kept_node WHERE kind='commit');\
DELETE FROM layer WHERE layer_id NOT IN \
(SELECT node_id FROM kept_node WHERE kind='layer');\
DELETE FROM branch WHERE branch_id NOT IN \
(SELECT node_id FROM kept_node WHERE kind='branch');\
DELETE FROM layer_stack WHERE layer_stack_id NOT IN \
(SELECT node_id FROM kept_node WHERE kind='stack');\
DELETE FROM layer_lineage WHERE layer_id NOT IN \
(SELECT node_id FROM kept_node WHERE kind='layer');\
DELETE FROM commit_lineage WHERE commit_id NOT IN \
(SELECT node_id FROM kept_node WHERE kind='commit');\
DELETE FROM absorbed WHERE run NOT IN (SELECT run FROM commit_lineage);\
DELETE FROM reach WHERE key NOT IN \
(SELECT layer_stack_id FROM layer_stack UNION SELECT run FROM commit_lineage);\
DELETE FROM branch_retired WHERE branch_id NOT IN \
(SELECT node_id FROM kept_node WHERE kind='branch');\
DELETE FROM layer_stack_retired WHERE layer_stack_id NOT IN \
(SELECT layer_stack_id FROM layer_stack);\
DELETE FROM path_removed WHERE layer_stack_id NOT IN \
(SELECT layer_stack_id FROM layer_stack);\
DELETE FROM project_epoch WHERE layer_stack_id NOT IN \
(SELECT layer_stack_id FROM layer_stack);\
DELETE FROM merge_record;\
DELETE FROM merge_operation;\
DELETE FROM gc_intent WHERE op_id<>x'{}';\
DELETE FROM gc_dead;\
DELETE FROM gc_op;",
            hex(&op.op_id)
        );
        if units {
            deletes.push_str(
                "DELETE FROM pack_unit WHERE pack_id NOT IN \
(SELECT pack_id FROM scratch.live_pack);",
            );
        }
        deletes.push_str(
            "DELETE FROM pack WHERE pack_id NOT IN (SELECT pack_id FROM scratch.live_pack);",
        );
        if acquisition_tables {
            for table in ["init_operation", "init_entry", "init_host_file"] {
                deletes.push_str(&format!("DELETE FROM {table};"));
            }
        }
        for trigger in &triggers {
            deletes.push_str(trigger.trim_end_matches(';'));
            deletes.push_str(";\n");
        }
        deletes.push_str(&format!(
            "UPDATE history_meta SET catalog_incarnation={} WHERE id=1;\
UPDATE store_totals SET \
logical=(SELECT COALESCE(SUM(logical),0) FROM \"commit\")+\
(SELECT COALESCE(SUM(logical),0) FROM layer),\
canonical=(SELECT COALESCE(SUM(canonical_length),0) FROM object_location),\
stored=(SELECT COALESCE(SUM(length),0) FROM pack),\
metadata=(SELECT COALESCE(SUM(length),0) FROM pack WHERE domain=0) WHERE id=1;",
            tuple.generation + 1
        ));
        connection.execute_batch(&deletes).map_err(db_error)?;
        Ok(())
    })();
    match staged {
        Ok(()) => {
            connection.execute_batch("COMMIT").map_err(db_error)?;
        }
        Err(error) => {
            let _ = connection.execute_batch("ROLLBACK");
            let _ = connection.execute_batch("DETACH DATABASE scratch");
            let _ = connection.close();
            return Err(error);
        }
    }
    connection
        .execute_batch("DETACH DATABASE scratch")
        .map_err(db_error)?;
    // Reclaim the deleted pages: a wholly live Store, no freelist.
    connection.execute_batch("VACUUM").map_err(db_error)?;
    if !sequences.is_empty() && table_present(&connection, "sqlite_sequence")? {
        connection
            .execute_batch("DELETE FROM sqlite_sequence")
            .map_err(db_error)?;
        let mut statement = connection
            .prepare("INSERT INTO sqlite_sequence(name,seq) VALUES(?1,?2)")
            .map_err(db_error)?;
        for (name, seq) in &sequences {
            statement.execute(params![name, seq]).map_err(db_error)?;
        }
        drop(statement);
        // §4.1/§7: the acquisition operation counter never falls. The copy
        // keeps every row the source had, so a restored sequence below the
        // source one is a rebuild bug, never a smaller Store.
        for (name, seq) in &sequences {
            let restored: i64 = connection
                .query_row(
                    "SELECT seq FROM sqlite_sequence WHERE name=?1",
                    params![name],
                    |row| row.get(0),
                )
                .map_err(db_error)?;
            if restored < *seq {
                return Err(HistoryError::Integrity("staged Store sqlite_sequence fell"));
            }
        }
    }
    let totals: (i64, i64, i64, i64) = connection
        .query_row(
            "SELECT logical,canonical,stored,metadata FROM store_totals WHERE id=1",
            params![],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .map_err(db_error)?;
    connection.close().map_err(|(_, error)| db_error(error))?;

    // §7 step 5: switch to WAL, checkpoint, and leave no sidecar.
    let connection = Connection::open_with_flags(
        &target,
        OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(db_error)?;
    connection
        .busy_timeout(std::time::Duration::ZERO)
        .map_err(db_error)?;
    let mode: String = connection
        .query_row("PRAGMA journal_mode=wal", params![], |row| row.get(0))
        .map_err(db_error)?;
    if mode != "wal" {
        return Err(HistoryError::Integrity("staged Store is not in WAL mode"));
    }
    connection
        .execute_batch(&format!("PRAGMA synchronous={synchronous}"))
        .map_err(db_error)?;
    let busy: i64 = connection
        .query_row("PRAGMA wal_checkpoint(TRUNCATE)", params![], |row| {
            row.get(0)
        })
        .map_err(db_error)?;
    if busy != 0 {
        return Err(HistoryError::Busy);
    }
    connection.close().map_err(|(_, error)| db_error(error))?;
    for suffix in ["-wal", "-shm", "-journal"] {
        let mut sidecar = target.as_os_str().to_os_string();
        sidecar.push(suffix);
        if std::fs::symlink_metadata(sidecar).is_ok() {
            return Err(HistoryError::Integrity("staged Store sidecar"));
        }
    }
    let target_bytes = std::fs::metadata(&target).map_err(io_error)?.len();
    let free_bytes = file_free_bytes(&target)?;
    Ok(Built {
        target,
        target_bytes,
        staged_bytes,
        journal_bound_bytes,
        visited_bytes,
        logical: u64::try_from(totals.0).unwrap_or(0),
        canonical: u64::try_from(totals.1).unwrap_or(0),
        stored: u64::try_from(totals.2).unwrap_or(0),
        metadata: u64::try_from(totals.3).unwrap_or(0),
        free_bytes,
    })
}

/// The staged Store and the mark a verification reads it against.
struct VerifyInput<'a> {
    store: &'a Path,
    built: &'a Built,
    mark: &'a MarkResult,
    tuple: &'a MarkTuple,
    layout: crate::SqlitePackLayout,
    acquisition: crate::SqliteAcquisitionSchema,
}

/// The §7 step 6 verification through an `immutable=1` reader: schema identity,
/// foreign keys, row counts against the mark, both live-heads digests, the
/// allocator tuple and the recomputed totals. `quick_check` and the full doctor
/// stay separate commands.
fn verify(input: VerifyInput<'_>, request: &CompactRequest, op: &RunOp) -> HistoryResult<()> {
    let VerifyInput {
        store,
        built,
        mark,
        tuple,
        layout,
        acquisition,
    } = input;
    let connection = open_immutable(&built.target)?;
    schema::check(
        &connection,
        &RefCell::new(SqlWork::default()),
        (layout, acquisition),
    )
    .map_err(HistoryError::from)?;
    let count = |sql: &str| -> HistoryResult<u64> {
        let value: i64 = connection
            .query_row(sql, params![], |row| row.get(0))
            .map_err(db_error)?;
        u64::try_from(value).map_err(|_| HistoryError::Integrity("verification count"))
    };
    if count("SELECT count(*) FROM pragma_foreign_key_check")? != 0 {
        return Err(HistoryError::Integrity("staged Store foreign key"));
    }
    if count("SELECT count(*) FROM object_location")? != mark.objects().len() as u64 {
        return Err(HistoryError::Integrity("staged Store object count"));
    }
    if count("SELECT count(*) FROM pack")? != mark.objects().packs().len() as u64 {
        return Err(HistoryError::Integrity("staged Store pack count"));
    }
    if count("SELECT count(*) FROM kept_node")? != mark.history().len() as u64 {
        return Err(HistoryError::Integrity("staged Store history count"));
    }
    stop_at(request, CompactFailpoint::DuringVerify, op, store)?;
    if file_heads_digest(&connection)? != mark.live_heads_digest() {
        return Err(HistoryError::Conflict(
            "staged Store live heads differ from the mark",
        ));
    }
    let checked = file_tuple(&connection)?;
    if checked.catalog_id != tuple.catalog_id
        || checked.generation != tuple.generation + 1
        || checked.next_history_serial != tuple.next_history_serial
        || checked.next_pack_id != tuple.next_pack_id
        || checked.next_ordinal != tuple.next_ordinal
        || checked.stamp > tuple.stamp
    {
        return Err(HistoryError::Integrity("staged Store allocator tuple"));
    }
    let totals = connection
        .query_row(
            "SELECT \
(SELECT logical FROM store_totals WHERE id=1),\
(SELECT COALESCE(SUM(logical),0) FROM \"commit\")+\
(SELECT COALESCE(SUM(logical),0) FROM layer),\
(SELECT canonical FROM store_totals WHERE id=1),\
(SELECT COALESCE(SUM(canonical_length),0) FROM object_location),\
(SELECT stored FROM store_totals WHERE id=1),\
(SELECT COALESCE(SUM(length),0) FROM pack),\
(SELECT metadata FROM store_totals WHERE id=1),\
(SELECT COALESCE(SUM(length),0) FROM pack WHERE domain=0)",
            params![],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, i64>(4)?,
                    row.get::<_, i64>(5)?,
                    row.get::<_, i64>(6)?,
                    row.get::<_, i64>(7)?,
                ))
            },
        )
        .map_err(db_error)?;
    if totals.0 != totals.1 || totals.2 != totals.3 || totals.4 != totals.5 || totals.6 != totals.7
    {
        return Err(HistoryError::Integrity("staged Store totals"));
    }
    let built_logical = u64::try_from(checked.logical).unwrap_or(u64::MAX);
    let built_canonical = u64::try_from(checked.canonical).unwrap_or(u64::MAX);
    let built_stored = u64::try_from(checked.stored).unwrap_or(u64::MAX);
    let built_metadata = u64::try_from(checked.metadata).unwrap_or(u64::MAX);
    if built_logical != built.logical
        || built_canonical != built.canonical
        || built_stored != built.stored
        || built_metadata != built.metadata
    {
        return Err(HistoryError::Integrity("staged Store build totals"));
    }
    connection.close().map_err(|(_, error)| db_error(error))?;
    Ok(())
}

fn file_free_bytes(target: &Path) -> HistoryResult<u64> {
    let measure = open_immutable(target)?;
    let (free, page) = file_page_metrics(&measure)?;
    measure.close().map_err(|(_, error)| db_error(error))?;
    Ok(free.saturating_mul(page))
}

fn source_state(store: &Path) -> HistoryResult<(u64, SystemTime, u32)> {
    let metadata = std::fs::metadata(store).map_err(io_error)?;
    let modified = metadata.modified().map_err(io_error)?;
    let mut file = std::fs::File::open(store).map_err(io_error)?;
    use std::io::{Read as _, Seek as _, SeekFrom};
    let mut header = [0u8; 4];
    file.seek(SeekFrom::Start(24)).map_err(io_error)?;
    file.read_exact(&mut header).map_err(io_error)?;
    Ok((metadata.len(), modified, u32::from_be_bytes(header)))
}

/// True when one live or staged file carries a `-wal`, `-shm`, `-journal` or
/// multi-database super-journal (§7 step 7).
fn sidecar_present(path: &Path) -> Option<PathBuf> {
    for suffix in ["-wal", "-shm", "-journal"] {
        let mut sidecar = path.as_os_str().to_os_string();
        sidecar.push(suffix);
        let sidecar = PathBuf::from(sidecar);
        if std::fs::symlink_metadata(&sidecar).is_ok() {
            return Some(sidecar);
        }
    }
    let name = path.file_name().map_or_else(
        || path.as_os_str().to_os_string(),
        |name| name.to_os_string(),
    );
    let mut prefix = name;
    prefix.push("-mj");
    let prefix = prefix.to_string_lossy().into_owned();
    if let Some(parent) = path.parent() {
        if let Ok(entries) = std::fs::read_dir(parent) {
            for entry in entries.flatten() {
                if entry.file_name().to_string_lossy().starts_with(&prefix) {
                    return Some(entry.path());
                }
            }
        }
    }
    None
}

/// §7 step 7's fence before the rename: no sidecar, the same `G`, the same
/// catalog identity and the sealed `(size, mtime, change counter)`.
fn install_fence(facts: &SealedFacts, tuple: &MarkTuple) -> HistoryResult<()> {
    // §10: the path must still name the sealed file. A replacement is
    // `StaleStore` before the rename could install over a different Store.
    if let Some(sealed) = facts.identity {
        match sealed.names(&facts.path) {
            Ok(true) => {}
            Ok(false) => return Err(HistoryError::StaleStore),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Err(HistoryError::StaleStore);
            }
            Err(_) => return Err(HistoryError::Integrity("Store identity at install")),
        }
    }
    for path in [&facts.path, &compact_path(&facts.path)] {
        if sidecar_present(path).is_some() {
            return Err(HistoryError::Conflict("Store sidecar at install"));
        }
    }
    let connection = open_immutable(&facts.path)?;
    let current = file_tuple(&connection)?;
    connection.close().map_err(|(_, error)| db_error(error))?;
    if &current != tuple {
        return Err(HistoryError::Conflict("the sealed Store changed"));
    }
    let (bytes, modified, change_counter) = source_state(&facts.path)?;
    if bytes != facts.bytes || modified != facts.modified || change_counter != facts.change_counter
    {
        return Err(HistoryError::Conflict("the sealed Store changed"));
    }
    Ok(())
}

/// §7 step 7: `chmod 0600` and `sync_all` before the rename.
fn prepare_target(target: &Path) -> HistoryResult<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(target, std::fs::Permissions::from_mode(0o600))
            .map_err(io_error)?;
    }
    let file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(target)
        .map_err(io_error)?;
    file.sync_all().map_err(io_error)?;
    Ok(())
}

fn sync_directory(store: &Path) -> HistoryResult<()> {
    if let Some(parent) = store.parent() {
        let directory = std::fs::File::open(parent).map_err(io_error)?;
        directory.sync_all().map_err(io_error)?;
    }
    Ok(())
}

/// Removes the fixed artifact set and reports only what remains, so a
/// successful settle has a zero residue (§11).
fn clear_artifacts(store: &Path) -> CompactResidue {
    let mut bytes = 0u64;
    let mut left = Vec::new();
    for path in artifact_paths(store) {
        let Ok(metadata) = std::fs::symlink_metadata(&path) else {
            continue;
        };
        if std::fs::remove_file(&path).is_err() && std::fs::symlink_metadata(&path).is_ok() {
            // §11: residue is what the settle left behind. An artifact that
            // was removed is not residue, so its bytes are not counted.
            bytes = bytes.saturating_add(metadata.len());
            left.push(path.file_name().map_or_else(
                || path.display().to_string(),
                |name| name.to_string_lossy().into_owned(),
            ));
        }
    }
    CompactResidue {
        bytes,
        artifacts: left,
        reason: None,
    }
}

/// Settles one run (§9, §11 `cleanup`): the artifact set is removed, the
/// admission intent and the transient `kept_node` rows are cleared from the
/// installed Store, and the op record is removed last. The intent is released
/// *after* the artifact set, so a crash anywhere in the settle leaves a record
/// the next run adopts rather than a live intent no call can settle.
fn settle(store: &Path, request: &CompactRequest, op: &RunOp) -> HistoryResult<CompactResidue> {
    // §11: the settle records its own phase, so a crash here reads `cleanup`.
    let mut cleaning = op.clone();
    cleaning.phase = "cleanup".to_owned();
    let _ = cleaning.write(store);
    let mut residue = clear_artifacts(store);
    // §9: the op settles only after the artifact set is gone. A crash here
    // leaves the installed `G + 1` and the record for the next run.
    stop_at(request, CompactFailpoint::DuringSettle, op, store)?;
    let record_path = op_path(store);
    let settled = clear_installed_mark(store, &op.op_id);
    if settled && std::fs::symlink_metadata(&record_path).is_ok() {
        let bytes = std::fs::metadata(&record_path).map_or(0, |metadata| metadata.len());
        if std::fs::remove_file(&record_path).is_err()
            && std::fs::symlink_metadata(&record_path).is_ok()
        {
            residue.bytes = residue.bytes.saturating_add(bytes);
            residue.artifacts.push(record_path.file_name().map_or_else(
                || record_path.display().to_string(),
                |name| name.to_string_lossy().into_owned(),
            ));
        }
    }
    if !settled {
        // The op record stays the next run's recovery breadcrumb (§9).
        residue.artifacts.push(record_path.file_name().map_or_else(
            || record_path.display().to_string(),
            |name| name.to_string_lossy().into_owned(),
        ));
    }
    let _ = sync_directory(store);
    if !residue.artifacts.is_empty() && residue.reason.is_none() {
        residue.reason = Some("the artifact set could not be removed".to_owned());
    }
    Ok(residue)
}

/// Clears the installed Store's admission intent and the transient §6.1
/// `kept_node` rows in one raw transaction. After the seal the run holds no
/// session, so this is the only handle on the Store; the writer lane is already
/// released, and a busy Store keeps the op record for the next run instead.
fn clear_installed_mark(store: &Path, op_id: &[u8; 16]) -> bool {
    let Ok(connection) = Connection::open_with_flags(
        store,
        OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    ) else {
        return false;
    };
    let _ = connection.busy_timeout(std::time::Duration::from_secs(5));
    let cleared = connection
        .execute_batch(&format!(
            "BEGIN IMMEDIATE;DELETE FROM gc_intent WHERE op_id=x'{}';DELETE FROM kept_node;COMMIT;",
            hex(op_id)
        ))
        .is_ok();
    if !cleared {
        let _ = connection.execute_batch("ROLLBACK;");
    }
    let closed = connection.close().is_ok();
    cleared && closed
}

/// Releases the compaction's admission intent after the seal, when the run
/// holds no session: the row is in the Store file, so one raw delete on the
/// live path is enough. A failure keeps the op record, which the next run's
/// `settle_leftover` adopts (§9).
fn release_intent(store: &Path, op_id: &[u8; 16]) -> bool {
    let Ok(connection) = Connection::open_with_flags(
        store,
        OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    ) else {
        return false;
    };
    let _ = connection.busy_timeout(std::time::Duration::from_secs(5));
    let deleted = connection
        .execute(
            &format!("DELETE FROM gc_intent WHERE op_id=x'{}'", hex(op_id)),
            params![],
        )
        .is_ok();
    let closed = connection.close().is_ok();
    deleted && closed
}

/// Removes the §6.3 visited scratch a `none` path created: the file and any
/// journal SQLite might have left. `none` replies only after it is gone, so it
/// leaves no residue (§6.3, §9).
fn remove_scratch(scratch: &Path) -> HistoryResult<()> {
    let _ = std::fs::remove_file(scratch);
    for suffix in ["-journal", "-wal", "-shm"] {
        let mut sidecar = scratch.as_os_str().to_os_string();
        sidecar.push(suffix);
        let _ = std::fs::remove_file(PathBuf::from(sidecar));
    }
    if std::fs::symlink_metadata(scratch).is_ok() {
        return Err(HistoryError::Integrity(
            "the none path could not remove its visited scratch",
        ));
    }
    Ok(())
}

/// Drops every artifact and the op record after a definite failure before the
/// rename; the live Store is unchanged apart from the seal's checkpoint and the
/// release of the admission intent (§8/§9). `op_id` names the intent to release
/// when the run no longer holds a session; `None` means it was already released
/// before the seal.
fn abort_before_install(store: &Path, op_id: Option<&[u8; 16]>) {
    let released = op_id.is_none_or(|op_id| release_intent(store, op_id));
    let residue = clear_artifacts(store);
    if residue.artifacts.is_empty() && released {
        let _ = std::fs::remove_file(op_path(store));
    }
    let _ = sync_directory(store);
}

/// Releases the admission intent of a definite failure before the seal and
/// then removes the artifact set and the op record. When the release itself
/// fails the op record is kept as the next run's recovery breadcrumb (§9):
/// deleting it would leave a live intent that no later call could settle.
fn abort_run(history: &crate::HistoryProvider, store: &Path, op_id: &[u8; 16]) {
    if history.gc_release(op_id).is_ok() {
        // The intent is already gone; only the artifact set and the record
        // remain to be dropped.
        abort_before_install(store, None);
    }
}

/// Judges and removes a leftover an earlier run recorded (§9): the artifact set
/// it names and the admission intent it took. The caller already holds the
/// writer lane, so a leftover owner is gone.
fn settle_leftover(
    history: &crate::HistoryProvider,
    store: &Path,
    generation: u64,
    metrics: &mut RunMetrics,
) -> HistoryResult<()> {
    let Some(op) = RunOp::read(store) else {
        // `.compact-op.tmp` without a record is this Store's own incomplete op
        // write: the run died inside `RunOp::write`, before any artifact. It is
        // removed here, and only a staged name no record names stays the §9
        // `Exists` refusal.
        let incomplete = op_temporary_path(store);
        let unrecorded = artifact_paths(store)
            .into_iter()
            .filter(|path| path != &incomplete)
            .any(|path| std::fs::symlink_metadata(&path).is_ok());
        if unrecorded {
            return Err(HistoryError::Exists("unrecorded compaction artifact"));
        }
        if std::fs::symlink_metadata(&incomplete).is_ok() {
            std::fs::remove_file(&incomplete).map_err(io_error)?;
        }
        return Ok(());
    };
    // §9: the record names `G`. A Store still at `G` was never installed, so
    // its artifact set is removed and the run is settled. At `G+1` (or a
    // Store that advanced further past an older record) the rename installed a
    // rebuild, so only the record and the intent it kept remain. A record from
    // a *later* generation does not describe this Store, so nothing it names is
    // deleted.
    let current = i64::try_from(generation)
        .map_err(|_| HistoryError::Integrity("Store generation"))?;
    if op.generation > current {
        return Err(HistoryError::Integrity("compaction record generation"));
    }
    // Adopt the recorded owner's admission intent before its artifact is gone.
    history.gc_release(&op.op_id)?;
    let residue = clear_artifacts(store);
    if !residue.artifacts.is_empty() {
        return Err(HistoryError::Integrity(
            "recorded compaction artifact could not be removed",
        ));
    }
    if std::fs::symlink_metadata(op_path(store)).is_ok() {
        std::fs::remove_file(op_path(store)).map_err(io_error)?;
    }
    metrics.leftover_settled = true;
    Ok(())
}

/// True while the Store holds an Init or merge working row. Admission
/// already refuses a live Init/merge intent; this catches the leftover rows
/// of an operation whose intent was lost, which a rebuild must not wipe
/// without refusing (§5).
fn unfinished_work(store: &Path) -> HistoryResult<bool> {
    if std::fs::symlink_metadata(store).is_err() {
        return Ok(false);
    }
    let connection = Connection::open_with_flags(
        store,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(db_error)?;
    connection
        .busy_timeout(std::time::Duration::ZERO)
        .map_err(db_error)?;
    let _ = connection.execute_batch("PRAGMA query_only=ON");
    let mut found = false;
    for table in [
        "merge_operation",
        "merge_record",
        "init_operation",
        "init_entry",
        "init_host_file",
    ] {
        if table_present(&connection, table)? {
            let count: i64 = connection
                .query_row(&format!("SELECT count(*) FROM {table}"), params![], |row| {
                    row.get(0)
                })
                .map_err(db_error)?;
            if count > 0 {
                found = true;
                break;
            }
        }
    }
    let _ = connection.close();
    Ok(found)
}

/// A writer held across the seal, install and settle, released on every exit.
struct WriterGuard(Option<super::lane::writer_admission::Writer>);

impl WriterGuard {
    fn new(writer: super::lane::writer_admission::Writer) -> Self {
        Self(Some(writer))
    }

    fn take(&mut self) -> super::lane::writer_admission::Writer {
        self.0.take().expect("a compaction holds its writer")
    }

    fn put(&mut self, writer: super::lane::writer_admission::Writer) {
        self.0 = Some(writer);
    }

    fn release(&mut self) -> HistoryResult<()> {
        match self.0.take() {
            Some(writer) => writer.release().map_err(|_| HistoryError::UnknownOutcome),
            None => Ok(()),
        }
    }
}

impl Drop for WriterGuard {
    fn drop(&mut self) {
        if let Some(writer) = self.0.take() {
            let _ = writer.release();
        }
    }
}

/// Set by the real-process crash harness (T12.2's second arm): when present,
/// a reached failpoint connects to this Unix socket, writes one readiness
/// byte and blocks until the harness SIGKILLs the process. Production callers
/// never set it, so a failpoint keeps its in-process behaviour.
const CRASH_GATE_ENV: &str = "LAYERFS_COMPACT_CRASH_GATE";

/// Holds a reached failpoint until the test harness kills this process. The
/// variable being absent is a no-op: the in-process stop applies.
#[cfg(unix)]
fn crash_gate() -> HistoryResult<()> {
    use std::io::{Read, Write};
    use std::os::unix::net::UnixStream;
    let Some(path) = std::env::var_os(CRASH_GATE_ENV) else {
        return Ok(());
    };
    let mut gate = UnixStream::connect(path).map_err(io_error)?;
    gate.write_all(b"R").map_err(io_error)?;
    gate.flush().map_err(io_error)?;
    // The harness never releases the gate: it SIGKILLs this process here.
    let mut release = [0u8; 1];
    let _ = gate.read(&mut release);
    Ok(())
}

/// A test-only crash stop: records the phase and leaves the state a crash
/// would leave.
fn stop_at(
    request: &CompactRequest,
    point: CompactFailpoint,
    op: &RunOp,
    store: &Path,
) -> HistoryResult<()> {
    if request.failpoint != Some(point) {
        return Ok(());
    }
    let mut stopped = op.clone();
    stopped.phase = point.phase().to_owned();
    stopped.write(store)?;
    #[cfg(unix)]
    crash_gate()?;
    Err(HistoryError::UnknownOutcome)
}

fn immutable_tuple(path: &Path) -> HistoryResult<MarkTuple> {
    let connection = open_immutable(path)?;
    let tuple = file_tuple(&connection)?;
    connection.close().map_err(|(_, error)| db_error(error))?;
    Ok(tuple)
}

fn immutable_digest(path: &Path) -> HistoryResult<[u8; 32]> {
    let connection = open_immutable(path)?;
    let digest = file_heads_digest(&connection)?;
    connection.close().map_err(|(_, error)| db_error(error))?;
    Ok(digest)
}

/// §11: a zero field always carries one word of the closed vocabulary.
#[allow(clippy::too_many_arguments)]
fn receipt(
    path: CompactPath,
    generation_before: u64,
    generation_after: u64,
    dead_bytes: u64,
    logical_discarded: u64,
    backend_reusable: u64,
    physical_shrink: u64,
    kept_waste: CompactKeptWaste,
    peak_temp_bytes: u64,
    residue: CompactResidue,
    total_stored: u64,
    history_rows: u64,
) -> CompactReceipt {
    let mut zero_reasons = Vec::new();
    if dead_bytes == 0 {
        let reason = if total_stored == 0 {
            "empty-store"
        } else {
            "no-waste"
        };
        zero_reasons.push(CompactZeroReason {
            field: "deadBytesBound",
            reason,
        });
        zero_reasons.push(CompactZeroReason {
            field: "deadBytesRefined",
            reason,
        });
    }
    if logical_discarded == 0 {
        let reason = if history_rows == 0 {
            "no-history"
        } else if path == CompactPath::None {
            "none-path"
        } else {
            "no-waste"
        };
        zero_reasons.push(CompactZeroReason {
            field: "logicalDiscarded",
            reason,
        });
    }
    if physical_shrink == 0 {
        zero_reasons.push(CompactZeroReason {
            field: "physicalShrink",
            reason: if path == CompactPath::None {
                "none-path"
            } else {
                "no-waste"
            },
        });
    }
    if backend_reusable == 0 {
        zero_reasons.push(CompactZeroReason {
            field: "backendReusable",
            reason: if path == CompactPath::None {
                "none-path"
            } else {
                "no-waste"
            },
        });
    }
    if kept_waste.is_empty() {
        zero_reasons.push(CompactZeroReason {
            field: "keptWaste",
            reason: "no-waste",
        });
    }
    if peak_temp_bytes == 0 {
        zero_reasons.push(CompactZeroReason {
            field: "peakTempBytes",
            reason: "none-path",
        });
    }
    CompactReceipt {
        path,
        generation_before,
        generation_after,
        dead_bytes_bound: dead_bytes,
        dead_bytes_refined: dead_bytes,
        logical_discarded,
        backend_reusable,
        physical_shrink,
        kept_waste,
        peak_temp_bytes,
        residue,
        estimate: true,
        zero_reasons,
    }
}

impl SqliteHandles {
    /// Runs one §7 SQLite Store compaction on this Store's own machine.
    ///
    /// The call consumes the handles like [`Self::seal`]: it checkpoints and
    /// closes the sole session, and a rebuild installs a new file at `G + 1`.
    /// It takes the Store's exclusive writer lane before its admission intent,
    /// so no admitted writer can interleave between the mark, the seal and the
    /// rename. The caller reopens the Store afterwards.
    ///
    /// A definite failure before the rename removes the artifact set and leaves
    /// the old Store unchanged apart from the seal's checkpoint. A rename or
    /// directory-fsync failure is installed-but-uncertain: the new file and the
    /// op record are kept for the next `store.compact` to judge (§9).
    pub fn compact(self, request: &CompactRequest) -> HistoryResult<CompactReceipt> {
        let mut metrics = RunMetrics::start();
        let result = self.compact_measured(request, &mut metrics);
        metrics.finish(&result);
        result
    }

    /// The measured body of [`Self::compact`]; `metrics` observes it.
    fn compact_measured(
        self,
        request: &CompactRequest,
        metrics: &mut RunMetrics,
    ) -> HistoryResult<CompactReceipt> {
        #[cfg(not(any(target_os = "linux", target_os = "macos")))]
        {
            let _ = (request, metrics);
            return Err(HistoryError::Unsupported("SQLite Store compaction"));
        }
        #[cfg(any(target_os = "linux", target_os = "macos"))]
        {
            let SqliteHandles {
                storage,
                history,
                acquisition,
            } = self;
            drop(acquisition);
            let session = storage.session.clone();
            let store = session.path.clone();
            let profile = session.profile.clone();
            let layout = session.layout;
            let acquisition_schema = session.acquisition;
            let synchronous = profile.synchronous;

            // §5: an unfinished Init or merge holds the Store; its working
            // rows are not wiped by a compaction that was never admitted.
            if unfinished_work(&store)? {
                return Err(HistoryError::Busy);
            }

            // §7 step 1: the exclusive lane first, so the generation this run
            // records cannot move between the mark, the seal and the rename.
            let lane = session.hold_lane().map_err(HistoryError::from)?;
            // §7 step 1's sealed source's WAL is emptied after admission, so
            // the intent's own write is checkpointed with it.
            let live = live_tuple(&session)?;
            let generation = u64::try_from(live.generation)
                .map_err(|_| HistoryError::Integrity("Store generation"))?;
            metrics.generation_before = generation;
            settle_leftover(&history, &store, generation, metrics)?;

            let op_id = request.op_id;
            // Spec 5.1: the exclusive operation intent is registered before
            // the first checkpoint, so a removal sees an admitted compaction
            // even while the WAL is being emptied, and the whole run is one
            // admitted sequence.
            let intent = GcIntent::operation(
                op_id,
                GcIntentKind::Compaction,
                GcScopeKind::Store,
                history.catalog_id.as_slice().to_vec(),
            )
            .owned(
                request.owner_runtime.as_deref(),
                request.owner_boot.as_deref(),
            )
            .with_phase(Some("mark"));
            history.gc_register(&intent)?;
            let mut op = RunOp {
                op_id,
                generation: live.generation,
                phase: "admission".to_owned(),
                started_ms: now_ms(),
                owner_pid: request.owner_pid,
                owner_pid_start: request.owner_pid_start,
                owner_pid_boot: request.owner_pid_boot,
                tuple: live.clone(),
                live_heads_digest: [0u8; 32],
            };
            if let Err(error) = op.write(&store) {
                let _ = history.gc_release(&op_id);
                return Err(error);
            }
            // §7 step 1: with the lane held, no writer can append while the
            // intent, the op record and the WAL are settled together.
            if let Err(error) = session.checkpoint_holding() {
                let _ = history.gc_release(&op_id);
                let _ = std::fs::remove_file(op_path(&store));
                return Err(HistoryError::from(error));
            }
            // §8: the registration, the mark and the totals read are one
            // admitted sequence. A definite failure here releases the intent
            // before the artifact set and the op record go, so the Store is
            // left exactly as it was and no later call meets BusyOp.
            metrics.begin_phase(CompactPhase::Mark);
            let admitted =
                (|| -> HistoryResult<(MarkResult, MarkTuple, u64, u64, CompactKeptWaste, u64)> {
                    let mark = history.mark()?;
                    let tuple = live_tuple(&session)?;
                    op.tuple = tuple.clone();
                    op.live_heads_digest = mark.live_heads_digest();
                    op.phase = "mark".to_owned();
                    op.write(&store)?;
                    let lengths = pack_lengths(&session)?;
                    let total = lengths.values().copied().fold(0u64, u64::saturating_add);
                    let mut live_stored = 0u64;
                    for pack in mark.objects().packs() {
                        let length = lengths
                            .get(pack)
                            .ok_or(HistoryError::Integrity("mark pack"))?;
                        live_stored = live_stored.saturating_add(*length);
                    }
                    let dead = total.saturating_sub(live_stored);
                    let kept_waste = CompactKeptWaste::from_waste(mark.objects().waste());
                    let history_rows = u64::try_from(tuple.history_rows).unwrap_or(0);
                    Ok((mark, tuple, total, dead, kept_waste, history_rows))
                })();
            let (mark, tuple, total, dead, kept_waste, history_rows) = match admitted {
                Ok(admitted) => admitted,
                Err(error) => {
                    abort_run(&history, &store, &op_id);
                    return Err(error);
                }
            };
            let rebuild = total > 0 && dead.saturating_mul(10) >= total;
            metrics.end_phase();

            if !rebuild {
                // §3/§6.3 `none`: mark only. Both paths spill the closure to
                // `<store>.compact-visited`, so the none path writes it and
                // deletes it before it replies, and the reply counts it in
                // `peakTempBytes`. The transient `kept_node` rows the mark wrote
                // are cleared and the admission intent released, so the live
                // Store keeps no mark state and the generation is unchanged.
                let scratch = visited_path(&store);
                let visited_bytes = match write_visited(&scratch, &mark) {
                    Ok(bytes) => bytes,
                    Err(error) => {
                        abort_run(&history, &store, &op_id);
                        return Err(error);
                    }
                };
                if let Err(error) = remove_scratch(&scratch) {
                    abort_run(&history, &store, &op_id);
                    return Err(error);
                }
                if let Err(error) = history.clear_kept_node() {
                    abort_run(&history, &store, &op_id);
                    return Err(error);
                }
                let (free, page) = live_page_metrics(&session)?;
                history.gc_release(&op_id)?;
                let _ = std::fs::remove_file(op_path(&store));
                let _ = sync_directory(&store);
                drop(lane);
                return Ok(receipt(
                    CompactPath::None,
                    generation,
                    generation,
                    dead,
                    0,
                    free.saturating_mul(page),
                    0,
                    kept_waste,
                    visited_bytes,
                    CompactResidue::default(),
                    total,
                    history_rows,
                ));
            }

            // §7 step 2: a stop here leaves the open session, the writer lane,
            // the admission intent and the op record; the live Store is at `G`.
            stop_at(request, CompactFailpoint::BeforeSeal, &op, &store)?;

            // §7 step 2 onward: seal and rebuild. Only the session remains.
            // §5.1/§9: the admission intent is *not* released here. It is
            // copied into `<store>.compact`, lives through the build and the
            // install, and the cleanup settle clears it on the installed Store;
            // a definite failure before the rename releases it with a raw
            // delete (the run no longer holds a session), and a crash leaves the
            // op record for the next run to adopt.
            drop(history);
            drop(storage);
            let mut guard = WriterGuard::new(lane.into_writer());
            let session = match Arc::try_unwrap(session) {
                Ok(session) => session,
                Err(_) => {
                    // A definite failure before the seal: the live Store is
                    // unchanged, so its intent is released and the op record
                    // removed. A held clone elsewhere is `Busy`, never unknown.
                    abort_before_install(&store, Some(&op_id));
                    return Err(HistoryError::Busy);
                }
            };
            let writer = guard.take();
            let (facts, writer) = match session.seal_holding(writer) {
                Ok(sealed) => sealed,
                Err(error) => {
                    let mapped = seal_error(error);
                    if !mapped.unknown() {
                        // Definite: nothing was installed, so the run leaves no
                        // op record and no live intent behind.
                        abort_before_install(&store, Some(&op_id));
                    }
                    return Err(mapped);
                }
            };
            guard.put(writer);
            // §7 step 1.5: the sealed file is about to be read back.
            stop_at(request, CompactFailpoint::BeforeSealRead, &op, &store)?;

            // §7 step 1.5's fence: the seal must not have moved the generation
            // or the heads the mark recorded.
            let sealed_tuple = match immutable_tuple(&facts.path) {
                Ok(tuple) => tuple,
                Err(error) => {
                    abort_before_install(&store, Some(&op_id));
                    return Err(error);
                }
            };
            if sealed_tuple != tuple {
                abort_before_install(&store, Some(&op_id));
                return Err(HistoryError::Conflict("the Store changed after the mark"));
            }
            match immutable_digest(&facts.path) {
                Ok(digest) if digest == mark.live_heads_digest() => {}
                Ok(_) => {
                    abort_before_install(&store, Some(&op_id));
                    return Err(HistoryError::Conflict(
                        "the Store heads changed after the mark",
                    ));
                }
                Err(error) => {
                    abort_before_install(&store, Some(&op_id));
                    return Err(error);
                }
            }
            stop_at(request, CompactFailpoint::AfterSeal, &op, &store)?;

            metrics.begin_phase(CompactPhase::Build);
            op.phase = "build".to_owned();
            if let Err(error) = op.write(&store) {
                // A definite I/O failure after the seal: release the intent and
                // drop the artifact set, so the next call does not meet BusyOp.
                abort_before_install(&store, Some(&op_id));
                return Err(error);
            }
            let capacity = request.failpoint == Some(CompactFailpoint::CapacityAtBuild);
            let built = match build(
                BuildInput {
                    facts: &facts,
                    mark: &mark,
                    tuple: &tuple,
                    layout,
                    synchronous,
                },
                capacity,
                request,
                &op,
            ) {
                Ok(built) => built,
                // A stop point leaves the crash state on purpose: never abort
                // an artifact set a recorded run names (§9).
                Err(HistoryError::UnknownOutcome) => return Err(HistoryError::UnknownOutcome),
                Err(error) => {
                    abort_before_install(&store, Some(&op_id));
                    return Err(error);
                }
            };
            // §7 step 7: every artifact and the op record are durable before
            // the verification reads them back.
            if let Err(error) = sync_directory(&store) {
                abort_before_install(&store, Some(&op_id));
                return Err(error);
            }
            stop_at(request, CompactFailpoint::AfterBuild, &op, &store)?;
            stop_at(request, CompactFailpoint::AfterWalSwitch, &op, &store)?;
            metrics.end_phase();

            metrics.begin_phase(CompactPhase::Verify);
            op.phase = "verify".to_owned();
            if let Err(error) = op.write(&store) {
                abort_before_install(&store, Some(&op_id));
                return Err(error);
            }
            if let Err(error) = verify(
                VerifyInput {
                    store: &store,
                    built: &built,
                    mark: &mark,
                    tuple: &tuple,
                    layout,
                    acquisition: acquisition_schema,
                },
                request,
                &op,
            ) {
                if matches!(error, HistoryError::UnknownOutcome) {
                    return Err(error);
                }
                abort_before_install(&store, Some(&op_id));
                return Err(error);
            }
            stop_at(request, CompactFailpoint::AfterVerify, &op, &store)?;
            metrics.end_phase();

            metrics.begin_phase(CompactPhase::Install);
            if let Some(hook) = request.before_install_hook {
                hook(&facts.path);
            }
            if let Err(error) = install_fence(&facts, &tuple) {
                abort_before_install(&store, Some(&op_id));
                return Err(error);
            }
            if let Err(error) = prepare_target(&built.target) {
                abort_before_install(&store, Some(&op_id));
                return Err(error);
            }
            stop_at(request, CompactFailpoint::BeforeInstall, &op, &store)?;

            if std::fs::rename(&built.target, &facts.path).is_err() {
                // Installed-but-uncertain: never delete the new Store as if
                // nothing happened (§7 step 8, §9).
                op.phase = "install".to_owned();
                let _ = op.write(&store);
                return Err(HistoryError::UnknownOutcome);
            }
            // §7 step 7: the rename happened; a crash before the directory
            // fsync leaves an installed-but-uncertain `G + 1`.
            stop_at(request, CompactFailpoint::DuringRename, &op, &store)?;
            if let Err(_error) = sync_directory(&facts.path) {
                op.phase = "install".to_owned();
                let _ = op.write(&store);
                return Err(HistoryError::UnknownOutcome);
            }
            op.phase = "install".to_owned();
            let _ = op.write(&store);
            stop_at(request, CompactFailpoint::AfterInstall, &op, &store)?;
            guard.release()?;
            let residue = settle(&store, request, &op)?;
            let logical_discarded = u64::try_from(tuple.logical)
                .unwrap_or(0)
                .saturating_sub(built.logical);
            let physical_shrink = facts.bytes.saturating_sub(built.target_bytes);
            Ok(receipt(
                CompactPath::Rebuild,
                generation,
                generation.saturating_add(1),
                dead,
                logical_discarded,
                built.free_bytes,
                physical_shrink,
                kept_waste,
                built.peak_temp_bytes(),
                residue,
                total,
                history_rows,
            ))
        }
    }
}

/// The phase clock and §11 values one measured `compact` run observes.
struct RunMetrics {
    started: Instant,
    phase: Option<(CompactPhase, Instant)>,
    phase_nanos: Vec<(CompactPhase, u64)>,
    generation_before: u64,
    generation_after: u64,
    dead_bytes_bound: u64,
    dead_bytes_refined: u64,
    kept_waste: CompactKeptWaste,
    residue_bytes: u64,
    leftover_settled: bool,
}

impl RunMetrics {
    fn start() -> Self {
        Self {
            started: Instant::now(),
            phase: None,
            phase_nanos: Vec::new(),
            generation_before: 0,
            generation_after: 0,
            dead_bytes_bound: 0,
            dead_bytes_refined: 0,
            kept_waste: CompactKeptWaste::default(),
            residue_bytes: 0,
            leftover_settled: false,
        }
    }

    /// Ends the current phase and starts `phase`; a phase is inclusive wall
    /// time, never an exclusive CPU decomposition.
    fn begin_phase(&mut self, phase: CompactPhase) {
        self.end_phase();
        self.phase = Some((phase, Instant::now()));
    }

    fn end_phase(&mut self) {
        if let Some((phase, started)) = self.phase.take() {
            self.phase_nanos.push((
                phase,
                u64::try_from(started.elapsed().as_nanos()).unwrap_or(u64::MAX),
            ));
        }
    }

    /// Records this run once, whatever its result. The outcome and refusal
    /// come from the typed error, so a `Busy`/`Unresolved` refusal is counted
    /// without changing the error the caller receives.
    fn finish(mut self, result: &HistoryResult<CompactReceipt>) {
        self.end_phase();
        let total_nanos = u64::try_from(self.started.elapsed().as_nanos()).unwrap_or(u64::MAX);
        let (outcome, refusal) = match result {
            Ok(receipt) => {
                self.generation_after = receipt.generation_after;
                self.dead_bytes_bound = receipt.dead_bytes_bound;
                self.dead_bytes_refined = receipt.dead_bytes_refined;
                self.kept_waste = receipt.kept_waste.clone();
                self.residue_bytes = receipt.residue.bytes;
                let outcome = match receipt.path {
                    CompactPath::Rebuild => CompactOutcome::Rebuild,
                    CompactPath::None => CompactOutcome::None,
                };
                (outcome, None)
            }
            Err(error) => {
                if !error.unknown() {
                    self.generation_after = self.generation_before;
                }
                (
                    if error.unknown() {
                        CompactOutcome::Uncertain
                    } else {
                        CompactOutcome::Failed
                    },
                    match error {
                        HistoryError::Busy => Some(CompactRefusal::Busy),
                        HistoryError::Unresolved(_) => Some(CompactRefusal::Unresolved),
                        _ => None,
                    },
                )
            }
        };
        compact_metrics::record_run(&CompactRunObservation {
            outcome,
            generation_before: self.generation_before,
            generation_after: self.generation_after,
            dead_bytes_bound: self.dead_bytes_bound,
            dead_bytes_refined: self.dead_bytes_refined,
            kept_waste: self.kept_waste,
            residue_bytes: self.residue_bytes,
            phase_nanos: self.phase_nanos,
            total_nanos,
            leftover_settled: self.leftover_settled,
            refusal,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::{
        cap_delta_references, db_error, io_error, journal_bound_bytes, Built,
        DELTA_BASE_REFERENCES_CAP,
    };
    use layerfs_content::ObjectId;
    use layerfs_history::HistoryError;

    /// §7 step 3 on a bounded filesystem: `ENOSPC`, `EDQUOT` and `EFBIG` are
    /// the report's `Capacity` refusal, never a corruption verdict.
    #[test]
    fn bounded_filesystem_errors_are_capacity() {
        for code in [28, 122, 27, 69] {
            let error = io_error(std::io::Error::from_raw_os_error(code));
            assert!(
                matches!(error, HistoryError::Capacity(_)),
                "errno {code}: {error:?}"
            );
        }
    }

    /// Every other I/O failure keeps the integrity classification.
    #[test]
    fn another_io_error_is_integrity() {
        let error = io_error(std::io::Error::from_raw_os_error(5));
        assert!(matches!(error, HistoryError::Integrity(_)), "{error:?}");
    }

    /// §7 step 8: `SQLITE_NOMEM` is the report's `Capacity` refusal, not an
    /// uncertain outcome. A definite classification is what lets the caller
    /// remove the staged artifact set instead of keeping it as a leftover.
    #[test]
    fn sqlite_out_of_memory_is_capacity() {
        let error = rusqlite::Error::SqliteFailure(
            rusqlite::ffi::Error {
                code: rusqlite::ErrorCode::OutOfMemory,
                extended_code: 7,
            },
            None,
        );
        assert!(
            matches!(super::db_error(error), HistoryError::Capacity(_)),
            "SQLITE_NOMEM must be Capacity"
        );
    }

    /// §11/S7: `deltaBases.references` is capped, and the cap is reported; the
    /// bytes and the retained set are unaffected.
    #[test]
    fn delta_base_references_are_capped_and_the_cap_is_reported() {
        let short: Vec<ObjectId> = (0..3u8)
            .map(|index| ObjectId::from_bytes(&[index; 32]).unwrap())
            .collect();
        let (references, capped) = cap_delta_references(&short);
        assert_eq!(references.len(), 3);
        assert!(!capped);

        let long: Vec<ObjectId> = (0..=DELTA_BASE_REFERENCES_CAP)
            .map(|index| ObjectId::from_bytes(&[(index % 251) as u8; 32]).unwrap())
            .collect();
        let (references, capped) = cap_delta_references(&long);
        assert_eq!(references.len(), DELTA_BASE_REFERENCES_CAP);
        assert!(capped, "a list at the cap is reported as capped");
    }

    /// §11 `peakTempBytes` on the 50 GB arm (T-G66): the staged copy holds the
    /// source extent, the rewrite holds that copy beside the new Store, and the
    /// rollback journal is bounded by the staged page count. The receipt must
    /// cover the measured 143.1 GB pre-rebuild peak and the 44.6 GB journal the
    /// old `new + visited` formula dropped.
    #[test]
    fn peak_temp_covers_the_staged_copy_and_the_rollback_journal() {
        let page_size = 4096;
        // 50,862,854,144 B staged copy = the sealed Store's page count.
        let staged_pages = 12_417_689;
        let staged_bytes = staged_pages * page_size;
        let target_bytes = 45_763_203_072;
        let visited_bytes = 20_365_312;
        let journal = journal_bound_bytes(staged_pages, page_size);
        assert_eq!(journal, 50_962_203_848);
        let built = Built {
            target: std::path::PathBuf::new(),
            target_bytes,
            staged_bytes,
            journal_bound_bytes: journal,
            visited_bytes,
            logical: 0,
            canonical: 0,
            stored: 0,
            metadata: 0,
            free_bytes: 0,
        };
        let peak = built.peak_temp_bytes();
        assert_eq!(peak, 147_608_626_376);
        assert!(peak >= target_bytes + journal + visited_bytes);
        assert!(
            peak >= target_bytes + 44_630_503_928 + visited_bytes,
            "the bound must cover the 50 GB arm's observed 44.6 GB journal"
        );
        assert!(
            peak >= 143_100_000_000,
            "the bound must cover the measured 143.1 GB sampled 50 GB peak"
        );
    }
}

//! The `<store>.compact-op` sidecar record, its artifact paths and the crash failpoints.

use super::types::io_error;
use super::types::CompactFailpoint;
use super::types::CompactRequest;
use super::types::MarkTuple;
use super::types::ARTIFACT_SUFFIXES;
use super::types::COMPACT_SUFFIX;
use super::types::OP_SUFFIX;
use super::types::VISITED_SUFFIX;
use layerfs_history::error::HistoryError;
use layerfs_history::error::HistoryResult;
use std::collections::BTreeMap;
use std::path::Path;
use std::path::PathBuf;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;

/// One recorded compaction run beside the Store.
#[derive(Clone, Debug)]
pub(crate) struct RunOp {
    pub(crate) op_id: [u8; 16],
    pub(crate) generation: i64,
    pub(crate) phase: String,
    pub(crate) started_ms: i64,
    pub(crate) owner_pid: Option<u32>,
    pub(crate) owner_pid_start: Option<u64>,
    pub(crate) owner_pid_boot: Option<u128>,
    pub(crate) tuple: MarkTuple,
    pub(crate) live_heads_digest: [u8; 32],
}

impl RunOp {
    pub(crate) fn encode(&self) -> String {
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

    pub(crate) fn write(&self, store: &Path) -> HistoryResult<()> {
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

    pub(crate) fn read(store: &Path) -> Option<Self> {
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

pub(crate) fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| {
            i64::try_from(elapsed.as_millis()).unwrap_or(i64::MAX)
        })
}

pub(crate) fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        let _ = write!(out, "{byte:02x}");
    }
    out
}

pub(crate) fn unhex_vec(text: &str) -> Option<Vec<u8>> {
    if text.len() % 2 != 0 {
        return None;
    }
    (0..text.len())
        .step_by(2)
        .map(|index| u8::from_str_radix(&text[index..index + 2], 16).ok())
        .collect()
}

pub(crate) fn unhex<const N: usize>(text: &str) -> Option<[u8; N]> {
    let bytes = unhex_vec(text)?;
    bytes.try_into().ok()
}

pub(crate) fn with_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.as_os_str().to_os_string();
    name.push(suffix);
    PathBuf::from(name)
}

pub(crate) fn compact_path(store: &Path) -> PathBuf {
    with_suffix(store, COMPACT_SUFFIX)
}

pub(crate) fn visited_path(store: &Path) -> PathBuf {
    with_suffix(store, VISITED_SUFFIX)
}

pub(crate) fn op_path(store: &Path) -> PathBuf {
    with_suffix(store, OP_SUFFIX)
}

/// The incomplete op record an op write leaves when it dies before its rename.
pub(crate) fn op_temporary_path(store: &Path) -> PathBuf {
    with_suffix(&op_path(store), ".tmp")
}

/// The §9 SQLite artifact set: the fixed suffixes plus every multi-database
/// super-journal the staged file may have left (`<store>.compact-mj*`).
pub(crate) fn artifact_paths(store: &Path) -> Vec<PathBuf> {
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

/// Set by the real-process crash harness (T12.2's second arm): when present,
/// a reached failpoint connects to this Unix socket, writes one readiness
/// byte and blocks until the harness SIGKILLs the process. Production callers
/// never set it, so a failpoint keeps its in-process behaviour.
pub(crate) const CRASH_GATE_ENV: &str = "LAYERFS_COMPACT_CRASH_GATE";

/// Holds a reached failpoint until the test harness kills this process. The
/// variable being absent is a no-op: the in-process stop applies.
#[cfg(unix)]
pub(crate) fn crash_gate() -> HistoryResult<()> {
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
pub(crate) fn stop_at(
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

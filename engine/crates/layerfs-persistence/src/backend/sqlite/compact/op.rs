//! The `<store>.compact-op` sidecar record, its artifact paths and the crash failpoints.

use super::types::io_error;
use super::types::CompactFailpoint;
use super::types::CompactKeptWaste;
use super::types::CompactRequest;
use super::types::MarkTuple;
use super::types::OwnerLiveness;
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

/// The §11 totals a rebuild recorded once it built the installed Store, so a
/// later call that adopts an installed `G + 1` leftover answers the original
/// outcome instead of re-running the decision on the already rebuilt Store.
#[derive(Clone, Debug)]
pub(crate) struct InstalledReport {
    pub(crate) dead_bytes: u64,
    pub(crate) logical_discarded: u64,
    pub(crate) backend_reusable: u64,
    pub(crate) physical_shrink: u64,
    pub(crate) kept_waste: CompactKeptWaste,
    pub(crate) peak_temp: u64,
    pub(crate) total_stored: u64,
}

impl InstalledReport {
    /// The record's `installed_*` block; appended after the base fields so a
    /// record written before these existed still reads as uninstalled data.
    fn encode(&self) -> String {
        let references = self
            .kept_waste
            .delta_base_references
            .iter()
            .map(|reference| hex(reference))
            .collect::<Vec<_>>()
            .join(",");
        format!(
            "installed=1\ninstalled_dead={}\ninstalled_logical={}\n\
             installed_reusable={}\ninstalled_shrink={}\ninstalled_peak_temp={}\n\
             installed_total={}\ninstalled_kept_dead={}\ninstalled_kept_delta={}\n\
             installed_kept_inode={}\ninstalled_kept_capped={}\ninstalled_kept_refs={}\n",
            self.dead_bytes,
            self.logical_discarded,
            self.backend_reusable,
            self.physical_shrink,
            self.peak_temp,
            self.total_stored,
            self.kept_waste.dead_in_retained_packs_bytes,
            self.kept_waste.delta_bases_bytes,
            self.kept_waste.inode_blocks_bytes,
            u8::from(self.kept_waste.delta_base_references_capped),
            references,
        )
    }

    /// Reads the `installed_*` block; `None` when it is absent or unreadable,
    /// so a record written before it keeps the pre-install resume path.
    fn read(values: &BTreeMap<&str, &str>) -> Option<Self> {
        if values.get("installed") != Some(&"1") {
            return None;
        }
        let number = |key: &str| -> Option<u64> { values.get(key)?.parse().ok() };
        let references = values
            .get("installed_kept_refs")
            .filter(|text| !text.is_empty())
            .map(|text| {
                text.split(',')
                    .map(unhex::<32>)
                    .collect::<Option<Vec<[u8; 32]>>>()
            })
            .unwrap_or(Some(Vec::new()))?;
        Some(Self {
            dead_bytes: number("installed_dead")?,
            logical_discarded: number("installed_logical")?,
            backend_reusable: number("installed_reusable")?,
            physical_shrink: number("installed_shrink")?,
            kept_waste: CompactKeptWaste {
                dead_in_retained_packs_bytes: number("installed_kept_dead")?,
                delta_bases_bytes: number("installed_kept_delta")?,
                delta_base_references: references,
                delta_base_references_capped: values
                    .get("installed_kept_capped")
                    .is_some_and(|value| *value == "1"),
                inode_blocks_bytes: number("installed_kept_inode")?,
            },
            peak_temp: number("installed_peak_temp")?,
            total_stored: number("installed_total")?,
        })
    }
}

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
    /// The PID namespace the owner's id is meaningful in, when the kernel
    /// names one. A record written before this fact was kept leaves it unset
    /// and is matched by pid, start and boot alone.
    pub(crate) owner_pid_namespace: Option<u64>,
    pub(crate) tuple: MarkTuple,
    pub(crate) live_heads_digest: [u8; 32],
    /// True when the run that wrote this record has returned to its caller.
    /// Such a record is a leftover the next run judges, never an in-flight
    /// operation, even while the owner pid is still alive (§9).
    pub(crate) returned: bool,
    /// The §11 totals once the rebuild's target is built; `None` before any
    /// report value exists and in a record written before they were kept.
    pub(crate) installed: Option<InstalledReport>,
}

impl RunOp {
    pub(crate) fn encode(&self) -> String {
        let mut text = format!(
            "version=1\nop_id={}\ngeneration={}\nphase={}\nstarted_ms={}\n\
owner_pid={}\nowner_pid_start={}\nowner_pid_boot={}\nowner_pid_namespace={}\n\
catalog_id={}\nnext_history_serial={}\nnext_pack_id={}\nnext_ordinal={}\n\
mark_serial={}\nmark_pack_id={}\nmark_digest={}\nmark_totals={},{},{},{}\nmark_stamp={}\nhistory_rows={}\n\
live_heads_digest={}\nreturned={}\n",
            hex(&self.op_id),
            self.generation,
            self.phase,
            self.started_ms,
            self.owner_pid.map_or_else(String::new, |pid| pid.to_string()),
            self.owner_pid_start.map_or_else(String::new, |start| start.to_string()),
            self.owner_pid_boot.map_or_else(String::new, |boot| format!("{boot:032x}")),
            self.owner_pid_namespace.map_or_else(String::new, |namespace| namespace.to_string()),
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
            u8::from(self.returned),
        );
        if let Some(installed) = &self.installed {
            text.push_str(&installed.encode());
        }
        text
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
        // Optional: a record written before the namespace was kept leaves the
        // key out and is adopted by pid, start and boot alone, as it was then.
        let owner_pid_namespace = values
            .get("owner_pid_namespace")
            .filter(|text| !text.is_empty())
            .and_then(|text| text.parse().ok());
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
        // Optional: a record written before this marker existed stays an
        // in-flight run's until its owner is gone, as it was then.
        let returned = values.get("returned").is_some_and(|value| *value == "1");
        Some(Self {
            op_id,
            generation,
            phase,
            started_ms,
            owner_pid,
            owner_pid_start,
            owner_pid_boot,
            owner_pid_namespace,
            tuple,
            live_heads_digest,
            returned,
            installed: InstalledReport::read(&values),
        })
    }

    /// True when the caller's process facts prove the recorded owner is still
    /// live *and* the run has not returned, so this record is an in-flight
    /// run's, not a leftover (§9). A returned run's record stays a leftover
    /// the next call judges even while the owner pid lives: the run released
    /// the writer lane and left its state behind. A record with no owner
    /// facts, or whose owner is gone, is adoptable as before.
    pub(crate) fn owner_provably_live(&self, owner_live: Option<OwnerLiveness>) -> bool {
        !self.returned
            && owner_live.is_some_and(|live| {
                live(
                    self.owner_pid,
                    self.owner_pid_start,
                    self.owner_pid_boot,
                    self.owner_pid_namespace,
                )
            })
    }
}

/// True when `<store>.compact-op` still names `op_id`.
pub(crate) fn op_record_names(store: &Path, op_id: &[u8; 16]) -> bool {
    RunOp::read(store).is_some_and(|recorded| recorded.op_id == *op_id)
}

/// Removes `<store>.compact-op` only while it still names `op_id`: a record
/// another run wrote is never deleted. A record that names `op_id` and cannot
/// be removed is the `Integrity` refusal a settle or abort reports.
pub(crate) fn remove_op_record(store: &Path, op_id: &[u8; 16]) -> HistoryResult<()> {
    if !op_record_names(store, op_id) {
        return Ok(());
    }
    let path = op_path(store);
    if std::fs::remove_file(&path).is_err() && std::fs::symlink_metadata(&path).is_ok() {
        return Err(HistoryError::Integrity("compaction record"));
    }
    Ok(())
}

/// Marks this run's record as returned to its caller before the call replies,
/// so the next `store.compact` judges a leftover even though the owner pid is
/// still alive (§9). Only the run's own record is marked: a record another run
/// wrote in the settle window is never touched.
pub(crate) fn mark_returned(store: &Path, op_id: &[u8; 16]) -> HistoryResult<()> {
    let Some(mut op) = RunOp::read(store) else {
        return Ok(());
    };
    if op.op_id != *op_id || op.returned {
        return Ok(());
    }
    op.returned = true;
    op.write(store)
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
    /// is unknown. `store.status` reports `busy` while an in-flight owner
    /// lives and `unresolved` for a returned run's leftover or a gone owner.
    pub uncertain: bool,
    /// True when the run that wrote the record returned to its caller. A
    /// returned record is a leftover, so its state is `unresolved`, never
    /// `busy`, even while its owner pid is alive.
    pub returned: bool,
}

impl CompactOpStatus {
    /// The `store.status` projection of this operation; `alive` says whether
    /// the owning process is still running. An in-flight live owner is `busy`;
    /// a returned run's leftover is `unresolved` even while its owner pid
    /// lives, and so is a gone owner's. `uncertain` stays the record's own
    /// fact: it is true for every unsettled run, live or leftover.
    #[must_use]
    pub fn status(&self, alive: bool) -> crate::gc::compact::CompactStatus {
        // A read in the admission window still projects a §11 phase: the run
        // is about to mark, and `admission` is not one of the four.
        let phase = if self.phase == "admission" {
            "mark".to_owned()
        } else {
            self.phase.clone()
        };
        crate::gc::compact::CompactStatus {
            id: format!("op-{}", hex(&self.op_id)),
            name: "store.compact",
            phase,
            started: self.started_ms,
            uncertain: self.uncertain,
            state: if alive && !self.returned {
                "busy"
            } else {
                "unresolved"
            },
            phases: &crate::gc::compact::SQLITE_COMPACT_PHASES,
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
        returned: op.returned,
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

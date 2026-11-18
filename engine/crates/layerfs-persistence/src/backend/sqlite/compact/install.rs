//! §10 install fence, directory sync, atomic rename and §9 residue settling.

use super::build::table_present;
use super::op::artifact_paths;
use super::op::compact_path;
use super::op::hex;
use super::op::op_path;
use super::op::op_temporary_path;
use super::op::remove_op_record;
use super::op::stop_at;
use super::op::RunOp;
use super::receipt::receipt;
use super::types::db_error;
use super::types::io_error;
use super::types::CompactFailpoint;
use super::types::CompactPath;
use super::types::CompactReceipt;
use super::types::CompactRequest;
use super::types::CompactResidue;
use super::types::MarkTuple;
use super::types::OwnerLiveness;
use super::verify::file_tuple;
use super::verify::open_immutable;
use crate::backend::sqlite::lane::writer_admission::Writer;
use crate::backend::sqlite::lifecycle::seal::SealedFacts;
use crate::compact_metrics::RunMetrics;
use layerfs_history::error::HistoryError;
use layerfs_history::error::HistoryResult;
use layerfs_history::error::OpConflict;
use layerfs_history::records::GcIntentKind;
use layerfs_history::records::GcScopeKind;
use layerfs_history::HistoryCatalog;
use rusqlite::params;
use rusqlite::Connection;
use rusqlite::OpenFlags;
use std::path::Path;
use std::path::PathBuf;
use std::time::SystemTime;

pub(crate) fn source_state(store: &Path) -> HistoryResult<(u64, SystemTime, u32)> {
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
pub(crate) fn sidecar_present(path: &Path) -> Option<PathBuf> {
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
pub(crate) fn install_fence(facts: &SealedFacts, tuple: &MarkTuple) -> HistoryResult<()> {
    // §7 step 7: the path must still name the sealed file, so the rename
    // cannot install over a different Store. This is the sealed-state fence,
    // not the §10 session fence: the writer lane is held across seal and
    // rename, so a changed identity is a `Conflict` with no writes;
    // `StaleStore` is for a session that opens or transacts after the install.
    if let Some(sealed) = facts.identity {
        match sealed.names(&facts.path) {
            Ok(true) => {}
            Ok(false) => {
                return Err(HistoryError::Conflict("the sealed Store identity changed"));
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Err(HistoryError::Conflict("the sealed Store identity changed"));
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
pub(crate) fn prepare_target(target: &Path) -> HistoryResult<()> {
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

pub(crate) fn sync_directory(store: &Path) -> HistoryResult<()> {
    if let Some(parent) = store.parent() {
        let directory = std::fs::File::open(parent).map_err(io_error)?;
        directory.sync_all().map_err(io_error)?;
    }
    Ok(())
}

/// Removes the fixed artifact set and reports only what remains, so a
/// successful settle has a zero residue (§11).
pub(crate) fn clear_artifacts(store: &Path) -> CompactResidue {
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
pub(crate) fn settle(
    store: &Path,
    request: &CompactRequest,
    op: &RunOp,
) -> HistoryResult<CompactResidue> {
    // §11: the settle records its own phase, so a crash here reads `cleanup`.
    let mut cleaning = op.clone();
    cleaning.phase = "cleanup".to_owned();
    let _ = cleaning.write(store);
    let mut residue = clear_artifacts(store);
    // §9: the op settles only after the artifact set is gone. A crash here
    // leaves the installed `G + 1` and the record for the next run.
    stop_at(request, CompactFailpoint::DuringSettle, op, store)?;
    let record_path = op_path(store);
    // §10: this is a write to the installed Store, so it takes the writers'
    // lane like every other. An admitted writer has no busy handler: if this
    // transaction ran beside it, that writer would fail `Busy`. The run let
    // go of its own hold after the rename, so this one queues behind whoever
    // took the lane since. A lane that cannot be taken leaves the record for
    // the next run, like a Store that could not be cleared.
    let settled = match Writer::acquire(store) {
        Ok(writer) => {
            let cleared = clear_installed_mark(store, &op.op_id);
            writer.release().is_ok() && cleared
        }
        Err(_) => false,
    };
    // §9/§11: the record settles only after the whole artifact set is gone. A
    // residue `clear_artifacts` could not remove keeps it, so the next run
    // adopts the recorded leftover instead of refusing the staged name as an
    // unrecorded `Exists`; `abort_before_install` gates the same way.
    if settled && residue.artifacts.is_empty() {
        // §9: the record is removed only while it still names this run's
        // operation, never one another run wrote in the window where this run
        // no longer holds the writer lane.
        if remove_op_record(store, &op.op_id).is_err() {
            if let Ok(metadata) = std::fs::symlink_metadata(&record_path) {
                residue.bytes = residue.bytes.saturating_add(metadata.len());
                residue.artifacts.push(record_path.file_name().map_or_else(
                    || record_path.display().to_string(),
                    |name| name.to_string_lossy().into_owned(),
                ));
            }
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
/// session, so this is the only handle on the Store. The caller holds the
/// writer lane: the run's own before the rename, a fresh hold in [`settle`]
/// after it. A busy Store keeps the op record for the next run instead.
pub(crate) fn clear_installed_mark(store: &Path, op_id: &[u8; 16]) -> bool {
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

/// Removes the §6.3 visited scratch a `none` path created: the file and any
/// journal SQLite might have left. `none` replies only after it is gone, so it
/// leaves no residue (§6.3, §9).
pub(crate) fn remove_scratch(scratch: &Path) -> HistoryResult<()> {
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
/// rename; the live Store is unchanged apart from the seal's checkpoint, the
/// released admission intent and the cleared `kept_node` closure the committed
/// mark left (§8/§9). `op_id` names this run's operation; `release_intent`
/// says whether a committed admission intent is still there to release (the
/// pre-seal abort does; the rolled-back `none` mark and the already-released
/// failure before the seal do not).
pub(crate) fn abort_before_install(store: &Path, op_id: &[u8; 16], release_intent: bool) {
    let released = !release_intent || clear_installed_mark(store, op_id);
    let residue = clear_artifacts(store);
    if residue.artifacts.is_empty() && released {
        // §9: only this run's record is removed, never one another run wrote.
        let _ = remove_op_record(store, op_id);
    }
    let _ = sync_directory(store);
}

/// Releases the admission intent and the transient §6.1 `kept_node` closure of
/// a definite failure before the seal, then removes the artifact set and the op
/// record. When the release itself fails the op record is kept as the next
/// run's recovery breadcrumb (§9): deleting it would leave a live intent that
/// no later call could settle.
pub(crate) fn abort_run(history: &crate::HistoryProvider, store: &Path, op_id: &[u8; 16]) {
    if history.gc_release(op_id).is_ok() {
        // The committed mark's closure is transient state: no abort may leave
        // it behind for a later run to mistake for a mark.
        let _ = history.clear_kept_node();
        abort_before_install(store, op_id, false);
    }
}

/// Judges and removes a leftover an earlier run recorded (§9): the artifact set
/// it names and the admission intent it took. The caller already holds the
/// writer lane, so a leftover owner is gone.
///
/// `None` means there was nothing to resume (or the leftover was at `G`, so the
/// caller decides on the still-live Store). `Some` is the installed `G + 1`
/// outcome the record kept: the rename installed the build and only its settle
/// was lost, so the original §11 report is answered from the record instead of
/// re-running the §3 decision on the already rebuilt Store.
pub(crate) fn settle_leftover(
    history: &crate::HistoryProvider,
    store: &Path,
    generation: u64,
    metrics: &mut RunMetrics,
    owner_live: Option<OwnerLiveness>,
) -> HistoryResult<Option<CompactReceipt>> {
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
        return Ok(None);
    };
    // §9: the record names `G`. A Store still at `G` was never installed, so
    // its artifact set is removed and the run is settled. At `G+1` (or a
    // Store that advanced further past an older record) the rename installed a
    // rebuild, so only the record and the intent it kept remain. A record from
    // a *later* generation does not describe this Store, so nothing it names is
    // deleted.
    let current =
        i64::try_from(generation).map_err(|_| HistoryError::Integrity("Store generation"))?;
    if op.generation > current {
        return Err(HistoryError::Integrity("compaction record generation"));
    }
    // §9: a leftover is adopted only when its owner is gone. The run releases
    // the writer lane before its `cleanup` settle, so a second call can hold
    // the lane while the first run still owns this record; adopting it would
    // release the first run's intent and clear its `kept_node` closure, its
    // artifacts and the record itself. A provably live owner keeps its state
    // and the caller reports it busy.
    if op.owner_provably_live(owner_live) {
        // §10: the refusal names what holds the Store. A plain `Busy` would
        // carry an empty detail; the record already knows the entity, its
        // phase and its operation identity.
        return Err(HistoryError::BusyOp(Box::new(OpConflict::new(
            GcIntentKind::Compaction,
            GcScopeKind::Store,
            "store",
            Some(op.phase.clone()),
            hex(&op.op_id),
            false,
        ))));
    }
    // Adopt the recorded owner's admission intent before its artifact is gone,
    // and clear the transient §6.1 `kept_node` closure its mark left: the
    // installed Store carries a copy of it, and a pre-install leftover carries
    // the committed mark's rows in the live Store.
    history.gc_release(&op.op_id)?;
    history.clear_kept_node()?;
    let residue = clear_artifacts(store);
    if !residue.artifacts.is_empty() {
        return Err(HistoryError::Integrity(
            "recorded compaction artifact could not be removed",
        ));
    }
    // §9: the record is this run's to remove only while it still names the
    // operation just adopted.
    remove_op_record(store, &op.op_id)?;
    metrics.leftover_settled = true;
    // §9: `G + 1` means the rename installed the build. The record's own §11
    // totals answer the original outcome, so a resumed install reports the
    // same `Rebuild` report the interrupted run would have. A record written
    // before the totals were kept falls back to the pre-install resume: the
    // caller decides on the installed Store.
    let generation_before = u64::try_from(op.generation).unwrap_or(0);
    if generation == generation_before.saturating_add(1) {
        if let Some(installed) = op.installed {
            return Ok(Some(receipt(
                CompactPath::Rebuild,
                generation_before,
                generation,
                installed.dead_bytes,
                installed.logical_discarded,
                installed.backend_reusable,
                installed.physical_shrink,
                installed.kept_waste,
                installed.peak_temp,
                CompactResidue::default(),
                installed.total_stored,
                u64::try_from(op.tuple.history_rows).unwrap_or(0),
            )));
        }
    }
    Ok(None)
}

/// True while the Store holds an Init or merge working row. Admission
/// already refuses a live Init/merge intent; this catches the leftover rows
/// of an operation whose intent was lost, which a rebuild must not wipe
/// without refusing (§5).
pub(crate) fn unfinished_work(store: &Path) -> HistoryResult<bool> {
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
pub(crate) struct WriterGuard(Option<Writer>);

impl WriterGuard {
    pub(crate) fn new(writer: Writer) -> Self {
        Self(Some(writer))
    }

    pub(crate) fn take(&mut self) -> Writer {
        self.0.take().expect("a compaction holds its writer")
    }

    pub(crate) fn put(&mut self, writer: Writer) {
        self.0 = Some(writer);
    }

    pub(crate) fn release(&mut self) -> HistoryResult<()> {
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

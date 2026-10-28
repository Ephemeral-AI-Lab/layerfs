//! One atomic bounded namespace job: several inode/name finals, one ticket.
use crate::namespace::job_rows::JobRows;
use crate::{
    db::integer,
    inode::{check, check_name},
    sql, BaseSource, Binding, Changes, Generation, Inode, NameLayers, Overlay, OverlayError,
    OverlayResult, Publication, StatementKind, WorkspaceState, COMPOUND_DIRECTORY_ENTRIES,
    COMPOUND_INODES,
};

/// Consistent local rows for one owner job over an exact owned base source.
/// The single owner runs a job to completion, so these point reads and the
/// job's one `apply` observe the same state without holding a transaction
/// across caller logic. It must not outlive its job.
#[derive(Clone, Copy)]
pub struct SourceRows<'a> {
    db: &'a Overlay,
    source: BaseSource,
    state: WorkspaceState,
    /// The running atomic job's own reads, when its rows are also the rows
    /// it publishes over.
    seen: Option<&'a JobRows>,
}
/// Validated name keys of one compound job, computed before its transaction.
pub(crate) struct CheckedChanges {
    keys: Vec<(i64, Option<i64>)>,
    moved_directory: Option<(i64, i64)>,
}
impl Overlay {
    /// Checks source custody once for the bounded point reads of one job.
    pub fn source_rows(&self, source: BaseSource) -> OverlayResult<SourceRows<'_>> {
        let state = self.source_state(source)?;
        Ok(self.source_rows_at(source, state, None))
    }
    /// Rows of a source whose custody this transaction has already checked
    /// against `state`. `seen` belongs to the one atomic job that is
    /// running: it reads each inode row once, however many evaluation
    /// rounds ask for it.
    pub(crate) fn source_rows_at<'a>(
        &'a self,
        source: BaseSource,
        state: WorkspaceState,
        seen: Option<&'a JobRows>,
    ) -> SourceRows<'a> {
        SourceRows {
            db: self,
            source,
            state,
            seen,
        }
    }
    /// Publishes every final value of one namespace operation or none of them.
    /// Values are semantically checked by Workspace against rows read in this
    /// same owner job. A closed Workspace refuses; a failed job changes nothing
    /// and leaves no ticket. Exactly one reply-attempt ticket covers the job.
    pub fn apply(&self, source: BaseSource, changes: &Changes) -> OverlayResult<Publication> {
        let checked = self.check_changes(changes)?;
        self.atomic(|| {
            let state = self.source_state(source)?;
            self.apply_checked(source, state, None, None, changes, &checked)
        })
    }
    /// Window and grammar checks of one compound job, before any transaction.
    pub(crate) fn check_changes(&self, changes: &Changes) -> OverlayResult<CheckedChanges> {
        if changes.inodes.len() > COMPOUND_INODES
            || changes.directory_entries.len() > COMPOUND_DIRECTORY_ENTRIES
            || (changes.inodes.is_empty() && changes.directory_entries.is_empty())
        {
            return Err(OverlayError::Invalid("compound window"));
        }
        if changes.detached.is_some_and(|serial| {
            !changes
                .inodes
                .iter()
                .any(|i| i.serial == serial && i.nlink == 0)
        }) {
            return Err(OverlayError::Invalid("removed inode final"));
        }
        if changes.created.is_some_and(|serial| {
            !changes
                .inodes
                .iter()
                .any(|i| i.serial == serial && i.nlink != 0)
        }) {
            return Err(OverlayError::Invalid("created inode final"));
        }
        let moved_directory = changes.moved_directory.map(|(serial, parent)| {
            if serial == parent || !changes.directory_entries.iter().any(|entry| {
                entry.parent == parent && matches!(entry.binding, crate::Binding::Bound { serial: target, .. } if target == serial)
            }) {
                return Err(OverlayError::Invalid("moved directory final binding"));
            }
            Ok((integer(serial)?, integer(parent)?))
        }).transpose()?;
        for (index, inode) in changes.inodes.iter().enumerate() {
            check(inode)?;
            if changes.inodes[..index]
                .iter()
                .any(|other| other.serial == inode.serial)
            {
                return Err(OverlayError::Invalid("duplicate compound inode"));
            }
        }
        let mut keys = Vec::with_capacity(changes.directory_entries.len());
        for (index, change) in changes.directory_entries.iter().enumerate() {
            let parent = check_name(change.parent, &change.name)?;
            let target = match change.binding {
                Binding::Bound { serial: 0, .. } => {
                    return Err(OverlayError::Invalid("zero inode serial"))
                }
                Binding::Bound { serial, .. } => Some(integer(serial)?),
                Binding::Removed { .. } => None,
            };
            if changes.directory_entries[..index]
                .iter()
                .any(|other| other.parent == change.parent && other.name == change.name)
            {
                return Err(OverlayError::Invalid("duplicate compound name"));
            }
            keys.push((parent, target));
        }
        if let Some((serial, cell)) = &changes.cell {
            crate::payload::check(cell)?;
            if !changes.inodes.iter().any(|inode| inode.serial == *serial) {
                return Err(OverlayError::Invalid("compound cell owner"));
            }
        }
        if let Some(write) = &changes.write {
            // The owner's final value must already cover the written window.
            let end = write.offset.checked_add(write.data.len() as u64);
            let owner = changes
                .inodes
                .iter()
                .find(|inode| inode.serial == write.serial && inode.kind == crate::InodeKind::File);
            if write.data.is_empty()
                || write.data.len() > crate::WRITE_WINDOW
                || owner.is_none_or(|inode| end.is_none_or(|end| end > inode.size))
            {
                return Err(OverlayError::Invalid("compound write window"));
            }
        }
        if changes.predecessor.is_some_and(|(moved, replaced)| {
            moved == replaced
                || !changes.directory_entries.iter().any(|entry| {
                    matches!(entry.binding, Binding::Bound { serial, .. } if serial == moved)
                })
                || !changes
                    .inodes
                    .iter()
                    .any(|inode| inode.serial == replaced && inode.kind == crate::InodeKind::File)
        }) {
            return Err(OverlayError::Invalid("rename predecessor"));
        }
        crate::payload::pieces::check_claims(changes)?;
        Ok(CheckedChanges {
            keys,
            moved_directory,
        })
    }
    /// The transaction body of one checked compound job. The caller owns the
    /// transaction, so kernel kernel custody can commit with the publication.
    /// `state` is the Workspace row this job read when it checked the
    /// source's custody, `held` the descriptor its fence read and `seen` the
    /// inode rows its evaluation read: none of them is read again here.
    pub(crate) fn apply_checked(
        &self,
        source: BaseSource,
        state: WorkspaceState,
        held: Option<crate::OpenFile>,
        seen: Option<&JobRows>,
        changes: &Changes,
        checked: &CheckedChanges,
    ) -> OverlayResult<Publication> {
        let CheckedChanges {
            keys,
            moved_directory,
        } = checked;
        if state.closed {
            return Err(OverlayError::Closed);
        }
        if let Some(file) = changes.open {
            if file.engine() != source.engine
                || changes.inodes.len() != 1
                || changes.inodes[0].serial != file.serial()
                || !changes.directory_entries.is_empty()
            {
                return Err(OverlayError::Invalid("descriptor mutation domain"));
            }
            if held != Some(file) {
                self.check_file(file, true)?;
            } else if !file.writable {
                return Err(OverlayError::Invalid("read-only descriptor"));
            }
        }
        self.resume_digest(changes)?;
        let carried = self.carried_digests(changes)?;
        let mut inodes = 0_i64;
        let mut layers = Vec::with_capacity(changes.inodes.len());
        for inode in &changes.inodes {
            let (added, layer) = self.put_inode_domain(
                &state,
                inode,
                changes.open.is_some(),
                changes.created == Some(inode.serial),
                seen,
            )?;
            inodes += i64::from(added);
            layers.push((inode.serial, layer));
        }
        let layer = |serial: u64| {
            layers
                .iter()
                .find(|(owner, _)| *owner == serial)
                .map(|(_, layer)| layer)
                .ok_or(OverlayError::Invalid("compound payload owner"))
        };
        let mut directory_entries = 0_i64;
        for (change, (parent, target)) in changes.directory_entries.iter().zip(keys) {
            let inherited = match change.binding {
                Binding::Bound { inherited, .. } | Binding::Removed { inherited } => inherited,
            };
            // Computed once: it decides the row and is the value the row records.
            let (added, lower) =
                self.name_inheritance(&state, *parent, &change.name, inherited, seen)?;
            if target.is_some() || lower {
                self.bind_name(&state, *parent, &change.name, *target, lower)?;
                directory_entries += i64::from(added);
            } else {
                // Nothing below binds this name: no row is the final state.
                directory_entries -= self.execute(
                    StatementKind::DirectoryEntry,
                    sql::DIRECTORY_ENTRY_DROP,
                    &[parent, &change.name, &state.active.0],
                    24 + change.name.len() as u64,
                )? as i64;
            }
        }
        if let Some((serial, parent)) = moved_directory {
            let key = *serial as u64;
            let held = self.open.borrow().parents.rows.get(&key).copied();
            if let Some((mount, old)) = held {
                if old != *serial as u64 {
                    self.open
                        .borrow_mut()
                        .parents
                        .put(key, (mount, *parent as u64));
                }
            }
        }
        if let Some((moved, replaced)) = changes.predecessor {
            let fresh = changes
                .inodes
                .iter()
                .any(|inode| inode.serial == replaced && source.created_above(inode.born));
            self.record_predecessor(moved, replaced, fresh)?;
        }
        if let Some((serial, cell)) = &changes.cell {
            self.put_cell(integer(*serial)?, layer(*serial)?, cell)?;
        }
        if let Some(write) = &changes.write {
            let (serial, layer) = (integer(write.serial)?, layer(write.serial)?);
            match &changes.claims {
                None => self.write_cells(serial, layer, write.offset, &write.data)?,
                Some(claims) => self.write_claimed(serial, layer, write, claims)?,
            }
        }
        for inode in &changes.inodes {
            if inode.kind == crate::InodeKind::File || changes.detached == Some(inode.serial) {
                self.detach_orphan(&state, inode)?;
            }
        }
        self.follow_digests(&state, changes, &carried)?;
        self.settle(&state, inodes, directory_entries)
    }
}
impl SourceRows<'_> {
    pub const fn source(&self) -> BaseSource {
        self.source
    }
    /// Active generation for values this job will publish.
    pub const fn active(&self) -> Generation {
        self.state.active
    }
    /// Latest local inode row of the current view; None delegates to the base.
    /// Inside a job that keeps its reads, a serial is read once.
    pub fn inode(&self, serial: u64) -> OverlayResult<Option<Inode>> {
        let state = &self.state;
        let Some(seen) = self.seen else {
            return self.db.inode_at(serial, state.active, state.installed);
        };
        let key = integer(serial)?;
        if let Some(inode) = seen.seen(key) {
            return Ok(inode);
        }
        // An orphan-domain row is not a row of the view: it is not kept.
        if let Some(orphan) = self.db.orphan_inode(key)? {
            return Ok(Some(orphan));
        }
        let row = self.db.local_row(key, state.active, state.installed)?;
        seen.record(key, &row);
        Ok(row.map(|row| row.inode))
    }
    /// The active row and the latest lower row of one name, in one seek.
    /// Inside a job that keeps its reads, a name is read once.
    pub fn name(&self, parent: u64, name: &[u8]) -> OverlayResult<NameLayers> {
        let key = check_name(parent, name)?;
        self.db.name_layers_seen(&self.state, key, name, self.seen)
    }
}

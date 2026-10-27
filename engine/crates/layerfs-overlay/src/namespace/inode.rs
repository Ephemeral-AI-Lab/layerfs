//! Bounded metadata/payload publication, with one transaction per attempted job.
use crate::layers::Layer;
use crate::namespace::job_rows::{JobRows, LocalRow};
use crate::{
    db::{integer, unsigned},
    sql, Cell, DirectoryEntry, Generation, Inode, InodeKind, NameLayers, Overlay, OverlayError,
    OverlayResult, Publication, StatementKind, WorkspaceState,
};

pub(crate) fn decode(row: &rusqlite::Row<'_>) -> rusqlite::Result<Inode> {
    decode_at(row, 0)
}
/// The eleven inode columns starting at column `at`.
fn decode_at(row: &rusqlite::Row<'_>, at: usize) -> rusqlite::Result<Inode> {
    let kind = match row.get::<_, i64>(at + 1)? {
        1 => InodeKind::File,
        2 => InodeKind::Directory,
        3 => InodeKind::Symlink,
        _ => return Err(rusqlite::Error::InvalidQuery),
    };
    Ok(Inode {
        serial: unsigned(row, at)?,
        kind,
        mode: row.get(at + 2)?,
        mtime_seconds: row.get(at + 3)?,
        mtime_nanoseconds: row.get(at + 4)?,
        nlink: unsigned(row, at + 5)?,
        size: unsigned(row, at + 6)?,
        inherited_cutoff: unsigned(row, at + 7)?,
        born: unsigned(row, at + 8)?,
        entries: unsigned(row, at + 9)?,
        subdirs: unsigned(row, at + 10)?,
    })
}
/// One local row in the column order of `INODE_LOOKUP`, starting at column
/// `at`.
pub(crate) fn local_row_at(row: &rusqlite::Row<'_>, at: usize) -> rusqlite::Result<LocalRow> {
    Ok(LocalRow {
        inode: decode_at(row, at)?,
        gen: row.get(at + 11)?,
        epoch: row.get(at + 12)?,
        height: row.get(at + 13)?,
        referenced: row.get(at + 14)?,
        sequence: row.get(at + 15)?,
    })
}
pub(crate) fn optional_serial(value: Option<i64>, column: usize) -> rusqlite::Result<Option<u64>> {
    value
        .map(|v| u64::try_from(v).map_err(|_| rusqlite::Error::IntegralValueOutOfRange(column, v)))
        .transpose()
}
/// Representable-value grammar only; filesystem semantics belong to Workspace.
pub(crate) fn check(inode: &Inode) -> OverlayResult<()> {
    let allowed = match inode.kind {
        InodeKind::Directory => 0o1777,
        _ => 0o777,
    };
    if inode.serial == 0
        || inode.mode & !allowed != 0
        || (inode.kind == InodeKind::Symlink && inode.mode != 0o777)
        || inode.mtime_nanoseconds >= 1_000_000_000
        || (inode.kind != InodeKind::Directory && (inode.entries != 0 || inode.subdirs != 0))
    {
        return Err(OverlayError::Invalid("inode metadata"));
    }
    for value in [
        inode.serial,
        inode.size,
        inode.nlink,
        inode.inherited_cutoff,
        inode.born,
        inode.entries,
        inode.subdirs,
    ] {
        integer(value)?;
    }
    Ok(())
}
pub(crate) fn check_name(parent: u64, name: &[u8]) -> OverlayResult<i64> {
    if name.is_empty() || name.len() > 255 || parent == 0 {
        return Err(OverlayError::Invalid("directory_entry window"));
    }
    integer(parent)
}
impl Overlay {
    /// The live half of an mtime-only capture (STORAGE_BATCH_SPEC §4.8):
    /// construction kept the base row of `serial`, so its install alone
    /// would show the base's stored time. When the active generation has no
    /// row of the serial, its latest captured row is carried there with every
    /// byte falling through to the lower view: the captured layer before the
    /// install, the identical base after it. A serial the active generation
    /// already changed keeps that newer row. One atomic job; sends nothing to
    /// the kernel, whose view does not change. True when a row was written.
    /// A carried row is not counted dirty: the serial is kept in
    /// `kept_live` until a writer of its active row counts it (`unkeep`) or
    /// the next Capture clears the set (#359 E10).
    pub fn keep_live_row(&self, reader: crate::CapturedReader, serial: u64) -> OverlayResult<bool> {
        self.atomic(|| {
            self.check_captured_reader(reader)?;
            let state = self.live()?;
            let key = integer(serial)?;
            let row = self
                .local_row(key, state.active, state.installed)?
                .ok_or(OverlayError::Invalid("kept live row"))?;
            if row.gen == state.active.0 {
                return Ok(false);
            }
            self.put_inode(&state, &row.inode)?;
            let revision = state
                .revision
                .checked_add(1)
                .ok_or(OverlayError::Invalid("revision exhausted"))?;
            self.open.borrow_mut().kept_live.put(key, ());
            self.change_state(|state| {
                state.revision = revision;
                Ok(())
            })?;
            Ok(true)
        })
    }
    /// True when `serial`'s active row was kept live and not written since.
    /// It is no longer kept: the caller writes that row and counts it dirty.
    pub(crate) fn unkeep(&self, serial: i64) -> bool {
        let mut open = self.open.borrow_mut();
        let kept = open.kept_live.rows.contains_key(&serial);
        if kept {
            open.kept_live.remove(serial);
        }
        kept
    }
    /// Latest local inode at the current view; None delegates to immutable base.
    pub fn inode(&self, serial: u64) -> OverlayResult<Option<Inode>> {
        let state = self.live()?;
        self.inode_at(serial, state.active, state.installed)
    }
    pub(crate) fn inode_at(
        &self,
        serial: u64,
        gen: Generation,
        installed: i64,
    ) -> OverlayResult<Option<Inode>> {
        let serial = integer(serial)?;
        if let Some(orphan) = self.orphan_inode(serial)? {
            return Ok(Some(orphan));
        }
        Ok(self.local_row(serial, gen, installed)?.map(|row| row.inode))
    }
    /// The latest local row of one serial within `(installed, gen]`, with
    /// the generation that holds it and its payload-layer columns.
    pub(crate) fn local_row(
        &self,
        serial: i64,
        gen: Generation,
        installed: i64,
    ) -> OverlayResult<Option<LocalRow>> {
        Ok(self
            .query(
                StatementKind::Inode,
                sql::INODE_LOOKUP,
                &[&serial, &gen.0, &installed],
                32,
                |row| local_row_at(row, 0),
            )?
            .pop())
    }
    /// Writes one checked inode at the active generation inside the caller's
    /// transaction and returns its payload layer. The first row of a serial in
    /// a generation starts a fresh layer whose cutoff is the lower view's
    /// length; a smaller size on a regular file is a shrink of that layer.
    pub(crate) fn put_inode(
        &self,
        state: &WorkspaceState,
        inode: &Inode,
    ) -> OverlayResult<(bool, Layer)> {
        self.put_inode_domain(state, inode, false, false, None)
    }
    /// The same write for one inode of a compound job. `created` is a serial
    /// reserved for this job: no row of it exists, so none is read and its
    /// row is inserted. `seen` holds the rows this job read: a serial it
    /// read is written without reading its layer rows again, as an insert
    /// when the active generation has no row of it and as an update of the
    /// columns that changed when it has. A serial the job did not read is
    /// classified by its active and lower layer rows. Every write is a plain
    /// insert or update: a row that exists is never inserted over and a row
    /// that is gone is never created by an update.
    pub(crate) fn put_inode_domain(
        &self,
        state: &WorkspaceState,
        inode: &Inode,
        owned: bool,
        created: bool,
        seen: Option<&JobRows>,
    ) -> OverlayResult<(bool, Layer)> {
        let serial = integer(inode.serial)?;
        let orphan = owned && inode.nlink == 0 && self.orphan(serial)?.is_some();
        let gen = if orphan {
            crate::lifetime::orphan::DOMAIN
        } else {
            state.active.0
        };
        if inode.born > state.active.0 as u64 {
            return Err(OverlayError::Invalid("inode creation generation"));
        }
        // The job read the view's rows, not the orphan domain's.
        let known = match seen {
            Some(seen) if !orphan => seen.take(serial),
            _ => None,
        };
        // A row this job inserts holds no piece and has no earlier sequence.
        let new = (false, -1);
        let fresh = (inode.inherited_cutoff, inode.inherited_cutoff, 0, 0);
        let (active, (size, cutoff, epoch, height), (referenced, sequence)) = match known {
            _ if created => (false, fresh, new),
            // The active row's value as this job read it.
            Some(Some(row)) if row.gen == gen => (
                true,
                (
                    row.inode.size,
                    row.inode.inherited_cutoff,
                    row.epoch,
                    row.height,
                ),
                (row.referenced, row.sequence),
            ),
            // The latest row is a lower generation's: its length is inherited.
            Some(Some(row)) => (false, (row.inode.size, row.inode.size, 0, 0), new),
            Some(None) => (false, fresh, new),
            None => match self
                .query(
                    StatementKind::Inode,
                    sql::LAYER_ACTIVE,
                    &[&serial, &gen],
                    24,
                    |r| {
                        Ok((
                            (unsigned(r, 0)?, unsigned(r, 1)?, r.get(2)?, r.get(3)?),
                            (r.get(4)?, r.get(5)?),
                        ))
                    },
                )?
                .pop()
            {
                Some((layer, piece)) => (true, layer, piece),
                None => {
                    let lower = self
                        .query(
                            StatementKind::Inode,
                            sql::LAYER_LOWER,
                            &[&serial, &gen, &state.installed],
                            32,
                            |r| unsigned(r, 0),
                        )?
                        .pop()
                        .unwrap_or(inode.inherited_cutoff);
                    (false, (lower, lower, 0, 0), new)
                }
            },
        };
        let mut layer = Layer {
            gen,
            kind: inode.kind,
            nlink: inode.nlink,
            size,
            cutoff,
            epoch,
            height,
            referenced,
            sequence,
        };
        if inode.kind == InodeKind::File && inode.size < layer.size {
            self.shrink(serial, &mut layer, inode.size)?;
        }
        layer.size = inode.size;
        let changed = self.execute(
            StatementKind::Inode,
            if active {
                sql::INODE_UPDATE
            } else {
                sql::INODE_INSERT
            },
            &[
                &serial,
                &gen,
                &(inode.kind as i64),
                &i64::from(inode.mode),
                &inode.mtime_seconds,
                &i64::from(inode.mtime_nanoseconds),
                &integer(inode.nlink)?,
                &integer(inode.size)?,
                &integer(layer.cutoff)?,
                &integer(inode.born)?,
                &integer(inode.entries)?,
                &integer(inode.subdirs)?,
                &layer.epoch,
                &layer.height,
                // The revision this job publishes: no later row write of
                // this serial can carry the same value.
                &(state.revision + 1),
            ],
            128,
        )?;
        if changed != 1 {
            return Err(OverlayError::Invalid("inode row of the running job"));
        }
        // A kept row is active but was never counted: this write counts it.
        let kept = !orphan && self.unkeep(serial);
        Ok(((!active && !orphan) || kept, layer))
    }
    /// The active row and the latest lower row of one name within
    /// `(floor, top]`, in one seek of the name's rows.
    pub(crate) fn name_layers(
        &self,
        parent: i64,
        name: &[u8],
        top: i64,
        floor: i64,
    ) -> OverlayResult<NameLayers> {
        let mut rows = self
            .query(
                StatementKind::DirectoryEntry,
                sql::NAME_LAYERS,
                &[&parent, &name, &top, &floor],
                32 + name.len() as u64,
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        optional_serial(row.get(1)?, 1)?,
                        row.get::<_, bool>(2)?,
                    ))
                },
            )?
            .into_iter();
        let (active, lower) = match rows.next() {
            Some(row) if row.0 == top => (Some(row), rows.next()),
            row => (None, row),
        };
        Ok(NameLayers {
            active: active.map(|row| row.1),
            active_inherited: active.map(|row| row.2),
            lower: lower.map(|row| row.1),
        })
    }
    /// The same rows at the current view for a job that keeps its reads:
    /// the statement runs for the job's first read of the name only.
    pub(crate) fn name_layers_seen(
        &self,
        state: &WorkspaceState,
        parent: i64,
        name: &[u8],
        seen: Option<&JobRows>,
    ) -> OverlayResult<NameLayers> {
        if let Some(layers) = seen.and_then(|seen| seen.name(parent, name)) {
            return Ok(layers);
        }
        let layers = self.name_layers(parent, name, state.active.0, state.installed)?;
        if let Some(seen) = seen {
            seen.record_name(parent, name, layers);
        }
        Ok(layers)
    }
    /// Whether binding one name adds a row to the active generation, and
    /// whether the view below that generation binds the name: an active row
    /// has recorded it, a lower local row decides it, and otherwise the
    /// caller's base fact does. `seen` holds the name rows the running job
    /// read: a name it read is not read again.
    pub(crate) fn name_inheritance(
        &self,
        state: &WorkspaceState,
        parent: i64,
        name: &[u8],
        base: bool,
        seen: Option<&JobRows>,
    ) -> OverlayResult<(bool, bool)> {
        let layers = self.name_layers_seen(state, parent, name, seen)?;
        Ok(match layers.active_inherited {
            Some(inherited) => (false, inherited),
            None => (true, layers.lower.map_or(base, |serial| serial.is_some())),
        })
    }
    /// Writes one name row at the active generation: a binding or a whiteout,
    /// with the inheritance its job computed once.
    pub(crate) fn bind_name(
        &self,
        state: &WorkspaceState,
        parent: i64,
        name: &[u8],
        target: Option<i64>,
        inherited: bool,
    ) -> OverlayResult<()> {
        self.execute(
            StatementKind::DirectoryEntry,
            sql::DIRECTORY_ENTRY_PUT,
            &[&parent, &name, &state.active.0, &target, &inherited],
            33 + name.len() as u64,
        )?;
        Ok(())
    }
    /// Binds or whiteouts one name at the active generation inside the caller's
    /// transaction; true when the active generation gained a row.
    pub(crate) fn put_directory_entry(
        &self,
        state: &WorkspaceState,
        parent: i64,
        name: &[u8],
        target: Option<i64>,
        base: bool,
    ) -> OverlayResult<bool> {
        let (added, inherited) = self.name_inheritance(state, parent, name, base, None)?;
        self.bind_name(state, parent, name, target, inherited)?;
        Ok(added)
    }
    /// One reply-attempt ticket and revision for a whole atomic job.
    pub(crate) fn settle(
        &self,
        state: &WorkspaceState,
        inodes: i64,
        directory_entries: i64,
    ) -> OverlayResult<Publication> {
        let revision = state
            .revision
            .checked_add(1)
            .ok_or(OverlayError::Invalid("revision exhausted"))?;
        self.dirty_counts(inodes, directory_entries)?;
        self.change_state(|state| {
            state.revision = revision;
            Ok(())
        })?;
        let publication = Publication {
            engine: self.identity,
            revision,
            generation: state.active,
        };
        self.issue(publication);
        Ok(publication)
    }
    /// Publishes a bounded first-path job and returns retained reply-attempt custody.
    /// The caller supplies semantically checked final values. Atomic namespace
    /// operations with several affected inodes use the compound job instead.
    pub fn publish(
        &self,
        inode: &Inode,
        name: Option<&DirectoryEntry>,
        cell: Option<&Cell>,
    ) -> OverlayResult<Publication> {
        check(inode)?;
        let key = name
            .map(|name| {
                let parent = check_name(name.parent, &name.name)?;
                let target = match name.serial {
                    Some(0) => return Err(OverlayError::Invalid("zero inode serial")),
                    Some(serial) => Some(integer(serial)?),
                    None => None,
                };
                Ok((parent, target))
            })
            .transpose()?;
        if let Some(cell) = cell {
            crate::payload::check(cell)?;
        }
        self.atomic(|| {
            let state = self.live()?;
            let (added, layer) = self.put_inode(&state, inode)?;
            let mut directory_entries = 0;
            if let (Some(name), Some((parent, target))) = (name, key) {
                directory_entries = i64::from(self.put_directory_entry(
                    &state,
                    parent,
                    &name.name,
                    target,
                    name.inherited,
                )?);
            }
            if let Some(cell) = cell {
                self.put_cell(integer(inode.serial)?, &layer, cell)?;
            }
            if inode.kind == InodeKind::File {
                self.detach_orphan(&state, inode)?;
            }
            self.settle(&state, i64::from(added), directory_entries)
        })
    }
    /// Final local name row, with a whiteout distinguishable from no overlay row.
    pub fn directory_entry(
        &self,
        parent: u64,
        name: &[u8],
    ) -> OverlayResult<Option<DirectoryEntry>> {
        let state = self.live()?;
        let parent = integer(parent)?;
        Ok(self
            .query(
                StatementKind::DirectoryEntry,
                sql::DIRECTORY_ENTRY_LOOKUP,
                &[&parent, &name, &state.active.0, &state.installed],
                24 + name.len() as u64,
                |r| {
                    Ok(DirectoryEntry {
                        inherited: r.get(1)?,
                        parent: parent as u64,
                        name: name.to_vec(),
                        serial: optional_serial(r.get(0)?, 0)?,
                    })
                },
            )?
            .pop())
    }
}

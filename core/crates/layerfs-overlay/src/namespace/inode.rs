//! Bounded metadata/payload publication, with one transaction per attempted job.
use crate::layers::Layer;
use crate::{
    db::{integer, unsigned},
    sql, Cell, DirectoryEntry, Generation, Inode, InodeKind, Overlay, OverlayError, OverlayResult,
    Publication, Route, StatementKind, WorkspaceState,
};

pub(crate) fn decode(row: &rusqlite::Row<'_>) -> rusqlite::Result<Inode> {
    let kind = match row.get::<_, i64>(1)? {
        1 => InodeKind::File,
        2 => InodeKind::Directory,
        3 => InodeKind::Symlink,
        _ => return Err(rusqlite::Error::InvalidQuery),
    };
    Ok(Inode {
        serial: unsigned(row, 0)?,
        kind,
        mode: row.get(2)?,
        mtime_seconds: row.get(3)?,
        mtime_nanoseconds: row.get(4)?,
        nlink: unsigned(row, 5)?,
        size: unsigned(row, 6)?,
        inherited_cutoff: unsigned(row, 7)?,
        born: unsigned(row, 8)?,
        entries: unsigned(row, 9)?,
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
        || (inode.kind != InodeKind::Directory && inode.entries != 0)
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
    /// Latest local inode at the current view; None delegates to immutable base.
    pub fn inode(&self, route: Route, serial: u64) -> OverlayResult<Option<Inode>> {
        let state = self.live(route)?;
        self.inode_at(route, serial, state.active, state.installed)
    }
    pub(crate) fn inode_at(
        &self,
        route: Route,
        serial: u64,
        gen: Generation,
        installed: i64,
    ) -> OverlayResult<Option<Inode>> {
        let serial = integer(serial)?;
        if let Some(orphan) = self.orphan_inode(route.ns, serial)? {
            return Ok(Some(orphan));
        }
        Ok(self
            .query(
                StatementKind::Inode,
                sql::INODE_LOOKUP,
                &[&route.ns, &serial, &gen.0, &installed],
                32,
                decode,
            )?
            .pop())
    }
    /// Writes one checked inode at the active generation inside the caller's
    /// transaction and returns its payload layer. The first row of a serial in
    /// a generation starts a fresh layer whose cutoff is the lower view's
    /// length; a smaller size on a regular file is a shrink of that layer.
    pub(crate) fn put_inode(
        &self,
        route: Route,
        state: &WorkspaceState,
        inode: &Inode,
    ) -> OverlayResult<(bool, Layer)> {
        self.put_inode_domain(route, state, inode, false)
    }
    pub(crate) fn put_inode_domain(
        &self,
        route: Route,
        state: &WorkspaceState,
        inode: &Inode,
        owned: bool,
    ) -> OverlayResult<(bool, Layer)> {
        let serial = integer(inode.serial)?;
        let orphan = owned && inode.nlink == 0 && self.orphan(route.ns, serial)?.is_some();
        let gen = if orphan {
            crate::lifetime::orphan::DOMAIN
        } else {
            state.active.0
        };
        if inode.born > state.active.0 as u64 {
            return Err(OverlayError::Invalid("inode creation generation"));
        }
        let active = self
            .query(
                StatementKind::Inode,
                sql::LAYER_ACTIVE,
                &[&route.ns, &serial, &gen],
                24,
                |r| Ok((unsigned(r, 0)?, unsigned(r, 1)?, r.get(2)?, r.get(3)?)),
            )?
            .pop();
        let added = active.is_none();
        let (size, cutoff, epoch, height) = match active {
            Some(layer) => layer,
            None => {
                let lower = self
                    .query(
                        StatementKind::Inode,
                        sql::LAYER_LOWER,
                        &[&route.ns, &serial, &gen, &state.installed],
                        32,
                        |r| unsigned(r, 0),
                    )?
                    .pop()
                    .unwrap_or(inode.inherited_cutoff);
                (lower, lower, 0, 0)
            }
        };
        let mut layer = Layer {
            gen,
            kind: inode.kind,
            nlink: inode.nlink,
            size,
            cutoff,
            epoch,
            height,
        };
        if inode.kind == InodeKind::File && inode.size < layer.size {
            self.shrink(route.ns, serial, &mut layer, inode.size)?;
        }
        layer.size = inode.size;
        self.execute(
            StatementKind::Inode,
            sql::INODE_PUT,
            &[
                &route.ns,
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
                &layer.epoch,
                &layer.height,
            ],
            112,
        )?;
        Ok((added && !orphan, layer))
    }
    /// Binds or whiteouts one name at the active generation inside the caller's
    /// transaction; true when the active generation gained a row.
    pub(crate) fn name_inheritance(
        &self,
        state: &WorkspaceState,
        ns: i64,
        parent: i64,
        name: &[u8],
        base: bool,
    ) -> OverlayResult<(bool, bool)> {
        let active = self
            .query(
                StatementKind::DirectoryEntry,
                sql::DIRECTORY_ENTRY_ACTIVE,
                &[&ns, &parent, &name, &state.active.0],
                24 + name.len() as u64,
                |r| r.get::<_, bool>(1),
            )?
            .pop();
        if let Some(inherited) = active {
            return Ok((false, inherited));
        }
        let lower = self
            .query(
                StatementKind::DirectoryEntry,
                sql::DIRECTORY_ENTRY_LOOKUP,
                &[&ns, &parent, &name, &(state.active.0 - 1), &state.installed],
                32 + name.len() as u64,
                |r| r.get::<_, Option<i64>>(0),
            )?
            .pop();
        Ok((true, lower.map_or(base, |serial| serial.is_some())))
    }
    pub(crate) fn put_directory_entry(
        &self,
        route: Route,
        state: &WorkspaceState,
        parent: i64,
        name: &[u8],
        target: Option<i64>,
        base: bool,
    ) -> OverlayResult<bool> {
        let (added, inherited) = self.name_inheritance(state, route.ns, parent, name, base)?;
        self.execute(
            StatementKind::DirectoryEntry,
            sql::DIRECTORY_ENTRY_PUT,
            &[
                &route.ns,
                &parent,
                &name,
                &state.active.0,
                &target,
                &inherited,
            ],
            33 + name.len() as u64,
        )?;
        Ok(added)
    }
    /// One reply-attempt ticket and revision for a whole atomic job.
    pub(crate) fn settle(
        &self,
        route: Route,
        state: &WorkspaceState,
        inodes: i64,
        directory_entries: i64,
    ) -> OverlayResult<Publication> {
        let revision = state
            .revision
            .checked_add(1)
            .ok_or(OverlayError::Invalid("revision exhausted"))?;
        self.execute(
            StatementKind::Workspace,
            sql::FRONTIER_ADVANCE,
            &[&route.ns, &revision, &inodes, &directory_entries],
            32,
        )?;
        let publication = Publication {
            route,
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
        route: Route,
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
            let state = self.live(route)?;
            let (added, layer) = self.put_inode(route, &state, inode)?;
            let mut directory_entries = 0;
            if let (Some(name), Some((parent, target))) = (name, key) {
                directory_entries = i64::from(self.put_directory_entry(
                    route,
                    &state,
                    parent,
                    &name.name,
                    target,
                    name.inherited,
                )?);
            }
            if let Some(cell) = cell {
                self.put_cell(route, integer(inode.serial)?, &layer, cell)?;
            }
            if inode.kind == InodeKind::File {
                self.detach_orphan(route, &state, inode)?;
            }
            self.settle(route, &state, i64::from(added), directory_entries)
        })
    }
    /// Final local name row, with a whiteout distinguishable from no overlay row.
    pub fn directory_entry(
        &self,
        route: Route,
        parent: u64,
        name: &[u8],
    ) -> OverlayResult<Option<DirectoryEntry>> {
        let state = self.live(route)?;
        let parent = integer(parent)?;
        Ok(self
            .query(
                StatementKind::DirectoryEntry,
                sql::DIRECTORY_ENTRY_LOOKUP,
                &[&route.ns, &parent, &name, &state.active.0, &state.installed],
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

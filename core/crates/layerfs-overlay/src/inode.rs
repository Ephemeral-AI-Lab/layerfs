//! Bounded metadata/payload publication, with one transaction per attempted job.
use crate::{
    db::{integer, unsigned},
    sql, Cell, Dentry, Generation, Inode, InodeKind, Overlay, OverlayError, OverlayResult,
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
        return Err(OverlayError::Invalid("dentry window"));
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
    /// transaction; true when the active generation gained a row.
    pub(crate) fn put_inode(
        &self,
        route: Route,
        generation: Generation,
        inode: &Inode,
    ) -> OverlayResult<bool> {
        if inode.born > generation.0 as u64 {
            return Err(OverlayError::Invalid("inode creation generation"));
        }
        let serial = integer(inode.serial)?;
        let added = self
            .query(
                StatementKind::Inode,
                sql::INODE_ACTIVE,
                &[&route.ns, &serial, &generation.0],
                24,
                |r| r.get::<_, i64>(0),
            )?
            .is_empty();
        self.execute(
            StatementKind::Inode,
            sql::INODE_PUT,
            &[
                &route.ns,
                &serial,
                &generation.0,
                &(inode.kind as i64),
                &i64::from(inode.mode),
                &inode.mtime_seconds,
                &i64::from(inode.mtime_nanoseconds),
                &integer(inode.nlink)?,
                &integer(inode.size)?,
                &integer(inode.inherited_cutoff)?,
                &integer(inode.born)?,
                &integer(inode.entries)?,
            ],
            96,
        )?;
        Ok(added)
    }
    /// Binds or whiteouts one name at the active generation inside the caller's
    /// transaction; true when the active generation gained a row.
    pub(crate) fn put_name(
        &self,
        route: Route,
        generation: Generation,
        parent: i64,
        name: &[u8],
        target: Option<i64>,
    ) -> OverlayResult<bool> {
        let added = self
            .query(
                StatementKind::Dentry,
                sql::DENTRY_ACTIVE,
                &[&route.ns, &parent, &name, &generation.0],
                24 + name.len() as u64,
                |r| r.get::<_, Option<i64>>(0),
            )?
            .is_empty();
        self.execute(
            StatementKind::Dentry,
            sql::DENTRY_PUT,
            &[&route.ns, &parent, &name, &generation.0, &target],
            32 + name.len() as u64,
        )?;
        Ok(added)
    }
    /// One reply-attempt ticket and revision for a whole atomic job.
    pub(crate) fn settle(
        &self,
        route: Route,
        state: &WorkspaceState,
        inodes: i64,
        names: i64,
    ) -> OverlayResult<Publication> {
        let revision = state
            .revision
            .checked_add(1)
            .ok_or(OverlayError::Invalid("revision exhausted"))?;
        self.execute(
            StatementKind::Frontier,
            sql::TICKET_PUT,
            &[&route.ns, &revision, &state.active.0],
            24,
        )?;
        self.execute(
            StatementKind::Workspace,
            sql::FRONTIER_ADVANCE,
            &[&route.ns, &revision, &inodes, &names],
            32,
        )?;
        Ok(Publication {
            route,
            revision,
            generation: state.active,
        })
    }
    /// Publishes a bounded first-path job and returns retained reply-attempt custody.
    /// The caller supplies semantically checked final values. Atomic namespace
    /// operations with several affected inodes use the compound job instead.
    pub fn publish(
        &self,
        route: Route,
        inode: &Inode,
        name: Option<&Dentry>,
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
            let inodes = i64::from(self.put_inode(route, state.active, inode)?);
            let mut names = 0;
            if let (Some(name), Some((parent, target))) = (name, key) {
                names =
                    i64::from(self.put_name(route, state.active, parent, &name.name, target)?);
            }
            if let Some(cell) = cell {
                self.put_cell(route, integer(inode.serial)?, state.active, cell)?;
            }
            self.settle(route, &state, inodes, names)
        })
    }
    /// Final local name row, with a whiteout distinguishable from no overlay row.
    pub fn dentry(&self, route: Route, parent: u64, name: &[u8]) -> OverlayResult<Option<Dentry>> {
        let state = self.live(route)?;
        let parent = integer(parent)?;
        Ok(self
            .query(
                StatementKind::Dentry,
                sql::DENTRY_LOOKUP,
                &[&route.ns, &parent, &name, &state.active.0, &state.installed],
                24 + name.len() as u64,
                |r| {
                    Ok(Dentry {
                        parent: parent as u64,
                        name: name.to_vec(),
                        serial: optional_serial(r.get(0)?, 0)?,
                    })
                },
            )?
            .pop())
    }
}

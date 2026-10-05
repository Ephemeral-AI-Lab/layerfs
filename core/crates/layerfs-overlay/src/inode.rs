//! Bounded metadata/payload publication, with one transaction per attempted job.
use crate::{
    db::{integer, unsigned},
    sql, Cell, Dentry, Generation, Inode, InodeKind, Overlay, OverlayError, OverlayResult,
    Publication, Route, StatementKind,
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
        mtime_ns: row.get(3)?,
        nlink: unsigned(row, 4)?,
        size: unsigned(row, 5)?,
        inherited_cutoff: unsigned(row, 6)?,
    })
}
impl Overlay {
    /// Latest local inode at the current view; None delegates to immutable base.
    pub fn inode(&self, route: Route, serial: u64) -> OverlayResult<Option<Inode>> {
        let state = self.live(route)?;
        self.inode_at(route, serial, state.active)
    }
    pub(crate) fn inode_at(
        &self,
        route: Route,
        serial: u64,
        gen: Generation,
    ) -> OverlayResult<Option<Inode>> {
        let serial = integer(serial)?;
        Ok(self
            .query(
                StatementKind::Inode,
                sql::INODE_LOOKUP,
                &[&route.ns, &serial, &gen.0],
                24,
                decode,
            )?
            .pop())
    }
    /// Publishes a bounded first-path job and returns retained reply-attempt custody.
    /// The caller supplies semantically checked final values. Namespace compounds
    /// with multiple affected inodes belong to the later S4 transaction surface.
    pub fn publish(
        &self,
        route: Route,
        inode: &Inode,
        name: Option<&Dentry>,
        cell: Option<&Cell>,
    ) -> OverlayResult<Publication> {
        let serial = integer(inode.serial)?;
        if serial == 0 || inode.mode > 0o777 {
            return Err(OverlayError::Invalid("inode metadata"));
        }
        let size = integer(inode.size)?;
        let nlink = integer(inode.nlink)?;
        let cutoff = integer(inode.inherited_cutoff)?;
        if let Some(name) = name {
            if name.name.is_empty() || name.name.len() > 255 || name.parent == 0 {
                return Err(OverlayError::Invalid("dentry window"));
            }
            integer(name.parent)?;
            if let Some(serial) = name.serial {
                if serial == 0 {
                    return Err(OverlayError::Invalid("zero inode serial"));
                }
                integer(serial)?;
            }
        }
        if let Some(cell) = cell {
            crate::payload::check(cell)?;
        }
        self.atomic(|| {
            let state = self.live(route)?;
            let revision = state
                .revision
                .checked_add(1)
                .ok_or(OverlayError::Invalid("revision exhausted"))?;
            let generation = state.active;
            let existed = self
                .query(
                    StatementKind::Inode,
                    "SELECT 1 FROM inode WHERE ns=?1 AND serial=?2 AND gen=?3",
                    &[&route.ns, &serial, &generation.0],
                    24,
                    |r| r.get::<_, i64>(0),
                )?
                .len();
            self.execute(
                StatementKind::Inode,
                "INSERT INTO inode VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9)
                 ON CONFLICT(ns,serial,gen) DO UPDATE SET kind=excluded.kind,mode=excluded.mode,
                 mtime_ns=excluded.mtime_ns,nlink=excluded.nlink,size=excluded.size,
                 inherited_cutoff=excluded.inherited_cutoff",
                &[
                    &route.ns,
                    &serial,
                    &generation.0,
                    &(inode.kind as i64),
                    &i64::from(inode.mode),
                    &inode.mtime_ns,
                    &nlink,
                    &size,
                    &cutoff,
                ],
                72,
            )?;
            let mut new_name = 0;
            if let Some(name) = name {
                let parent = integer(name.parent)?;
                let target = name.serial.map(integer).transpose()?;
                new_name = i64::from(
                    self.query(
                        StatementKind::Dentry,
                        "SELECT 1 FROM dentry WHERE ns=?1 AND parent=?2 AND name=?3 AND gen=?4",
                        &[&route.ns, &parent, &name.name, &generation.0],
                        24 + name.name.len() as u64,
                        |r| r.get::<_, i64>(0),
                    )?
                    .is_empty(),
                );
                self.execute(
                    StatementKind::Dentry,
                    "INSERT INTO dentry VALUES(?1,?2,?3,?4,?5)
                     ON CONFLICT(ns,parent,name,gen) DO UPDATE SET serial=excluded.serial",
                    &[&route.ns, &parent, &name.name, &generation.0, &target],
                    32 + name.name.len() as u64,
                )?;
            }
            if let Some(cell) = cell {
                self.put_cell(route, serial, generation, cell)?;
            }
            self.execute(
                StatementKind::Frontier,
                "INSERT INTO request VALUES(?1,?2)",
                &[&route.ns, &revision],
                16,
            )?;
            self.execute(
                StatementKind::Workspace,
                "UPDATE workspace SET revision=?2,dirty_inodes=dirty_inodes+?3,
                 dirty_names=dirty_names+?4 WHERE ns=?1",
                &[&route.ns, &revision, &i64::from(existed == 0), &new_name],
                32,
            )?;
            Ok(Publication {
                route,
                revision,
                generation,
            })
        })
    }
    /// Final local name row, with a whiteout distinguishable from no overlay row.
    pub fn dentry(&self, route: Route, parent: u64, name: &[u8]) -> OverlayResult<Option<Dentry>> {
        let state = self.live(route)?;
        let parent = integer(parent)?;
        Ok(self
            .query(
                StatementKind::Dentry,
                "SELECT serial FROM dentry WHERE ns=?1 AND parent=?2 AND name=?3 AND gen<=?4
             ORDER BY gen DESC LIMIT 1",
                &[&route.ns, &parent, &name, &state.active.0],
                24 + name.len() as u64,
                |r| {
                    Ok(Dentry {
                        parent: parent as u64,
                        name: name.to_vec(),
                        serial: r
                            .get::<_, Option<i64>>(0)?
                            .map(|v| {
                                u64::try_from(v)
                                    .map_err(|_| rusqlite::Error::IntegralValueOutOfRange(0, v))
                            })
                            .transpose()?,
                    })
                },
            )?
            .pop())
    }
}

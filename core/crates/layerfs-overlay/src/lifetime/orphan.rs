//! Independent mutable orphan domain and its exact immutable inheritance root.
use crate::{
    db::integer,
    inode,
    layers::Layer,
    maintenance::{ORPHAN, SERIAL_RETIRE},
    sql, FileRead, Inode, LocalRead, Overlay, OverlayError, OverlayResult, Route, StatementKind,
    WorkspaceState,
};
pub(crate) const DOMAIN: i64 = -1;
#[derive(Clone, Copy)]
pub(crate) struct Orphan {
    pub root: [u8; 32],
    pub top: i64,
    pub floor: i64,
}
impl Overlay {
    pub(crate) fn orphan(&self, ns: i64, serial: i64) -> OverlayResult<Option<Orphan>> {
        self.query(
            StatementKind::Lease,
            crate::sql::ORPHAN_LOOKUP,
            &[&ns, &serial],
            16,
            |r| {
                let root: Vec<u8> = r.get(0)?;
                Ok(Orphan {
                    root: root.try_into().map_err(|_| rusqlite::Error::InvalidQuery)?,
                    top: r.get(1)?,
                    floor: r.get(2)?,
                })
            },
        )
        .map(|mut rows| rows.pop())
    }
    pub(crate) fn orphan_holds(&self, ns: i64, serial: i64, gen: i64) -> OverlayResult<bool> {
        Ok(self
            .orphan(ns, serial)?
            .is_some_and(|o| gen > o.floor && gen <= o.top))
    }
    pub(crate) fn orphan_inode(&self, ns: i64, serial: i64) -> OverlayResult<Option<Inode>> {
        self.query(StatementKind::Inode,"SELECT serial,kind,mode,mtime_seconds,mtime_nanoseconds,nlink,size,inherited_cutoff,born,entries
            FROM inode WHERE ns=?1 AND serial=?2 AND gen=-1",&[&ns,&serial],16,inode::decode).map(|mut rows|rows.pop())
    }
    /// Last unlink creates only ownership/metadata. Its at-most-two lower
    /// domains are retained, never copied or folded in the unlink transaction.
    pub(crate) fn detach_orphan(
        &self,
        route: Route,
        state: &WorkspaceState,
        inode: &Inode,
    ) -> OverlayResult<()> {
        if inode.nlink != 0 {
            return Ok(());
        }
        let serial = integer(inode.serial)?;
        let orphan = self.orphan(route.ns, serial)?;
        if orphan.is_none() {
            self.enqueue(route.ns, SERIAL_RETIRE, serial, state.active.0)?;
        }
        if orphan.is_some() || self.file_refs(route.ns, serial)? == 0 {
            return Ok(());
        }
        self.execute(
            StatementKind::Lease,
            "INSERT INTO orphan VALUES(?1,?2,?3,?4,?5)",
            &[
                &route.ns,
                &serial,
                &state.base_root.as_slice(),
                &state.active.0,
                &state.installed,
            ],
            56,
        )?;
        self.execute(
            StatementKind::Inode,
            sql::INODE_PUT,
            &[
                &route.ns,
                &serial,
                &DOMAIN,
                &(inode.kind as i64),
                &i64::from(inode.mode),
                &inode.mtime_seconds,
                &i64::from(inode.mtime_nanoseconds),
                &0_i64,
                &integer(inode.size)?,
                &integer(inode.size)?,
                &integer(inode.born)?,
                &0_i64,
                &0_i64,
                &0_i64,
            ],
            112,
        )?;
        self.enqueue(route.ns, ORPHAN, serial, DOMAIN)
    }
    pub(crate) fn orphan_layer(&self, ns: i64, serial: i64) -> OverlayResult<Layer> {
        self.layers(ns, serial, DOMAIN, DOMAIN - 1)?
            .pop()
            .ok_or(OverlayError::Stale)
    }
    pub(crate) fn release_orphan_layer(
        &self,
        ns: i64,
        serial: i64,
        gen: i64,
        next: i64,
    ) -> OverlayResult<()> {
        self.execute(
            StatementKind::Lease,
            "UPDATE orphan SET lower_top=?3 WHERE ns=?1 AND serial=?2",
            &[&ns, &serial, &next],
            24,
        )?;
        // A generation cursor that already skipped this independent source
        // does not restart. The orphan's last reference owns targeted cleanup.
        self.enqueue(ns, SERIAL_RETIRE, serial, gen)?;
        self.execute(
            StatementKind::Reclaim,
            "UPDATE maintenance SET ready=1 WHERE ns=?1 AND kind=?2 AND resource=?3 AND target=?4",
            &[&ns, &SERIAL_RETIRE, &serial, &gen],
            32,
        )?;
        self.maintenance_ready.set(true);
        Ok(())
    }
    /// One current descriptor-read window. Its independent reader/source
    /// reference survives descriptor close and protects base demand outside SQL.
    pub fn read_file(
        &self,
        read: FileRead,
        offset: u64,
        length: u32,
    ) -> OverlayResult<Option<LocalRead>> {
        self.check_file_read(read)?;
        let ns = read.source.route.ns;
        let serial = integer(read.serial)?;
        let Some(orphan) = self.orphan(ns, serial)? else {
            return self.source_read(read.source, read.serial, offset, length);
        };
        self.orphan_read(ns, serial, orphan, offset, length)
    }
    pub(crate) fn orphan_read(
        &self,
        ns: i64,
        serial: i64,
        orphan: Orphan,
        offset: u64,
        length: u32,
    ) -> OverlayResult<Option<LocalRead>> {
        let mut layers = vec![self.orphan_layer(ns, serial)?];
        layers.extend(self.layers(ns, serial, orphan.top, orphan.floor)?);
        if layers.len() > 3 {
            return Err(OverlayError::Invalid("orphan composition invariant"));
        }
        let mut plan = self.compose_layers(ns, serial, offset, length, layers)?;
        if let Some(ref mut plan) = plan {
            plan.base_root = Some(orphan.root)
        }
        Ok(plan)
    }
}

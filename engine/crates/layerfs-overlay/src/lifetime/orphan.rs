//! Independent mutable orphan domain and its exact immutable inheritance root.
use crate::{
    db::integer,
    inode,
    layers::Layer,
    maintenance::{ORPHAN, SERIAL_RETIRE},
    sql, FileRead, Inode, LocalRead, Overlay, OverlayError, OverlayResult, StatementKind,
    WorkspaceState,
};
pub(crate) const DOMAIN: i64 = -1;
#[derive(Clone, Copy)]
pub(crate) struct Orphan {
    pub root: [u8; 32],
    pub top: i64,
    pub floor: i64,
}
impl Orphan {
    /// Whether the orphan still reads generation `gen` of its serial.
    pub(crate) fn holds(&self, gen: i64) -> bool {
        gen > self.floor && gen <= self.top
    }
}
impl Overlay {
    pub(crate) fn orphan(&self, serial: i64) -> OverlayResult<Option<Orphan>> {
        self.available()?;
        Ok(self.open.borrow().orphans.rows.get(&serial).copied())
    }
    pub(crate) fn orphan_holds(&self, serial: i64, gen: i64) -> OverlayResult<bool> {
        Ok(self.orphan(serial)?.is_some_and(|o| o.holds(gen)))
    }
    /// The orphan-domain row of one serial. A serial has such a row only
    /// while the journaled orphan map holds it: the row is written after the
    /// map entry (`detach_orphan`, `put_inode_domain`) and deleted before it
    /// (`reclaim_orphan`). Nothing is read for any other serial (#359 F-B2).
    pub(crate) fn orphan_inode(&self, serial: i64) -> OverlayResult<Option<Inode>> {
        if !self.orphan_seen.get() || self.orphan(serial)?.is_none() {
            return Ok(None);
        }
        self.query(StatementKind::Inode,"SELECT inode,kind,mode,mtime_seconds,mtime_nanoseconds,nlink,size,inherited_cutoff,born,entries,subdirs
            FROM fs_inode WHERE inode=?1 AND generation=-1",&[&serial],16,inode::decode).map(|mut rows|rows.pop())
    }
    /// After a transaction deleted orphan rows: the engine's exact count of
    /// them, held by the journaled orphan map. At
    /// zero this Workspace holds no orphan owner, and each orphan-domain inode
    /// row was deleted before its orphan row, so inode reads stop probing.
    pub(crate) fn orphans_deleted(&self) -> OverlayResult<()> {
        self.orphan_seen
            .set(!self.open.borrow().orphans.rows.is_empty());
        Ok(())
    }
    /// Last unlink creates only ownership/metadata. Its at-most-two lower
    /// domains are retained, never copied or folded in the unlink transaction.
    pub(crate) fn detach_orphan(&self, state: &WorkspaceState, inode: &Inode) -> OverlayResult<()> {
        if inode.nlink != 0 {
            return Ok(());
        }
        let serial = integer(inode.serial)?;
        // While the engine holds no orphan this serial has none.
        if self.orphan_seen.get() && self.orphan(serial)?.is_some() {
            return Ok(());
        }
        // Every reference, and the ones that can read or write the file.
        let refs = self
            .open
            .borrow()
            .custody
            .rows
            .get(&serial)
            .copied()
            .unwrap_or_default();
        let (holders, users) = (refs.total(), refs.opens + refs.readers);
        if holders == 0 {
            // Nothing holds the file: its active layer retires directly.
            return self.enqueue(SERIAL_RETIRE, serial, state.active.0);
        }
        // The orphan holds its lower layers, whose retirement is queued by
        // the step that releases each of them.
        // From here an orphan-domain row may exist: inode reads probe for it
        // until the last orphan row of this engine is deleted.
        self.orphan_seen.set(true);
        {
            let mut open = self.open.borrow_mut();
            if open.orphans.rows.contains_key(&serial) {
                return Err(OverlayError::Stale);
            }
            open.orphans.put(
                serial,
                Orphan {
                    root: state.base_root,
                    top: state.active.0,
                    floor: state.installed,
                },
            );
        }
        self.execute(
            StatementKind::Inode,
            sql::INODE_INSERT,
            &[
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
                &0_i64,
                &(state.revision + 1),
            ],
            128,
        )?;
        // Migration serves a descriptor or a reader. Under kernel lookups
        // alone nothing is queued: the last lookup's release queues the
        // orphan's item and an earlier open or read queues it then.
        if users != 0 {
            self.enqueue(ORPHAN, serial, DOMAIN)?;
        }
        Ok(())
    }
    pub(crate) fn orphan_layer(&self, serial: i64) -> OverlayResult<Layer> {
        self.layers(serial, DOMAIN, DOMAIN - 1)?
            .pop()
            .ok_or(OverlayError::Stale)
    }
    pub(crate) fn release_orphan_layer(
        &self,
        serial: i64,
        gen: i64,
        next: i64,
    ) -> OverlayResult<()> {
        let mut state = self.open.borrow_mut();
        state.waits.remove((gen, serial));
        let mut orphan = state
            .orphans
            .rows
            .get(&serial)
            .copied()
            .ok_or(OverlayError::Stale)?;
        orphan.top = next;
        state.orphans.put(serial, orphan);
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
        let serial = integer(read.serial)?;
        let Some(orphan) = self.orphan(serial)? else {
            return self.source_read(read.source, read.serial, offset, length);
        };
        self.orphan_read(serial, orphan, offset, length)
    }
    pub(crate) fn orphan_read(
        &self,
        serial: i64,
        orphan: Orphan,
        offset: u64,
        length: u32,
    ) -> OverlayResult<Option<LocalRead>> {
        let mut layers = vec![self.orphan_layer(serial)?];
        layers.extend(self.layers(serial, orphan.top, orphan.floor)?);
        if layers.len() > 3 {
            return Err(OverlayError::Invalid("orphan composition invariant"));
        }
        let mut plan = self.compose_layers(serial, offset, length, layers)?;
        if let Some(ref mut plan) = plan {
            plan.base_root = Some(orphan.root)
        }
        Ok(plan)
    }
}

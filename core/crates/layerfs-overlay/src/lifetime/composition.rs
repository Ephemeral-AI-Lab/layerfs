//! Definite failure resolution and bounded cell/name transfer into active state.
use crate::{
    cells::Window,
    db::integer,
    inode,
    layers::Layer,
    maintenance::{Item, FOLD, RETIRE},
    sql, Capture, Generation, Overlay, OverlayError, OverlayResult, Route, StatementKind,
    CELL_BYTES,
};

impl Overlay {
    /// Releases an exact capture only after definite nonpublication and caller
    /// fencing. Unknown stage/transition/discard must retain original custody.
    /// This performs fixed metadata work; payload composition is maintained.
    pub fn resolve_failed_capture(&self, capture: Capture) -> OverlayResult<()> {
        self.atomic_cleanup(|| {
            self.checked_capture(capture)?;
            if self.state(capture.route)?.closed {
                return Err(OverlayError::Closed);
            };
            self.execute(
                StatementKind::Capture,
                "UPDATE workspace SET consolidating=captured,
                captured=NULL,captured_revision=NULL WHERE ns=?1",
                &[&capture.route.ns],
                8,
            )?;
            self.wake_orphan_sources(capture.route.ns, capture.generation.0)?;
            self.enqueue(capture.route.ns, FOLD, 0, capture.generation.0)
        })
    }
    pub(crate) fn route_for_ns(&self, ns: i64) -> OverlayResult<Route> {
        let incarnation = self
            .query(
                StatementKind::Workspace,
                "SELECT incarnation FROM workspace WHERE ns=?1",
                &[&ns],
                8,
                |row| row.get::<_, Vec<u8>>(0),
            )?
            .pop()
            .ok_or(OverlayError::Stale)?;
        Ok(Route {
            engine: self.identity,
            ns,
            incarnation: incarnation.try_into().map_err(|_| OverlayError::Stale)?,
        })
    }
    pub(crate) fn fold_namespace(&self, item: &Item) -> OverlayResult<(u64, u64, bool)> {
        let route = self.route_for_ns(item.ns)?;
        let state = self.state(route)?;
        if state.closed {
            self.hold_item(item)?;
            return Ok((0, 0, false));
        }
        if state.consolidating != Some(Generation(item.target)) {
            return Err(OverlayError::Stale);
        }
        if item.phase == 0 {
            let row = self
                .query(
                    StatementKind::Reclaim,
                    "SELECT parent,name,serial,inherited FROM directory_entry INDEXED BY directory_entry_capture
                WHERE ns=?1 AND gen=?2 AND (parent,name)>(?3,?4) ORDER BY parent,name LIMIT 1",
                    &[&item.ns, &item.target, &item.cursor, &item.name],
                    24 + item.name.len() as u64,
                    |r| {
                        Ok((
                            r.get::<_, i64>(0)?,
                            r.get::<_, Vec<u8>>(1)?,
                            r.get::<_, Option<i64>>(2)?,
                            r.get::<_, bool>(3)?,
                        ))
                    },
                )?
                .pop();
            if let Some((parent, name, serial, inherited)) = row {
                let active = self
                    .query(
                        StatementKind::DirectoryEntry,
                        sql::DIRECTORY_ENTRY_ACTIVE,
                        &[&item.ns, &parent, &name, &state.active.0],
                        24 + name.len() as u64,
                        |r| r.get::<_, Option<i64>>(0),
                    )?
                    .pop();
                let final_serial = active.unwrap_or(serial);
                let delta = if final_serial.is_some() || inherited {
                    self.execute(
                        StatementKind::DirectoryEntry,
                        sql::DIRECTORY_ENTRY_PUT,
                        &[
                            &item.ns,
                            &parent,
                            &name,
                            &state.active.0,
                            &final_serial,
                            &inherited,
                        ],
                        33 + name.len() as u64,
                    )?;
                    i64::from(active.is_none())
                } else if active.is_some() {
                    self.execute(
                        StatementKind::DirectoryEntry,
                        sql::DIRECTORY_ENTRY_DROP,
                        &[&item.ns, &parent, &name, &state.active.0],
                        24 + name.len() as u64,
                    )?;
                    -1
                } else {
                    0
                };
                if delta != 0 {
                    self.execute(
                        StatementKind::Workspace,
                        "UPDATE workspace SET dirty_directory_entries=dirty_directory_entries+?2 WHERE ns=?1",
                        &[&item.ns, &delta],
                        16,
                    )?;
                }
                self.execute(
                    StatementKind::Reclaim,
                    sql::DIRECTORY_ENTRY_DROP,
                    &[&item.ns, &parent, &name, &item.target],
                    24 + name.len() as u64,
                )?;
                self.advance_item(item, 0, parent, -1, &name)?;
                return Ok((1, name.len() as u64, false));
            }
            self.advance_item(item, 1, 0, -1, &[])?;
            return Ok((0, 0, false));
        }
        let lower=self.query(StatementKind::Inode,
            "SELECT serial,kind,mode,mtime_seconds,mtime_nanoseconds,nlink,size,inherited_cutoff,born,entries
             FROM inode INDEXED BY inode_capture WHERE ns=?1 AND gen=?2 AND serial>=?3 ORDER BY serial LIMIT 1",
            &[&item.ns,&item.target,&item.cursor],24,inode::decode)?.pop();
        let Some(lower) = lower else {
            self.execute(
                StatementKind::Capture,
                "UPDATE workspace SET consolidating=NULL WHERE ns=?1",
                &[&item.ns],
                8,
            )?;
            self.enqueue(item.ns, RETIRE, 0, item.target)?;
            self.finish_item(item)?;
            return Ok((0, 0, true));
        };
        let serial = integer(lower.serial)?;
        if self.orphan_holds(item.ns, serial, item.target)? {
            // The independent orphan retains bytes; the namespace still needs
            // its zero-reference metadata over an unchanged immutable base.
            if lower.nlink == 0
                && self
                    .layers(item.ns, serial, state.active.0, state.active.0 - 1)?
                    .is_empty()
            {
                self.put_inode(route, &state, &lower)?;
                self.execute(
                    StatementKind::Workspace,
                    "UPDATE workspace SET dirty_inodes=dirty_inodes+1 WHERE ns=?1",
                    &[&item.ns],
                    8,
                )?;
            }
            if let Some(next) = serial.checked_add(1) {
                self.advance_item(item, 1, next, -1, &[])?;
                return Ok((1, 0, false));
            }
            // The maximum representable serial is already the last key.
            self.execute(
                StatementKind::Capture,
                "UPDATE workspace SET consolidating=NULL WHERE ns=?1",
                &[&item.ns],
                8,
            )?;
            self.enqueue(item.ns, RETIRE, 0, item.target)?;
            self.finish_item(item)?;
            return Ok((1, 0, true));
        }
        let layers = self.layers(item.ns, serial, state.active.0, state.installed)?;
        let bottom = layers
            .iter()
            .find(|layer| layer.gen == item.target)
            .ok_or(OverlayError::Stale)?;
        let top = match layers.first().filter(|layer| layer.gen == state.active.0) {
            Some(layer) => *layer,
            None => {
                let (_, layer) = self.put_inode(route, &state, &lower)?;
                self.execute(
                    StatementKind::Workspace,
                    "UPDATE workspace SET dirty_inodes=dirty_inodes+1 WHERE ns=?1",
                    &[&item.ns],
                    8,
                )?;
                layer
            }
        };
        let cell = self
            .query(
                StatementKind::Reclaim,
                "SELECT cell_offset FROM payload
            WHERE ns=?1 AND serial=?2 AND gen=?3 AND cell_offset>?4 ORDER BY cell_offset LIMIT 1",
                &[
                    &item.ns,
                    &serial,
                    &item.target,
                    &if serial == item.cursor { item.aux } else { -1 },
                ],
                32,
                |r| r.get::<_, i64>(0),
            )?
            .pop();
        if let Some(cell) = cell {
            let bytes = if top.nlink == 0 {
                0
            } else {
                self.compose_cell(item.ns, serial, bottom, &top, cell)?
            };
            self.execute(
                StatementKind::Reclaim,
                sql::CELL_DROP,
                &[&item.ns, &serial, &item.target, &cell],
                32,
            )?;
            self.advance_item(item, 1, serial, cell, &[])?;
            return Ok((1, bytes, false));
        }
        self.execute(
            StatementKind::Inode,
            "UPDATE inode SET inherited_cutoff=min(inherited_cutoff,?4)
            WHERE ns=?1 AND serial=?2 AND gen=?3",
            &[&item.ns, &serial, &top.gen, &integer(bottom.cutoff)?],
            32,
        )?;
        self.execute(
            StatementKind::Reclaim,
            "DELETE FROM inode WHERE ns=?1 AND serial=?2 AND gen=?3",
            &[&item.ns, &serial, &item.target],
            24,
        )?;
        self.advance_item(item, 1, serial, -1, &[])?;
        Ok((1, 0, false))
    }
    /// Copies only effective lower bytes into unwritten upper positions. This
    /// runs atomically with the caller's source/reference advance; upper shrink
    /// and write stamps are re-read each turn, never saved in a work item.
    pub(crate) fn compose_cell(
        &self,
        ns: i64,
        serial: i64,
        lower: &Layer,
        upper: &Layer,
        cell: i64,
    ) -> OverlayResult<u64> {
        let Some(old) = self.stored(ns, serial, lower.gen, cell)? else {
            return Ok(0);
        };
        if self.stale(ns, serial, lower, cell, old.epoch)? {
            return Ok(0);
        };
        let mut window = match self.stored(ns, serial, upper.gen, cell)? {
            Some(current) if !self.stale(ns, serial, upper, cell, current.epoch)? => {
                current.expand(&self.payload_work)?
            }
            _ => Window::empty(&self.payload_work),
        };
        let end = old.data.len().min(
            lower
                .size
                .min(upper.cutoff)
                .saturating_sub(cell as u64)
                .min(CELL_BYTES as u64) as usize,
        );
        let mut copied = 0_u64;
        for at in 0..end {
            if old.valid(at) && window.mask[at / 8] & (1 << (at % 8)) == 0 {
                window.data[at] = old.data[at];
                copied += 1;
                window.set(at, at + 1);
            }
        }
        let mut observed = self.payload_work.get();
        observed.cell_copy_bytes = observed.cell_copy_bytes.saturating_add(copied);
        self.payload_work.set(observed);
        self.store(
            ns,
            serial,
            upper.gen,
            cell,
            upper.epoch,
            window.trim(&self.payload_work),
        )?;
        Ok(end as u64)
    }
}

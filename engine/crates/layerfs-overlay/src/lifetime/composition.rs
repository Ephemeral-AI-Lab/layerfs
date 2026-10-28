//! Definite failure resolution and bounded cell/name transfer into active state.
use crate::{
    db::integer,
    inode,
    maintenance::{Item, FOLD, PAGE, RETIRE},
    sql, Capture, Generation, Overlay, OverlayError, OverlayResult, StatementKind,
};

impl Overlay {
    /// Releases an exact capture only after definite nonpublication and caller
    /// fencing. Unknown stage/transition/discard must retain original custody.
    /// This performs fixed metadata work; payload composition is maintained.
    pub fn resolve_failed_capture(&self, capture: Capture) -> OverlayResult<()> {
        self.atomic_cleanup(|| {
            self.checked_capture(capture)?;
            if self.state()?.closed {
                return Err(OverlayError::Closed);
            };
            self.change_state(|state| {
                state.consolidating = state.captured;
                state.captured = None;
                state.captured_revision = None;
                Ok(())
            })?;
            self.wake_orphan_sources(capture.generation.0)?;
            self.enqueue(FOLD, 0, capture.generation.0)
        })
    }
    pub(crate) fn fold_namespace(&self, item: &Item) -> OverlayResult<(u64, u64, bool)> {
        let state = self.state()?;
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
                    "SELECT parent,name,inode,inherited FROM fs_entry INDEXED BY fs_entry_capture
                WHERE generation=?1 AND (parent,name)>(?2,?3) ORDER BY parent,name LIMIT 1",
                    &[&item.target, &item.cursor, &item.name],
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
                        &[&parent, &name, &state.active.0],
                        24 + name.len() as u64,
                        |r| r.get::<_, Option<i64>>(0),
                    )?
                    .pop();
                let final_serial = active.unwrap_or(serial);
                let delta = if final_serial.is_some() || inherited {
                    self.execute(
                        StatementKind::DirectoryEntry,
                        sql::DIRECTORY_ENTRY_PUT,
                        &[&parent, &name, &state.active.0, &final_serial, &inherited],
                        33 + name.len() as u64,
                    )?;
                    i64::from(active.is_none())
                } else if active.is_some() {
                    self.execute(
                        StatementKind::DirectoryEntry,
                        sql::DIRECTORY_ENTRY_DROP,
                        &[&parent, &name, &state.active.0],
                        24 + name.len() as u64,
                    )?;
                    -1
                } else {
                    0
                };
                if delta != 0 {
                    self.dirty_counts(0, delta)?;
                }
                self.execute(
                    StatementKind::Reclaim,
                    sql::DIRECTORY_ENTRY_DROP,
                    &[&parent, &name, &item.target],
                    24 + name.len() as u64,
                )?;
                self.advance_item(item, 0, parent, -1, &name)?;
                return Ok((1, name.len() as u64, false));
            }
            self.advance_item(item, 1, 0, -1, &[])?;
            return Ok((0, 0, false));
        }
        let lower=self.query(StatementKind::Inode,
            "SELECT inode,kind,mode,mtime_seconds,mtime_nanoseconds,nlink,size,inherited_cutoff,born,entries,subdirs
             FROM fs_inode INDEXED BY fs_inode_capture WHERE generation=?1 AND inode>=?2 ORDER BY inode LIMIT 1",
            &[&item.target,&item.cursor],24,inode::decode)?.pop();
        let Some(lower) = lower else {
            self.change_state(|state| {
                state.consolidating = None;
                Ok(())
            })?;
            self.enqueue(RETIRE, 0, item.target)?;
            self.finish_item(item)?;
            return Ok((0, 0, true));
        };
        let serial = integer(lower.serial)?;
        if self.orphan_holds(serial, item.target)? {
            // The independent orphan retains bytes; the namespace still needs
            // its zero-reference metadata over an unchanged immutable base.
            if lower.nlink == 0
                && self
                    .layers(serial, state.active.0, state.active.0 - 1)?
                    .is_empty()
            {
                self.put_inode(&state, &lower)?;
                self.dirty_counts(1, 0)?;
            }
            if let Some(next) = serial.checked_add(1) {
                self.advance_item(item, 1, next, -1, &[])?;
                return Ok((1, 0, false));
            }
            // The maximum representable serial is already the last key.
            self.change_state(|state| {
                state.consolidating = None;
                Ok(())
            })?;
            self.enqueue(RETIRE, 0, item.target)?;
            self.finish_item(item)?;
            return Ok((1, 0, true));
        }
        let layers = self.layers(serial, state.active.0, state.installed)?;
        let bottom = layers
            .iter()
            .find(|layer| layer.gen == item.target)
            .ok_or(OverlayError::Stale)?;
        let top = match layers.first().filter(|layer| layer.gen == state.active.0) {
            Some(layer) => {
                // Folding into a kept row writes it: it is counted now.
                if self.unkeep(serial) {
                    self.dirty_counts(1, 0)?;
                }
                *layer
            }
            None => {
                let (_, layer) = self.put_inode(&state, &lower)?;
                self.dirty_counts(1, 0)?;
                layer
            }
        };
        let cell = self
            .query(
                StatementKind::Reclaim,
                "SELECT cell_offset FROM fs_data
            WHERE inode=?1 AND generation=?2 AND cell_offset>?3 ORDER BY cell_offset LIMIT 1",
                &[
                    &serial,
                    &item.target,
                    &if serial == item.cursor { item.aux } else { -1 },
                ],
                32,
                |r| r.get::<_, i64>(0),
            )?
            .pop();
        if let Some(cell) = cell {
            // One row a step: it moves, or its effective bytes are merged
            // below the active ones, and it leaves the folded layer.
            let bytes = if top.nlink == 0 {
                self.execute(
                    StatementKind::Reclaim,
                    sql::CELL_DROP,
                    &[&serial, &item.target, &cell],
                    32,
                )?;
                0
            } else {
                self.transfer_row(serial, bottom, &top, cell)?
            };
            self.advance_item(item, 1, serial, cell, &[])?;
            return Ok((1, bytes, false));
        }
        // Its cells have left the folded layer: its pieces follow, one step
        // each, or go with a file the active generation has unlinked.
        if bottom.referenced {
            let moved = if top.nlink == 0 {
                self.drop_pieces(serial, item.target, PAGE.rows)? != 0
            } else {
                self.transfer_piece(serial, bottom, &top)?
            };
            if moved {
                return Ok((1, 0, false));
            }
        }
        self.execute(
            StatementKind::Inode,
            "UPDATE fs_inode SET inherited_cutoff=min(inherited_cutoff,?3)
            WHERE inode=?1 AND generation=?2",
            &[&serial, &top.gen, &integer(bottom.cutoff)?],
            32,
        )?;
        self.execute(
            StatementKind::Reclaim,
            "DELETE FROM fs_inode WHERE inode=?1 AND generation=?2",
            &[&serial, &item.target],
            24,
        )?;
        self.advance_item(item, 1, serial, -1, &[])?;
        Ok((1, 0, false))
    }
}

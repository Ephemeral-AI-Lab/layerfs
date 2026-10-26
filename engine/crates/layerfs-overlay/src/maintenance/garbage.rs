//! Bounded live garbage and generation-selective retirement, metadata first.
use crate::{
    db::unsigned,
    maintenance::{Item, OPERATION_RECORD, PAGE, STALE, STALE_PIECES, STEPS},
    sql, Overlay, OverlayResult, StatementKind,
};

impl Overlay {
    pub(crate) fn retire_generation(&self, item: &Item) -> OverlayResult<(u64, u64, bool)> {
        let gen = item.target;
        let mut bytes = 0;
        let count = match item.phase {
            0 => {
                let rows=self.query(StatementKind::Reclaim,"SELECT inode,cell_offset,length(data)+ifnull(length(validity),0)
                    FROM fs_data INDEXED BY fs_data_generation WHERE generation=?1 AND (inode,cell_offset)>(?2,?3)
                    ORDER BY inode,cell_offset LIMIT 14", &[&gen,&item.cursor,&item.aux],32,
                    |r|Ok((r.get::<_,i64>(0)?,r.get::<_,i64>(1)?,unsigned(r,2)?)))?;
                // One page of cells, however wide the rows that hold them.
                let rows = &rows[..PAGE.fit(rows.iter().map(|row| row.2)).0];
                for (serial, cell, size) in rows {
                    if self.orphan_holds(*serial, gen)? {
                        continue;
                    }
                    self.execute(
                        StatementKind::Reclaim,
                        sql::CELL_DROP,
                        &[serial, &gen, cell],
                        32,
                    )?;
                    bytes += size;
                }
                if let Some((serial, cell, _)) = rows.last() {
                    self.advance_item(item, 0, *serial, *cell, &[])?
                }
                rows.len()
            }
            1 => {
                let rows = self.query(
                    StatementKind::Reclaim,
                    "SELECT inode,depth FROM fs_truncate INDEXED BY fs_truncate_generation
                    WHERE generation=?1 AND (inode,depth)>(?2,?3) ORDER BY inode,depth LIMIT 64",
                    &[&gen, &item.cursor, &item.aux],
                    32,
                    |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)),
                )?;
                for (serial, depth) in &rows {
                    if self.orphan_holds(*serial, gen)? {
                        continue;
                    }
                    self.execute(
                        StatementKind::Reclaim,
                        "DELETE FROM fs_truncate WHERE inode=?1 AND generation=?2 AND depth=?3",
                        &[serial, &gen, depth],
                        32,
                    )?;
                }
                if let Some((serial, depth)) = rows.last() {
                    self.advance_item(item, 1, *serial, *depth, &[])?
                }
                rows.len()
            }
            2 => {
                let rows = self.query(
                    StatementKind::Reclaim,
                    "SELECT parent,name FROM fs_entry INDEXED BY fs_entry_capture
                    WHERE generation=?1 AND (parent,name)>(?2,?3) ORDER BY parent,name LIMIT 64",
                    &[&gen, &item.cursor, &item.name],
                    24 + item.name.len() as u64,
                    |r| Ok((r.get::<_, i64>(0)?, r.get::<_, Vec<u8>>(1)?)),
                )?;
                for (parent, name) in &rows {
                    self.execute(
                        StatementKind::Reclaim,
                        sql::DIRECTORY_ENTRY_DROP,
                        &[parent, name, &gen],
                        24 + name.len() as u64,
                    )?;
                    bytes += name.len() as u64;
                }
                if let Some((parent, name)) = rows.last() {
                    self.advance_item(item, 2, *parent, -1, name)?
                }
                rows.len()
            }
            3 => {
                let rows = self.query(
                    StatementKind::Reclaim,
                    "SELECT inode FROM fs_inode INDEXED BY fs_inode_capture
                    WHERE generation=?1 AND inode>?2 ORDER BY inode LIMIT 64",
                    &[&gen, &item.cursor],
                    24,
                    |r| r.get::<_, i64>(0),
                )?;
                for serial in &rows {
                    if self.orphan_holds(*serial, gen)? {
                        continue;
                    }
                    self.execute(
                        StatementKind::Reclaim,
                        "DELETE FROM fs_inode WHERE inode=?1 AND generation=?2",
                        &[serial, &gen],
                        24,
                    )?;
                }
                if let Some(serial) = rows.last() {
                    self.advance_item(item, 3, *serial, -1, &[])?
                }
                rows.len()
            }
            // An engine that never wrote a piece has none to retire.
            4 if self.pieces_seen.get() => self.retire_pieces(item, 4)?,
            _ => {
                self.finish_item(item)?;
                return Ok((0, 0, true));
            }
        };
        if count == 0 {
            self.advance_item(item, item.phase + 1, 0, -1, &[])?
        }
        Ok((count as u64, bytes, false))
    }
    pub(crate) fn clean_live_item(&self, item: &Item) -> OverlayResult<(u64, u64, bool)> {
        if item.kind == OPERATION_RECORD {
            return self.clean_operation_record(item);
        }
        let layer = self
            .layers(item.resource, item.target, item.target - 1)?
            .pop();
        let mut bytes = 0;
        let count = if item.kind == STALE {
            let rows=self.query(StatementKind::Reclaim,"SELECT cell_offset,epoch,length(data)+ifnull(length(validity),0)
                FROM fs_data WHERE inode=?1 AND generation=?2 AND cell_offset>?3 ORDER BY cell_offset LIMIT 14",
                &[&item.resource,&item.target,&item.cursor],32,
                |r|Ok((r.get::<_,i64>(0)?,r.get::<_,i64>(1)?,unsigned(r,2)?)))?;
            let rows = &rows[..PAGE.fit(rows.iter().map(|row| row.2)).0];
            for (cell, stamp, size) in rows {
                let stale = match layer {
                    Some(ref layer) => self.stale(item.resource, layer, *cell, *stamp)?,
                    None => true,
                };
                if stale {
                    self.execute(StatementKind::Reclaim,"DELETE FROM fs_data WHERE inode=?1 AND generation=?2 AND cell_offset=?3 AND epoch=?4",
                        &[&item.resource,&item.target,cell,stamp],40)?;
                    bytes += size;
                }
            }
            if let Some((cell, _, _)) = rows.last() {
                self.advance_item(item, item.phase, *cell, -1, &[])?
            }
            rows.len()
        } else {
            debug_assert_eq!(item.kind, STEPS);
            let floor = layer.map_or(0, |layer| layer.height).max(item.cursor);
            let rows = self.query(
                StatementKind::Reclaim,
                "SELECT depth FROM fs_truncate
                WHERE inode=?1 AND generation=?2 AND depth>?3 ORDER BY depth LIMIT 64",
                &[&item.resource, &item.target, &floor],
                32,
                |r| r.get::<_, i64>(0),
            )?;
            for depth in &rows {
                self.execute(
                    StatementKind::Reclaim,
                    "DELETE FROM fs_truncate WHERE inode=?1 AND generation=?2 AND depth=?3",
                    &[&item.resource, &item.target, depth],
                    32,
                )?;
            }
            if let Some(depth) = rows.last() {
                self.advance_item(item, item.phase, *depth, -1, &[])?
            }
            rows.len()
        };
        if count == 0 {
            self.finish_item(item)?
        }
        Ok((count as u64, bytes, count == 0))
    }
    fn clean_operation_record(&self, item: &Item) -> OverlayResult<(u64, u64, bool)> {
        let (count, bytes) = self.delete_indexed_operation_record(item.resource)?;
        if count == 0 {
            self.finish_item(item)?;
        }
        Ok((count, bytes, count == 0))
    }
    /// Queues the cleanup of what one shrink hid: cells, superseded steps
    /// and, for a layer that holds pieces, its pieces beyond the boundary.
    pub(crate) fn schedule_shrink(
        &self,
        serial: i64,
        (gen, referenced): (i64, bool),
        boundary: i64,
        height: i64,
    ) -> OverlayResult<()> {
        let pieces = referenced.then_some((STALE_PIECES, boundary - 1));
        for (kind, cursor) in [(STALE, boundary - 1), (STEPS, height)]
            .into_iter()
            .chain(pieces)
        {
            self.execute(StatementKind::Reclaim,"INSERT INTO cleanup_task(kind,resource,target,cursor) VALUES(?1,?2,?3,?4)
                ON CONFLICT(kind,resource,target) DO UPDATE SET cursor=min(cursor,excluded.cursor),ready=1",
                &[&kind,&serial,&gen,&cursor],40)?;
        }
        self.maintenance_ready.set(true);
        Ok(())
    }
}

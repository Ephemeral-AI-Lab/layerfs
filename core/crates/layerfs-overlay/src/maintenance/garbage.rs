//! Bounded live garbage and generation-selective retirement, metadata first.
use crate::{
    db::unsigned,
    maintenance::{Item, OPERATION_RECORD, PAGE, STALE, STEPS},
    sql, Overlay, OverlayResult, StatementKind,
};

impl Overlay {
    pub(crate) fn retire_generation(&self, item: &Item) -> OverlayResult<(u64, u64, bool)> {
        let (ns, gen) = (item.ns, item.target);
        let mut bytes = 0;
        let count = match item.phase {
            0 => {
                let rows=self.query(StatementKind::Reclaim,"SELECT serial,cell_offset,length(data)+ifnull(length(validity),0)
                    FROM payload INDEXED BY payload_generation WHERE ns=?1 AND gen=?2 AND (serial,cell_offset)>(?3,?4)
                    ORDER BY serial,cell_offset LIMIT 14", &[&ns,&gen,&item.cursor,&item.aux],32,
                    |r|Ok((r.get::<_,i64>(0)?,r.get::<_,i64>(1)?,unsigned(r,2)?)))?;
                // One page of cells, however wide the rows that hold them.
                let rows = &rows[..PAGE.fit(rows.iter().map(|row| row.2)).0];
                for (serial, cell, size) in rows {
                    if self.orphan_holds(ns, *serial, gen)? {
                        continue;
                    }
                    self.execute(
                        StatementKind::Reclaim,
                        sql::CELL_DROP,
                        &[&ns, serial, &gen, cell],
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
                let rows=self.query(StatementKind::Reclaim,"SELECT serial,depth FROM shrink INDEXED BY shrink_generation
                    WHERE ns=?1 AND gen=?2 AND (serial,depth)>(?3,?4) ORDER BY serial,depth LIMIT 64",
                    &[&ns,&gen,&item.cursor,&item.aux],32,|r|Ok((r.get::<_,i64>(0)?,r.get::<_,i64>(1)?)))?;
                for (serial, depth) in &rows {
                    if self.orphan_holds(ns, *serial, gen)? {
                        continue;
                    }
                    self.execute(
                        StatementKind::Reclaim,
                        "DELETE FROM shrink WHERE ns=?1 AND serial=?2 AND gen=?3 AND depth=?4",
                        &[&ns, serial, &gen, depth],
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
                    "SELECT parent,name FROM directory_entry INDEXED BY directory_entry_capture
                    WHERE ns=?1 AND gen=?2 AND (parent,name)>(?3,?4) ORDER BY parent,name LIMIT 64",
                    &[&ns, &gen, &item.cursor, &item.name],
                    24 + item.name.len() as u64,
                    |r| Ok((r.get::<_, i64>(0)?, r.get::<_, Vec<u8>>(1)?)),
                )?;
                for (parent, name) in &rows {
                    self.execute(
                        StatementKind::Reclaim,
                        sql::DIRECTORY_ENTRY_DROP,
                        &[&ns, parent, name, &gen],
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
                    "SELECT serial FROM inode INDEXED BY inode_capture
                    WHERE ns=?1 AND gen=?2 AND serial>?3 ORDER BY serial LIMIT 64",
                    &[&ns, &gen, &item.cursor],
                    24,
                    |r| r.get::<_, i64>(0),
                )?;
                for serial in &rows {
                    if self.orphan_holds(ns, *serial, gen)? {
                        continue;
                    }
                    self.execute(
                        StatementKind::Reclaim,
                        "DELETE FROM inode WHERE ns=?1 AND serial=?2 AND gen=?3",
                        &[&ns, serial, &gen],
                        24,
                    )?;
                }
                if let Some(serial) = rows.last() {
                    self.advance_item(item, 3, *serial, -1, &[])?
                }
                rows.len()
            }
            _ => {
                self.execute(
                    StatementKind::Reclaim,
                    "DELETE FROM reclaim WHERE ns=?1 AND queue_key=?2",
                    &[&ns, &gen],
                    16,
                )?;
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
            .layers(item.ns, item.resource, item.target, item.target - 1)?
            .pop();
        let mut bytes = 0;
        let count = if item.kind == STALE {
            let rows=self.query(StatementKind::Reclaim,"SELECT cell_offset,epoch,length(data)+ifnull(length(validity),0)
                FROM payload WHERE ns=?1 AND serial=?2 AND gen=?3 AND cell_offset>?4 ORDER BY cell_offset LIMIT 14",
                &[&item.ns,&item.resource,&item.target,&item.cursor],32,
                |r|Ok((r.get::<_,i64>(0)?,r.get::<_,i64>(1)?,unsigned(r,2)?)))?;
            let rows = &rows[..PAGE.fit(rows.iter().map(|row| row.2)).0];
            for (cell, stamp, size) in rows {
                let stale = match layer {
                    Some(ref layer) => self.stale(item.ns, item.resource, layer, *cell, *stamp)?,
                    None => true,
                };
                if stale {
                    self.execute(StatementKind::Reclaim,"DELETE FROM payload WHERE ns=?1 AND serial=?2 AND gen=?3 AND cell_offset=?4 AND epoch=?5",
                        &[&item.ns,&item.resource,&item.target,cell,stamp],40)?;
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
                "SELECT depth FROM shrink
                WHERE ns=?1 AND serial=?2 AND gen=?3 AND depth>?4 ORDER BY depth LIMIT 64",
                &[&item.ns, &item.resource, &item.target, &floor],
                32,
                |r| r.get::<_, i64>(0),
            )?;
            for depth in &rows {
                self.execute(
                    StatementKind::Reclaim,
                    "DELETE FROM shrink WHERE ns=?1 AND serial=?2 AND gen=?3 AND depth=?4",
                    &[&item.ns, &item.resource, &item.target, depth],
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
        if item.target == 2 {
            let (count, bytes) =
                self.delete_indexed_operation_record(item.ns, Some(item.resource))?;
            if count == 0 {
                self.finish_item(item)?;
            }
            return Ok((count, bytes, count == 0));
        }
        let (page, delete) = if item.target == 1 {
            (
                "SELECT kind,key,length(value) FROM owned_operation_record
            WHERE ns=?1 AND operation=?2 AND (kind,key)>(?3,?4) ORDER BY kind,key LIMIT 64",
                sql::OWNED_OPERATION_RECORD_DELETE,
            )
        } else {
            (
                "SELECT kind,key,length(value) FROM operation_record
            WHERE ns=?1 AND operation=?2 AND (kind,key)>(?3,?4) ORDER BY kind,key LIMIT 64",
                sql::OPERATION_RECORD_DELETE,
            )
        };
        let rows = self.query(
            StatementKind::OperationRecord,
            page,
            &[&item.ns, &item.resource, &item.cursor, &item.aux],
            32,
            |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?, unsigned(r, 2)?)),
        )?;
        let (mut count, mut bytes) = (0, 0);
        for (kind, key, size) in &rows {
            if count != 0 && bytes + size > 65536 {
                break;
            }
            self.execute(
                StatementKind::Reclaim,
                delete,
                &[&item.ns, &item.resource, kind, key],
                32,
            )?;
            self.advance_item(item, 0, *kind, *key, &[])?;
            count += 1;
            bytes += size;
        }
        if count == 0 {
            self.finish_item(item)?
        }
        Ok((count, bytes, count == 0))
    }
    pub(crate) fn schedule_shrink(
        &self,
        ns: i64,
        serial: i64,
        gen: i64,
        boundary: i64,
        height: i64,
    ) -> OverlayResult<()> {
        for (kind, cursor) in [(STALE, boundary - 1), (STEPS, height)] {
            self.execute(StatementKind::Reclaim,"INSERT INTO maintenance(ns,kind,resource,target,cursor) VALUES(?1,?2,?3,?4,?5)
                ON CONFLICT(ns,kind,resource,target) DO UPDATE SET cursor=min(cursor,excluded.cursor),ready=1",
                &[&ns,&kind,&serial,&gen,&cursor],40)?;
        }
        self.maintenance_ready.set(true);
        Ok(())
    }
}

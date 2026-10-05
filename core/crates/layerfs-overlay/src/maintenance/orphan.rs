//! Bounded orphan migration and last-owner deletion, independent of Commit count.
use crate::{
    db::integer,
    lifetime::orphan::DOMAIN,
    maintenance::{Item, ORPHAN},
    sql, Overlay, OverlayResult, StatementKind,
};
impl Overlay {
    pub(crate) fn maintain_orphan(&self, item: &Item) -> OverlayResult<(u64, u64, bool)> {
        let (ns, serial) = (item.ns, item.resource);
        let Some(orphan) = self.orphan(ns, serial)? else {
            self.execute(
                StatementKind::Lease,
                "DELETE FROM file_custody WHERE ns=?1 AND serial=?2 AND opens+lookups+readers=0",
                &[&ns, &serial],
                16,
            )?;
            self.finish_item(item)?;
            return Ok((0, 0, true));
        };
        let held = self.file_refs(ns, serial)? != 0;
        if held && orphan.top > orphan.floor {
            let lower = self
                .layers(ns, serial, orphan.top, orphan.floor)?
                .first()
                .copied();
            if let Some(lower) = lower {
                let upper = self.orphan_layer(ns, serial)?;
                let cell=self.query(StatementKind::Reclaim,"SELECT cell_offset FROM payload
                    WHERE ns=?1 AND serial=?2 AND gen=?3 AND cell_offset>?4 ORDER BY cell_offset LIMIT 1",
                    &[&ns,&serial,&lower.gen,&if item.phase==lower.gen {item.aux}else{-1}],32,|r|r.get::<_,i64>(0))?.pop();
                if let Some(cell) = cell {
                    let bytes = self.compose_cell(ns, serial, &lower, &upper, cell)?;
                    self.advance_item(item, lower.gen, 0, cell, &[])?;
                    return Ok((1, bytes, false));
                }
                self.execute(StatementKind::Inode,"UPDATE inode SET inherited_cutoff=min(inherited_cutoff,?3) WHERE ns=?1 AND serial=?2 AND gen=-1",
                    &[&ns,&serial,&integer(lower.cutoff)?],24)?;
                self.release_orphan_layer(
                    ns,
                    serial,
                    lower.gen,
                    (lower.gen - 1).max(orphan.floor),
                )?;
                self.advance_item(item, 0, 0, -1, &[])?;
                return Ok((1, 0, false));
            }
            self.execute(
                StatementKind::Lease,
                "UPDATE orphan SET lower_top=lower_floor WHERE ns=?1 AND serial=?2",
                &[&ns, &serial],
                16,
            )?;
        }
        if held {
            self.hold_item(item)?;
            return Ok((0, 0, false));
        }
        // On last owner release, lower namespace ownership is separate. Target
        // only already-retired source rows; future installs own remaining ones.
        if orphan.top > orphan.floor {
            let lower = self
                .layers(ns, serial, orphan.top, orphan.floor)?
                .first()
                .copied();
            if let Some(lower) = lower {
                self.release_orphan_layer(
                    ns,
                    serial,
                    lower.gen,
                    (lower.gen - 1).max(orphan.floor),
                )?;
                return Ok((0, 0, false));
            }
        }
        let rows = self.query(
            StatementKind::Reclaim,
            "SELECT cell_offset,length(data)+ifnull(length(validity),0) FROM payload
            WHERE ns=?1 AND serial=?2 AND gen=-1 ORDER BY cell_offset LIMIT 14",
            &[&ns, &serial],
            16,
            |r| Ok((r.get::<_, i64>(0)?, crate::db::unsigned(r, 1)?)),
        )?;
        if !rows.is_empty() {
            let mut bytes = 0;
            for (cell, size) in &rows {
                self.execute(
                    StatementKind::Reclaim,
                    sql::CELL_DROP,
                    &[&ns, &serial, &DOMAIN, cell],
                    32,
                )?;
                bytes += size;
            }
            return Ok((rows.len() as u64, bytes, false));
        }
        let steps = self.query(
            StatementKind::Reclaim,
            "SELECT depth FROM shrink WHERE ns=?1 AND serial=?2 AND gen=-1 ORDER BY depth LIMIT 64",
            &[&ns, &serial],
            16,
            |r| r.get::<_, i64>(0),
        )?;
        if !steps.is_empty() {
            for depth in &steps {
                self.execute(
                    StatementKind::Reclaim,
                    "DELETE FROM shrink WHERE ns=?1 AND serial=?2 AND gen=-1 AND depth=?3",
                    &[&ns, &serial, depth],
                    24,
                )?;
            }
            return Ok((steps.len() as u64, 0, false));
        }
        self.execute(
            StatementKind::Reclaim,
            "DELETE FROM inode WHERE ns=?1 AND serial=?2 AND gen=-1",
            &[&ns, &serial],
            16,
        )?;
        self.execute(
            StatementKind::Reclaim,
            "DELETE FROM orphan WHERE ns=?1 AND serial=?2",
            &[&ns, &serial],
            16,
        )?;
        self.execute(
            StatementKind::Lease,
            "DELETE FROM file_custody WHERE ns=?1 AND serial=?2 AND opens+lookups+readers=0",
            &[&ns, &serial],
            16,
        )?;
        self.finish_item(item)?;
        Ok((3, 0, true))
    }
    pub(crate) fn retire_serial(&self, item: &Item) -> OverlayResult<(u64, u64, bool)> {
        let (ns, serial, gen) = (item.ns, item.resource, item.target);
        if self.generation_held(ns, gen)? || self.orphan_holds(ns, serial, gen)? {
            self.hold_item(item)?;
            return Ok((0, 0, false));
        }
        let rows = self.query(
            StatementKind::Reclaim,
            "SELECT cell_offset,length(data)+ifnull(length(validity),0) FROM payload
            WHERE ns=?1 AND serial=?2 AND gen=?3 ORDER BY cell_offset LIMIT 14",
            &[&ns, &serial, &gen],
            24,
            |r| Ok((r.get::<_, i64>(0)?, crate::db::unsigned(r, 1)?)),
        )?;
        if !rows.is_empty() {
            let mut bytes = 0;
            for (cell, size) in &rows {
                self.execute(
                    StatementKind::Reclaim,
                    sql::CELL_DROP,
                    &[&ns, &serial, &gen, cell],
                    32,
                )?;
                bytes += size;
            }
            return Ok((rows.len() as u64, bytes, false));
        }
        let steps = self.query(
            StatementKind::Reclaim,
            "SELECT depth FROM shrink WHERE ns=?1 AND serial=?2 AND gen=?3 ORDER BY depth LIMIT 64",
            &[&ns, &serial, &gen],
            24,
            |r| r.get::<_, i64>(0),
        )?;
        if !steps.is_empty() {
            for depth in &steps {
                self.execute(
                    StatementKind::Reclaim,
                    "DELETE FROM shrink WHERE ns=?1 AND serial=?2 AND gen=?3 AND depth=?4",
                    &[&ns, &serial, &gen, depth],
                    32,
                )?;
            }
            return Ok((steps.len() as u64, 0, false));
        }
        // Active tombstones prevent serial lookup falling through to the old
        // base. Only already-installed metadata may be physically removed.
        let state = self.state(self.route_for_ns(ns)?)?;
        let superseded = !self
            .layers(ns, serial, state.active.0, gen.max(state.installed))?
            .is_empty();
        if gen <= state.installed || superseded {
            self.execute(
                StatementKind::Reclaim,
                "DELETE FROM inode WHERE ns=?1 AND serial=?2 AND gen=?3",
                &[&ns, &serial, &gen],
                24,
            )?;
        }
        self.finish_item(item)?;
        Ok((1, 0, true))
    }
    pub(crate) fn wake_orphan(&self, ns: i64, serial: i64) -> OverlayResult<()> {
        let changed = self.execute(
            StatementKind::Reclaim,
            "UPDATE maintenance SET ready=1 WHERE ns=?1 AND kind=?2 AND resource=?3 AND target=-1",
            &[&ns, &ORPHAN, &serial],
            24,
        )?;
        if changed != 0 {
            self.maintenance_ready.set(true)
        }
        Ok(())
    }
}

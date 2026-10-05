//! Indexed snapshot-source readiness and bounded orphan cell moves.
use super::ready::WAKE_ORPHAN;
use crate::{
    layers::Layer,
    maintenance::{Item, ORPHAN},
    Overlay, OverlayResult, StatementKind,
};
pub(crate) const GENERATION_WINDOW: &str =
    "SELECT kind,resource FROM maintenance INDEXED BY maintenance_generation
    WHERE ns=?1 AND target=?2 AND kind IN(1,3,7) AND (kind,resource)>(?3,?4)
    ORDER BY kind,resource LIMIT 64";
impl Overlay {
    pub(crate) fn wake_orphan_sources(&self, ns: i64, gen: i64) -> OverlayResult<()> {
        if !self
            .query(
                StatementKind::Lease,
                "SELECT 1 FROM orphan_wait WHERE ns=?1 AND gen=?2 LIMIT 1",
                &[&ns, &gen],
                16,
                |_| Ok(()),
            )?
            .is_empty()
        {
            self.enqueue(ns, WAKE_ORPHAN, 0, gen)?;
        }
        Ok(())
    }
    pub(crate) fn wake_orphan_step(&self, item: &Item) -> OverlayResult<(u64, u64, bool)> {
        let rows = self.query(
            StatementKind::Lease,
            "SELECT serial FROM orphan_wait WHERE ns=?1 AND gen=?2 ORDER BY serial LIMIT 64",
            &[&item.ns, &item.target],
            16,
            |r| r.get::<_, i64>(0),
        )?;
        for serial in &rows {
            self.execute(StatementKind::Reclaim,"UPDATE maintenance SET ready=1 WHERE ns=?1 AND kind=?2 AND resource=?3 AND target=-1",&[&item.ns,&ORPHAN,serial],24)?;
            self.execute(
                StatementKind::Lease,
                "DELETE FROM orphan_wait WHERE ns=?1 AND gen=?2 AND serial=?3",
                &[&item.ns, &item.target, serial],
                24,
            )?;
        }
        if rows.is_empty() {
            self.finish_item(item)?;
        }
        Ok((rows.len() as u64, 0, rows.is_empty()))
    }
    pub(crate) fn wake_generation_step(&self, item: &Item) -> OverlayResult<(u64, u64, bool)> {
        let rows = self.query(
            StatementKind::Reclaim,
            GENERATION_WINDOW,
            &[&item.ns, &item.target, &item.cursor, &item.aux],
            32,
            |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)),
        )?;
        for (kind, resource) in &rows {
            self.execute(StatementKind::Reclaim,
                "UPDATE maintenance SET ready=1 WHERE ns=?1 AND kind=?2 AND resource=?3 AND target=?4",
                &[&item.ns,kind,resource,&item.target],32)?;
        }
        if let Some((kind, resource)) = rows.last() {
            self.advance_item(item, 0, *kind, *resource, &[])?;
        } else {
            self.finish_item(item)?;
        }
        Ok((rows.len() as u64, 0, rows.is_empty()))
    }
    /// Snapshot owners are fenced by the caller. Reuse the existing payload row
    /// when no upper byte or cutoff needs merging, preserving physical custody;
    /// otherwise compose one cell and release its lower physical row atomically.
    pub(crate) fn move_orphan_cell(
        &self,
        ns: i64,
        serial: i64,
        lower: &Layer,
        upper: &Layer,
        cell: i64,
    ) -> OverlayResult<u64> {
        let old = self.stored(ns, serial, lower.gen, cell)?;
        let current = self.stored(ns, serial, upper.gen, cell)?;
        let stale_upper = match current {
            Some(ref c) => self.stale(ns, serial, upper, cell, c.epoch)?,
            None => true,
        };
        if let Some(ref old) = old {
            if stale_upper
                && !self.stale(ns, serial, lower, cell, old.epoch)?
                && (cell as u64).saturating_add(old.data.len() as u64)
                    <= lower.size.min(upper.cutoff)
            {
                if current.is_some() {
                    self.execute(
                        StatementKind::Reclaim,
                        crate::sql::CELL_DROP,
                        &[&ns, &serial, &upper.gen, &cell],
                        32,
                    )?;
                }
                self.execute(StatementKind::Payload,"UPDATE payload SET gen=?5,epoch=?6 WHERE ns=?1 AND serial=?2 AND gen=?3 AND cell_offset=?4",
                    &[&ns,&serial,&lower.gen,&cell,&upper.gen,&upper.epoch],48)?;
                return Ok(old.data.len() as u64);
            }
        }
        let bytes = self.compose_cell(ns, serial, lower, upper, cell)?;
        self.execute(
            StatementKind::Reclaim,
            crate::sql::CELL_DROP,
            &[&ns, &serial, &lower.gen, &cell],
            32,
        )?;
        Ok(bytes)
    }
}

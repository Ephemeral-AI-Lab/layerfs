//! Indexed snapshot-source readiness and bounded orphan cell moves.
use super::ready::WAKE_ORPHAN;
use crate::{maintenance::Item, Overlay, OverlayResult, StatementKind};
pub(crate) const GENERATION_WINDOW: &str =
    "SELECT kind,resource FROM cleanup_task INDEXED BY cleanup_task_generation
    WHERE target=?1 AND kind IN(1,3,7) AND (kind,resource)>(?2,?3)
    ORDER BY kind,resource LIMIT 64";
impl Overlay {
    /// Whether an orphan that reads layer `gen` must still wait for it: a
    /// capture has not settled it, or a captured reader holds it. Parking and
    /// waking share this one rule.
    pub(crate) fn orphan_source_awaited(&self, gen: i64) -> OverlayResult<bool> {
        Ok(self.state()?.captured.is_some_and(|g| g.0 == gen) || self.generation_held(gen)?)
    }
    pub(crate) fn wake_orphan_sources(&self, gen: i64) -> OverlayResult<()> {
        // Orphans woken while the layer is awaited wait again at their next
        // turn, and the rows they leave keep this wake pending: the drain after
        // the job that settled the capture never ends. The release that ends
        // the wait wakes them (`wake_generation`).
        if self.orphan_source_awaited(gen)? {
            return Ok(());
        }
        if self
            .open
            .borrow()
            .waits
            .rows
            .range((gen, 0)..=(gen, i64::MAX))
            .next()
            .is_some()
        {
            self.enqueue(WAKE_ORPHAN, 0, gen)?;
        }
        Ok(())
    }
    pub(crate) fn wake_orphan_step(&self, item: &Item) -> OverlayResult<(u64, u64, bool)> {
        let rows = self
            .open
            .borrow()
            .waits
            .rows
            .range((item.target, 0)..=(item.target, i64::MAX))
            .take(64)
            .map(|(key, _)| key.1)
            .collect::<Vec<_>>();
        for serial in &rows {
            self.wake_orphan(*serial)?;
            let mut state = self.open.borrow_mut();
            state.waits.remove((item.target, *serial));
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
            &[&item.target, &item.cursor, &item.aux],
            32,
            |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)),
        )?;
        for (kind, resource) in &rows {
            self.execute(
                StatementKind::Reclaim,
                "UPDATE cleanup_task SET ready=1 WHERE kind=?1 AND resource=?2 AND target=?3",
                &[kind, resource, &item.target],
                32,
            )?;
        }
        if let Some((kind, resource)) = rows.last() {
            self.advance_item(item, 0, *kind, *resource, &[])?;
        } else {
            self.finish_item(item)?;
        }
        Ok((rows.len() as u64, 0, rows.is_empty()))
    }
}

//! Bounded orphan migration and last-owner deletion, independent of Commit count.
use crate::{
    db::integer,
    lifetime::orphan::{Orphan, DOMAIN},
    maintenance::{Item, Page, ORPHAN, PAGE, SERIAL_RETIRE},
    sql, Overlay, OverlayResult, StatementKind,
};
/// Step calls one releasing job makes: two for the orphan and two for each
/// of its at most two lower layers.
const RELEASE_CALLS: usize = 6;
/// What one step leaves of its work.
#[derive(Clone, Copy)]
pub(crate) enum Next {
    /// More is ready now.
    Ready,
    /// More is ready, from this migration cursor: the layer and its last cell.
    From(i64, i64),
    /// Its holder's release wakes it.
    Held,
    Done,
}
/// One step of an orphan or of a layer's retirement without its queue item:
/// the rows and bytes it reports, what it leaves, and the lower layer an
/// orphan's step released, whose retirement the caller queues or runs.
pub(crate) struct Step {
    rows: u64,
    bytes: u64,
    /// Payload cells of the page this step spent.
    cells: usize,
    next: Next,
    released: Option<i64>,
}
fn step(rows: u64, bytes: u64, next: Next) -> Step {
    Step {
        rows,
        bytes,
        cells: 0,
        next,
        released: None,
    }
}
/// A step that dropped the leading `fit` of a layer's selected payload
/// rows. When what is left of the page holds none of them, the page is
/// spent: the rows stay for the next step.
fn dropped(page: Page, fit: (usize, usize), bytes: u64) -> Step {
    Step {
        cells: if fit.0 == 0 { page.cells } else { fit.1 },
        ..step(fit.0 as u64, bytes, Next::Ready)
    }
}
impl Overlay {
    /// The job that dropped an orphan's last reference runs, in its own
    /// transaction, the steps the owner thread would run next for that
    /// serial, in the owner's order: the orphan's step, then the retirement
    /// of each layer this job released, around again. Each call is the step
    /// the queue runs, with its own hold checks, but no queue item is
    /// written for work that ends here. All calls together drop at most one
    /// step's page. What is left is queued as the owner's own step leaves
    /// it: ready, or parked under its hold. `orphan` is the row this job
    /// read after the reference count reached zero.
    pub(crate) fn finish_release(
        &self,
        ns: i64,
        serial: i64,
        mut orphan: Orphan,
    ) -> OverlayResult<()> {
        let ready = self.maintenance_ready.get();
        let item = |kind, target| Item {
            ns,
            kind,
            resource: serial,
            target,
            phase: 0,
            cursor: 0,
            aux: -1,
            name: Vec::new(),
        };
        let (mut page, mut calls) = (PAGE, 0);
        let spent = |calls, page: Page| calls == RELEASE_CALLS || page.cells == 0 || page.rows == 0;
        let charge = |page: &mut Page, step: &Step| {
            if !matches!(step.next, Next::Done) {
                page.rows -= step.rows as usize;
                page.cells -= step.cells;
            }
        };
        // Each call of the orphan's step releases at most one layer.
        let mut own = Next::Ready;
        let mut layers = [(0, Next::Done); RELEASE_CALLS];
        let mut released = 0;
        'rounds: loop {
            let mut unfinished = false;
            if matches!(own, Next::Ready) {
                if spent(calls, page) {
                    break;
                }
                calls += 1;
                let step = self.reclaim_orphan(ns, serial, &mut orphan, page)?;
                charge(&mut page, &step);
                if let Some(gen) = step.released {
                    layers[released] = (gen, Next::Ready);
                    released += 1;
                }
                own = step.next;
                unfinished |= !matches!(own, Next::Done);
            }
            // Layers are released newest first and retired oldest first.
            for (gen, next) in layers[..released].iter_mut().rev() {
                if matches!(next, Next::Ready) {
                    if spent(calls, page) {
                        break 'rounds;
                    }
                    calls += 1;
                    let step = self.retire_layer(ns, serial, *gen, Some(orphan), page)?;
                    charge(&mut page, &step);
                    *next = step.next;
                    unfinished |= !matches!(next, Next::Done);
                }
            }
            if !unfinished {
                break;
            }
        }
        let mut left = false;
        if matches!(own, Next::Done) {
            // An earlier descriptor or reader may have queued the orphan.
            self.finish_item(&item(ORPHAN, DOMAIN))?;
        } else {
            self.enqueue(ns, ORPHAN, serial, DOMAIN)?;
            self.wake_orphan(ns, serial)?;
            left = true;
        }
        for (gen, next) in &layers[..released] {
            match next {
                Next::Done => {}
                Next::Held => {
                    self.enqueue(ns, SERIAL_RETIRE, serial, *gen)?;
                    self.hold_item(&item(SERIAL_RETIRE, *gen))?;
                }
                _ => {
                    self.queue_retirement(ns, serial, *gen)?;
                    left = true;
                }
            }
        }
        // Nothing of this job is ready: the hint is what it was.
        if !left {
            self.maintenance_ready.set(ready);
        }
        Ok(())
    }
    /// The queue item's part of a step: the released layer's retirement
    /// queued, the item advanced, parked or finished.
    fn settle_item(&self, item: &Item, step: Step) -> OverlayResult<(u64, u64, bool)> {
        if let Some(gen) = step.released {
            self.queue_retirement(item.ns, item.resource, gen)?;
        }
        match step.next {
            Next::Ready => {}
            Next::From(phase, aux) => self.advance_item(item, phase, 0, aux, &[])?,
            Next::Held => self.hold_item(item)?,
            Next::Done => self.finish_item(item)?,
        }
        Ok((step.rows, step.bytes, matches!(step.next, Next::Done)))
    }
    /// A generation cursor that already skipped this independent source
    /// does not restart: the layer an orphan released is retired by its own
    /// ready item.
    fn queue_retirement(&self, ns: i64, serial: i64, gen: i64) -> OverlayResult<()> {
        self.enqueue(ns, SERIAL_RETIRE, serial, gen)?;
        self.execute(
            StatementKind::Reclaim,
            "UPDATE maintenance SET ready=1 WHERE ns=?1 AND kind=?2 AND resource=?3 AND target=?4",
            &[&ns, &SERIAL_RETIRE, &serial, &gen],
            32,
        )?;
        Ok(())
    }
    fn drop_custody(&self, ns: i64, serial: i64) -> OverlayResult<()> {
        self.execute(
            StatementKind::Lease,
            "DELETE FROM file_custody WHERE ns=?1 AND serial=?2 AND opens+lookups+readers=0",
            &[&ns, &serial],
            16,
        )?;
        Ok(())
    }
    pub(crate) fn maintain_orphan(
        &self,
        item: &Item,
        page: Page,
    ) -> OverlayResult<(u64, u64, bool)> {
        let (ns, serial) = (item.ns, item.resource);
        let Some(mut orphan) = self.orphan(ns, serial)? else {
            self.drop_custody(ns, serial)?;
            self.finish_item(item)?;
            return Ok((0, 0, true));
        };
        let step = if self.file_refs(ns, serial)? != 0 {
            self.migrate_orphan(ns, serial, orphan, (item.phase, item.aux))?
        } else {
            self.reclaim_orphan(ns, serial, &mut orphan, page)?
        };
        self.settle_item(item, step)
    }
    /// One step of an orphan that a descriptor, a reader or a lookup holds:
    /// one cell of its newest lower layer moves into its own domain, or that
    /// layer is released. `cursor` is the layer and the last cell moved.
    fn migrate_orphan(
        &self,
        ns: i64,
        serial: i64,
        orphan: Orphan,
        cursor: (i64, i64),
    ) -> OverlayResult<Step> {
        if orphan.top > orphan.floor {
            let lower = self
                .layers(ns, serial, orphan.top, orphan.floor)?
                .first()
                .copied();
            if let Some(lower) = lower {
                let state = self.state(self.route_for_ns(ns)?)?;
                if state.captured.is_some_and(|g| g.0 == lower.gen)
                    || self.generation_held(ns, lower.gen)?
                {
                    self.execute(
                        StatementKind::Lease,
                        "INSERT INTO orphan_wait VALUES(?1,?2,?3) ON CONFLICT DO NOTHING",
                        &[&ns, &lower.gen, &serial],
                        24,
                    )?;
                    return Ok(step(0, 0, Next::Held));
                }
                let upper = self.orphan_layer(ns, serial)?;
                let cell=self.query(StatementKind::Reclaim,"SELECT cell_offset FROM payload
                    WHERE ns=?1 AND serial=?2 AND gen=?3 AND cell_offset>?4 ORDER BY cell_offset LIMIT 1",
                    &[&ns,&serial,&lower.gen,&if cursor.0==lower.gen {cursor.1}else{-1}],32,|r|r.get::<_,i64>(0))?.pop();
                if let Some(cell) = cell {
                    // Snapshot owners are fenced above: the row moves into
                    // the orphan's domain, keeping its physical custody.
                    let bytes = self.transfer_row(ns, serial, &lower, &upper, cell)?;
                    return Ok(step(1, bytes, Next::From(lower.gen, cell)));
                }
                self.execute(StatementKind::Inode,"UPDATE inode SET inherited_cutoff=min(inherited_cutoff,?3) WHERE ns=?1 AND serial=?2 AND gen=-1",
                    &[&ns,&serial,&integer(lower.cutoff)?],24)?;
                self.release_orphan_layer(
                    ns,
                    serial,
                    lower.gen,
                    (lower.gen - 1).max(orphan.floor),
                )?;
                return Ok(Step {
                    released: Some(lower.gen),
                    ..step(1, 0, Next::From(0, -1))
                });
            }
            self.execute(
                StatementKind::Lease,
                "UPDATE orphan SET lower_top=lower_floor WHERE ns=?1 AND serial=?2",
                &[&ns, &serial],
                16,
            )?;
        }
        Ok(step(0, 0, Next::Held))
    }
    /// One step of an orphan nothing holds: its newest lower layer is
    /// released, or a page of its own cells or shrink rows is dropped, or
    /// its rows are deleted. `orphan` follows the row.
    fn reclaim_orphan(
        &self,
        ns: i64,
        serial: i64,
        orphan: &mut Orphan,
        page: Page,
    ) -> OverlayResult<Step> {
        // On last owner release, lower namespace ownership is separate. Target
        // only already-retired source rows; future installs own remaining ones.
        if orphan.top > orphan.floor {
            let lower = self
                .layers(ns, serial, orphan.top, orphan.floor)?
                .first()
                .copied();
            if let Some(lower) = lower {
                let next = (lower.gen - 1).max(orphan.floor);
                self.release_orphan_layer(ns, serial, lower.gen, next)?;
                orphan.top = next;
                return Ok(Step {
                    released: Some(lower.gen),
                    ..step(0, 0, Next::Ready)
                });
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
            let fit = page.fit(rows.iter().map(|row| row.1));
            let mut bytes = 0;
            for (cell, size) in &rows[..fit.0] {
                self.execute(
                    StatementKind::Reclaim,
                    sql::CELL_DROP,
                    &[&ns, &serial, &DOMAIN, cell],
                    32,
                )?;
                bytes += size;
            }
            return Ok(dropped(page, fit, bytes));
        }
        let mut steps = self.query(
            StatementKind::Reclaim,
            "SELECT depth FROM shrink WHERE ns=?1 AND serial=?2 AND gen=-1 ORDER BY depth LIMIT 64",
            &[&ns, &serial],
            16,
            |r| r.get::<_, i64>(0),
        )?;
        steps.truncate(page.rows);
        if !steps.is_empty() {
            for depth in &steps {
                self.execute(
                    StatementKind::Reclaim,
                    "DELETE FROM shrink WHERE ns=?1 AND serial=?2 AND gen=-1 AND depth=?3",
                    &[&ns, &serial, depth],
                    24,
                )?;
            }
            return Ok(step(steps.len() as u64, 0, Next::Ready));
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
        self.orphans_deleted()?;
        self.drop_custody(ns, serial)?;
        Ok(step(3, 0, Next::Done))
    }
    pub(crate) fn retire_serial(&self, item: &Item, page: Page) -> OverlayResult<(u64, u64, bool)> {
        let step = self.retire_layer(item.ns, item.resource, item.target, None, page)?;
        self.settle_item(item, step)
    }
    /// One step of retiring layer `gen` of a serial: held by a generation
    /// owner or by the serial's orphan, or a page of its cells or shrink
    /// rows dropped, or its superseded row deleted. `orphan` is the serial's
    /// orphan row when the caller holds it as this job left it.
    fn retire_layer(
        &self,
        ns: i64,
        serial: i64,
        gen: i64,
        orphan: Option<Orphan>,
        page: Page,
    ) -> OverlayResult<Step> {
        if self.generation_held(ns, gen)?
            || match orphan {
                Some(orphan) => orphan.holds(gen),
                None => self.orphan_holds(ns, serial, gen)?,
            }
        {
            return Ok(step(0, 0, Next::Held));
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
            let fit = page.fit(rows.iter().map(|row| row.1));
            let mut bytes = 0;
            for (cell, size) in &rows[..fit.0] {
                self.execute(
                    StatementKind::Reclaim,
                    sql::CELL_DROP,
                    &[&ns, &serial, &gen, cell],
                    32,
                )?;
                bytes += size;
            }
            return Ok(dropped(page, fit, bytes));
        }
        let mut steps = self.query(
            StatementKind::Reclaim,
            "SELECT depth FROM shrink WHERE ns=?1 AND serial=?2 AND gen=?3 ORDER BY depth LIMIT 64",
            &[&ns, &serial, &gen],
            24,
            |r| r.get::<_, i64>(0),
        )?;
        steps.truncate(page.rows);
        if !steps.is_empty() {
            for depth in &steps {
                self.execute(
                    StatementKind::Reclaim,
                    "DELETE FROM shrink WHERE ns=?1 AND serial=?2 AND gen=?3 AND depth=?4",
                    &[&ns, &serial, &gen, depth],
                    32,
                )?;
            }
            return Ok(step(steps.len() as u64, 0, Next::Ready));
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
        Ok(step(1, 0, Next::Done))
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

//! Bounded orphan migration and last-owner deletion, independent of Commit count.
use crate::{
    db::integer,
    lifetime::orphan::{Orphan, DOMAIN},
    maintenance::{Item, Page, ORPHAN, PAGE, SERIAL_RETIRE},
    sql, Overlay, OverlayResult, StatementKind,
};
/// One page of a layer's payload rows: `PAGE.cells` rows at most, so fewer
/// rows are all the layer holds.
const LAYER_PAYLOAD_PAGE: &str =
    "SELECT cell_offset,length(data)+ifnull(length(validity),0) FROM fs_data
            WHERE inode=?1 AND generation=?2 ORDER BY cell_offset LIMIT 14";
const _: () = assert!(PAGE.cells == 14);
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
/// What one job has established about a layer it is retiring, for its next
/// step of that layer in the same transaction. Nothing of it outlives the
/// job: a queue item's step starts from nothing.
#[derive(Clone, Copy, Default)]
pub(crate) struct Retiring {
    /// No generation owner holds the layer. The job writes no such owner.
    free: bool,
    /// A step of this job selected fewer payload rows than a page and
    /// dropped them all: the layer holds no cell, and the job adds none.
    cells_gone: bool,
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
    pub(crate) fn finish_release(&self, serial: i64, mut orphan: Orphan) -> OverlayResult<()> {
        let ready = self.maintenance_ready.get();
        let item = |kind, target| Item {
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
        let mut layers = [(0, Next::Done, Retiring::default()); RELEASE_CALLS];
        let mut released = 0;
        'rounds: loop {
            let mut unfinished = false;
            if matches!(own, Next::Ready) {
                if spent(calls, page) {
                    break;
                }
                calls += 1;
                let step = self.reclaim_orphan(serial, &mut orphan, page)?;
                charge(&mut page, &step);
                if let Some(gen) = step.released {
                    layers[released] = (gen, Next::Ready, Retiring::default());
                    released += 1;
                }
                own = step.next;
                unfinished |= !matches!(own, Next::Done);
            }
            // Layers are released newest first and retired oldest first.
            for (gen, next, known) in layers[..released].iter_mut().rev() {
                if matches!(next, Next::Ready) {
                    if spent(calls, page) {
                        break 'rounds;
                    }
                    calls += 1;
                    let step = self.retire_layer(serial, *gen, Some(orphan), page, known)?;
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
            self.enqueue(ORPHAN, serial, DOMAIN)?;
            self.wake_orphan(serial)?;
            left = true;
        }
        for (gen, next, _) in &layers[..released] {
            match next {
                Next::Done => {}
                Next::Held => {
                    self.enqueue(SERIAL_RETIRE, serial, *gen)?;
                    self.hold_item(&item(SERIAL_RETIRE, *gen))?;
                }
                _ => {
                    self.queue_retirement(serial, *gen)?;
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
            self.queue_retirement(item.resource, gen)?;
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
    fn queue_retirement(&self, serial: i64, gen: i64) -> OverlayResult<()> {
        self.enqueue(SERIAL_RETIRE, serial, gen)?;
        self.execute(
            StatementKind::Reclaim,
            "UPDATE cleanup_task SET ready=1 WHERE kind=?1 AND resource=?2 AND target=?3",
            &[&SERIAL_RETIRE, &serial, &gen],
            32,
        )?;
        Ok(())
    }
    /// The custody row of a file nothing references.
    pub(crate) fn drop_custody(&self, serial: i64) -> OverlayResult<()> {
        let mut state = self.open.borrow_mut();
        if state
            .custody
            .rows
            .get(&serial)
            .is_some_and(|refs| refs.total() == 0)
        {
            state.custody.remove(serial);
            state.parents.remove(serial as u64);
        }
        Ok(())
    }
    pub(crate) fn maintain_orphan(
        &self,
        item: &Item,
        page: Page,
    ) -> OverlayResult<(u64, u64, bool)> {
        let serial = item.resource;
        let Some(mut orphan) = self.orphan(serial)? else {
            self.drop_custody(serial)?;
            self.finish_item(item)?;
            return Ok((0, 0, true));
        };
        let step = if self.file_refs(serial)? != 0 {
            self.migrate_orphan(serial, orphan, (item.phase, item.aux))?
        } else {
            self.reclaim_orphan(serial, &mut orphan, page)?
        };
        self.settle_item(item, step)
    }
    /// One step of an orphan that a descriptor, a reader or a lookup holds:
    /// one cell of its newest lower layer moves into its own domain, or that
    /// layer is released. `cursor` is the layer and the last cell moved.
    fn migrate_orphan(
        &self,
        serial: i64,
        orphan: Orphan,
        cursor: (i64, i64),
    ) -> OverlayResult<Step> {
        if orphan.top > orphan.floor {
            let lower = self
                .layers(serial, orphan.top, orphan.floor)?
                .first()
                .copied();
            if let Some(lower) = lower {
                if self.orphan_source_awaited(lower.gen)? {
                    let mut state = self.open.borrow_mut();
                    state.waits.put((lower.gen, serial), ());
                    return Ok(step(0, 0, Next::Held));
                }
                let upper = self.orphan_layer(serial)?;
                let cell=self.query(StatementKind::Reclaim,"SELECT cell_offset FROM fs_data
                    WHERE inode=?1 AND generation=?2 AND cell_offset>?3 ORDER BY cell_offset LIMIT 1",
                    &[&serial,&lower.gen,&if cursor.0==lower.gen {cursor.1}else{-1}],32,|r|r.get::<_,i64>(0))?.pop();
                if let Some(cell) = cell {
                    // Snapshot owners are fenced above: the row moves into
                    // the orphan's domain, keeping its physical custody.
                    let bytes = self.transfer_row(serial, &lower, &upper, cell)?;
                    return Ok(step(1, bytes, Next::From(lower.gen, cell)));
                }
                // Its cells have moved: its pieces follow, one step each.
                if lower.referenced && self.transfer_piece(serial, &lower, &upper)? {
                    return Ok(step(1, 0, Next::Ready));
                }
                self.execute(StatementKind::Inode,"UPDATE fs_inode SET inherited_cutoff=min(inherited_cutoff,?2) WHERE inode=?1 AND generation=-1",
                    &[&serial,&integer(lower.cutoff)?],24)?;
                self.release_orphan_layer(serial, lower.gen, (lower.gen - 1).max(orphan.floor))?;
                return Ok(Step {
                    released: Some(lower.gen),
                    ..step(1, 0, Next::From(0, -1))
                });
            }
            let mut state = self.open.borrow_mut();
            state.orphans.put(
                serial,
                Orphan {
                    top: orphan.floor,
                    ..orphan
                },
            );
        }
        Ok(step(0, 0, Next::Held))
    }
    /// One step of an orphan nothing holds: its newest lower layer is
    /// released, or a page of its own cells or shrink rows is dropped, or
    /// its rows are deleted. `orphan` follows the row.
    fn reclaim_orphan(&self, serial: i64, orphan: &mut Orphan, page: Page) -> OverlayResult<Step> {
        // On last owner release, lower namespace ownership is separate. Target
        // only already-retired source rows; future installs own remaining ones.
        if orphan.top > orphan.floor {
            let lower = self
                .layers(serial, orphan.top, orphan.floor)?
                .first()
                .copied();
            if let Some(lower) = lower {
                let next = (lower.gen - 1).max(orphan.floor);
                self.release_orphan_layer(serial, lower.gen, next)?;
                orphan.top = next;
                return Ok(Step {
                    released: Some(lower.gen),
                    ..step(0, 0, Next::Ready)
                });
            }
        }
        let rows = self.query(
            StatementKind::Reclaim,
            "SELECT cell_offset,length(data)+ifnull(length(validity),0) FROM fs_data
            WHERE inode=?1 AND generation=-1 ORDER BY cell_offset LIMIT 14",
            &[&serial],
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
                    &[&serial, &DOMAIN, cell],
                    32,
                )?;
                bytes += size;
            }
            return Ok(dropped(page, fit, bytes));
        }
        if self.pieces_seen.get() {
            let pieces = self.drop_pieces(serial, DOMAIN, page.rows)?;
            if pieces != 0 {
                return Ok(step(pieces, 0, Next::Ready));
            }
        }
        let mut steps = self.query(
            StatementKind::Reclaim,
            "SELECT depth FROM fs_truncate WHERE inode=?1 AND generation=-1 ORDER BY depth LIMIT 64",
            &[ &serial],
            16,
            |r| r.get::<_, i64>(0),
        )?;
        steps.truncate(page.rows);
        if !steps.is_empty() {
            for depth in &steps {
                self.execute(
                    StatementKind::Reclaim,
                    "DELETE FROM fs_truncate WHERE inode=?1 AND generation=-1 AND depth=?2",
                    &[&serial, depth],
                    24,
                )?;
            }
            return Ok(step(steps.len() as u64, 0, Next::Ready));
        }
        self.execute(
            StatementKind::Reclaim,
            "DELETE FROM fs_inode WHERE inode=?1 AND generation=-1",
            &[&serial],
            16,
        )?;
        {
            let mut state = self.open.borrow_mut();
            state.orphans.remove(serial);
        }
        self.orphans_deleted()?;
        self.drop_custody(serial)?;
        Ok(step(3, 0, Next::Done))
    }
    pub(crate) fn retire_serial(&self, item: &Item, page: Page) -> OverlayResult<(u64, u64, bool)> {
        let mut known = Retiring::default();
        let step = self.retire_layer(item.resource, item.target, None, page, &mut known)?;
        self.settle_item(item, step)
    }
    /// One step of retiring layer `gen` of a serial: held by a generation
    /// owner or by the serial's orphan, or a page of its cells or shrink
    /// rows dropped, or its superseded row deleted. `orphan` is the serial's
    /// orphan row when the caller holds it as this job left it. `known` is
    /// what earlier steps of the same job established about the layer: a
    /// fact it holds is not read again.
    fn retire_layer(
        &self,
        serial: i64,
        gen: i64,
        orphan: Option<Orphan>,
        page: Page,
        known: &mut Retiring,
    ) -> OverlayResult<Step> {
        if (!known.free && self.generation_held(gen)?)
            || match orphan {
                Some(orphan) => orphan.holds(gen),
                None => self.orphan_holds(serial, gen)?,
            }
        {
            return Ok(step(0, 0, Next::Held));
        }
        known.free = true;
        let rows = match known.cells_gone {
            true => Vec::new(),
            false => self.query(
                StatementKind::Reclaim,
                LAYER_PAYLOAD_PAGE,
                &[&serial, &gen],
                24,
                |r| Ok((r.get::<_, i64>(0)?, crate::db::unsigned(r, 1)?)),
            )?,
        };
        if !rows.is_empty() {
            let fit = page.fit(rows.iter().map(|row| row.1));
            known.cells_gone = rows.len() < PAGE.cells && fit.0 == rows.len();
            let mut bytes = 0;
            for (cell, size) in &rows[..fit.0] {
                self.execute(
                    StatementKind::Reclaim,
                    sql::CELL_DROP,
                    &[&serial, &gen, cell],
                    32,
                )?;
                bytes += size;
            }
            return Ok(dropped(page, fit, bytes));
        }
        if self.pieces_seen.get() {
            let pieces = self.drop_pieces(serial, gen, page.rows)?;
            if pieces != 0 {
                return Ok(step(pieces, 0, Next::Ready));
            }
        }
        let mut steps = self.query(
            StatementKind::Reclaim,
            "SELECT depth FROM fs_truncate WHERE inode=?1 AND generation=?2 ORDER BY depth LIMIT 64",
            &[ &serial, &gen],
            24,
            |r| r.get::<_, i64>(0),
        )?;
        steps.truncate(page.rows);
        if !steps.is_empty() {
            for depth in &steps {
                self.execute(
                    StatementKind::Reclaim,
                    "DELETE FROM fs_truncate WHERE inode=?1 AND generation=?2 AND depth=?3",
                    &[&serial, &gen, depth],
                    32,
                )?;
            }
            return Ok(step(steps.len() as u64, 0, Next::Ready));
        }
        // Active tombstones prevent serial lookup falling through to the old
        // base. Only already-installed metadata may be physically removed:
        // a row at or below the install frontier, or one a later row of the
        // view supersedes. The fixed runtime frontier and this serial's
        // indexed later rows decide the same predicate.
        let state = self.state()?;
        let removable = self
            .query(
                StatementKind::Inode,
                sql::LAYER_SUPERSEDED,
                &[&serial, &gen, &state.installed, &state.active.0],
                24,
                |r| r.get::<_, bool>(0),
            )?
            .pop()
            .ok_or(crate::OverlayError::Stale)?;
        if removable {
            self.execute(
                StatementKind::Reclaim,
                "DELETE FROM fs_inode WHERE inode=?1 AND generation=?2",
                &[&serial, &gen],
                24,
            )?;
        }
        Ok(step(1, 0, Next::Done))
    }
    pub(crate) fn wake_orphan(&self, serial: i64) -> OverlayResult<()> {
        let changed = self.execute(
            StatementKind::Reclaim,
            "UPDATE cleanup_task SET ready=1 WHERE kind=?1 AND resource=?2 AND target=-1",
            &[&ORPHAN, &serial],
            24,
        )?;
        if changed != 0 {
            self.maintenance_ready.set(true)
        }
        Ok(())
    }
}

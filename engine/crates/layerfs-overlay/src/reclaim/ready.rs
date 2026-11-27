//! Backed ready work with bounded indexed turns and no lifetime-sized roster.
use crate::{db::integer, Overlay, OverlayError, OverlayResult, ReclaimStep, StatementKind};

pub(crate) const FOLD: i64 = 1;
pub(crate) const WAKE_ORPHAN: i64 = 8;
pub(crate) const WAKE_GENERATION: i64 = 9;
pub(crate) const ORPHAN: i64 = 2;
pub(crate) const SERIAL_RETIRE: i64 = 7;
pub(crate) const RETIRE: i64 = 3;
pub(crate) const STALE: i64 = 4;
pub(crate) const STEPS: i64 = 5;
pub(crate) const OPERATION_RECORD: i64 = 6;
/// Stale reference pieces of one layer after a shrink.
pub(crate) const STALE_PIECES: i64 = 12;
const READY: &str = "SELECT kind,resource,target,phase,cursor,aux,name FROM cleanup_task
    INDEXED BY cleanup_task_ready WHERE ready=1 AND (kind,resource,target)>(?1,?2,?3)
    ORDER BY kind,resource,target LIMIT 1";
/// What a step of an orphan or of a layer's retirement may still drop:
/// payload cells, and rows of any kind. A row wider than one cell counts as
/// the cells it holds, so a page is the same bytes however its rows are
/// shaped. The statements select one whole page; a caller that has spent
/// part of it drops fewer.
#[derive(Clone, Copy)]
pub(crate) struct Page {
    pub cells: usize,
    pub rows: usize,
}
/// The page of one maintenance step.
pub(crate) const PAGE: Page = Page {
    cells: 14,
    rows: 64,
};
// A whole page holds the widest row: every step of a fresh page progresses.
const _: () = assert!(crate::RUN_BYTES / crate::CELL_BYTES <= PAGE.cells);
impl Page {
    /// The leading rows this page can still drop, and the cells they hold.
    /// `stored` is each selected row's stored bytes, in order. None fits
    /// only a page that is partly spent.
    pub(crate) fn fit(self, stored: impl Iterator<Item = u64>) -> (usize, usize) {
        let (mut rows, mut cells) = (0, 0);
        for bytes in stored {
            // A row of one cell may carry a mask beside its bytes.
            let held = (bytes as usize / crate::CELL_BYTES).max(1);
            if rows == self.rows || cells + held > self.cells {
                break;
            }
            rows += 1;
            cells += held;
        }
        (rows, cells)
    }
}

/// Fixed scheduler cursor, independent of the number of maintenance targets.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct MaintenanceCursor(pub(crate) [i64; 3]);
/// One actually serviced maintenance item and its next scheduling cursor.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MaintenanceStep {
    pub cursor: MaintenanceCursor,
    pub work: ReclaimStep,
}
pub(crate) struct Item {
    pub kind: i64,
    pub resource: i64,
    pub target: i64,
    pub phase: i64,
    pub cursor: i64,
    pub aux: i64,
    pub name: Vec<u8>,
}
impl Item {
    /// One cleanup row in this Workspace database.
    fn decode(r: &rusqlite::Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            kind: r.get(0)?,
            resource: r.get(1)?,
            target: r.get(2)?,
            phase: r.get(3)?,
            cursor: r.get(4)?,
            aux: r.get(5)?,
            name: r.get(6)?,
        })
    }
    pub fn key(&self) -> MaintenanceCursor {
        MaintenanceCursor([self.kind, self.resource, self.target])
    }
}
impl Overlay {
    /// Bounded read-only observation of pending namespace maintenance. Held
    /// items are pending, not silently reclaimed. This never drives cleanup.
    pub fn maintenance_idle(&self) -> OverlayResult<bool> {
        self.state()?;
        Ok(self
            .query(
                StatementKind::Reclaim,
                "SELECT 1 FROM cleanup_task LIMIT 1",
                &[],
                8,
                |_| Ok(()),
            )?
            .is_empty())
    }
    pub(crate) fn enqueue(&self, kind: i64, resource: i64, target: i64) -> OverlayResult<()> {
        self.execute(
            StatementKind::Reclaim,
            "INSERT INTO cleanup_task(kind,resource,target) VALUES(?1,?2,?3)
            ON CONFLICT(kind,resource,target) DO NOTHING",
            &[&kind, &resource, &target],
            32,
        )?;
        self.maintenance_ready.set(true);
        Ok(())
    }
    pub(crate) fn advance_item(
        &self,
        item: &Item,
        phase: i64,
        cursor: i64,
        aux: i64,
        name: &[u8],
    ) -> OverlayResult<()> {
        self.execute(
            StatementKind::Reclaim,
            "UPDATE cleanup_task SET phase=?4,cursor=?5,aux=?6,name=?7
            WHERE kind=?1 AND resource=?2 AND target=?3",
            &[
                &item.kind,
                &item.resource,
                &item.target,
                &phase,
                &cursor,
                &aux,
                &name,
            ],
            56 + name.len() as u64,
        )?;
        Ok(())
    }
    pub(crate) fn finish_item(&self, item: &Item) -> OverlayResult<()> {
        self.execute(
            StatementKind::Reclaim,
            "DELETE FROM cleanup_task
            WHERE kind=?1 AND resource=?2 AND target=?3",
            &[&item.kind, &item.resource, &item.target],
            32,
        )?;
        Ok(())
    }
    pub(crate) fn hold_item(&self, item: &Item) -> OverlayResult<()> {
        self.execute(
            StatementKind::Reclaim,
            "UPDATE cleanup_task SET ready=0
            WHERE kind=?1 AND resource=?2 AND target=?3",
            &[&item.kind, &item.resource, &item.target],
            32,
        )?;
        Ok(())
    }
    pub(crate) fn generation_held(&self, generation: i64) -> OverlayResult<bool> {
        self.available()?;
        Ok(self
            .open
            .borrow()
            .captured_generations
            .rows
            .contains_key(&generation))
    }
    /// One weighted live-state maintenance turn. It never waits, processes a
    /// whole payload, retries a failure or scans a held population. The daemon
    /// continues this work during foreground service and idle periods.
    pub fn maintain(&self, after: MaintenanceCursor) -> OverlayResult<Option<MaintenanceStep>> {
        self.available()?;
        if !self.maintenance_ready.get() {
            return Ok(None);
        }
        self.atomic_cleanup(|| {
            // A step is already one bounded page: a release inside it queues.
            self.release_step.set(false);
            let mut item = self
                .query(
                    StatementKind::Reclaim,
                    READY,
                    &[&after.0[0], &after.0[1], &after.0[2]],
                    32,
                    Item::decode,
                )?
                .pop();
            if item.is_none() && after != MaintenanceCursor::default() {
                item = self
                    .query(
                        StatementKind::Reclaim,
                        READY,
                        &[&0_i64, &0_i64, &0_i64],
                        32,
                        Item::decode,
                    )?
                    .pop();
            }
            let Some(item) = item else {
                self.maintenance_ready.set(false);
                return Ok(None);
            };
            let (rows, bytes, done) =
                if matches!(item.kind, FOLD | RETIRE) && self.generation_held(item.target)? {
                    self.hold_item(&item)?;
                    (0, 0, false)
                } else {
                    match item.kind {
                        FOLD => self.fold_namespace(&item)?,
                        RETIRE => self.retire_generation(&item)?,
                        ORPHAN => self.maintain_orphan(&item, PAGE)?,
                        WAKE_ORPHAN => self.wake_orphan_step(&item)?,
                        WAKE_GENERATION => self.wake_generation_step(&item)?,
                        SERIAL_RETIRE => self.retire_serial(&item, PAGE)?,
                        STALE | STEPS | OPERATION_RECORD => self.clean_live_item(&item)?,
                        STALE_PIECES => self.clean_pieces(&item)?,
                        _ => return Err(OverlayError::Invalid("maintenance kind")),
                    }
                };
            Ok(Some(MaintenanceStep {
                cursor: item.key(),
                work: ReclaimStep {
                    rows,
                    data_bytes: bytes,
                    done,
                },
            }))
        })
    }
    pub(crate) fn wake_generation(&self, generation: u64) -> OverlayResult<()> {
        let gen = integer(generation)?;
        if gen != 0 && !self.generation_held(gen)? {
            // Only enqueue here: one last-owner release must not update the
            // arbitrarily many serial-retirement targets for this generation.
            self.execute(
                StatementKind::Reclaim,
                "INSERT INTO cleanup_task(kind,resource,target) VALUES(9,0,?1)
                 ON CONFLICT(kind,resource,target) DO UPDATE SET cursor=0,aux=-1,ready=1",
                &[&gen],
                16,
            )?;
            self.maintenance_ready.set(true);
            self.wake_orphan_sources(gen)?;
        }
        Ok(())
    }
}

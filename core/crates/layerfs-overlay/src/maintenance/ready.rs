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
pub(crate) const SCRATCH: i64 = 6;
const READY: &str = "SELECT ns,kind,resource,target,phase,cursor,aux,name FROM maintenance
    INDEXED BY maintenance_ready WHERE ready=1 AND (ns,kind,resource,target)>(?1,?2,?3,?4)
    ORDER BY ns,kind,resource,target LIMIT 1";

/// Fixed scheduler cursor, independent of the number of maintenance targets.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct MaintenanceCursor(pub(crate) [i64; 4]);
/// One actually serviced maintenance item and its next scheduling cursor.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MaintenanceStep {
    pub cursor: MaintenanceCursor,
    pub work: ReclaimStep,
}
pub(crate) struct Item {
    pub ns: i64,
    pub kind: i64,
    pub resource: i64,
    pub target: i64,
    pub phase: i64,
    pub cursor: i64,
    pub aux: i64,
    pub name: Vec<u8>,
}
impl Item {
    fn decode(r: &rusqlite::Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            ns: r.get(0)?,
            kind: r.get(1)?,
            resource: r.get(2)?,
            target: r.get(3)?,
            phase: r.get(4)?,
            cursor: r.get(5)?,
            aux: r.get(6)?,
            name: r.get(7)?,
        })
    }
    pub fn key(&self) -> MaintenanceCursor {
        MaintenanceCursor([self.ns, self.kind, self.resource, self.target])
    }
}
impl Overlay {
    /// Bounded read-only observation of pending namespace maintenance. Held
    /// items are pending, not silently reclaimed. This never drives cleanup.
    pub fn maintenance_idle(&self, route: crate::Route) -> OverlayResult<bool> {
        self.state(route)?;
        Ok(self
            .query(
                StatementKind::Reclaim,
                "SELECT 1 FROM maintenance WHERE ns=?1 LIMIT 1",
                &[&route.ns],
                8,
                |_| Ok(()),
            )?
            .is_empty())
    }
    pub(crate) fn enqueue(
        &self,
        ns: i64,
        kind: i64,
        resource: i64,
        target: i64,
    ) -> OverlayResult<()> {
        self.execute(
            StatementKind::Reclaim,
            "INSERT INTO maintenance(ns,kind,resource,target) VALUES(?1,?2,?3,?4)
            ON CONFLICT(ns,kind,resource,target) DO NOTHING",
            &[&ns, &kind, &resource, &target],
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
            "UPDATE maintenance SET phase=?5,cursor=?6,aux=?7,name=?8
            WHERE ns=?1 AND kind=?2 AND resource=?3 AND target=?4",
            &[
                &item.ns,
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
            "DELETE FROM maintenance
            WHERE ns=?1 AND kind=?2 AND resource=?3 AND target=?4",
            &[&item.ns, &item.kind, &item.resource, &item.target],
            32,
        )?;
        Ok(())
    }
    pub(crate) fn hold_item(&self, item: &Item) -> OverlayResult<()> {
        self.execute(
            StatementKind::Reclaim,
            "UPDATE maintenance SET ready=0
            WHERE ns=?1 AND kind=?2 AND resource=?3 AND target=?4",
            &[&item.ns, &item.kind, &item.resource, &item.target],
            32,
        )?;
        Ok(())
    }
    pub(crate) fn generation_held(&self, ns: i64, generation: i64) -> OverlayResult<bool> {
        Ok(!self
            .query(
                StatementKind::Lease,
                crate::sql::GENERATION_HELD,
                &[&ns, &generation],
                16,
                |_| Ok(()),
            )?
            .is_empty())
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
            let mut item = self
                .query(
                    StatementKind::Reclaim,
                    READY,
                    &[&after.0[0], &after.0[1], &after.0[2], &after.0[3]],
                    32,
                    Item::decode,
                )?
                .pop();
            if item.is_none() && after != MaintenanceCursor::default() {
                item = self
                    .query(
                        StatementKind::Reclaim,
                        READY,
                        &[&0_i64, &0_i64, &0_i64, &0_i64],
                        32,
                        Item::decode,
                    )?
                    .pop();
            }
            let Some(item) = item else {
                self.maintenance_ready.set(false);
                return Ok(None);
            };
            let (rows, bytes, done) = if matches!(item.kind, FOLD | RETIRE)
                && self.generation_held(item.ns, item.target)?
            {
                self.hold_item(&item)?;
                (0, 0, false)
            } else {
                match item.kind {
                    FOLD => self.fold_namespace(&item)?,
                    RETIRE => self.retire_generation(&item)?,
                    ORPHAN => self.maintain_orphan(&item)?,
                    WAKE_ORPHAN => self.wake_orphan_step(&item)?,
                    WAKE_GENERATION => self.wake_generation_step(&item)?,
                    SERIAL_RETIRE => self.retire_serial(&item)?,
                    STALE | STEPS | SCRATCH => self.clean_live_item(&item)?,
                    _ => return Err(OverlayError::Invalid("maintenance kind")),
                }
            };
            Ok(Some(MaintenanceStep {
                cursor: item.key(),
                work: ReclaimStep {
                    namespace: item.ns as u64,
                    rows,
                    data_bytes: bytes,
                    done,
                },
            }))
        })
    }
    /// Ready queue strategy and live physical/debt observations are separate:
    /// this returns the actual indexed queue plan without scanning its items.
    pub fn explain_maintenance(&self) -> OverlayResult<Vec<String>> {
        let mut plans = Vec::new();
        for statement in [READY, super::source_wait::GENERATION_WINDOW] {
            plans.extend(self.query(
                StatementKind::Explain,
                &format!("EXPLAIN QUERY PLAN {statement}"),
                &[&0_i64, &0_i64, &0_i64, &0_i64],
                32,
                |row| row.get::<_, String>(3),
            )?);
        }
        Ok(plans)
    }
    pub(crate) fn wake_generation(&self, ns: i64, generation: u64) -> OverlayResult<()> {
        let gen = integer(generation)?;
        if gen != 0 && !self.generation_held(ns, gen)? {
            // Only enqueue here: one last-owner release must not update the
            // arbitrarily many serial-retirement targets for this generation.
            self.execute(
                StatementKind::Reclaim,
                "INSERT INTO maintenance(ns,kind,resource,target) VALUES(?1,9,0,?2)
                 ON CONFLICT(ns,kind,resource,target) DO UPDATE SET cursor=0,aux=-1,ready=1",
                &[&ns, &gen],
                16,
            )?;
            self.maintenance_ready.set(true);
            self.wake_orphan_sources(ns, gen)?;
        }
        Ok(())
    }
}

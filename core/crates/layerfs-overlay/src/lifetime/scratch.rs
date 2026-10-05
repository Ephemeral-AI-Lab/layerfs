//! Backed operation scratch and exact owner references, all namespace-qualified.
use crate::{
    db::{integer, unsigned},
    sql, Lease, LeaseKind, Overlay, OverlayError, OverlayResult, Route, ScratchRecord,
    StatementKind, SCRATCH_BYTES,
};

impl Overlay {
    /// Acquires one exact independently keyed owner. Duplicate acquisition fails.
    pub fn acquire(&self, route: Route, lease: Lease) -> OverlayResult<()> {
        if matches!(
            lease.kind,
            LeaseKind::FileReader
                | LeaseKind::CapturedReader
                | LeaseKind::FileHandle
                | LeaseKind::Processing
        ) {
            return Err(OverlayError::Invalid("minted owner kind"));
        }
        let owner = integer(lease.owner)?;
        let resource = integer(lease.resource)?;
        if owner == 0 {
            return Err(OverlayError::Invalid("zero owner"));
        }
        self.atomic(|| {
            self.live(route)?;
            self.execute(
                StatementKind::Lease,
                "INSERT INTO lease VALUES(?1,?2,?3,?4)",
                &[&route.ns, &(lease.kind as i64), &owner, &resource],
                32,
            )?;
            if lease.resource != 0 {
                self.file_ref(route.ns, resource, lease.kind, true)?;
            }
            Ok(())
        })
    }
    /// Last-owner release is exact, with no whole-namespace owner sweep.
    /// Automatic eligibility/reclamation scheduling is the later S6/daemon lane.
    pub fn release(&self, route: Route, lease: Lease) -> OverlayResult<()> {
        if matches!(
            lease.kind,
            LeaseKind::FileReader
                | LeaseKind::CapturedReader
                | LeaseKind::FileHandle
                | LeaseKind::Processing
        ) {
            return Err(OverlayError::Invalid("minted owner kind"));
        }
        self.atomic(|| {
            self.state(route)?;
            let changed = self.execute(
                StatementKind::Lease,
                "DELETE FROM lease WHERE ns=?1 AND kind=?2 AND owner=?3 AND resource=?4",
                &[
                    &route.ns,
                    &(lease.kind as i64),
                    &integer(lease.owner)?,
                    &integer(lease.resource)?,
                ],
                32,
            )?;
            if changed != 1 {
                return Err(OverlayError::Missing);
            }
            if lease.resource != 0 {
                self.file_ref(route.ns, integer(lease.resource)?, lease.kind, false)?;
            }
            if lease.kind == LeaseKind::Reader {
                self.wake_generation(route.ns, lease.resource)?;
            }
            if lease.kind == LeaseKind::Operation && lease.resource == 0 {
                self.enqueue(
                    route.ns,
                    crate::maintenance::SCRATCH,
                    integer(lease.owner)?,
                    -2,
                )?;
            }
            self.queue_closed(route)
        })
    }
    pub(crate) fn operation(&self, route: Route, owner: u64) -> OverlayResult<()> {
        self.live(route)?;
        if self
            .query(
                StatementKind::Lease,
                "SELECT 1 FROM lease WHERE ns=?1 AND kind=?2 AND owner=?3 AND resource=0",
                &[&route.ns, &(LeaseKind::Operation as i64), &integer(owner)?],
                24,
                |r| r.get::<_, i64>(0),
            )?
            .is_empty()
        {
            return Err(OverlayError::Missing);
        }
        Ok(())
    }
    /// Stores one bounded scratch record under an existing operation owner.
    pub fn put_scratch(
        &self,
        route: Route,
        operation: u64,
        record: &ScratchRecord,
    ) -> OverlayResult<()> {
        if record.value.len() > SCRATCH_BYTES {
            return Err(OverlayError::Invalid("scratch window"));
        }
        self.atomic(|| {
            self.operation(route, operation)?;
            self.execute(
                StatementKind::Scratch,
                "INSERT INTO scratch VALUES(?1,?2,?3,?4,?5)
                 ON CONFLICT(ns,operation,kind,key) DO UPDATE SET value=excluded.value",
                &[
                    &route.ns,
                    &integer(operation)?,
                    &i64::from(record.kind),
                    &integer(record.key)?,
                    &record.value,
                ],
                32 + record.value.len() as u64,
            )?;
            Ok(())
        })
    }
    /// Fixed keyset window; caller continues after its last returned key.
    pub fn scratch_page(
        &self,
        route: Route,
        operation: u64,
        kind: u32,
        after: Option<u64>,
    ) -> OverlayResult<Vec<ScratchRecord>> {
        self.operation(route, operation)?;
        self.query(
            StatementKind::Scratch,
            sql::SCRATCH_PAGE,
            &[
                &route.ns,
                &integer(operation)?,
                &i64::from(kind),
                &after.map(integer).transpose()?.unwrap_or(-1),
            ],
            32,
            |r| {
                Ok(ScratchRecord {
                    kind: r.get(0)?,
                    key: unsigned(r, 1)?,
                    value: r.get(2)?,
                })
            },
        )
    }
}

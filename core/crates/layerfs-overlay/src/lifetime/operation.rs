//! Engine-minted operation ownership and independent bounded scratch custody.
use crate::{
    db::{integer, unsigned},
    OperationOwner, Overlay, OverlayError, OverlayResult, Route, ScratchRecord, StatementKind,
    SCRATCH_BYTES,
};
impl Overlay {
    /// One acquisition attempt. Query retained custody by request after a lost reply.
    pub fn acquire_operation(&self, route: Route, request: u64) -> OverlayResult<OperationOwner> {
        let request = integer(request)?;
        if request == 0 {
            return Err(OverlayError::Invalid("zero operation request"));
        }
        self.atomic(|| {
            self.live(route)?;
            let owner = self.mint_owner(route)?;
            self.execute(
                StatementKind::Lease,
                "INSERT INTO operation_owner VALUES(?1,?2,?3)",
                &[&route.ns, &request, &integer(owner)?],
                24,
            )?;
            self.execute(
                StatementKind::Lease,
                "INSERT INTO lease VALUES(?1,8,?2,0)",
                &[&route.ns, &integer(owner)?],
                16,
            )?;
            Ok(OperationOwner { route, owner })
        })
    }
    pub fn retained_operation(
        &self,
        route: Route,
        request: u64,
    ) -> OverlayResult<Option<OperationOwner>> {
        self.state(route)?;
        self.query(
            StatementKind::Lease,
            "SELECT owner FROM operation_owner WHERE ns=?1 AND request=?2",
            &[&route.ns, &integer(request)?],
            16,
            |r| {
                Ok(OperationOwner {
                    route,
                    owner: unsigned(r, 0)?,
                })
            },
        )
        .map(|mut rows| rows.pop())
    }
    pub(crate) fn check_operation(&self, owner: OperationOwner) -> OverlayResult<()> {
        self.state(owner.route)?;
        if self
            .query(
                StatementKind::Lease,
                crate::sql::OPERATION_CUSTODY,
                &[&owner.route.ns, &integer(owner.owner)?],
                16,
                |_| Ok(()),
            )?
            .is_empty()
        {
            return Err(OverlayError::Stale);
        }
        Ok(())
    }
    /// Existing processing custody remains readable after logical close; new
    /// scratch mutation refuses. No process exit or Drop implies release.
    pub fn put_owned_scratch(
        &self,
        owner: OperationOwner,
        record: &ScratchRecord,
    ) -> OverlayResult<()> {
        if record.value.len() > SCRATCH_BYTES {
            return Err(OverlayError::Invalid("scratch window"));
        }
        self.atomic(|| {
            self.live(owner.route)?;
            self.check_operation(owner)?;
            self.execute(
                StatementKind::Scratch,
                "INSERT INTO owned_scratch VALUES(?1,?2,?3,?4,?5)
                ON CONFLICT(ns,operation,kind,key) DO UPDATE SET value=excluded.value",
                &[
                    &owner.route.ns,
                    &integer(owner.owner)?,
                    &i64::from(record.kind),
                    &integer(record.key)?,
                    &record.value,
                ],
                32 + record.value.len() as u64,
            )?;
            Ok(())
        })
    }
    pub fn owned_scratch_page(
        &self,
        owner: OperationOwner,
        kind: u32,
        after: Option<u64>,
    ) -> OverlayResult<Vec<ScratchRecord>> {
        self.check_operation(owner)?;
        self.query(
            StatementKind::Scratch,
            "SELECT kind,key,value FROM owned_scratch
            WHERE ns=?1 AND operation=?2 AND kind=?3 AND key>?4 ORDER BY key LIMIT 64",
            &[
                &owner.route.ns,
                &integer(owner.owner)?,
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
    /// Exact last-consumer fence. Cleanup owns scratch independently afterwards.
    pub fn release_operation(&self, owner: OperationOwner) -> OverlayResult<()> {
        self.atomic(|| {
            self.check_operation(owner)?;
            self.execute(
                StatementKind::Lease,
                "DELETE FROM operation_owner WHERE ns=?1 AND owner=?2",
                &[&owner.route.ns, &integer(owner.owner)?],
                16,
            )?;
            self.execute(
                StatementKind::Lease,
                "DELETE FROM lease WHERE ns=?1 AND kind=8 AND owner=?2 AND resource=0",
                &[&owner.route.ns, &integer(owner.owner)?],
                16,
            )?;
            self.enqueue(
                owner.route.ns,
                crate::maintenance::SCRATCH,
                integer(owner.owner)?,
                1,
            )?;
            self.queue_closed(owner.route)
        })
    }
}

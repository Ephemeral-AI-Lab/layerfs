//! Engine-minted operation ownership and independent bounded operation_record custody.
use crate::{db::integer, OperationOwner, Overlay, OverlayError, OverlayResult};
impl Overlay {
    /// One acquisition attempt. Query retained custody by request after a lost reply.
    pub fn acquire_operation(&self, request: u64) -> OverlayResult<OperationOwner> {
        let request = integer(request)?;
        if request == 0 {
            return Err(OverlayError::Invalid("zero operation request"));
        }
        self.atomic(|| {
            self.live()?;
            let owner = self.mint_owner()?;
            let value = OperationOwner {
                engine: self.identity,
                owner,
            };
            self.open.borrow_mut().operations.insert(
                owner,
                request,
                value,
                "duplicate operation request",
            )?;
            Ok(value)
        })
    }
    pub fn retained_operation(&self, request: u64) -> OverlayResult<Option<OperationOwner>> {
        self.state()?;
        Ok(self.open.borrow().operations.retained(integer(request)?))
    }
    pub(crate) fn check_operation(&self, owner: OperationOwner) -> OverlayResult<()> {
        self.state()?;
        self.operation_custody(owner)
    }
    /// The owner half of `check_operation`.
    fn operation_custody(&self, owner: OperationOwner) -> OverlayResult<()> {
        if self
            .open
            .borrow()
            .operations
            .get(owner.owner)
            .map(|(held, _)| held)
            != Some(owner)
        {
            return Err(OverlayError::Stale);
        }
        Ok(())
    }
    /// The whole fence of one indexed operation_record job: current runtime
    /// Workspace state and the exact journaled operation owner.
    /// Its refusals and their order are those of `live`/`check_operation`: a
    /// stale route first, then a closed Workspace when the job writes, then a
    /// released owner. Existing custody stays readable after logical close.
    pub(crate) fn fence_operation(&self, owner: OperationOwner, write: bool) -> OverlayResult<()> {
        if write && self.state()?.closed {
            return Err(OverlayError::Closed);
        }
        self.check_operation(owner)
    }
    /// Exact last-consumer fence. Cleanup owns operation_record independently afterwards.
    pub fn release_operation(&self, owner: OperationOwner) -> OverlayResult<()> {
        self.atomic_cleanup(|| {
            self.check_operation(owner)?;
            self.open
                .borrow_mut()
                .operations
                .remove(owner.owner, |held| *held == owner)?;
            self.enqueue(
                crate::maintenance::OPERATION_RECORD,
                integer(owner.owner)?,
                2,
            )?;
            Ok(())
        })
    }
}

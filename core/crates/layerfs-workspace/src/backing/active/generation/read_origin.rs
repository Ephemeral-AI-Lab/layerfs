//! Byte-proven Base sharing used only by the ordinary WRITE publisher.
use super::*;

impl ActiveBacking {
    pub(crate) fn publish_read_origin(
        &self,
        inode: u64,
        original: HotInode,
        offset: u64,
        length: u64,
        source: u64,
    ) -> Result<ActivePublication, WorkspaceError> {
        if length == 0 || original.base == [0; 32] {
            return Err(WorkspaceError::InvalidInput);
        }
        let mut state = self.state.lock().map_err(|_| WorkspaceError::Io)?;
        if state.stopped || state.closed {
            return Err(WorkspaceError::Busy);
        }
        let _working = self.store.budget().reserve(256 * 1024)?;
        self.index.maintain()?;
        let (generation, prior) = self.index.generation_revision()?;
        let revision = prior.checked_add(1).ok_or(WorkspaceError::Capacity)?;
        let plan =
            ExtentPlan::read_origin(&self.index, inode, original.length, offset, length, source)?;
        let extra = self.inode_updates(inode, original, generation, revision, plan.length)?;
        self.publish_no_pack(&mut state, plan, extra, None, None)
    }
}

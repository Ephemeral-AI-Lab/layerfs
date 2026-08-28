use crate::{BranchId, FetchBranchReceipt, LayerFs, Result, ResumeToken};
use layerfs_sync::DurableControlEndpoint;

impl LayerFs {
    pub fn fetch_branch(
        &self,
        source: &impl DurableControlEndpoint,
        request_id: [u8; 32],
        branch: BranchId,
        resume: ResumeToken,
    ) -> Result<FetchBranchReceipt> {
        Ok(layerfs_sync::fetch_branch(
            source,
            &self.working,
            request_id,
            branch,
            resume,
        )?)
    }
}

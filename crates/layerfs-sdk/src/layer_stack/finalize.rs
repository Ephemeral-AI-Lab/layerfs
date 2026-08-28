use crate::{
    BranchId, LayerCandidate, LayerFs, LayerId, LayerStackHead, LayerStackId,
    LayerStackMergeOutcome, LayerStackRollbackOutcome, PushLayerStackGenesisReceipt, Result,
    ResumeToken,
};
use layerfs_core::ObjectId;
use layerfs_sync::DurableControlEndpoint;

impl LayerFs {
    pub fn create_layer_stack(
        &self,
        id: LayerStackId,
        genesis: LayerId,
        name: &str,
        root: ObjectId,
    ) -> Result<LayerStackHead> {
        Ok(self.working.create_layer_stack(id, genesis, name, root)?)
    }

    pub fn layer_stack_head(&self, id: LayerStackId) -> Result<Option<LayerStackHead>> {
        Ok(self.working.layer_stack_head(id)?)
    }

    pub fn push_layer_stack_genesis(
        &self,
        destination: &impl DurableControlEndpoint,
        request_id: [u8; 32],
        branch: BranchId,
        stack: LayerStackHead,
        name: &str,
        resume: ResumeToken,
    ) -> Result<PushLayerStackGenesisReceipt> {
        Ok(layerfs_sync::push_layer_stack_genesis(
            &self.working,
            destination,
            request_id,
            branch,
            stack,
            name,
            resume,
        )?)
    }

    pub fn push_layer_stack_merge(
        &self,
        destination: &impl DurableControlEndpoint,
        candidate: LayerCandidate,
        expected: LayerStackHead,
    ) -> Result<LayerStackMergeOutcome> {
        Ok(layerfs_sync::push_layer_stack_merge(
            destination,
            candidate,
            expected,
        )?)
    }

    pub fn push_layer_stack_rollback(
        &self,
        destination: &impl DurableControlEndpoint,
        expected: LayerStackHead,
        target: LayerId,
    ) -> Result<LayerStackRollbackOutcome> {
        Ok(layerfs_sync::push_layer_stack_rollback(
            destination,
            expected,
            target,
        )?)
    }
}

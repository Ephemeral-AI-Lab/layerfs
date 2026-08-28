use crate::{
    BranchHead, LayerCandidate, LayerFs, LayerId, LayerPreparationResult, LayerStackHead, Result,
};

impl LayerFs {
    pub fn prepare_layer_stack_merge(
        &self,
        source: BranchHead,
        expected_stack: LayerStackHead,
    ) -> Result<LayerPreparationResult> {
        Ok(self
            .working
            .prepare_layer_stack_merge(source, expected_stack)?)
    }

    pub fn recoverable_layer_candidates_after(
        &self,
        after: Option<LayerId>,
        limit: usize,
    ) -> Result<Vec<LayerCandidate>> {
        Ok(self
            .working
            .recoverable_layer_candidates_after(after, limit)?)
    }

    pub fn drop_layer_candidate(&self, layer_id: LayerId) -> Result<bool> {
        Ok(self.working.drop_layer_candidate(layer_id)?)
    }
}

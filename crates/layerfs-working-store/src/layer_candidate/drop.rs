use crate::{LayerCandidate, LayerId, Result, WorkingStore};

impl WorkingStore {
    pub fn recoverable_layer_candidates_after(
        &self,
        after: Option<LayerId>,
        limit: usize,
    ) -> Result<Vec<LayerCandidate>> {
        Ok(self.storage.product_layer_candidates_after(after, limit)?)
    }

    pub fn drop_layer_candidate(&self, layer_id: LayerId) -> Result<bool> {
        Ok(self.storage.product_drop_layer_candidate(layer_id)?)
    }
}

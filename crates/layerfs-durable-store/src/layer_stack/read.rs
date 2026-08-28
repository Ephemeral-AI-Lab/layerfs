use crate::{DurableStore, LayerStackHead, LayerStackId, Result};

impl DurableStore {
    pub fn layer_stack_head(&self, id: LayerStackId) -> Result<Option<LayerStackHead>> {
        Ok(self.storage.layer_stack_head(id)?)
    }
}

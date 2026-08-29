use crate::StackStore;
use layerfs_storage::{
    CreatedStack, LayerId, Result, StackHistoryId, StackHistoryRecord, StackId, StackRecord,
    StorageError,
};

impl StackStore {
    pub fn create_stack(&self, layer_id: LayerId) -> Result<CreatedStack> {
        let _operation = self.db.enter_operation()?;
        let layer = self
            .db
            .layer(layer_id)?
            .ok_or(StorageError::NotFound("Layer"))?;
        if !self.db.has_object(layer.root_id)? {
            return Err(StorageError::MissingBaseData);
        }
        let history_id = StackHistoryId::new(&self.writer.public_key());
        let stack = StackRecord {
            id: StackId::derive(history_id, None, layer.root_id),
            history_id,
            parent_id: None,
            root_id: layer.root_id,
        };
        let history = StackHistoryRecord {
            id: history_id,
            base_layer_id: layer_id,
            head_stack_id: stack.id,
        };
        self.db.create_stack_history_record(history, stack)?;
        Ok((history, stack))
    }
}

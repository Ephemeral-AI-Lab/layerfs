use crate::StackStore;
use layerfs_storage_core::{
    LayerHistoryId, LayerId, Result, StackHistoryId, StackHistoryRecord, StackId, StackRecord,
    StorageError,
};

impl StackStore {
    pub fn create_stack_history_from_layer(
        &self,
        layer_history_id: LayerHistoryId,
        layer_id: LayerId,
    ) -> Result<(StackHistoryRecord, StackRecord)> {
        let _operation = self.db.enter_operation()?;
        let layer = self
            .db
            .layer(layer_id)?
            .ok_or(StorageError::NotFound("Layer"))?;
        if layer.history_id != layer_history_id {
            return Err(StorageError::WrongLayerHistory(
                layerfs_storage_core::WrongHistory {
                    expected: layer_history_id,
                    actual: layer.history_id,
                },
            ));
        }
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

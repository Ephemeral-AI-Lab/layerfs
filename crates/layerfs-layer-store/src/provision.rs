use crate::LayerStore;
use layerfs_storage_core::{
    empty_root, LayerHistoryId, LayerHistoryRecord, LayerId, LayerRecord, Result, StorageId,
};

impl LayerStore {
    pub fn provision(&self) -> Result<(LayerHistoryRecord, LayerRecord)> {
        let _operation = self.db.enter_operation()?;
        let history_id = LayerHistoryId::new();
        let root = empty_root(*blake3::hash(history_id.as_slice()).as_bytes())?;
        let layer = LayerRecord {
            id: LayerId::derive(history_id, None, root.root_id),
            history_id,
            parent_id: None,
            root_id: root.root_id,
        };
        let history = LayerHistoryRecord {
            id: history_id,
            head_layer_id: layer.id,
        };
        self.db
            .provision_layer_history(history, layer, &root.objects)?;
        Ok((history, layer))
    }
}

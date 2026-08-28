use crate::{LayerFs, Result, ResumeToken};
use layerfs_core::ObjectId;
use layerfs_sync::{DurableEndpoint, TransferReceipt};

impl LayerFs {
    pub fn push_objects(
        &self,
        destination: &impl DurableEndpoint,
        request_id: [u8; 32],
        ids: impl IntoIterator<Item = ObjectId>,
        resume: ResumeToken,
    ) -> Result<TransferReceipt> {
        Ok(layerfs_sync::push_objects(
            &self.working,
            destination,
            request_id,
            ids,
            resume,
        )?)
    }
}

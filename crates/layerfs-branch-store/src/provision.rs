use layerfs_storage::{
    BuiltRoot, DeferredObjectStore, LocalAdmissionReceipt, LocalObjectReceipt, Result, StoreDb,
};

use crate::BranchStore;

pub(crate) fn admit_built(db: &StoreDb, built: &BuiltRoot) -> Result<()> {
    admit_deferred(db, &built.objects)
}

pub(crate) fn admit_deferred(db: &StoreDb, objects: &DeferredObjectStore) -> Result<()> {
    let mut receipt = LocalAdmissionReceipt {
        objects: LocalObjectReceipt {
            candidate_ids: objects.len(),
            candidate_bytes: objects.encoded_bytes(),
            ..LocalObjectReceipt::default()
        },
    };
    objects.visit_batches(&mut |objects, _last| {
        let admitted = db.admit_objects(objects)?;
        receipt.objects.inserted_ids += admitted.inserted_ids;
        receipt.objects.inserted_bytes += admitted.inserted_bytes;
        receipt.objects.reused_ids += admitted.raced_existing_ids;
        receipt.objects.reused_bytes += admitted.raced_existing_bytes;
        Ok(())
    })?;
    layerfs_storage::record_local_admission(receipt)
}

impl BranchStore {
    pub(crate) fn verify_local_closure(&self, root: layerfs_content::ObjectId) -> Result<()> {
        self.db.verify_complete_roots([root])
    }
}

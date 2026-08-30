use layerfs_storage::{BuiltRoot, LocalAdmissionReceipt, LocalObjectReceipt, Result, StoreDb};

pub(crate) fn admit_built(db: &StoreDb, built: &BuiltRoot) -> Result<()> {
    let mut receipt = LocalAdmissionReceipt {
        objects: LocalObjectReceipt {
            candidate_ids: built.objects.len(),
            candidate_bytes: built.objects.encoded_bytes(),
            ..LocalObjectReceipt::default()
        },
        cdc_bytes_scanned: built.counters.cdc_bytes_scanned,
        encode_hash_invocations: built.counters.encode_hash_invocations,
        source_reused_ids: 0,
        source_reused_bytes: 0,
    };
    built.objects.visit_batches(&mut |objects, _last| {
        let admitted = db.admit_objects(objects)?;
        receipt.objects.inserted_ids += admitted.inserted_ids;
        receipt.objects.inserted_bytes += admitted.inserted_bytes;
        receipt.objects.reused_ids += admitted.raced_existing_ids;
        receipt.objects.reused_bytes += admitted.raced_existing_bytes;
        Ok(())
    })?;
    layerfs_storage::record_local_admission(receipt)
}

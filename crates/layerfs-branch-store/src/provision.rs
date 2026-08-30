use layerfs_storage::{
    BuiltRoot, CanonicalObject, DeferredObjectStore, LayerStackEndpoint, LocalAdmissionReceipt,
    LocalObjectReceipt, Result, StorageError, StoreDb,
};
use std::collections::BTreeMap;

use crate::BranchStore;

pub(crate) fn admit_built(
    db: &StoreDb,
    parent: Option<&dyn LayerStackEndpoint>,
    built: &BuiltRoot,
) -> Result<()> {
    admit_deferred_with_counters(
        db,
        parent,
        &built.objects,
        built.counters.cdc_bytes_scanned,
        built.counters.encode_hash_invocations,
    )
}

pub(crate) fn admit_deferred(db: &StoreDb, objects: &DeferredObjectStore) -> Result<()> {
    admit_deferred_with_counters(db, None, objects, 0, 0)
}

fn admit_deferred_with_counters(
    db: &StoreDb,
    parent: Option<&dyn LayerStackEndpoint>,
    objects: &DeferredObjectStore,
    cdc_bytes_scanned: u64,
    encode_hash_invocations: u64,
) -> Result<()> {
    let started = std::time::Instant::now();
    let mut receipt = LocalAdmissionReceipt {
        objects: LocalObjectReceipt {
            candidate_ids: objects.len(),
            candidate_bytes: objects.encoded_bytes(),
            ..LocalObjectReceipt::default()
        },
        cdc_bytes_scanned,
        encode_hash_invocations,
        source_reused_ids: 0,
        source_reused_bytes: 0,
    };
    let mut pending = Vec::with_capacity(layerfs_storage::OBJECT_BATCH_COUNT);
    let mut pending_bytes = 0_usize;
    objects.visit_batches(&mut |objects, _last| {
        let mut ids = objects.iter().map(|object| object.id).collect::<Vec<_>>();
        ids.sort();
        if ids.windows(2).any(|pair| pair[0] == pair[1]) {
            return Err(StorageError::Integrity("candidate object duplicate"));
        }
        let local = membership(db.object_membership(&ids)?, &ids)?;
        let parent_missing = ids
            .iter()
            .filter(|id| !local.contains_key(id))
            .copied()
            .collect::<Vec<_>>();
        let source = if let (Some(parent), false) = (parent, parent_missing.is_empty()) {
            membership(parent.object_membership(&parent_missing)?, &parent_missing)?
        } else {
            BTreeMap::new()
        };
        for object in objects {
            if let Some(length) = local.get(&object.id) {
                if *length != object.bytes.len() as u64 {
                    return Err(StorageError::Integrity("local object length"));
                }
                receipt.objects.reused_ids += 1;
                receipt.objects.reused_bytes += *length;
                continue;
            }
            if let Some(length) = source.get(&object.id) {
                if *length != object.bytes.len() as u64 {
                    return Err(StorageError::Integrity("source object length"));
                }
                receipt.objects.reused_ids += 1;
                receipt.objects.reused_bytes += *length;
                receipt.source_reused_ids += 1;
                receipt.source_reused_bytes += *length;
                continue;
            }
            if !pending.is_empty()
                && (pending.len() == layerfs_storage::OBJECT_BATCH_COUNT
                    || pending_bytes + object.bytes.len() > layerfs_storage::OBJECT_BATCH_BYTES)
            {
                admit_pending(db, &mut pending, &mut pending_bytes, &mut receipt)?;
            }
            pending_bytes += object.bytes.len();
            pending.push(object.clone());
            if pending.len() == layerfs_storage::OBJECT_BATCH_COUNT
                || pending_bytes >= layerfs_storage::OBJECT_BATCH_BYTES
            {
                admit_pending(db, &mut pending, &mut pending_bytes, &mut receipt)?;
            }
        }
        Ok(())
    })?;
    admit_pending(db, &mut pending, &mut pending_bytes, &mut receipt)?;
    let recorded = layerfs_storage::record_local_admission(receipt);
    layerfs_storage::note_workspace_commit_phase(
        layerfs_storage::WorkspaceCommitPhase::LocalAdmission,
        started.elapsed().as_nanos().min(u128::from(u64::MAX)) as u64,
    );
    recorded
}

fn membership(
    membership: (layerfs_storage::MissingBitmap, Vec<Option<u64>>),
    ids: &[layerfs_content::ObjectId],
) -> Result<BTreeMap<layerfs_content::ObjectId, u64>> {
    let (missing, lengths) = membership;
    missing.validate_tail(ids.len())?;
    if lengths.len() != ids.len() {
        return Err(StorageError::Integrity("object membership lengths"));
    }
    let mut known = BTreeMap::new();
    for (index, (id, length)) in ids.iter().zip(lengths).enumerate() {
        if missing.is_missing(index)? != length.is_none() {
            return Err(StorageError::Integrity("object membership length"));
        }
        if let Some(length) = length {
            known.insert(*id, length);
        }
    }
    Ok(known)
}

fn admit_pending(
    db: &StoreDb,
    pending: &mut Vec<CanonicalObject>,
    pending_bytes: &mut usize,
    receipt: &mut LocalAdmissionReceipt,
) -> Result<()> {
    if pending.is_empty() {
        return Ok(());
    }
    let admitted = db.admit_objects(pending)?;
    receipt.objects.inserted_ids += admitted.inserted_ids;
    receipt.objects.inserted_bytes += admitted.inserted_bytes;
    receipt.objects.reused_ids += admitted.raced_existing_ids;
    receipt.objects.reused_bytes += admitted.raced_existing_bytes;
    pending.clear();
    *pending_bytes = 0;
    Ok(())
}

impl BranchStore {
    pub(crate) fn verify_local_closure(&self, root: layerfs_content::ObjectId) -> Result<()> {
        self.db.verify_complete_roots([root])
    }
}

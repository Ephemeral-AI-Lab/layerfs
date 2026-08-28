use crate::{
    derive_id, DurableError, DurableStore, LayerCandidate, LayerId, LayerStackHead, LayerStackId,
    LayerStackMergeOutcome, LayerStackRollbackOutcome, RequestId, Result,
};
use layerfs_core::content::rope::ObjectStore;
use layerfs_storage::{EngineError, FullStorage};

impl DurableStore {
    pub fn bootstrap_layer_stack(
        &self,
        layer_stack_id: LayerStackId,
        layer_id: LayerId,
        name: &str,
        root: layerfs_core::ObjectId,
    ) -> Result<LayerStackHead> {
        Ok(self
            .storage
            .bootstrap_layer_stack(layer_stack_id, layer_id, name, root)?)
    }

    pub fn accept_layer_stack_merge(
        &self,
        candidate: LayerCandidate,
        expected: LayerStackHead,
    ) -> Result<LayerStackMergeOutcome> {
        let ancestry = self
            .storage
            .authoritative_branch_ancestry(candidate.source.branch_id)?
            .ok_or(DurableError::Storage(EngineError::InvalidRecord(
                "Layer candidate source Branch",
            )))?;
        if ancestry.origin_layer_stack_id != expected.layer_stack_id
            || candidate.layer_stack_id != expected.layer_stack_id
            || candidate.parent_layer_id != expected.layer_id
            || candidate.layer_id
                != LayerId::from_bytes(derive_id(
                    b"candidate-layer",
                    &[
                        expected.layer_stack_id.as_bytes(),
                        candidate.request_id.as_bytes(),
                        candidate.root.as_bytes(),
                    ],
                ))
        {
            return Err(DurableError::Storage(EngineError::InvalidRecord(
                "Layer candidate identity",
            )));
        }
        let origin = self
            .storage
            .layer_root(ancestry.origin_layer_stack_id, ancestry.origin_layer_id)?
            .ok_or(DurableError::Storage(EngineError::InvalidRecord(
                "Layer candidate origin",
            )))?;
        verify_full_merge_root(
            &self.storage,
            origin,
            candidate.source.root,
            expected.root,
            candidate.root,
        )?;
        self.storage.import_layer_candidate(candidate, expected)?;
        let request_id = RequestId::from_bytes(derive_id(
            b"durable-layer-stack-merge",
            &[
                self.storage_id().as_slice(),
                candidate.request_id.as_bytes(),
                candidate.layer_id.as_bytes(),
                expected.layer_id.as_bytes(),
            ],
        ));
        Ok(self
            .storage
            .finalize_layer_stack_merge(candidate, expected, request_id)?)
    }

    pub fn layer_stack_rollback(
        &self,
        expected: LayerStackHead,
        target: LayerId,
    ) -> Result<LayerStackRollbackOutcome> {
        let request_id = RequestId::from_bytes(derive_id(
            b"durable-layer-stack-rollback",
            &[
                self.storage_id().as_slice(),
                expected.layer_stack_id.as_bytes(),
                &expected.generation.to_be_bytes(),
                target.as_bytes(),
            ],
        ));
        Ok(self
            .storage
            .rollback_layer_stack(expected, target, request_id)?)
    }
}

pub(crate) fn verify_full_merge_root(
    storage: &FullStorage,
    base: layerfs_core::ObjectId,
    source: layerfs_core::ObjectId,
    destination: layerfs_core::ObjectId,
    claimed: layerfs_core::ObjectId,
) -> Result<()> {
    let mut store = FullVerificationStore(storage);
    let recomputed = layerfs_core::logical::merge_roots(&mut store, base, source, destination)?
        .map_err(|_| DurableError::Storage(EngineError::InvalidRecord("Durable merge conflict")))?;
    if recomputed.root() == claimed {
        Ok(())
    } else {
        Err(DurableError::Storage(EngineError::InvalidRecord(
            "Durable merge result",
        )))
    }
}

struct FullVerificationStore<'a>(&'a FullStorage);

impl ObjectStore for FullVerificationStore<'_> {
    fn get(
        &self,
        id: layerfs_core::ObjectId,
    ) -> std::result::Result<Vec<u8>, layerfs_core::CoreError> {
        self.0
            .load_canonical_authenticated_bounded(id, usize::MAX)
            .map_err(core_error)
    }

    fn put(
        &mut self,
        canonical: &[u8],
    ) -> std::result::Result<layerfs_core::ObjectId, layerfs_core::CoreError> {
        let id = layerfs_core::ObjectId::for_bytes(canonical);
        let stored = self
            .0
            .load_canonical_authenticated_bounded(id, canonical.len())
            .map_err(core_error)?;
        if stored == canonical {
            Ok(id)
        } else {
            Err(layerfs_core::CoreError::IdentityMismatch)
        }
    }
}

fn core_error(error: EngineError) -> layerfs_core::CoreError {
    match error {
        EngineError::Core(error) => error,
        EngineError::MissingObject(_) => layerfs_core::CoreError::MissingObject,
        EngineError::IdentityMismatch { .. }
        | EngineError::MalformedObject { .. }
        | EngineError::ImmutableConflict(..) => layerfs_core::CoreError::IdentityMismatch,
        EngineError::SchemaMismatch => layerfs_core::CoreError::SchemaMismatch,
        EngineError::ProfileMismatch | EngineError::SqliteProfileMismatch(_) => {
            layerfs_core::CoreError::ProfileMismatch
        }
        EngineError::InvalidRecord(record) => layerfs_core::CoreError::InvalidRecord(record),
        EngineError::PublicationConflict => layerfs_core::CoreError::PublicationConflict,
        EngineError::AmbiguousDurability => layerfs_core::CoreError::AmbiguousDurability,
        _ => layerfs_core::CoreError::Io,
    }
}

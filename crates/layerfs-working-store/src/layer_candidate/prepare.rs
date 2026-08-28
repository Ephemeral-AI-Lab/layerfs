use crate::operation::operation_entropy;
use crate::{BranchHead, LayerCandidate, LayerStackHead, Result, WorkingError, WorkingStore};
use layerfs_storage::{derive_id, LayerCandidateRequest, RequestId};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LayerPreparationResult {
    Prepared(LayerCandidate),
    ContentConflict(layerfs_core::logical::MergeConflict),
}

impl WorkingStore {
    pub fn prepare_layer_stack_merge(
        &self,
        source: BranchHead,
        expected_stack: LayerStackHead,
    ) -> Result<LayerPreparationResult> {
        let ancestry = self
            .storage
            .product_branch_ancestry(source.branch_id)?
            .ok_or(WorkingError::InvalidReceipt)?;
        if ancestry.origin_layer_stack_id != expected_stack.layer_stack_id
            || self.storage.product_branch_head(source.branch_id)? != Some(source)
            || self
                .storage
                .product_layer_stack_head(expected_stack.layer_stack_id)?
                != Some(expected_stack)
        {
            return Err(WorkingError::InvalidReceipt);
        }
        let origin_root = self
            .storage
            .product_layer_root(ancestry.origin_layer_stack_id, ancestry.origin_layer_id)?
            .ok_or(WorkingError::InvalidReceipt)?;
        let mut writer = self.storage.begin_candidate_write()?;
        let merged = match layerfs_core::logical::merge_roots(
            &mut writer,
            origin_root,
            source.root,
            expected_stack.root,
        )? {
            Ok(candidate) => candidate,
            Err(conflict) => return Ok(LayerPreparationResult::ContentConflict(conflict)),
        };
        writer.commit_candidate(merged.root())?;
        let entropy = operation_entropy(self.storage_id())?;
        let request_id = RequestId::from_bytes(derive_id(
            b"working-layer-stack-candidate",
            &[
                source.branch_id.as_bytes(),
                expected_stack.layer_stack_id.as_bytes(),
                &expected_stack.generation.to_be_bytes(),
                merged.root().as_bytes(),
                &entropy,
            ],
        ));
        Ok(LayerPreparationResult::Prepared(
            self.storage
                .product_prepare_layer_candidate(LayerCandidateRequest {
                    source,
                    expected_stack,
                    result_root: merged.root(),
                    source_transition: Vec::new(),
                    applied_transition: Vec::new(),
                    request_id,
                })?,
        ))
    }
}

use crate::{BranchHead, ChildMergePublication, Result, WorkingError, WorkingStore};
use layerfs_storage::{derive_id, ChildMergeCandidate, ChildMergeOutcome, RequestId};

#[derive(Clone, Debug, Eq, PartialEq)]
#[allow(clippy::large_enum_variant)]
pub enum ChildMergeResult {
    WorkingRecorded {
        parent_head: BranchHead,
        publication: ChildMergePublication,
        reconciled: bool,
    },
    ContentConflict(layerfs_core::logical::MergeConflict),
    DestinationConflict {
        actual_parent: BranchHead,
    },
}

impl WorkingStore {
    pub fn child_branch_merge(
        &self,
        source: BranchHead,
        expected_parent: BranchHead,
    ) -> Result<ChildMergeResult> {
        let ancestry = self
            .storage
            .product_branch_ancestry(source.branch_id)?
            .ok_or(WorkingError::InvalidReceipt)?;
        if ancestry.immediate_parent_branch_id != Some(expected_parent.branch_id)
            || self.storage.product_branch_head(source.branch_id)? != Some(source)
        {
            return Err(WorkingError::InvalidReceipt);
        }
        let mut writer = self.storage.begin_candidate_write()?;
        let merged = match layerfs_core::logical::merge_roots(
            &mut writer,
            ancestry.fork_root,
            source.root,
            expected_parent.root,
        )? {
            Ok(candidate) => candidate,
            Err(conflict) => return Ok(ChildMergeResult::ContentConflict(conflict)),
        };
        writer.commit_candidate(merged.root())?;
        let source_version = source
            .operation_version_id
            .ok_or(WorkingError::InvalidReceipt)?;
        let request_id = RequestId::from_bytes(derive_id(
            b"working-child-branch-merge",
            &[
                source.branch_id.as_bytes(),
                source_version.as_bytes(),
                expected_parent.branch_id.as_bytes(),
                &expected_parent.generation.to_be_bytes(),
                merged.root().as_bytes(),
            ],
        ));
        let candidate = ChildMergeCandidate {
            source,
            expected_parent,
            result_root: merged.root(),
            source_transition: Vec::new(),
            applied_transition: Vec::new(),
            request_id,
        };
        Ok(
            match self.storage.product_child_branch_merge(candidate.clone())? {
                ChildMergeOutcome::WorkingRecorded {
                    parent_head,
                    reconciled,
                } => ChildMergeResult::WorkingRecorded {
                    parent_head,
                    publication: ChildMergePublication {
                        candidate,
                        accepted_parent: parent_head,
                    },
                    reconciled,
                },
                ChildMergeOutcome::Conflict { actual_parent } => {
                    ChildMergeResult::DestinationConflict { actual_parent }
                }
            },
        )
    }
}

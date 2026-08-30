use crate::store::PushPlan;
use crate::BranchStore;
use layerfs_content::filesystem::ContentChange;
use layerfs_storage::{
    apply_changes, apply_reconcile_choices, BuiltRoot, CommitId, CommitRecord, LayerId,
    LayerStackEndpoint, ReconcileChoice, ReconcileConflict, Result, StorageError,
};
use std::sync::Arc;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CommitOutcome {
    Created {
        previous_head: Option<CommitId>,
        commit_id: CommitId,
    },
    UpToDate {
        head: Option<CommitId>,
    },
}

pub struct PreparedReconciliation {
    pub branch_id: layerfs_storage::BranchId,
    pub expected_head: CommitId,
    pub old_base_layer_id: LayerId,
    pub current_layer_id: LayerId,
    pub old_base_root: layerfs_content::ObjectId,
    pub branch_root: layerfs_content::ObjectId,
    pub layer_root: layerfs_content::ObjectId,
    pub root_id: layerfs_content::ObjectId,
    pub conflicts: Vec<ReconcileConflict>,
}

impl BranchStore {
    pub fn prepare_reconciliation(
        &self,
        parent: Arc<dyn LayerStackEndpoint>,
        branch_id: layerfs_storage::BranchId,
        current_layer_id: LayerId,
    ) -> Result<PreparedReconciliation> {
        let branch = self.require_local_branch(branch_id)?;
        let expected_head = branch
            .head_commit_id
            .ok_or(StorageError::InvalidInput("Branch without Commit"))?;
        let commit = self
            .db
            .commit(expected_head)?
            .ok_or(StorageError::Integrity("Branch head Commit"))?;
        let old_base = self
            .db
            .layer(branch.base_layer_id)?
            .ok_or(StorageError::Integrity("old base Layer"))?;
        let current = self
            .db
            .layer(current_layer_id)?
            .ok_or(StorageError::NotFound("pulled current Layer"))?;
        if old_base.layer_stack_id != branch.layer_stack_id
            || current.layer_stack_id != branch.layer_stack_id
        {
            return Err(StorageError::InvalidInput("LayerStack mismatch"));
        }
        let branch_local = self.db.complete_root(commit.root_id)?;
        let layer_local = self.layer_requires_local(current.id, current.root_id)?;
        let reader = self.pair_reader_with_policy(
            parent,
            commit.root_id,
            branch_local,
            current.root_id,
            layer_local,
        )?;
        let reconciled = layerfs_storage::reconcile_candidate(
            &reader,
            old_base.root_id,
            commit.root_id,
            current.root_id,
        )?;
        crate::provision::admit_deferred(&self.db, &reconciled.objects)?;
        Ok(PreparedReconciliation {
            branch_id,
            expected_head,
            old_base_layer_id: old_base.id,
            current_layer_id,
            old_base_root: old_base.root_id,
            branch_root: commit.root_id,
            layer_root: current.root_id,
            root_id: reconciled.root_id,
            conflicts: reconciled.conflicts,
        })
    }

    pub fn commit_reconciliation(
        &self,
        parent: Arc<dyn LayerStackEndpoint>,
        prepared: &PreparedReconciliation,
        working: BuiltRoot,
        choices: &[ReconcileChoice],
    ) -> Result<CommitOutcome> {
        crate::provision::admit_built(&self.db, None, &working)?;
        let working_local = self.local_closure_complete(working.root_id)?;
        let branch_local = self.db.complete_root(prepared.branch_root)?;
        let layer_local =
            self.layer_requires_local(prepared.current_layer_id, prepared.layer_root)?;
        let reader = self.roots_reader_with_policy(
            parent.clone(),
            &[
                (working.root_id, working_local),
                (prepared.branch_root, branch_local),
                (prepared.layer_root, layer_local),
            ],
        )?;
        let built = apply_reconcile_choices(
            &reader,
            working.root_id,
            prepared.branch_root,
            prepared.layer_root,
            &prepared.conflicts,
            choices,
        )?;
        crate::provision::admit_built(&self.db, None, &built)?;
        let complete = self.local_closure_complete(built.root_id)?;
        self.commit_candidate_impl(
            parent,
            prepared.branch_id,
            Some(prepared.expected_head),
            prepared.old_base_layer_id,
            prepared.branch_root,
            built,
            prepared.current_layer_id,
            complete,
            true,
            false,
        )
    }

    fn local_closure_complete(&self, root: layerfs_content::ObjectId) -> Result<bool> {
        match self.db.verify_complete_roots([root]) {
            Ok(_) => Ok(true),
            Err(StorageError::MissingObject(_)) => Ok(false),
            Err(error) => Err(error),
        }
    }

    pub fn commit_changes(
        &self,
        parent: Arc<dyn LayerStackEndpoint>,
        branch_id: layerfs_storage::BranchId,
        expected_head: Option<CommitId>,
        changes: &[ContentChange],
    ) -> Result<CommitOutcome> {
        self.require_local_branch(branch_id)?;
        let pinned = self.pin_branch(parent.clone(), branch_id)?;
        if pinned.branch.head_commit_id != expected_head {
            return Err(StorageError::CommitHeadMoved {
                expected: expected_head,
                actual: pinned.branch.head_commit_id,
            });
        }
        let seed = *layerfs_content::filesystem::namespace(
            &layerfs_storage::CoreReader(&pinned.reader),
            pinned.root,
        )?
        .root_directory_inode
        .as_bytes();
        let built = apply_changes(&pinned.reader, pinned.root, changes, seed)?;
        self.commit_candidate(
            parent,
            branch_id,
            expected_head,
            pinned.branch.base_layer_id,
            pinned.root,
            built,
            pinned.branch.base_layer_id,
            false,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn commit_candidate(
        &self,
        parent: Arc<dyn LayerStackEndpoint>,
        branch_id: layerfs_storage::BranchId,
        expected_head: Option<CommitId>,
        expected_base: LayerId,
        base_root: layerfs_content::ObjectId,
        built: BuiltRoot,
        new_base: LayerId,
        force_complete: bool,
    ) -> Result<CommitOutcome> {
        self.commit_candidate_impl(
            parent,
            branch_id,
            expected_head,
            expected_base,
            base_root,
            built,
            new_base,
            force_complete,
            false,
            true,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn commit_candidate_impl(
        &self,
        parent: Arc<dyn LayerStackEndpoint>,
        branch_id: layerfs_storage::BranchId,
        expected_head: Option<CommitId>,
        expected_base: LayerId,
        base_root: layerfs_content::ObjectId,
        built: BuiltRoot,
        new_base: LayerId,
        force_complete: bool,
        objects_admitted: bool,
        inherit_base_completeness: bool,
    ) -> Result<CommitOutcome> {
        let _operation = self.db.enter_operation()?;
        let branch = self.require_local_branch(branch_id)?;
        if branch.head_commit_id != expected_head || branch.base_layer_id != expected_base {
            return Err(StorageError::CommitHeadMoved {
                expected: expected_head,
                actual: branch.head_commit_id,
            });
        }
        let actual_base_root = match expected_head {
            Some(commit_id) => {
                self.db
                    .commit(commit_id)?
                    .ok_or(StorageError::Integrity("Branch head Commit"))?
                    .root_id
            }
            None => {
                self.db
                    .layer(expected_base)?
                    .ok_or(StorageError::Integrity("Branch base Layer"))?
                    .root_id
            }
        };
        if actual_base_root != base_root {
            return Err(StorageError::Integrity("Workspace base root"));
        }
        if built.root_id == base_root && new_base == expected_base {
            return Ok(CommitOutcome::UpToDate {
                head: expected_head,
            });
        }
        let expected_layer = self
            .db
            .layer(expected_base)?
            .ok_or(StorageError::Integrity("Branch base Layer"))?;
        let new_layer = self
            .db
            .layer(new_base)?
            .ok_or(StorageError::Integrity("Commit base Layer"))?;
        if expected_layer.layer_stack_id != branch.layer_stack_id
            || new_layer.layer_stack_id != branch.layer_stack_id
        {
            return Err(StorageError::Integrity("Branch LayerStack ownership"));
        }
        let complete =
            force_complete || (inherit_base_completeness && self.db.complete_root(base_root)?);
        if !objects_admitted {
            if parent.store_id()? != self.parent_store_id() {
                return Err(StorageError::WrongParent);
            }
            crate::provision::admit_built(
                &self.db,
                (!complete).then_some(parent.as_ref()),
                &built,
            )?;
        }
        if complete {
            let started = std::time::Instant::now();
            let verified = self.verify_local_closure(built.root_id);
            layerfs_storage::note_workspace_commit_phase(
                layerfs_storage::WorkspaceCommitPhase::CompletenessVerify,
                started.elapsed().as_nanos().min(u128::from(u64::MAX)) as u64,
            );
            verified?;
        }
        let commit = CommitRecord {
            id: CommitId::derive(built.root_id, expected_head, new_base),
            root_id: built.root_id,
            parent_commit_id: expected_head,
            base_layer_id: new_base,
        };
        let push_plan = if inherit_base_completeness {
            built
                .objects
                .ids_in_order(crate::store::PUSH_PLAN_ID_CAP)?
                .map(|ids| PushPlan {
                    commit_id: commit.id,
                    base_root,
                    new_root: built.root_id,
                    ids,
                })
        } else {
            None
        };
        self.db.commit_branch(
            branch_id,
            expected_head,
            expected_base,
            commit,
            new_base,
            complete,
        )?;
        if let Some(plan) = push_plan {
            self.retain_push_plan(branch_id, plan);
        }
        Ok(CommitOutcome::Created {
            previous_head: expected_head,
            commit_id: commit.id,
        })
    }
}

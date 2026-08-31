use crate::{
    BranchId, BranchRecord, EntityName, LayerStackStore, LocalForkSource, Result, StoreError,
};
use rusqlite::{OptionalExtension, TransactionBehavior};

const MAX_ANCESTRY_PROOF: i64 = 1_000_000;

impl LayerStackStore {
    pub fn fork_branch(&self, name: EntityName, source: LocalForkSource) -> Result<BranchId> {
        let _operation = self.db.enter_operation()?;
        let incoming_id = BranchId::new();
        let branch = match source {
            LocalForkSource::Layer { layer_id } => {
                let layer = self.layer(layer_id)?.ok_or(StoreError::NotFound("Layer"))?;
                BranchRecord {
                    id: incoming_id,
                    layer_stack_id: layer.layer_stack_id,
                    name: name.clone(),
                    base_layer_id: layer.id,
                    head_commit_id: None,
                }
            }
            LocalForkSource::Branch {
                branch_id,
                commit_id,
            } => {
                let source = self
                    .branch(branch_id)?
                    .ok_or(StoreError::NotFound("Branch"))?;
                if !self.branch_contains_commit(branch_id, commit_id)? {
                    return Err(StoreError::InvalidInput("Commit outside Branch history"));
                }
                let commit = self
                    .commit(commit_id)?
                    .ok_or(StoreError::Integrity("Branch Commit"))?;
                let base = self
                    .layer(commit.base_layer_id)?
                    .ok_or(StoreError::Integrity("Commit base Layer"))?;
                if base.layer_stack_id != source.layer_stack_id {
                    return Err(StoreError::Integrity("Branch LayerStack ownership"));
                }
                BranchRecord {
                    id: incoming_id,
                    layer_stack_id: source.layer_stack_id,
                    name: name.clone(),
                    base_layer_id: commit.base_layer_id,
                    head_commit_id: Some(commit_id),
                }
            }
        };
        let mut connection = self.db.writer()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        crate::schema::fail_transaction_statement(1)?;
        if let Err(error) = transaction.execute(
            crate::statements::branch::INSERT,
            rusqlite::params![
                branch.id.as_slice(),
                branch.layer_stack_id.as_slice(),
                branch.name.as_str(),
                branch.base_layer_id.as_slice(),
                branch.head_commit_id.map(|id| id.to_bytes().to_vec()),
            ],
        ) {
            drop(transaction);
            drop(connection);
            if let Some(existing) = self.branch_by_name(branch.layer_stack_id, &name)? {
                return Err(StoreError::BranchNameConflict {
                    layer_stack_id: branch.layer_stack_id,
                    name,
                    existing_id: existing.id,
                    incoming_id,
                });
            }
            return Err(error.into());
        }
        transaction.commit()?;
        Ok(incoming_id)
    }

    pub fn branch_contains_commit(
        &self,
        branch_id: BranchId,
        commit_id: crate::CommitId,
    ) -> Result<bool> {
        Ok(self
            .db
            .reader()?
            .query_row(
                crate::statements::branch::CONTAINS_COMMIT,
                rusqlite::params![
                    branch_id.as_slice(),
                    commit_id.as_slice(),
                    MAX_ANCESTRY_PROOF
                ],
                |_| Ok(()),
            )
            .optional()?
            .is_some())
    }
}

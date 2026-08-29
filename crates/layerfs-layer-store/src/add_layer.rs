use crate::LayerStore;
use layerfs_storage::{
    merge_candidate, AddResult, BaseId, CandidateMergeOutcome, LayerHistoryId, LayerId,
    LayerRecord, LayerSource, Result, ResultId, SourceId, StorageError,
};

impl LayerStore {
    pub fn add_layer(&self, source: LayerSource) -> Result<AddResult<LayerId>> {
        let layer_history_id = match source {
            LayerSource::BranchCommit(source) => {
                let branch = self
                    .db
                    .branch(source.branch_id)?
                    .ok_or(StorageError::MissingBaseData)?;
                self.base_layer(branch.base_id)?.history_id
            }
            LayerSource::Stack(stack_id) => {
                let stack = self
                    .db
                    .stack(stack_id)?
                    .ok_or(StorageError::MissingBaseData)?;
                let history = self
                    .db
                    .stack_history(stack.history_id)?
                    .ok_or(StorageError::MissingBaseData)?;
                self.db
                    .layer(history.base_layer_id)?
                    .ok_or(StorageError::MissingBaseData)?
                    .history_id
            }
        };
        self.add_layer_to_history(layer_history_id, source)
    }

    pub(crate) fn add_layer_to_history(
        &self,
        layer_history_id: LayerHistoryId,
        source: LayerSource,
    ) -> Result<AddResult<LayerId>> {
        let _operation = self.db.enter_operation()?;
        let source_id = match source {
            LayerSource::BranchCommit(source) => SourceId::Branch(source.branch_id),
            LayerSource::Stack(stack_id) => SourceId::Stack(stack_id),
        };
        if let Some(existing) = self.db.add_result(source_id)? {
            return match existing.result_id {
                ResultId::Layer(result_id) => {
                    let layer = self
                        .db
                        .layer(result_id)?
                        .ok_or(StorageError::MissingBaseData)?;
                    if layer.history_id == layer_history_id {
                        Ok(AddResult { result_id })
                    } else {
                        Err(StorageError::WrongLayerHistory(
                            layerfs_storage::WrongHistory {
                                expected: layer_history_id,
                                actual: layer.history_id,
                            },
                        ))
                    }
                }
                ResultId::Stack(_) => Err(StorageError::WrongSourceRoute),
            };
        }
        let (base_layer, candidate_root) = match source {
            LayerSource::BranchCommit(source) => {
                let branch_id = source.branch_id;
                let commit_id = source.commit_id;
                let branch = self
                    .db
                    .branch(branch_id)?
                    .ok_or(StorageError::MissingBaseData)?;
                if branch.head_commit_id != commit_id {
                    return Err(StorageError::CommitHeadMoved(layerfs_storage::HeadMoved {
                        expected: Some(commit_id),
                        actual: Some(branch.head_commit_id),
                    }));
                }
                let BaseId::Layer(base_layer_id) = branch.base_id else {
                    return Err(StorageError::WrongSourceRoute);
                };
                let commit = self
                    .db
                    .commit(commit_id)?
                    .ok_or(StorageError::MissingBaseData)?;
                (
                    self.db
                        .layer(base_layer_id)?
                        .ok_or(StorageError::MissingBaseData)?,
                    commit.root_id,
                )
            }
            LayerSource::Stack(stack_id) => {
                let stack = self
                    .db
                    .stack(stack_id)?
                    .ok_or(StorageError::MissingBaseData)?;
                let history = self
                    .db
                    .stack_history(stack.history_id)?
                    .ok_or(StorageError::MissingBaseData)?;
                (
                    self.db
                        .layer(history.base_layer_id)?
                        .ok_or(StorageError::MissingBaseData)?,
                    stack.root_id,
                )
            }
        };
        if base_layer.history_id != layer_history_id {
            return Err(StorageError::WrongLayerHistory(
                layerfs_storage::WrongHistory {
                    expected: layer_history_id,
                    actual: base_layer.history_id,
                },
            ));
        }
        let history = self
            .db
            .layer_history(layer_history_id)?
            .ok_or(StorageError::NotFound("LayerHistory"))?;
        let current = self
            .db
            .layer(history.head_layer_id)?
            .ok_or(StorageError::MissingBaseData)?;
        let merged = match merge_candidate(
            &self.db,
            base_layer.root_id,
            current.root_id,
            candidate_root,
        )? {
            CandidateMergeOutcome::Conflict(conflict) => {
                return Err(StorageError::Conflict(Box::new(conflict)))
            }
            CandidateMergeOutcome::Clean(merged) => merged,
        };
        let layer = (merged.root_id != current.root_id).then(|| {
            let id = LayerId::derive(layer_history_id, Some(current.id), merged.root_id);
            LayerRecord {
                id,
                history_id: layer_history_id,
                parent_id: Some(current.id),
                root_id: merged.root_id,
            }
        });
        let result_id = self
            .db
            .add_layer_atomic(source_id, current.id, layer, &merged.objects)?;
        Ok(AddResult { result_id })
    }

    fn base_layer(&self, base: BaseId) -> Result<LayerRecord> {
        match base {
            BaseId::Layer(id) => self.db.layer(id)?.ok_or(StorageError::MissingBaseData),
            BaseId::Stack(id) => {
                let stack = self.db.stack(id)?.ok_or(StorageError::MissingBaseData)?;
                let history = self
                    .db
                    .stack_history(stack.history_id)?
                    .ok_or(StorageError::MissingBaseData)?;
                self.db
                    .layer(history.base_layer_id)?
                    .ok_or(StorageError::MissingBaseData)
            }
        }
    }
}

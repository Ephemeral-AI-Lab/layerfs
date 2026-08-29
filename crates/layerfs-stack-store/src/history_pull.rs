use crate::StackStore;
#[cfg(test)]
use layerfs_storage::DEFERRED_MEMORY_BYTES;
use layerfs_storage::{
    BaseId, DeferredFactStore, Fact, LayerHistoryId, LayerHistoryRecord, LayerId, RefOutcome,
    Result, StackHistoryId, StackHistoryRecord, StackId, StorageError, TransferPipeline,
};

impl StackStore {
    pub fn pull_layer(&self, through: LayerId) -> Result<RefOutcome<LayerId>> {
        let history_id = self
            .parent
            .base_snapshot(BaseId::Layer(through))?
            .layer_history_id;
        self.pull_layer_history(history_id, through)
    }

    pub(crate) fn pull_layer_history(
        &self,
        history_id: LayerHistoryId,
        through: LayerId,
    ) -> Result<RefOutcome<LayerId>> {
        let mut transfer = TransferPipeline::new(self)?;
        self.pull_layers(history_id, through, &mut transfer)?;
        transfer.finish_layer_history(LayerHistoryRecord {
            id: history_id,
            head_layer_id: through,
        })
    }

    fn pull_layers(
        &self,
        history_id: LayerHistoryId,
        through: LayerId,
        transfer: &mut TransferPipeline<'_>,
    ) -> Result<()> {
        self.parent.layer_history_record(history_id)?;
        let mut spool = DeferredFactStore::new()?;
        self.parent.visit_layers(
            history_id,
            through,
            &mut |kind, ids| transfer.announce_fact_ids(kind, ids),
            &mut |layers| {
                for layer in layers {
                    spool.stage(Fact::Layer(*layer))?;
                }
                Ok(())
            },
        )?;
        spool.visit_batches(&mut |facts| {
            let roots = facts
                .iter()
                .map(|fact| match fact {
                    Fact::Layer(layer) => Ok(layer.root_id),
                    _ => Err(StorageError::Integrity("Layer pull spool")),
                })
                .collect::<Result<Vec<_>>>()?;
            transfer.snapshots(self.parent.as_ref(), &roots)?;
            transfer.stage_facts(facts)
        })
    }

    pub fn pull_stack(&self, through: StackId) -> Result<RefOutcome<StackId>> {
        let history_id = self.parent.stack_record(through)?.history_id;
        self.pull_stack_history(history_id, through)
    }

    pub(crate) fn pull_stack_history(
        &self,
        history_id: StackHistoryId,
        through: StackId,
    ) -> Result<RefOutcome<StackId>> {
        let mut transfer = TransferPipeline::new(self)?;
        let history = self.parent.stack_history_record(history_id)?;
        let base = self
            .parent
            .base_snapshot(BaseId::Layer(history.base_layer_id))?;
        self.pull_layers(base.layer_history_id, history.base_layer_id, &mut transfer)?;
        let mut spool = DeferredFactStore::new()?;
        self.parent.visit_stacks(
            history_id,
            through,
            &mut |kind, ids| transfer.announce_fact_ids(kind, ids),
            &mut |stacks| {
                for stack in stacks {
                    spool.stage(Fact::Stack(*stack))?;
                }
                Ok(())
            },
        )?;
        spool.visit_batches(&mut |facts| {
            let roots = facts
                .iter()
                .map(|fact| match fact {
                    Fact::Stack(stack) => Ok(stack.root_id),
                    _ => Err(StorageError::Integrity("Stack pull spool")),
                })
                .collect::<Result<Vec<_>>>()?;
            transfer.snapshots(self.parent.as_ref(), &roots)?;
            transfer.stage_facts(facts)
        })?;
        let expected = self
            .db
            .stack_history(history_id)?
            .map(|value| value.head_stack_id);
        transfer.finish_stack_history(
            StackHistoryRecord {
                head_stack_id: through,
                ..history
            },
            expected,
        )
    }
}

#[cfg(test)]
#[path = "../tests/support/history_pull_unit.rs"]
mod tests;

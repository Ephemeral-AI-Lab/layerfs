use crate::{history_pull::DeferredFactStore, StackStore};
use layerfs_storage_core::{BranchId, CommitId, Fact, Result, StorageError, TransferPipeline};

impl StackStore {
    pub fn pull_commit_history(&self, branch_id: BranchId) -> Result<CommitId> {
        let branch = self.parent.branch_record(branch_id)?;
        let mut transfer = TransferPipeline::new(self)?;
        let mut spool = DeferredFactStore::new()?;
        self.parent.visit_commits(
            branch_id,
            &mut |kind, ids| transfer.announce_fact_ids(kind, ids),
            &mut |commits| {
                for commit in commits {
                    spool.stage(Fact::Commit(*commit))?;
                }
                Ok(())
            },
        )?;
        spool.visit_batches(&mut |facts| {
            let roots = facts
                .iter()
                .map(|fact| match fact {
                    Fact::Commit(commit) => Ok(commit.root_id),
                    _ => Err(StorageError::Integrity("Commit pull spool")),
                })
                .collect::<Result<Vec<_>>>()?;
            transfer.snapshots(self.parent.as_ref(), &roots)?;
            transfer.stage_facts(facts)
        })?;
        transfer.finish()?;
        Ok(branch.head_commit_id)
    }
}

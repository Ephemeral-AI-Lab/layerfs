use crate::BranchStore;
use layerfs_storage::internal::{TransferIntent, TransferOutcome, TransferPipeline};
use layerfs_storage::{
    BranchId, BranchRecord, CommitId, DeferredFactStore, Fact, PulledBranch, RefOutcome, Result,
    StorageError,
};

impl BranchStore {
    pub fn pull_commits(&self, branch_id: BranchId) -> Result<CommitId> {
        let branch = self.parent.branch_record(branch_id)?;
        let mut spool = DeferredFactStore::new()?;
        self.parent.visit_commits(
            branch_id,
            &mut |kind, ids| self.db.missing_facts(kind, ids),
            &mut |commits| {
                for commit in commits {
                    spool.stage(Fact::Commit(*commit))?;
                }
                Ok(())
            },
        )?;
        spool.visit_batches(&mut |facts| self.db.admit_facts(facts))?;
        Ok(branch.head_commit_id)
    }

    pub fn push_branch(&self, branch_id: BranchId) -> Result<RefOutcome<CommitId>> {
        let _operation = self.db.enter_operation()?;
        let branch = self
            .db
            .branch(branch_id)?
            .ok_or(StorageError::NotFound("Branch"))?;
        let head = self
            .db
            .commit(branch.head_commit_id)?
            .ok_or(StorageError::MissingBaseData)?;
        let mut transfer = TransferPipeline::new(self.parent.as_ref())?;
        let (expected, up_to_date) = transfer.preflight_branch(branch, head.root_id)?;
        if up_to_date {
            transfer.finish_up_to_date()?;
            return Ok(RefOutcome::UpToDate(
                expected.ok_or(StorageError::MissingBaseData)?,
            ));
        }
        self.db.visit_commit_ancestry(
            branch.head_commit_id,
            Some(self),
            &mut |source, commits| {
                let roots = commits
                    .iter()
                    .map(|commit| commit.root_id)
                    .collect::<Vec<_>>();
                transfer.snapshots(source, &roots)?;
                let facts = commits
                    .iter()
                    .copied()
                    .map(Fact::Commit)
                    .collect::<Vec<_>>();
                transfer.facts(&facts)?;
                Ok(())
            },
        )?;
        transfer.finish_branch(branch, expected)
    }

    pub fn pull_branch(&self, source_branch_id: BranchId) -> Result<PulledBranch> {
        let _operation = self.db.enter_operation()?;
        let source = self.parent.branch_record(source_branch_id)?;
        self.parent.base_snapshot(source.base_id)?;
        let existing = self.db.branch(source_branch_id)?;
        if existing.is_some_and(|branch| branch == source) {
            return Ok((source, RefOutcome::UpToDate(source.head_commit_id)));
        }
        let pending = std::cell::RefCell::new(None::<Vec<Fact>>);
        self.parent.visit_commits(
            source_branch_id,
            &mut |kind, ids| self.db.missing_facts(kind, ids),
            &mut |commits| {
                let facts = commits
                    .iter()
                    .copied()
                    .map(Fact::Commit)
                    .collect::<Vec<_>>();
                for batch in layerfs_storage::fact_batches(&facts)? {
                    if let Some(previous) = pending.replace(Some(batch.to_vec())) {
                        self.db.admit_facts(&previous)?;
                    }
                }
                Ok(())
            },
        )?;
        let local = BranchRecord {
            id: source_branch_id,
            head_commit_id: source.head_commit_id,
            base_id: source.base_id,
        };
        let (_, outcome) = self.db.finish_transfer(
            &[],
            &pending.into_inner().unwrap_or_default(),
            TransferIntent::Branch {
                branch: local,
                expected: existing.map(|branch| branch.head_commit_id),
            },
        )?;
        match outcome {
            TransferOutcome::Commit(outcome) => Ok((local, outcome)),
            _ => Err(StorageError::Integrity("Pull Branch outcome")),
        }
    }
}

use crate::pull::FactSpool;
use crate::BranchStore;
use layerfs_storage::{
    BranchRecord, EndpointTarget, Fact, LayerStackEndpoint, PushResult, Result,
    RootTransferRequest, StorageError, TransferPipeline, TransferTarget, FACT_BATCH_BYTES,
    FACT_BATCH_COUNT,
};
use std::sync::Arc;

const HISTORY_PAGE: u16 = 128;

impl BranchStore {
    pub fn push_branch(
        &self,
        parent: Arc<dyn LayerStackEndpoint>,
        branch_id: layerfs_storage::BranchId,
    ) -> Result<PushResult> {
        let _operation = self.db.enter_operation()?;
        if parent.store_id()? != self.parent_store_id() {
            return Err(StorageError::WrongParent);
        }
        let branch = self.require_local_branch(branch_id)?;
        let Some(local_head) = branch.head_commit_id else {
            return Ok(PushResult::NoChanges);
        };
        let authority = parent.branch(branch_id)?;
        if let Some(authority) = &authority {
            if authority.fact() != branch.fact() {
                return Err(StorageError::Integrity("Branch origin mismatch"));
            }
            let authority_head = authority
                .head_commit_id
                .ok_or(StorageError::Integrity("authority Branch head"))?;
            if authority_head == local_head {
                return parent.publish_branch(&branch, Some(authority_head));
            }
        } else {
            self.require_authority_origin(parent.as_ref(), &branch)?;
        }

        let observed_head = authority.and_then(|record| record.head_commit_id);
        let mut spool = FactSpool::new()?;
        let contains_observed =
            self.spool_owned_commits(&branch, local_head, observed_head, &mut spool)?;
        if let Some(authority_head) = observed_head.filter(|_| !contains_observed) {
            return Ok(PushResult::HeadMoved {
                authority_head,
                local_head,
            });
        }

        let target = EndpointTarget(parent.as_ref());
        self.transfer_push_roots(parent.clone(), &target, &branch, &mut spool)?;
        self.transfer_push_facts(&target, &mut spool)?;
        parent.publish_branch(&branch, observed_head)
    }

    fn require_authority_origin(
        &self,
        parent: &dyn LayerStackEndpoint,
        branch: &BranchRecord,
    ) -> Result<()> {
        match (
            branch.forked_from_layer_id,
            branch.forked_from_branch_id,
            branch.forked_from_commit_id,
        ) {
            (Some(layer_id), None, None) => {
                let layer = parent
                    .layer(layer_id)?
                    .ok_or(StorageError::NotFound("authority Branch Layer origin"))?;
                if layer.layer_stack_id != branch.layer_stack_id {
                    return Err(StorageError::Integrity("Branch LayerStack ownership"));
                }
            }
            (None, Some(source_branch_id), Some(source_commit_id)) => {
                let source = parent
                    .branch(source_branch_id)?
                    .ok_or(StorageError::NotFound("authority Branch origin"))?;
                if source.layer_stack_id != branch.layer_stack_id {
                    return Err(StorageError::Integrity("Branch LayerStack ownership"));
                }
                let page =
                    parent.commit_history_page(source_branch_id, source_commit_id, None, 1)?;
                if page.records.first().map(|record| record.id) != Some(source_commit_id) {
                    return Err(StorageError::NotFound("authority Commit origin"));
                }
            }
            _ => return Err(StorageError::Integrity("Branch origin")),
        }
        Ok(())
    }

    fn spool_owned_commits(
        &self,
        branch: &BranchRecord,
        through: layerfs_storage::CommitId,
        observed: Option<layerfs_storage::CommitId>,
        spool: &mut FactSpool,
    ) -> Result<bool> {
        let mut cursor = None;
        let mut expected = through;
        loop {
            let page = self
                .db
                .owned_commit_page(branch.id, through, cursor, HISTORY_PAGE)?;
            if page.records.is_empty() {
                break;
            }
            for commit in page.records {
                if commit.id != expected {
                    return Err(StorageError::Integrity("owned Commit order"));
                }
                if observed == Some(commit.id) {
                    return Ok(true);
                }
                let base = self
                    .db
                    .layer(commit.base_layer_id)?
                    .ok_or(StorageError::Integrity("Commit base Layer"))?;
                if base.layer_stack_id != branch.layer_stack_id {
                    return Err(StorageError::Integrity("Branch LayerStack ownership"));
                }
                expected = commit.parent_commit_id.unwrap_or(commit.id);
                spool.push(Fact::Commit(commit))?;
            }
            match page.continuation {
                Some(next) if next == expected => cursor = Some(next),
                Some(_) => return Err(StorageError::Integrity("owned Commit continuation")),
                None => break,
            }
        }
        Ok(false)
    }

    fn transfer_push_roots(
        &self,
        parent: Arc<dyn LayerStackEndpoint>,
        target: &dyn TransferTarget,
        branch: &BranchRecord,
        spool: &mut FactSpool,
    ) -> Result<()> {
        let head = self
            .db
            .commit(
                branch
                    .head_commit_id
                    .ok_or(StorageError::Integrity("local Branch head"))?,
            )?
            .ok_or(StorageError::Integrity("local Branch head Commit"))?;
        let reader =
            self.reader_with_policy(parent, head.root_id, self.db.complete_root(head.root_id)?)?;
        let mut requests = PushRootRequests::new(spool, target)?;
        let transferred = layerfs_storage::transfer_roots(&reader, target, &mut requests);
        requests.finish()?;
        transferred?;
        Ok(())
    }

    fn transfer_push_facts(
        &self,
        target: &dyn TransferTarget,
        spool: &mut FactSpool,
    ) -> Result<()> {
        let mut pipeline = TransferPipeline::new(target)?;
        let mut batch = Vec::with_capacity(FACT_BATCH_COUNT);
        let mut bytes = 0;
        spool.visit_reverse(&mut |fact| {
            let encoded = fact.encoded_size();
            if !batch.is_empty()
                && (batch.len() == FACT_BATCH_COUNT || bytes + encoded > FACT_BATCH_BYTES)
            {
                pipeline.facts(&batch)?;
                batch.clear();
                bytes = 0;
            }
            bytes += encoded;
            batch.push(fact);
            Ok(())
        })?;
        pipeline.facts(&batch)?;
        pipeline.finish()?;
        Ok(())
    }
}

fn root_present(target: &dyn TransferTarget, root: layerfs_content::ObjectId) -> Result<bool> {
    Ok(!target.missing_objects(&[root])?.is_missing(0)?)
}

struct PushRootRequests<'a> {
    facts: crate::pull::ReverseFacts<'a>,
    target: &'a dyn TransferTarget,
    error: Option<StorageError>,
}

impl<'a> PushRootRequests<'a> {
    fn new(spool: &'a mut FactSpool, target: &'a dyn TransferTarget) -> Result<Self> {
        Ok(Self {
            facts: spool.reverse()?,
            target,
            error: None,
        })
    }

    fn finish(&mut self) -> Result<()> {
        self.error.take().map_or(Ok(()), Err)
    }
}

impl Iterator for PushRootRequests<'_> {
    type Item = RootTransferRequest;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let fact = match self.facts.next()? {
                Ok(fact) => fact,
                Err(error) => {
                    self.error = Some(error);
                    return None;
                }
            };
            let Fact::Commit(commit) = fact else {
                continue;
            };
            return match root_present(self.target, commit.root_id) {
                Ok(known_complete) => Some(RootTransferRequest {
                    root_id: commit.root_id,
                    known_complete,
                }),
                Err(error) => {
                    self.error = Some(error);
                    None
                }
            };
        }
    }
}

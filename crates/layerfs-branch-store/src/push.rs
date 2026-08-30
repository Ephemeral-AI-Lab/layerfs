use crate::pull::FactSpool;
use crate::BranchStore;
use layerfs_storage::{
    begin_push, note_push_endpoint_call, note_push_phase, record_durability, BranchRecord,
    EndpointTarget, Fact, LayerStackEndpoint, PushPhase, PushResult, Result, RootTransferRequest,
    StorageError, TransferPipeline, TransferTarget, FACT_BATCH_BYTES, FACT_BATCH_COUNT,
};
use std::collections::BTreeSet;
use std::sync::Arc;

const HISTORY_PAGE: u16 = 128;

impl BranchStore {
    pub fn push_branch(
        &self,
        parent: Arc<dyn LayerStackEndpoint>,
        branch_id: layerfs_storage::BranchId,
    ) -> Result<PushResult> {
        let _operation = self.db.enter_operation()?;
        let _timing = begin_push()?;
        note_push_endpoint_call();
        if parent.store_id()? != self.parent_store_id() {
            return Err(StorageError::WrongParent);
        }
        let branch = self.require_local_branch(branch_id)?;
        let Some(local_head) = branch.head_commit_id else {
            return Ok(PushResult::NoChanges);
        };
        note_push_endpoint_call();
        let authority = parent.branch(branch_id)?;
        if let Some(authority) = &authority {
            if authority.fact() != branch.fact() {
                return Err(StorageError::Integrity("Branch origin mismatch"));
            }
            let authority_head = authority
                .head_commit_id
                .ok_or(StorageError::Integrity("authority Branch head"))?;
            if authority_head == local_head {
                let result = self.publish_stable(parent.as_ref(), &branch, Some(authority_head));
                if result.is_ok() {
                    self.clear_push_plan(branch.id, local_head);
                }
                return result;
            }
        } else {
            let started = std::time::Instant::now();
            let origin = self.require_authority_origin(parent.as_ref(), &branch);
            note_push_phase(
                PushPhase::AuthorityTransitionVerify,
                started.elapsed().as_nanos().min(u128::from(u64::MAX)) as u64,
            );
            origin?;
        }

        let observed_head = authority.and_then(|record| record.head_commit_id);
        let mut spool = FactSpool::new()?;
        let started = std::time::Instant::now();
        let contains_observed =
            self.spool_owned_commits(&branch, local_head, observed_head, &mut spool)?;
        note_push_phase(
            PushPhase::History,
            started.elapsed().as_nanos().min(u128::from(u64::MAX)) as u64,
        );
        if let Some(authority_head) = observed_head.filter(|_| !contains_observed) {
            return Ok(PushResult::HeadMoved {
                authority_head,
                local_head,
            });
        }

        let target = EndpointTarget(parent.as_ref());
        self.transfer_push_roots(parent.clone(), &target, &branch, observed_head, &mut spool)?;
        self.transfer_push_facts(&target, &mut spool)?;
        let result = self.publish_stable(parent.as_ref(), &branch, observed_head);
        if result.is_ok() {
            self.clear_push_plan(branch.id, local_head);
        }
        result
    }

    fn publish_stable(
        &self,
        parent: &dyn LayerStackEndpoint,
        branch: &BranchRecord,
        observed_head: Option<layerfs_storage::CommitId>,
    ) -> Result<PushResult> {
        note_push_endpoint_call();
        let result = parent.publish_branch(branch, observed_head)?;
        record_durability(self.db.stable_barrier()?)?;
        Ok(result)
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
                note_push_endpoint_call();
                let layer = parent
                    .layer(layer_id)?
                    .ok_or(StorageError::NotFound("authority Branch Layer origin"))?;
                if layer.layer_stack_id != branch.layer_stack_id {
                    return Err(StorageError::Integrity("Branch LayerStack ownership"));
                }
            }
            (None, Some(source_branch_id), Some(source_commit_id)) => {
                note_push_endpoint_call();
                let source = parent
                    .branch(source_branch_id)?
                    .ok_or(StorageError::NotFound("authority Branch origin"))?;
                if source.layer_stack_id != branch.layer_stack_id {
                    return Err(StorageError::Integrity("Branch LayerStack ownership"));
                }
                note_push_endpoint_call();
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
        observed_head: Option<layerfs_storage::CommitId>,
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
        let mut prior = if let Some(observed_head) = observed_head {
            self.db
                .commit(observed_head)?
                .ok_or(StorageError::Integrity("Push observed Commit"))?
                .root_id
        } else if let Some(origin) = branch.forked_from_commit_id {
            self.db
                .commit(origin)?
                .ok_or(StorageError::Integrity("Push origin Commit"))?
                .root_id
        } else {
            self.db
                .layer(
                    branch
                        .forked_from_layer_id
                        .ok_or(StorageError::Integrity("Push origin Layer"))?,
                )?
                .ok_or(StorageError::Integrity("Push origin Layer"))?
                .root_id
        };
        if reader.is_complete() {
            let mut requests = PushRootRequests::new(spool)?;
            let transferred = layerfs_storage::transfer_roots(&reader, target, &mut requests);
            requests.finish()?;
            transferred?;
            return Ok(());
        }
        if let Some(plan) = self.push_plan(branch.id).filter(|plan| {
            plan.commit_id == head.id
                && plan.base_root == prior
                && plan.new_root == head.root_id
                && !plan.ids.is_empty()
                && plan.ids.len() <= crate::store::PUSH_PLAN_ID_CAP
                && plan.ids.contains(&head.root_id)
                && plan.ids.iter().copied().collect::<BTreeSet<_>>().len() == plan.ids.len()
        }) {
            return transfer_push_plan(&reader, target, &plan.ids);
        }
        spool.visit_reverse(&mut |fact| {
            let Fact::Commit(commit) = fact else {
                return Ok(());
            };
            layerfs_storage::transfer_root_transition(&reader, target, prior, commit.root_id)?;
            prior = commit.root_id;
            Ok(())
        })?;
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

fn transfer_push_plan(
    reader: &impl layerfs_storage::ObjectSource,
    target: &dyn TransferTarget,
    postorder: &[layerfs_content::ObjectId],
) -> Result<()> {
    let started = std::time::Instant::now();
    let mut announced = postorder.to_vec();
    announced.sort();
    layerfs_storage::note_push_phase(
        PushPhase::Frontier,
        started.elapsed().as_nanos().min(u128::from(u64::MAX)) as u64,
    );
    let mut pipeline = TransferPipeline::new(target)?;
    let mut missing = Vec::new();
    for page in announced.chunks(layerfs_storage::ID_BATCH_COUNT) {
        let page_missing = pipeline.announce_objects(page)?;
        for (index, id) in page.iter().enumerate() {
            if page_missing.is_missing(index)? {
                missing.push(*id);
            }
        }
    }
    pipeline.prune_complete_root();
    let mut ids = Vec::with_capacity(layerfs_storage::OBJECT_BATCH_COUNT);
    for id in postorder
        .iter()
        .filter(|id| missing.binary_search(id).is_ok())
    {
        ids.push(*id);
        if ids.len() < layerfs_storage::OBJECT_BATCH_COUNT {
            continue;
        }
        transfer_push_plan_objects(reader, &mut pipeline, &ids)?;
        ids.clear();
    }
    transfer_push_plan_objects(reader, &mut pipeline, &ids)?;
    pipeline.finish()?;
    Ok(())
}

fn transfer_push_plan_objects(
    reader: &impl layerfs_storage::ObjectSource,
    pipeline: &mut TransferPipeline<'_>,
    ids: &[layerfs_content::ObjectId],
) -> Result<()> {
    if !ids.is_empty() {
        let started = std::time::Instant::now();
        let objects = reader.read_objects(ids)?;
        layerfs_storage::note_push_phase(
            PushPhase::SourceReadAuth,
            started.elapsed().as_nanos().min(u128::from(u64::MAX)) as u64,
        );
        if objects.len() != ids.len()
            || objects.iter().zip(ids).any(|(object, id)| object.id != *id)
        {
            return Err(StorageError::Integrity("Push plan object order"));
        }
        for object in objects {
            pipeline.stage_object(object)?;
        }
    }
    Ok(())
}

struct PushRootRequests<'a> {
    facts: crate::pull::ReverseFacts<'a>,
    error: Option<StorageError>,
}

impl<'a> PushRootRequests<'a> {
    fn new(spool: &'a mut FactSpool) -> Result<Self> {
        Ok(Self {
            facts: spool.reverse()?,
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
            return Some(RootTransferRequest {
                root_id: commit.root_id,
                known_complete: false,
            });
        }
    }
}

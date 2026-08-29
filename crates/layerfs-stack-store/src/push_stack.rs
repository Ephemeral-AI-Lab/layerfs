use crate::{history_pull::DeferredFactStore, StackStore};
use layerfs_storage_core::{
    visit_stack_push_facts, Fact, RefOutcome, Result, StackAttestation, StackId, StackPush,
    StorageError, TransferPipeline,
};

impl StackStore {
    pub fn push_stack(&self, stack_id: StackId) -> Result<RefOutcome<StackId>> {
        let _operation = self.db.enter_operation()?;
        let stack = self
            .db
            .stack(stack_id)?
            .ok_or(StorageError::NotFound("Stack"))?;
        let history = self
            .db
            .stack_history(stack.history_id)?
            .ok_or(StorageError::MissingBaseData)?;
        if history.head_stack_id != stack_id
            || history.id.verification_key_digest()
                != *blake3::hash(&self.writer.public_key()).as_bytes()
        {
            return Err(StorageError::ReadOnlyStackHistory(
                layerfs_storage_core::ReadOnlyHistory {
                    history_id: history.id,
                },
            ));
        }
        let mut pipeline = TransferPipeline::new(self.parent.as_ref())?;
        let (expected, up_to_date) =
            pipeline.preflight_stack(history.id, history.base_layer_id, stack_id, stack.root_id)?;
        if up_to_date {
            return Ok(RefOutcome::UpToDate(
                expected.ok_or(StorageError::MissingBaseData)?,
            ));
        }
        if let Some(expected) = expected {
            let prior = self
                .db
                .stack(expected)?
                .ok_or(StorageError::MissingBaseData)?;
            if prior.history_id != history.id || !self.db.is_stack_ancestor(expected, stack_id)? {
                return Err(StorageError::Integrity("Stack suffix predecessor"));
            }
        }
        pipeline.defer_stack_publication();
        let mut provenance = DeferredFactStore::new()?;
        let summary = std::cell::RefCell::new(StackAttestation::default());
        let mut publication = blake3::Hasher::new();
        publication.update(b"layerfs/stack-publication/v1\0");
        let publication = std::cell::RefCell::new(publication);
        let publication_count = std::cell::Cell::new(0_u64);
        visit_stack_push_facts(&self.db, history.id, expected, stack_id, &mut |facts| {
            for fact in facts
                .iter()
                .filter(|fact| matches!(fact, Fact::Branch(_) | Fact::AddResult(_)))
            {
                let bytes = fact.signing_bytes();
                publication
                    .borrow_mut()
                    .update(&(bytes.len() as u64).to_be_bytes());
                publication.borrow_mut().update(&bytes);
                publication_count.set(publication_count.get() + 1);
            }
            for fact in facts {
                provenance.stage(*fact)?;
            }
            Ok(())
        })?;
        provenance.visit_batches(&mut |facts| {
            let mut ids = facts.iter().copied().map(Fact::id).collect::<Vec<_>>();
            ids.sort();
            summary.borrow_mut().observe(facts[0].kind(), &ids);
            let roots = facts
                .iter()
                .filter_map(|fact| match fact {
                    Fact::Commit(value) => Some(value.root_id),
                    Fact::Stack(value) => Some(value.root_id),
                    _ => None,
                })
                .collect::<Vec<_>>();
            pipeline.snapshots(&self.db, &roots)?;
            pipeline.facts(facts)
        })?;
        let (fact_count, root_count, provenance_digest) = summary.into_inner().finish();
        let mut push = StackPush {
            history_id: history.id,
            base_layer_id: history.base_layer_id,
            expected_head: expected,
            incoming_head: stack_id,
            fact_count,
            root_count,
            provenance_digest,
            publication_count: publication_count.get(),
            publication_digest: *publication.into_inner().finalize().as_bytes(),
            public_key: self.writer.public_key(),
            signature: [0; 64],
        };
        push.signature = self.writer.sign(&push.signing_bytes());
        pipeline.finish_stack(push)
    }
}

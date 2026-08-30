use crate::LayerStackStore;
use layerfs_content::ObjectId;
use layerfs_storage::{
    record_durability, AdmissionSetReceipt, AuthorityAddResult, BranchFact, BranchId, BranchRecord,
    CanonicalObject, CommitHistoryPage, CommitId, CommitRecord, Fact, LayerId, LayerPrefixPage,
    LayerRecord, LayerStackEndpoint, LayerStackFact, LayerStackId, LayerStackRecord, MissingBitmap,
    ObjectSource, PushResult, Result, StorageError, StoreDb, StoreId,
};

impl ObjectSource for LayerStackStore {
    fn read_object(&self, id: ObjectId) -> Result<Vec<u8>> {
        self.db.read_object_row(id)
    }

    fn read_objects(&self, ids: &[ObjectId]) -> Result<Vec<CanonicalObject>> {
        self.db.read_object_rows(ids)
    }
}

impl LayerStackEndpoint for LayerStackStore {
    fn store_id(&self) -> Result<StoreId> {
        Ok(self.db.store_id())
    }

    fn layer_stack_fact(&self, id: LayerStackId) -> Result<Option<LayerStackFact>> {
        self.db.layer_stack_fact(id)
    }

    fn layer_stack(&self, id: LayerStackId) -> Result<Option<LayerStackRecord>> {
        self.db.layer_stack(id)
    }

    fn layer(&self, id: LayerId) -> Result<Option<LayerRecord>> {
        self.db.layer(id)
    }

    fn branch_fact(&self, id: BranchId) -> Result<Option<BranchFact>> {
        self.db.branch_fact(id)
    }

    fn branch(&self, id: BranchId) -> Result<Option<BranchRecord>> {
        self.db.branch(id)
    }

    fn commit(&self, id: CommitId) -> Result<Option<CommitRecord>> {
        self.db.commit(id)
    }

    fn layer_prefix_page(
        &self,
        through_layer_id: LayerId,
        cursor: Option<LayerId>,
        limit: u16,
    ) -> Result<LayerPrefixPage> {
        self.db.layer_prefix_page(through_layer_id, cursor, limit)
    }

    fn layer_ancestry_page(
        &self,
        through_layer_id: LayerId,
        stop_exclusive: Option<LayerId>,
        cursor: Option<LayerId>,
        limit: u16,
    ) -> Result<LayerPrefixPage> {
        self.db
            .layer_ancestry_page(through_layer_id, stop_exclusive, cursor, limit)
    }

    fn commit_history_page(
        &self,
        branch_id: BranchId,
        through_commit_id: CommitId,
        cursor: Option<CommitId>,
        limit: u16,
    ) -> Result<CommitHistoryPage> {
        self.db
            .commit_history_page(branch_id, through_commit_id, cursor, limit)
    }

    fn commit_ancestry_page(
        &self,
        through_commit_id: CommitId,
        stop_exclusive: Option<CommitId>,
        cursor: Option<CommitId>,
        limit: u16,
    ) -> Result<CommitHistoryPage> {
        self.db
            .commit_ancestry_page(through_commit_id, stop_exclusive, cursor, limit)
    }

    fn owned_commit_page(
        &self,
        branch_id: BranchId,
        through_commit_id: CommitId,
        cursor: Option<CommitId>,
        limit: u16,
    ) -> Result<CommitHistoryPage> {
        self.db
            .owned_commit_page(branch_id, through_commit_id, cursor, limit)
    }

    fn missing_objects(&self, ids: &[ObjectId]) -> Result<MissingBitmap> {
        self.db.missing_objects(ids)
    }

    fn object_membership(&self, ids: &[ObjectId]) -> Result<(MissingBitmap, Vec<Option<u64>>)> {
        self.db.object_membership(ids)
    }

    fn missing_facts(&self, facts: &[Fact]) -> Result<MissingBitmap> {
        self.db.missing_facts(facts)
    }

    fn admit_objects(&self, objects: &[CanonicalObject]) -> Result<AdmissionSetReceipt> {
        self.db.admit_objects(objects)
    }

    fn admit_facts(&self, facts: &[Fact]) -> Result<AdmissionSetReceipt> {
        self.db.admit_facts(facts)
    }

    fn publish_branch(
        &self,
        branch: &BranchRecord,
        observed_head: Option<CommitId>,
    ) -> Result<PushResult> {
        let started = std::time::Instant::now();
        let validated = self.validate_push(branch, observed_head);
        layerfs_storage::note_push_phase(
            layerfs_storage::PushPhase::AuthorityTransitionVerify,
            started.elapsed().as_nanos().min(u128::from(u64::MAX)) as u64,
        );
        validated?;
        let result = if branch.head_commit_id == branch.forked_from_commit_id {
            PushResult::NoChanges
        } else {
            self.db.authority_publish_branch(branch, observed_head)?
        };
        record_durability(self.db.stable_barrier()?)?;
        Ok(result)
    }

    fn add_layer(&self, branch_id: BranchId) -> Result<AuthorityAddResult> {
        LayerStackStore::add_layer(self, branch_id)
    }
}

impl LayerStackStore {
    fn validate_push(&self, branch: &BranchRecord, observed_head: Option<CommitId>) -> Result<()> {
        branch
            .fact()
            .validate_origin()
            .map_err(|_| StorageError::Integrity("pushed Branch origin"))?;
        self.db
            .layer_stack_fact(branch.layer_stack_id)?
            .ok_or(StorageError::Integrity("pushed Branch LayerStack"))?;
        let base = self
            .db
            .layer(branch.base_layer_id)?
            .ok_or(StorageError::Integrity("pushed Branch base Layer"))?;
        if base.layer_stack_id != branch.layer_stack_id {
            return Err(StorageError::Integrity("pushed Branch ownership"));
        }
        let head = branch
            .head_commit_id
            .ok_or(StorageError::InvalidInput("pushed Branch head"))?;
        let head_commit = self
            .db
            .commit(head)?
            .ok_or(StorageError::Integrity("pushed Branch head Commit"))?;
        if head_commit.base_layer_id != branch.base_layer_id {
            return Err(StorageError::Integrity("pushed Branch head base"));
        }

        self.validate_origin(branch)?;

        let existing = self.db.branch(branch.id)?;
        if let Some(existing) = &existing {
            if existing.fact() != branch.fact() {
                return Err(StorageError::Integrity("pushed Branch fact"));
            }
            if existing.head_commit_id == Some(head) {
                return Ok(());
            }
            if existing.head_commit_id != observed_head {
                return Ok(());
            }
        } else if observed_head.is_some() {
            return Err(StorageError::Integrity("Push observed head"));
        }

        let stop_exclusive = observed_head.or(branch.forked_from_commit_id);
        self.verify_owned_suffix(branch, head, stop_exclusive)
    }

    fn validate_origin(&self, branch: &BranchRecord) -> Result<()> {
        if let Some(layer_id) = branch.forked_from_layer_id {
            let layer = self
                .db
                .layer(layer_id)?
                .ok_or(StorageError::Integrity("pushed Branch Layer origin"))?;
            if layer.layer_stack_id != branch.layer_stack_id {
                return Err(StorageError::Integrity(
                    "pushed Branch Layer origin ownership",
                ));
            }
            return Ok(());
        }

        let source_branch_id = branch
            .forked_from_branch_id
            .ok_or(StorageError::Integrity("pushed Branch origin Branch"))?;
        let source_commit_id = branch
            .forked_from_commit_id
            .ok_or(StorageError::Integrity("pushed Branch origin Commit"))?;
        let source = self
            .db
            .branch_fact(source_branch_id)?
            .ok_or(StorageError::Integrity("pushed Branch origin Branch"))?;
        if source.layer_stack_id != branch.layer_stack_id {
            return Err(StorageError::Integrity("pushed Branch origin ownership"));
        }
        let origin = self
            .db
            .commit(source_commit_id)?
            .ok_or(StorageError::Integrity("pushed Branch origin Commit"))?;
        let origin_base = self
            .db
            .layer(origin.base_layer_id)?
            .ok_or(StorageError::Integrity("pushed Branch origin base Layer"))?;
        if origin_base.layer_stack_id != branch.layer_stack_id {
            return Err(StorageError::Integrity(
                "pushed Branch origin Commit ownership",
            ));
        }
        self.db
            .commit_history_page(source_branch_id, source_commit_id, None, 1)?;
        Ok(())
    }

    fn verify_owned_suffix(
        &self,
        branch: &BranchRecord,
        through_commit_id: CommitId,
        stop_exclusive: Option<CommitId>,
    ) -> Result<()> {
        let mut roots = OwnedRootIds::new(
            &self.db,
            branch.layer_stack_id,
            through_commit_id,
            stop_exclusive,
        );
        let suffix = roots.by_ref().collect::<Vec<_>>();
        roots.finish()?;
        let mut prior = if let Some(stop) = stop_exclusive {
            self.db
                .commit(stop)?
                .ok_or(StorageError::Integrity("pushed transition base Commit"))?
                .root_id
        } else {
            self.db
                .layer(
                    branch
                        .forked_from_layer_id
                        .ok_or(StorageError::Integrity("pushed transition base Layer"))?,
                )?
                .ok_or(StorageError::Integrity("pushed transition base Layer"))?
                .root_id
        };
        for root in suffix.into_iter().rev() {
            self.db.verify_complete_transition(prior, root)?;
            prior = root;
        }
        Ok(())
    }

    pub(crate) fn verify_complete(&self, root: ObjectId) -> Result<()> {
        self.db.verify_complete_roots([root])
    }
}

struct OwnedRootIds<'a> {
    db: &'a StoreDb,
    layer_stack_id: LayerStackId,
    through: CommitId,
    stop_exclusive: Option<CommitId>,
    cursor: Option<CommitId>,
    records: std::vec::IntoIter<CommitRecord>,
    terminal_parent: Option<CommitId>,
    done: bool,
    error: Option<StorageError>,
}

impl<'a> OwnedRootIds<'a> {
    fn new(
        db: &'a StoreDb,
        layer_stack_id: LayerStackId,
        through: CommitId,
        stop_exclusive: Option<CommitId>,
    ) -> Self {
        Self {
            db,
            layer_stack_id,
            through,
            stop_exclusive,
            cursor: None,
            records: Vec::new().into_iter(),
            terminal_parent: None,
            done: false,
            error: None,
        }
    }

    fn finish(&mut self) -> Result<()> {
        if let Some(error) = self.error.take() {
            return Err(error);
        }
        if !self.done || self.terminal_parent != self.stop_exclusive {
            return Err(StorageError::Integrity("pushed Branch owned suffix"));
        }
        Ok(())
    }

    fn refill(&mut self) -> Result<()> {
        let page =
            self.db
                .commit_ancestry_page(self.through, self.stop_exclusive, self.cursor, 128)?;
        if page.records.is_empty() {
            if self.cursor.is_some() || self.stop_exclusive != Some(self.through) {
                return Err(StorageError::Integrity("empty pushed Commit page"));
            }
            self.terminal_parent = self.stop_exclusive;
            self.done = true;
            return Ok(());
        }
        self.terminal_parent = page
            .records
            .last()
            .and_then(|commit| commit.parent_commit_id);
        match page.continuation {
            Some(next) => self.cursor = Some(next),
            None => self.done = true,
        }
        self.records = page.records.into_iter();
        Ok(())
    }
}

impl Iterator for OwnedRootIds<'_> {
    type Item = ObjectId;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if let Some(commit) = self.records.next() {
                let base = match self.db.layer(commit.base_layer_id) {
                    Ok(Some(base)) => base,
                    Ok(None) => {
                        self.error = Some(StorageError::Integrity("pushed Commit base Layer"));
                        return None;
                    }
                    Err(error) => {
                        self.error = Some(error);
                        return None;
                    }
                };
                if base.layer_stack_id != self.layer_stack_id {
                    self.error = Some(StorageError::Integrity("pushed Commit ownership"));
                    return None;
                }
                return Some(commit.root_id);
            }
            if self.done {
                return None;
            }
            if let Err(error) = self.refill() {
                self.error = Some(error);
                return None;
            }
        }
    }
}

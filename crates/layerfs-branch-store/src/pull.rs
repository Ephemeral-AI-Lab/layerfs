use crate::BranchStore;
use layerfs_content::ObjectId;
use layerfs_storage::{
    BranchId, BranchRecord, BranchScope, BranchScopeRecord, CommitId, Fact, LayerId, LayerRecord,
    LayerStackEndpoint, LayerStackScopeRecord, PullBranchResult, PullLayerResult, RemotePlacement,
    Result, RootTransferRequest, StorageError, StoreDb, TransferPipeline, FACT_BATCH_BYTES,
    FACT_BATCH_COUNT,
};
use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

const HISTORY_PAGE: u16 = 128;

impl BranchStore {
    pub fn pull_layer(
        &self,
        parent: Arc<dyn LayerStackEndpoint>,
        through_layer_id: LayerId,
        placement: RemotePlacement,
    ) -> Result<PullLayerResult> {
        let _operation = self.db.enter_operation()?;
        self.check_parent(parent.as_ref())?;
        let through = parent
            .layer(through_layer_id)?
            .ok_or(StorageError::NotFound("authority Layer"))?;
        if through.id != through_layer_id {
            return Err(StorageError::Integrity("authority Layer key"));
        }
        let stack = parent
            .layer_stack(through.layer_stack_id)?
            .ok_or(StorageError::Integrity("authority LayerStack"))?;
        if stack.id != through.layer_stack_id {
            return Err(StorageError::Integrity("LayerStack ownership"));
        }
        if self
            .db
            .layer_stack_fact(stack.id)?
            .is_some_and(|known| known != stack.fact())
        {
            return Err(StorageError::Integrity("LayerStack fact collision"));
        }
        if self
            .db
            .layer(through.id)?
            .is_some_and(|known| known != through)
        {
            return Err(StorageError::Integrity("Layer fact collision"));
        }

        let current = self.db.layer_stack_scope(stack.id)?;
        if let Some(current) = current {
            if current.through_layer_id == through_layer_id {
                return if current.serving_mode == placement {
                    Ok(PullLayerResult::UpToDate {
                        through_layer_id,
                        placement,
                    })
                } else if placement == RemotePlacement::Reference {
                    self.db.publish_layer_stack_scope(LayerStackScopeRecord {
                        layer_stack_id: stack.id,
                        through_layer_id,
                        serving_mode: placement,
                    })?;
                    Ok(PullLayerResult::ModeChanged {
                        through_layer_id,
                        previous: current.serving_mode,
                        placement,
                    })
                } else {
                    self.acquire_layer_prefix(parent.as_ref(), &stack, through, current, placement)
                };
            }
            if self.local_layer_ancestor(
                through.layer_stack_id,
                through.id,
                current.through_layer_id,
            )? {
                return Ok(PullLayerResult::AlreadyContained {
                    current_layer_id: current.through_layer_id,
                    requested_layer_id: through.id,
                    placement: current.serving_mode,
                });
            }
        }
        self.acquire_layer_prefix_optional(parent.as_ref(), &stack, through, current, placement)
    }

    fn acquire_layer_prefix(
        &self,
        parent: &dyn LayerStackEndpoint,
        stack: &layerfs_storage::LayerStackRecord,
        through: LayerRecord,
        current: LayerStackScopeRecord,
        placement: RemotePlacement,
    ) -> Result<PullLayerResult> {
        let mut spool = FactSpool::new()?;
        self.spool_local_layer_prefix(through.id, &mut spool)?;
        spool.push(Fact::LayerStack(stack.fact()))?;
        self.admit_spooled_facts(&mut spool)?;
        self.complete_spooled_roots(parent, &mut spool, RootSelection::All)?;
        self.db.publish_layer_stack_scope(LayerStackScopeRecord {
            layer_stack_id: through.layer_stack_id,
            through_layer_id: through.id,
            serving_mode: placement,
        })?;
        Ok(PullLayerResult::ModeChanged {
            through_layer_id: through.id,
            previous: current.serving_mode,
            placement,
        })
    }

    fn acquire_layer_prefix_optional(
        &self,
        parent: &dyn LayerStackEndpoint,
        stack: &layerfs_storage::LayerStackRecord,
        through: LayerRecord,
        current: Option<LayerStackScopeRecord>,
        placement: RemotePlacement,
    ) -> Result<PullLayerResult> {
        let mut spool = FactSpool::new()?;
        let contains_current = self.spool_layer_prefix(
            parent,
            through,
            current.map(|scope| scope.through_layer_id),
            &mut spool,
        )?;

        let outcome = match current {
            None => BoundaryOutcome::Created,
            Some(scope) if scope.through_layer_id == through.id => {
                BoundaryOutcome::ModeChanged(scope.serving_mode)
            }
            Some(scope) if contains_current => BoundaryOutcome::Advanced(scope.through_layer_id),
            Some(scope) => {
                return Ok(PullLayerResult::HeadMoved {
                    current_layer_id: scope.through_layer_id,
                    requested_layer_id: through.id,
                });
            }
        };

        if current.is_some_and(|scope| {
            scope.serving_mode == RemotePlacement::Reference
                && placement == RemotePlacement::Replica
        }) {
            self.spool_local_layer_prefix(current.unwrap().through_layer_id, &mut spool)?;
        }
        spool.push(Fact::LayerStack(stack.fact()))?;

        self.admit_spooled_facts(&mut spool)?;
        if placement == RemotePlacement::Replica {
            self.complete_spooled_roots(parent, &mut spool, RootSelection::All)?;
        }
        self.db.publish_layer_stack_scope(LayerStackScopeRecord {
            layer_stack_id: through.layer_stack_id,
            through_layer_id: through.id,
            serving_mode: placement,
        })?;
        Ok(match outcome {
            BoundaryOutcome::Created => PullLayerResult::Created {
                through_layer_id: through.id,
                placement,
            },
            BoundaryOutcome::Advanced(previous_layer_id) => PullLayerResult::Advanced {
                previous_layer_id,
                through_layer_id: through.id,
                placement,
            },
            BoundaryOutcome::ModeChanged(previous) => PullLayerResult::ModeChanged {
                through_layer_id: through.id,
                previous,
                placement,
            },
        })
    }

    pub fn pull_branch(
        &self,
        parent: Arc<dyn LayerStackEndpoint>,
        branch_id: BranchId,
        through_commit_id: CommitId,
        placement: RemotePlacement,
    ) -> Result<PullBranchResult> {
        let _operation = self.db.enter_operation()?;
        self.check_parent(parent.as_ref())?;
        let authority = parent
            .branch(branch_id)?
            .ok_or(StorageError::NotFound("authority Branch"))?;
        if authority.id != branch_id {
            return Err(StorageError::Integrity("authority Branch key"));
        }
        authority
            .fact()
            .validate_origin()
            .map_err(|_| StorageError::Integrity("authority Branch origin"))?;
        let through = parent
            .commit(through_commit_id)?
            .ok_or(StorageError::NotFound("authority Commit"))?;
        if through.id != through_commit_id {
            return Err(StorageError::Integrity("authority Commit key"));
        }
        let base = parent
            .layer(through.base_layer_id)?
            .ok_or(StorageError::Integrity("authority Commit base Layer"))?;
        if base.id != through.base_layer_id {
            return Err(StorageError::Integrity("authority Commit base Layer key"));
        }
        if base.layer_stack_id != authority.layer_stack_id {
            return Err(StorageError::Integrity("Branch LayerStack ownership"));
        }
        let stack = parent
            .layer_stack(authority.layer_stack_id)?
            .ok_or(StorageError::Integrity("authority LayerStack"))?;
        if stack.id != authority.layer_stack_id {
            return Err(StorageError::Integrity("authority LayerStack key"));
        }
        let current = self.db.branch_scope(branch_id)?;
        if self
            .db
            .branch_fact(branch_id)?
            .is_some_and(|known| known != authority.fact())
        {
            return Err(StorageError::Integrity("Branch fact collision"));
        }
        if self
            .db
            .layer_stack_fact(stack.id)?
            .is_some_and(|known| known != stack.fact())
        {
            return Err(StorageError::Integrity("LayerStack fact collision"));
        }
        if self
            .db
            .commit(through.id)?
            .is_some_and(|known| known != through)
        {
            return Err(StorageError::Integrity("Commit fact collision"));
        }
        if self.db.layer(base.id)?.is_some_and(|known| known != base) {
            return Err(StorageError::Integrity("Layer fact collision"));
        }
        if current.is_some_and(|scope| matches!(scope.scope, BranchScope::Local)) {
            return Err(StorageError::Integrity("local Branch ownership"));
        }
        let current_remote = current.and_then(|scope| match scope.scope {
            BranchScope::Remote {
                through_commit_id,
                serving_mode,
            } => Some((through_commit_id, serving_mode)),
            BranchScope::Local => None,
        });
        let current_base = current_remote
            .map(|_| {
                self.db
                    .branch(branch_id)?
                    .map(|branch| branch.base_layer_id)
                    .ok_or(StorageError::Integrity("remote Branch record"))
            })
            .transpose()?;
        if let Some((current_id, current_mode)) = current_remote {
            if current_id == through_commit_id {
                return if current_mode == placement {
                    self.reconcile_required_layer_scope_from_local(
                        parent.as_ref(),
                        &stack,
                        through.base_layer_id,
                        placement,
                    )?;
                    Ok(PullBranchResult::UpToDate {
                        through_commit_id,
                        placement,
                    })
                } else if placement == RemotePlacement::Reference {
                    self.reconcile_required_layer_scope_from_local(
                        parent.as_ref(),
                        &stack,
                        through.base_layer_id,
                        placement,
                    )?;
                    let branch = selected_branch(&authority, through);
                    self.db.publish_remote_branch_scope(
                        &branch,
                        BranchScopeRecord {
                            branch_id,
                            scope: BranchScope::Remote {
                                through_commit_id,
                                serving_mode: placement,
                            },
                        },
                    )?;
                    Ok(PullBranchResult::ModeChanged {
                        through_commit_id,
                        previous: current_mode,
                        placement,
                    })
                } else {
                    self.acquire_existing_branch_replica(
                        parent.as_ref(),
                        authority,
                        through,
                        stack,
                        current_mode,
                    )
                };
            }
            if self.local_commit_ancestor(authority.layer_stack_id, through.id, current_id)? {
                return Ok(PullBranchResult::AlreadyContained {
                    current_commit_id: current_id,
                    requested_commit_id: through.id,
                    placement: current_mode,
                });
            }
        }
        let boundary = parent.commit_history_page(branch_id, through_commit_id, None, 1)?;
        if boundary.records.first().map(|record| record.id) != Some(through_commit_id) {
            return Err(StorageError::NotFound("Commit in authority Branch history"));
        }
        if boundary.records.as_slice() != [through] {
            return Err(StorageError::Integrity(
                "authority Commit point/page mismatch",
            ));
        }
        self.acquire_branch_history(
            parent.as_ref(),
            authority,
            through,
            stack,
            current_remote,
            current_base,
            placement,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn acquire_branch_history(
        &self,
        parent: &dyn LayerStackEndpoint,
        authority: BranchRecord,
        through: layerfs_storage::CommitRecord,
        stack: layerfs_storage::LayerStackRecord,
        current: Option<(CommitId, RemotePlacement)>,
        current_base: Option<LayerId>,
        placement: RemotePlacement,
    ) -> Result<PullBranchResult> {
        let mut spool = FactSpool::new()?;
        self.spool_origin_facts(parent, &authority, &mut spool)?;
        let contains_current = self.spool_commit_history(
            parent,
            through,
            authority.layer_stack_id,
            current.map(|(id, _)| id),
            &mut spool,
        )?;

        let outcome = match current {
            None => BoundaryOutcome::Created,
            Some((current_id, current_mode)) if current_id == through.id => {
                BoundaryOutcome::ModeChanged(current_mode)
            }
            Some((current_id, _)) if contains_current => BoundaryOutcome::Advanced(current_id),
            Some((current_id, current_mode))
                if self.local_commit_ancestor(
                    authority.layer_stack_id,
                    through.id,
                    current_id,
                )? =>
            {
                return Ok(PullBranchResult::AlreadyContained {
                    current_commit_id: current_id,
                    requested_commit_id: through.id,
                    placement: current_mode,
                });
            }
            Some((current_id, _)) => {
                return Ok(PullBranchResult::HeadMoved {
                    current_commit_id: current_id,
                    requested_commit_id: through.id,
                });
            }
        };

        if let Some((current_id, RemotePlacement::Reference)) = current {
            if placement == RemotePlacement::Replica {
                self.spool_local_commit_history(current_id, &mut spool)?;
            }
        }
        let base_contains_current = self.spool_layer_prefix(
            parent,
            through_base(parent, through)?,
            current_base,
            &mut spool,
        )?;
        if current_base.is_some() && !base_contains_current {
            return Err(StorageError::Integrity("Branch base Layer ancestry"));
        }
        if current.is_some_and(|(_, mode)| {
            mode == RemotePlacement::Reference && placement == RemotePlacement::Replica
        }) {
            self.spool_local_layer_prefix(
                current_base.ok_or(StorageError::Integrity("remote Branch base"))?,
                &mut spool,
            )?;
        }
        spool.push(Fact::LayerStack(stack.fact()))?;

        self.admit_spooled_facts(&mut spool)?;
        let (layer_scope, layer_completion) =
            self.required_layer_scope(through.base_layer_id, authority.layer_stack_id, placement)?;
        if placement == RemotePlacement::Reference && layer_completion {
            self.spool_local_layer_prefix(through.base_layer_id, &mut spool)?;
        }
        if placement == RemotePlacement::Replica {
            self.complete_spooled_roots(parent, &mut spool, RootSelection::All)?;
        } else if layer_completion {
            self.complete_spooled_roots(parent, &mut spool, RootSelection::Layers)?;
        }
        self.publish_required_layer_scope(layer_scope)?;
        let branch = selected_branch(&authority, through);
        self.db.publish_remote_branch_scope(
            &branch,
            BranchScopeRecord {
                branch_id: branch.id,
                scope: BranchScope::Remote {
                    through_commit_id: through.id,
                    serving_mode: placement,
                },
            },
        )?;
        Ok(match outcome {
            BoundaryOutcome::Created => PullBranchResult::Created {
                through_commit_id: through.id,
                placement,
            },
            BoundaryOutcome::Advanced(previous_commit_id) => PullBranchResult::Advanced {
                previous_commit_id,
                through_commit_id: through.id,
                placement,
            },
            BoundaryOutcome::ModeChanged(previous) => PullBranchResult::ModeChanged {
                through_commit_id: through.id,
                previous,
                placement,
            },
        })
    }

    fn acquire_existing_branch_replica(
        &self,
        parent: &dyn LayerStackEndpoint,
        authority: BranchRecord,
        through: layerfs_storage::CommitRecord,
        stack: layerfs_storage::LayerStackRecord,
        previous: RemotePlacement,
    ) -> Result<PullBranchResult> {
        let mut spool = FactSpool::new()?;
        self.spool_local_commit_history(through.id, &mut spool)?;
        self.spool_local_layer_prefix(through.base_layer_id, &mut spool)?;
        spool.push(Fact::LayerStack(stack.fact()))?;
        self.admit_spooled_facts(&mut spool)?;
        self.complete_spooled_roots(parent, &mut spool, RootSelection::All)?;
        let (layer_scope, _) = self.required_layer_scope(
            through.base_layer_id,
            authority.layer_stack_id,
            RemotePlacement::Replica,
        )?;
        self.publish_required_layer_scope(layer_scope)?;
        let branch = selected_branch(&authority, through);
        self.db.publish_remote_branch_scope(
            &branch,
            BranchScopeRecord {
                branch_id: branch.id,
                scope: BranchScope::Remote {
                    through_commit_id: through.id,
                    serving_mode: RemotePlacement::Replica,
                },
            },
        )?;
        Ok(PullBranchResult::ModeChanged {
            through_commit_id: through.id,
            previous,
            placement: RemotePlacement::Replica,
        })
    }

    fn reconcile_required_layer_scope_from_local(
        &self,
        parent: &dyn LayerStackEndpoint,
        stack: &layerfs_storage::LayerStackRecord,
        through_layer_id: LayerId,
        requested: RemotePlacement,
    ) -> Result<()> {
        let mut spool = FactSpool::new()?;
        self.spool_local_layer_prefix(through_layer_id, &mut spool)?;
        spool.push(Fact::LayerStack(stack.fact()))?;
        self.admit_spooled_facts(&mut spool)?;
        let (scope, completion) =
            self.required_layer_scope(through_layer_id, stack.id, requested)?;
        if completion {
            self.complete_spooled_roots(parent, &mut spool, RootSelection::Layers)?;
        }
        self.publish_required_layer_scope(scope)
    }

    fn required_layer_scope(
        &self,
        required: LayerId,
        layer_stack_id: layerfs_storage::LayerStackId,
        requested: RemotePlacement,
    ) -> Result<(LayerStackScopeRecord, bool)> {
        let Some(current) = self.db.layer_stack_scope(layer_stack_id)? else {
            return Ok((
                LayerStackScopeRecord {
                    layer_stack_id,
                    through_layer_id: required,
                    serving_mode: requested,
                },
                requested == RemotePlacement::Replica,
            ));
        };
        if current.through_layer_id == required {
            let serving_mode = strongest_mode(current.serving_mode, requested);
            return Ok((
                LayerStackScopeRecord {
                    serving_mode,
                    ..current
                },
                current.serving_mode == RemotePlacement::Reference
                    && serving_mode == RemotePlacement::Replica,
            ));
        }
        if self.local_layer_ancestor(layer_stack_id, required, current.through_layer_id)? {
            return Ok((current, false));
        }
        if self.local_layer_ancestor(layer_stack_id, current.through_layer_id, required)? {
            let serving_mode = strongest_mode(current.serving_mode, requested);
            return Ok((
                LayerStackScopeRecord {
                    layer_stack_id,
                    through_layer_id: required,
                    serving_mode,
                },
                serving_mode == RemotePlacement::Replica,
            ));
        }
        Err(StorageError::Integrity(
            "required LayerStack prefix ancestry",
        ))
    }

    fn publish_required_layer_scope(&self, scope: LayerStackScopeRecord) -> Result<()> {
        if self.db.layer_stack_scope(scope.layer_stack_id)? != Some(scope) {
            self.db.publish_layer_stack_scope(scope)?;
        }
        Ok(())
    }

    fn check_parent(&self, parent: &dyn LayerStackEndpoint) -> Result<()> {
        if parent.store_id()? == self.parent_store_id() {
            Ok(())
        } else {
            Err(StorageError::WrongParent)
        }
    }

    fn spool_layer_prefix(
        &self,
        parent: &dyn LayerStackEndpoint,
        through: LayerRecord,
        current: Option<LayerId>,
        spool: &mut FactSpool,
    ) -> Result<bool> {
        let stop_exclusive = current;
        let mut cursor = None;
        let mut expected = through.id;
        let mut first = true;
        loop {
            let page =
                parent.layer_ancestry_page(through.id, stop_exclusive, cursor, HISTORY_PAGE)?;
            if page.records.is_empty() {
                return if cursor.is_none() && stop_exclusive == Some(through.id) {
                    Ok(true)
                } else {
                    Err(StorageError::Integrity("empty Layer prefix page"))
                };
            }
            for layer in page.records {
                if layer.id != expected
                    || layer.layer_stack_id != through.layer_stack_id
                    || first && layer != through
                {
                    return Err(StorageError::Integrity("Layer prefix order"));
                }
                first = false;
                expected = layer.parent_layer_id.unwrap_or(layer.id);
                spool.push(Fact::Layer(layer))?;
            }
            match page.continuation {
                Some(next) if next == expected => cursor = Some(next),
                Some(_) => return Err(StorageError::Integrity("Layer prefix continuation")),
                None => return Ok(stop_exclusive == Some(expected)),
            }
        }
    }

    fn spool_commit_history(
        &self,
        parent: &dyn LayerStackEndpoint,
        through: layerfs_storage::CommitRecord,
        layer_stack_id: layerfs_storage::LayerStackId,
        current: Option<CommitId>,
        spool: &mut FactSpool,
    ) -> Result<bool> {
        let stop_exclusive = current;
        let mut cursor = None;
        let mut expected = through.id;
        let mut newer_base = None;
        let mut first = true;
        loop {
            let page =
                parent.commit_ancestry_page(through.id, stop_exclusive, cursor, HISTORY_PAGE)?;
            if page.records.is_empty() {
                return if cursor.is_none() && stop_exclusive == Some(through.id) {
                    Ok(true)
                } else {
                    Err(StorageError::Integrity("empty Commit history page"))
                };
            }
            for commit in page.records {
                if commit.id != expected || first && commit != through {
                    return Err(StorageError::Integrity("Commit history order"));
                }
                first = false;
                let base = parent
                    .layer(commit.base_layer_id)?
                    .ok_or(StorageError::Integrity("Commit base Layer"))?;
                if base.id != commit.base_layer_id || base.layer_stack_id != layer_stack_id {
                    return Err(StorageError::Integrity("Branch LayerStack ownership"));
                }
                if let Some(newer) = newer_base {
                    if base.id != newer && !remote_layer_ancestor(parent, base.id, newer)? {
                        return Err(StorageError::Integrity("Commit base Layer ancestry"));
                    }
                }
                newer_base = Some(base.id);
                expected = commit.parent_commit_id.unwrap_or(commit.id);
                spool.push(Fact::Commit(commit))?;
            }
            match page.continuation {
                Some(next) if next == expected => cursor = Some(next),
                Some(_) => return Err(StorageError::Integrity("Commit history continuation")),
                None => return Ok(stop_exclusive == Some(expected)),
            }
        }
    }

    fn spool_local_layer_prefix(&self, through: LayerId, spool: &mut FactSpool) -> Result<()> {
        let mut cursor = None;
        let mut expected = through;
        loop {
            let page = self.db.layer_prefix_page(through, cursor, HISTORY_PAGE)?;
            if page.records.is_empty() {
                return Err(StorageError::Integrity("empty local Layer prefix page"));
            }
            for layer in page.records {
                if layer.id != expected {
                    return Err(StorageError::Integrity("local Layer prefix order"));
                }
                expected = layer.parent_layer_id.unwrap_or(layer.id);
                spool.push(Fact::Layer(layer))?;
            }
            match page.continuation {
                Some(next) if next == expected => cursor = Some(next),
                Some(_) => return Err(StorageError::Integrity("local Layer continuation")),
                None => return Ok(()),
            }
        }
    }

    fn spool_local_commit_history(&self, through: CommitId, spool: &mut FactSpool) -> Result<()> {
        let mut cursor = None;
        let mut expected = through;
        loop {
            let page = self
                .db
                .commit_ancestry_page(through, None, cursor, HISTORY_PAGE)?;
            if page.records.is_empty() {
                return Err(StorageError::Integrity("empty local Commit history page"));
            }
            for commit in page.records {
                if commit.id != expected {
                    return Err(StorageError::Integrity("local Commit history order"));
                }
                expected = commit.parent_commit_id.unwrap_or(commit.id);
                spool.push(Fact::Commit(commit))?;
            }
            match page.continuation {
                Some(next) if next == expected => cursor = Some(next),
                Some(_) => return Err(StorageError::Integrity("local Commit continuation")),
                None => return Ok(()),
            }
        }
    }

    fn spool_origin_facts(
        &self,
        parent: &dyn LayerStackEndpoint,
        branch: &BranchRecord,
        spool: &mut FactSpool,
    ) -> Result<()> {
        validate_origin_chain(parent, branch)?;
        let mut fact = branch.fact();
        loop {
            spool.push(Fact::Branch(fact.clone()))?;
            let Some(source_id) = fact.forked_from_branch_id else {
                break;
            };
            let source_commit_id = fact
                .forked_from_commit_id
                .ok_or(StorageError::Integrity("Branch origin Commit"))?;
            let page = parent.commit_history_page(source_id, source_commit_id, None, 1)?;
            if page.records.first().map(|commit| commit.id) != Some(source_commit_id) {
                return Err(StorageError::Integrity("Branch origin history"));
            }
            let source_commit = parent
                .commit(source_commit_id)?
                .ok_or(StorageError::Integrity("Branch origin Commit"))?;
            if source_commit.id != source_commit_id || page.records.as_slice() != [source_commit] {
                return Err(StorageError::Integrity("Branch origin Commit point/page"));
            }
            let source = parent
                .branch(source_id)?
                .ok_or(StorageError::Integrity("Branch origin"))?;
            if source.id != source_id || source.layer_stack_id != branch.layer_stack_id {
                return Err(StorageError::Integrity("Branch origin LayerStack"));
            }
            fact = source.fact();
        }
        Ok(())
    }

    fn admit_spooled_facts(&self, spool: &mut FactSpool) -> Result<()> {
        let mut pipeline = TransferPipeline::new(&self.db)?;
        let mut batch = Vec::with_capacity(FACT_BATCH_COUNT);
        let mut bytes = 0;
        let mut kind = None;
        spool.visit_reverse(&mut |fact| {
            let encoded = fact.encoded_size();
            if !batch.is_empty()
                && (kind != Some(fact.kind())
                    || batch.len() == FACT_BATCH_COUNT
                    || bytes + encoded > FACT_BATCH_BYTES)
            {
                pipeline.facts(&batch)?;
                batch.clear();
                bytes = 0;
            }
            kind = Some(fact.kind());
            bytes += encoded;
            batch.push(fact);
            Ok(())
        })?;
        pipeline.facts(&batch)?;
        pipeline.finish()?;
        Ok(())
    }

    fn complete_spooled_roots(
        &self,
        parent: &dyn LayerStackEndpoint,
        spool: &mut FactSpool,
        selection: RootSelection,
    ) -> Result<()> {
        let mut requests = RootRequests::new(spool, &self.db, selection)?;
        let transferred = layerfs_storage::transfer_roots(parent, &self.db, &mut requests);
        requests.finish()?;
        transferred?;

        let mut roots = RootIds::new(spool, selection)?;
        let verified = self.db.verify_and_record_complete_roots(&mut roots);
        roots.finish()?;
        verified?;
        Ok(())
    }

    pub(crate) fn local_layer_ancestor(
        &self,
        layer_stack_id: layerfs_storage::LayerStackId,
        ancestor: LayerId,
        descendant: LayerId,
    ) -> Result<bool> {
        let mut cursor = descendant;
        loop {
            if cursor == ancestor {
                return Ok(true);
            }
            let layer = self
                .db
                .layer(cursor)?
                .ok_or(StorageError::Integrity("local Layer prefix"))?;
            if layer.layer_stack_id != layer_stack_id {
                return Err(StorageError::Integrity("Layer prefix ownership"));
            }
            let Some(parent) = layer.parent_layer_id else {
                return Ok(false);
            };
            cursor = parent;
        }
    }

    fn local_commit_ancestor(
        &self,
        layer_stack_id: layerfs_storage::LayerStackId,
        ancestor: CommitId,
        descendant: CommitId,
    ) -> Result<bool> {
        let mut cursor = descendant;
        loop {
            if cursor == ancestor {
                return Ok(true);
            }
            let commit = self
                .db
                .commit(cursor)?
                .ok_or(StorageError::Integrity("local Commit history"))?;
            let base = self
                .db
                .layer(commit.base_layer_id)?
                .ok_or(StorageError::Integrity("local Commit base Layer"))?;
            if base.layer_stack_id != layer_stack_id {
                return Err(StorageError::Integrity("Branch LayerStack ownership"));
            }
            let Some(parent) = commit.parent_commit_id else {
                return Ok(false);
            };
            cursor = parent;
        }
    }
}

#[derive(Clone, Copy)]
enum BoundaryOutcome<T> {
    Created,
    Advanced(T),
    ModeChanged(RemotePlacement),
}

#[derive(Clone, Copy)]
enum RootSelection {
    All,
    Layers,
}

fn strongest_mode(left: RemotePlacement, right: RemotePlacement) -> RemotePlacement {
    if left == RemotePlacement::Replica || right == RemotePlacement::Replica {
        RemotePlacement::Replica
    } else {
        RemotePlacement::Reference
    }
}

fn through_base(
    parent: &dyn LayerStackEndpoint,
    commit: layerfs_storage::CommitRecord,
) -> Result<LayerRecord> {
    let layer = parent
        .layer(commit.base_layer_id)?
        .ok_or(StorageError::Integrity("authority Commit base Layer"))?;
    if layer.id != commit.base_layer_id {
        return Err(StorageError::Integrity("authority Commit base Layer key"));
    }
    Ok(layer)
}

fn remote_layer_ancestor(
    parent: &dyn LayerStackEndpoint,
    ancestor: LayerId,
    descendant: LayerId,
) -> Result<bool> {
    let mut cursor = None;
    let mut expected = descendant;
    loop {
        let page = parent.layer_ancestry_page(descendant, Some(ancestor), cursor, HISTORY_PAGE)?;
        if page.records.is_empty() {
            return Ok(cursor.is_none() && ancestor == descendant);
        }
        for layer in page.records {
            if layer.id != expected {
                return Err(StorageError::Integrity("Layer ancestry order"));
            }
            expected = layer.parent_layer_id.unwrap_or(layer.id);
        }
        match page.continuation {
            Some(next) if next == expected => cursor = Some(next),
            Some(_) => return Err(StorageError::Integrity("Layer ancestry continuation")),
            None => return Ok(expected == ancestor),
        }
    }
}

fn selected_branch(
    authority: &BranchRecord,
    through: layerfs_storage::CommitRecord,
) -> BranchRecord {
    BranchRecord {
        id: authority.id,
        layer_stack_id: authority.layer_stack_id,
        name: authority.name.clone(),
        base_layer_id: through.base_layer_id,
        head_commit_id: Some(through.id),
        forked_from_layer_id: authority.forked_from_layer_id,
        forked_from_branch_id: authority.forked_from_branch_id,
        forked_from_commit_id: authority.forked_from_commit_id,
    }
}

fn validate_origin_chain(parent: &dyn LayerStackEndpoint, branch: &BranchRecord) -> Result<()> {
    let mut slow = next_origin(parent, branch.id, branch.layer_stack_id)?;
    let mut fast = slow
        .map(|id| next_origin(parent, id, branch.layer_stack_id))
        .transpose()?
        .flatten();
    while let (Some(slow_id), Some(fast_id)) = (slow, fast) {
        if slow_id == fast_id {
            return Err(StorageError::Integrity("Branch origin cycle"));
        }
        slow = next_origin(parent, slow_id, branch.layer_stack_id)?;
        fast = next_origin(parent, fast_id, branch.layer_stack_id)?
            .map(|id| next_origin(parent, id, branch.layer_stack_id))
            .transpose()?
            .flatten();
    }
    Ok(())
}

fn next_origin(
    parent: &dyn LayerStackEndpoint,
    branch_id: BranchId,
    layer_stack_id: layerfs_storage::LayerStackId,
) -> Result<Option<BranchId>> {
    let branch = parent
        .branch(branch_id)?
        .ok_or(StorageError::Integrity("Branch origin"))?;
    if branch.id != branch_id || branch.layer_stack_id != layer_stack_id {
        return Err(StorageError::Integrity("Branch origin LayerStack"));
    }
    Ok(branch.forked_from_branch_id)
}

pub(crate) struct FactSpool {
    file: File,
    path: PathBuf,
}

impl FactSpool {
    pub(crate) fn new() -> Result<Self> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "layerfs-pull-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        let file = OpenOptions::new()
            .create_new(true)
            .read(true)
            .write(true)
            .open(&path)?;
        Ok(Self { file, path })
    }

    pub(crate) fn push(&mut self, fact: Fact) -> Result<()> {
        let bytes = layerfs_storage::encode_fact(&fact);
        let length = u32::try_from(bytes.len())
            .map_err(|_| StorageError::Integrity("Pull fact spool size"))?;
        self.file.seek(SeekFrom::End(0))?;
        self.file.write_all(&bytes)?;
        self.file.write_all(&length.to_be_bytes())?;
        Ok(())
    }

    pub(crate) fn visit_reverse(
        &mut self,
        visitor: &mut dyn FnMut(Fact) -> Result<()>,
    ) -> Result<()> {
        let mut facts = self.reverse()?;
        for fact in &mut facts {
            visitor(fact?)?;
        }
        Ok(())
    }

    pub(crate) fn reverse(&mut self) -> Result<ReverseFacts<'_>> {
        self.file.flush()?;
        let cursor = self.file.seek(SeekFrom::End(0))?;
        Ok(ReverseFacts {
            file: &mut self.file,
            cursor,
        })
    }
}

impl Drop for FactSpool {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

pub(crate) struct ReverseFacts<'a> {
    file: &'a mut File,
    cursor: u64,
}

impl Iterator for ReverseFacts<'_> {
    type Item = Result<Fact>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.cursor == 0 {
            return None;
        }
        Some(self.read_next())
    }
}

impl ReverseFacts<'_> {
    fn read_next(&mut self) -> Result<Fact> {
        if self.cursor < 4 {
            return Err(StorageError::Integrity("Pull fact spool"));
        }
        self.file.seek(SeekFrom::Start(self.cursor - 4))?;
        let mut length = [0; 4];
        self.file.read_exact(&mut length)?;
        let length = u64::from(u32::from_be_bytes(length));
        if length > self.cursor - 4 {
            return Err(StorageError::Integrity("Pull fact spool"));
        }
        self.cursor -= length + 4;
        self.file.seek(SeekFrom::Start(self.cursor))?;
        let mut bytes = vec![0; length as usize];
        self.file.read_exact(&mut bytes)?;
        layerfs_storage::decode_fact(&bytes)
    }
}

struct RootRequests<'a> {
    facts: ReverseFacts<'a>,
    db: &'a StoreDb,
    selection: RootSelection,
    error: Option<StorageError>,
}

impl<'a> RootRequests<'a> {
    fn new(spool: &'a mut FactSpool, db: &'a StoreDb, selection: RootSelection) -> Result<Self> {
        Ok(Self {
            facts: spool.reverse()?,
            db,
            selection,
            error: None,
        })
    }

    fn finish(&mut self) -> Result<()> {
        self.error.take().map_or(Ok(()), Err)
    }
}

impl Iterator for RootRequests<'_> {
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
            let root_id = match (self.selection, fact) {
                (RootSelection::Layers, Fact::Commit(_)) => continue,
                (_, Fact::Layer(layer)) => layer.root_id,
                (_, Fact::Commit(commit)) => commit.root_id,
                (_, Fact::Branch(_) | Fact::LayerStack(_)) => continue,
            };
            return match self.db.complete_root(root_id) {
                Ok(known_complete) => Some(RootTransferRequest {
                    root_id,
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

struct RootIds<'a> {
    facts: ReverseFacts<'a>,
    selection: RootSelection,
    error: Option<StorageError>,
}

impl<'a> RootIds<'a> {
    fn new(spool: &'a mut FactSpool, selection: RootSelection) -> Result<Self> {
        Ok(Self {
            facts: spool.reverse()?,
            selection,
            error: None,
        })
    }

    fn finish(&mut self) -> Result<()> {
        self.error.take().map_or(Ok(()), Err)
    }
}

impl Iterator for RootIds<'_> {
    type Item = ObjectId;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let fact = match self.facts.next()? {
                Ok(fact) => fact,
                Err(error) => {
                    self.error = Some(error);
                    return None;
                }
            };
            match (self.selection, fact) {
                (RootSelection::Layers, Fact::Commit(_)) => {}
                (_, Fact::Layer(layer)) => return Some(layer.root_id),
                (_, Fact::Commit(commit)) => return Some(commit.root_id),
                (_, Fact::Branch(_) | Fact::LayerStack(_)) => {}
            }
        }
    }
}

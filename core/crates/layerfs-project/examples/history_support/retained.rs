//! Real v4 C5 fork/stage/commit/layer transitions on supplied shared authority.
#![allow(dead_code)]
use layerfs_content::{filesystem::root::profile_id, InodeScope, ObjectId};
use layerfs_history::{
    AddLayerOutcome, AddLayerRequest, BranchId, CommitId, CommitStagedOutcome, CommitStagedRequest,
    ForkRequest, ForkSource, HistoryCatalog, HistoryCatalogConfig, HistoryError, HistoryName,
    HistoryResult, LayerId, LayerStackId, StackInitialization, StageRequest, WorkspaceId,
};
pub fn config() -> HistoryCatalogConfig {
    HistoryCatalogConfig {
        binding_key: b"layerfs/issue286/retained-history/v3".to_vec(),
        incarnation: 3,
        cursor_key: [0x28; 32],
    }
}
fn stack() -> LayerStackId {
    LayerStackId::from_authority([0x28; 16])
}
fn branch(ordinal: usize) -> BranchId {
    let mut body = [0x29; 16];
    body[15] = ordinal as u8;
    BranchId::from_authority(body)
}
fn workspace() -> WorkspaceId {
    WorkspaceId::from_authority([0x2a; 32]).expect("fixed nonzero authority")
}

/// C5 identity returned by one timed state publication.
#[derive(Clone, Copy, Debug)]
pub struct RetainedState {
    /// Genesis has no Commit; all subsequent states have one.
    pub commit: Option<CommitId>,
    /// Every selected state has a persisted Layer.
    pub layer: LayerId,
}

/// One fresh catalog, owned by the measured invocation rather than preparation.
pub struct RetainedHistory<'a> {
    catalog: &'a dyn HistoryCatalog,
    head: LayerId,
    next_ordinal: usize,
}

impl<'a> RetainedHistory<'a> {
    /// Creates the catalog, genesis Layer and Branch inside state 1's timer.
    pub fn create(
        catalog: &'a dyn HistoryCatalog,
        scope: InodeScope,
        root: ObjectId,
    ) -> HistoryResult<Self> {
        let initialized = catalog.initialize_layerstack(&StackInitialization {
            stack: stack(),
            name: HistoryName::new("retained")?,
            scope: scope.object(),
            profile: profile_id(),
            genesis_root: root,
        })?;
        catalog.fork(&ForkRequest {
            stack: stack(),
            branch: branch(1),
            name: HistoryName::new("state-1")?,
            source: ForkSource::Layer(initialized.head_layer),
        })?;
        Ok(Self {
            catalog,
            head: initialized.head_layer,
            next_ordinal: 2,
        })
    }

    /// Returns genesis without creating a duplicate first-state Commit.
    pub fn genesis(&self) -> RetainedState {
        RetainedState {
            commit: None,
            layer: self.head,
        }
    }

    /// Saves one real retained state through public C5 transitions, without retry.
    pub fn publish(&mut self, ordinal: usize, root: ObjectId) -> HistoryResult<RetainedState> {
        if ordinal != self.next_ordinal {
            return Err(HistoryError::InvalidInput("retained state order"));
        }
        // C5 Branch bases are immutable. Each publication forks the prior Layer.
        self.catalog.fork(&ForkRequest {
            stack: stack(),
            branch: branch(ordinal),
            name: HistoryName::new(&format!("state-{ordinal}"))?,
            source: ForkSource::Layer(self.head),
        })?;
        let old = self
            .catalog
            .branch_snapshot(branch(ordinal))?
            .ok_or(HistoryError::Integrity("retained Branch"))?;
        let stage = self.catalog.stage_changes(&StageRequest {
            workspace: workspace(),
            branch: branch(ordinal),
            expected_head: old.branch.head_commit,
            expected_base: old.branch.base_layer,
            expected_root: old.effective_root,
            construction_base_root: old.effective_root,
            intended_commit_base: old.branch.base_layer,
            candidate_root: root,
            profile: old.profile,
            scope: old.scope,
            generation: ordinal as u64,
        })?;
        let commit = match self.catalog.commit_staged(&CommitStagedRequest {
            workspace: workspace(),
            token: stage.token,
        })? {
            CommitStagedOutcome::Committed(commit) => commit,
            _ => {
                return Err(HistoryError::Integrity(
                    "retained state requires a new Commit",
                ))
            }
        };
        let layer = match self.catalog.add_layer(&AddLayerRequest {
            stack: stack(),
            branch: branch(ordinal),
            commit: commit.id,
            expected_stack_head: self.head,
            expected_branch_base: old.branch.base_layer,
        })? {
            AddLayerOutcome::Added(layer) => layer,
            _ => {
                return Err(HistoryError::Integrity(
                    "retained state requires a new Layer",
                ))
            }
        };
        self.head = layer.id;
        self.next_ordinal += 1;
        Ok(RetainedState {
            commit: Some(commit.id),
            layer: layer.id,
        })
    }
}

/// Queries EVERY supplied root through public C5 APIs.
/// The caller owns reopening an independent read-only session.
/// This proves C5 custody against the performance roots; independent O1 pins
/// and the corpus tree/byte oracle remain separate mandatory checks.
pub fn verify(
    catalog: &dyn HistoryCatalog,
    scope: InodeScope,
    roots: &[ObjectId],
) -> HistoryResult<usize> {
    let first = *roots
        .first()
        .ok_or(HistoryError::InvalidInput("retained roots"))?;
    let genesis = LayerId::derive(stack(), None, first);
    let mut parent = None;
    let mut last_commit = None;
    let mut last_base = genesis;
    for (index, root) in roots.iter().copied().enumerate() {
        let id = LayerId::derive(stack(), parent, root);
        let layer = catalog
            .layer(id)?
            .ok_or(HistoryError::Integrity("retained Layer"))?;
        if layer.root != root || layer.stack != stack() || layer.parent != parent {
            return Err(HistoryError::Integrity("retained Layer context"));
        }
        let source = if index == 0 {
            None
        } else {
            let base = parent.expect("non-genesis has a parent");
            let commit_id = CommitId::derive(root, None, base);
            let commit = catalog
                .commit(commit_id)?
                .ok_or(HistoryError::Integrity("retained Commit"))?;
            if commit.root != root
                || commit.stack != stack()
                || commit.parent.is_some()
                || commit.base_layer != base
            {
                return Err(HistoryError::Integrity("retained Commit context"));
            }
            last_commit = Some(commit_id);
            last_base = base;
            Some(commit_id)
        };
        if layer.source_commit != source || layer.source_branch != source.map(|_| branch(index + 1))
        {
            return Err(HistoryError::Integrity("retained publication source"));
        }
        parent = Some(id);
    }
    let snapshot = catalog
        .branch_snapshot(branch(roots.len()))?
        .ok_or(HistoryError::Integrity("retained Branch"))?;
    let held_stack = catalog
        .layer_stack(stack())?
        .ok_or(HistoryError::Integrity("retained stack"))?;
    if snapshot.effective_root != *roots.last().expect("nonempty")
        || snapshot.branch.head_commit != last_commit
        || snapshot.branch.base_layer != last_base
        || snapshot.scope != scope.object()
        || snapshot.profile != profile_id()
        || held_stack.head_layer != parent.expect("nonempty")
        || catalog.stage(workspace())?.is_some()
    {
        return Err(HistoryError::Integrity("retained final context"));
    }
    Ok(roots.len())
}

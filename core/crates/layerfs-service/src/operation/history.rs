//! History composition: authorization already happened, this is the work.
//!
//! Every history command here follows the same shape. Read the metadata the
//! request names, check the frozen expectations the caller stated, do the C1/C2
//! work **outside** any history transaction, then publish exactly one short
//! metadata transition. A metadata-only command never starts a content save, and
//! no history transaction is held across construction or a save's finish.
//!
//! `commit` is not a second implementation: it calls the same staging body as
//! `stage_changes` and then the same commit body as `commit_staged`, under the
//! one service admission the request already took.

use super::{
    failure::{content, storage},
    filesystem::{self, PreparedUpdate},
    history_bootstrap,
    read::id,
};
use layerfs_bridge::contract::*;
use layerfs_content::filesystem::scope_for_seed;
use layerfs_content::object::inode_leaf::InodeKind;

use layerfs_history::{
    AddLayerOutcome, AddLayerRequest, BranchId, BranchRecord, BranchSnapshot, CommitHistoryRequest,
    CommitId, CommitRecord, CommitStagedOutcome, CommitStagedRequest, DiscardOutcome,
    DiscardRequest, ForkRequest, ForkSource, HistoryCatalog, HistoryError, HistoryName,
    LayerHistoryRequest, LayerId, LayerRecord, LayerStackId, LayerStackRecord, NamespaceManifest,
    Page, Reservation, ReserveRequest, StackInitialization, StageRecord, StageRequest, WorkspaceId,
};
use layerfs_storage::{SaveHandoff, Store, StoreProvider};
use layerfs_telemetry::timer::{Active, TimingScope};
use std::time::Instant;

/// Maps one typed history failure onto its wire class without parsing a message.
pub(crate) fn failure(error: HistoryError) -> Failure {
    let conflict = match &error {
        HistoryError::HeadMoved(state) => Some(HistoryConflict::BranchMoved {
            expected_head: state.expected_head.map(CommitId::to_bytes),
            actual_head: state.actual_head.map(CommitId::to_bytes),
            expected_base: state.expected_base.to_bytes(),
            actual_base: state.actual_base.to_bytes(),
        }),
        HistoryError::StackMoved { expected, actual } => Some(HistoryConflict::StackMoved {
            expected: expected.to_bytes(),
            actual: actual.to_bytes(),
        }),
        HistoryError::BaseMismatch {
            commit_base,
            branch_base,
        } => Some(HistoryConflict::BaseMismatch {
            commit_base: commit_base.to_bytes(),
            branch_base: branch_base.to_bytes(),
        }),
        HistoryError::StageChanged { expected, actual } => Some(HistoryConflict::StageChanged {
            expected: expected.value(),
            actual: actual.map(|token| token.value()),
        }),
        _ => None,
    };
    let code = match error {
        HistoryError::WithStage { cause, stage } => {
            let mut failure = failure(*cause);
            let context = failure.history.get_or_insert_with(Default::default);
            context.stage = match stage {
                layerfs_history::error::StageDisposition::Absent(workspace) => {
                    StageObservation::Absent(workspace.to_bytes())
                }
                layerfs_history::error::StageDisposition::Retained(stage) => {
                    StageObservation::Retained(Box::new(stage_wire(&stage)))
                }
                layerfs_history::error::StageDisposition::AcknowledgedUnknown(stage) => {
                    StageObservation::AcknowledgedUnknown(Box::new(stage_wire(&stage)))
                }
            };
            return failure;
        }
        HistoryError::InvalidInput(_) => Code::InvalidInput,
        HistoryError::Missing(_) | HistoryError::NotInHistory(_) => Code::NotFound,
        HistoryError::Unsupported(_) => Code::Unsupported,
        HistoryError::Busy => Code::Busy,
        HistoryError::OwnershipUnavailable => Code::Ownership,
        HistoryError::Capacity(_) => Code::Capacity,
        HistoryError::Integrity(_) => Code::Integrity,
        HistoryError::HeadMoved(_)
        | HistoryError::BaseMismatch { .. }
        | HistoryError::StackMoved { .. } => Code::HeadMoved,
        HistoryError::StageChanged { .. } => Code::StageChanged,
        HistoryError::ContinuityUnavailable => Code::ContinuityUnavailable,
        HistoryError::UnknownOutcome => Code::Unknown,
    };
    let mut result = Failure::from(code);
    if conflict.is_some() {
        result.history = Some(Box::new(HistoryFailure {
            conflict,
            stage: StageObservation::Unobserved,
        }));
    }
    result
}

/// Answers one read-only history query. No content save is started.
pub(crate) fn query(
    catalog: &dyn HistoryCatalog,
    query: &HistoryQuery,
    store: &Store,
) -> Result<Response, Failure> {
    let result = match query {
        HistoryQuery::GetStack { stack } => HistoryResult::Stack(stack_wire(
            &catalog
                .layer_stack(stack_id(stack)?)
                .map_err(failure)?
                .ok_or(Code::NotFound)?,
        )),
        HistoryQuery::ListStacks { cursor, limit } => {
            let page = catalog
                .layer_stacks(&Page {
                    cursor: cursor_option(cursor),
                    limit: *limit,
                })
                .map_err(failure)?;
            HistoryResult::Stacks {
                continuation: page.continuation.unwrap_or_default(),
                records: page.records.iter().map(stack_wire).collect(),
            }
        }
        HistoryQuery::GetBranch { branch } => {
            let snapshot = catalog
                .branch_snapshot(branch_id(branch)?)
                .map_err(failure)?
                .ok_or(Code::NotFound)?;
            let provider = StoreProvider::new(store);
            let fs = layerfs_content::FilesystemRead::new(
                &provider,
                layerfs_content::filesystem::root::FilesystemRootId(snapshot.effective_root),
            )
            .map_err(content)?;
            if snapshot.profile != fs.root().profile()
                || fs.root().scope().object() != snapshot.scope
            {
                return Err(Code::Integrity.into());
            }
            let mut wire = snapshot_wire(&snapshot);
            wire.root_serial = Some(fs.root().root_inode().serial());
            HistoryResult::BranchSnapshot(wire)
        }
        HistoryQuery::ListBranches {
            stack,
            cursor,
            limit,
        } => {
            let page = catalog
                .branches(
                    stack_id(stack)?,
                    &Page {
                        cursor: cursor_option(cursor),
                        limit: *limit,
                    },
                )
                .map_err(failure)?;
            HistoryResult::Branches {
                continuation: page.continuation.unwrap_or_default(),
                records: page.records.iter().map(branch_wire).collect(),
            }
        }
        HistoryQuery::GetCommit { commit } => HistoryResult::Commit(commit_wire(
            &catalog
                .commit(commit_id(commit)?)
                .map_err(failure)?
                .ok_or(Code::NotFound)?,
        )),
        HistoryQuery::CommitHistory {
            branch,
            start,
            cursor,
            limit,
        } => {
            let page = catalog
                .commit_history(&CommitHistoryRequest {
                    branch: branch_id(branch)?,
                    start: optional_commit(start)?,
                    cursor: cursor_option(cursor),
                    limit: *limit,
                })
                .map_err(failure)?;
            HistoryResult::Commits {
                continuation: page.continuation.unwrap_or_default(),
                records: page.records.iter().map(commit_wire).collect(),
            }
        }
        HistoryQuery::GetLayer { layer } => HistoryResult::Layer(layer_wire(
            &catalog
                .layer(layer_id(layer)?)
                .map_err(failure)?
                .ok_or(Code::NotFound)?,
        )),
        HistoryQuery::LayerHistory {
            stack,
            start,
            cursor,
            limit,
        } => {
            let page = catalog
                .layer_history(&LayerHistoryRequest {
                    stack: stack_id(stack)?,
                    start: optional_layer(start)?,
                    cursor: cursor_option(cursor),
                    limit: *limit,
                })
                .map_err(failure)?;
            HistoryResult::Layers {
                continuation: page.continuation.unwrap_or_default(),
                records: page.records.iter().map(layer_wire).collect(),
            }
        }
        HistoryQuery::GetStage { workspace } => HistoryResult::Stage(stage_wire(
            &catalog
                .stage(workspace_id(workspace)?)
                .map_err(failure)?
                .ok_or(Code::NotFound)?,
        )),
        HistoryQuery::ListStages {
            branch,
            cursor,
            limit,
        } => {
            let page = catalog
                .stages(
                    branch_id(branch)?,
                    &Page {
                        cursor: cursor_option(cursor),
                        limit: *limit,
                    },
                )
                .map_err(failure)?;
            HistoryResult::Stages {
                continuation: page.continuation.unwrap_or_default(),
                records: page.records.iter().map(stage_wire).collect(),
            }
        }
    };
    Ok(Response::History(Box::new(result)))
}

/// Runs one mutating history command.
pub(crate) fn command(
    catalog: &dyn HistoryCatalog,
    store: &Store,
    command: &HistoryCommand,
    deadline: Instant,
    timer: &TimingScope<'_, Active>,
) -> Result<Response, Failure> {
    let result = match command {
        HistoryCommand::InitLayerStack {
            stack,
            name,
            scope_seed,
            manifest,
        } => {
            let stack = LayerStackId::from_authority(*stack);
            let name = name_of(name)?;
            let scope = scope_for_seed(*scope_seed);
            let manifest = manifest_of(manifest)?;
            manifest.check().map_err(|_| Code::InvalidInput)?;
            let reservation = catalog
                .reserve_inodes(&ReserveRequest {
                    scope: scope.object(),
                    count: u64::try_from(manifest.entries.len()).map_err(|_| Code::Capacity)?,
                })
                .map_err(failure)?;
            let profile = layerfs_content::filesystem::profile_id();
            let provider = StoreProvider::new(store);
            let root = history_bootstrap::build_namespace(
                store,
                &provider,
                scope,
                reservation.start,
                &manifest,
                deadline,
                timer,
            )?;
            let record = catalog
                .initialize_layerstack(&StackInitialization {
                    stack,
                    name,
                    scope: scope.object(),
                    profile,
                    genesis_root: root,
                })
                .map_err(failure)?;
            HistoryResult::StackCreated(StackCreatedWire {
                stack: stack_wire(&record),
                root: *root.as_bytes(),
                root_serial: reservation.start,
            })
        }
        HistoryCommand::Fork {
            stack,
            branch,
            name,
            source,
        } => {
            let snapshot = catalog
                .fork(&ForkRequest {
                    stack: stack_id(stack)?,
                    branch: BranchId::from_authority(*branch),
                    name: name_of(name)?,
                    source: match source {
                        HistoryForkSource::Layer(layer) => ForkSource::Layer(layer_id(layer)?),
                        HistoryForkSource::Commit { branch, commit } => ForkSource::Commit {
                            branch: branch_id(branch)?,
                            commit: commit_id(commit)?,
                        },
                    },
                })
                .map_err(failure)?;
            HistoryResult::BranchSnapshot(snapshot_wire(&snapshot))
        }
        HistoryCommand::StageChanges(changes) => {
            let stage = stage(catalog, store, changes, deadline, timer)?;
            HistoryResult::Stage(stage_wire(&stage))
        }
        HistoryCommand::Commit(changes) => {
            let stage = stage(catalog, store, changes, deadline, timer)?;
            let outcome = catalog
                .commit_staged(&CommitStagedRequest {
                    workspace: stage.workspace,
                    token: stage.token,
                })
                .map_err(|error| {
                    let mut failure = failure(error);
                    let context = failure.history.get_or_insert_with(Default::default);
                    if context.stage == StageObservation::Unobserved {
                        context.stage =
                            StageObservation::AcknowledgedUnknown(Box::new(stage_wire(&stage)));
                    }
                    failure
                })?;
            commit_outcome(outcome)
        }
        HistoryCommand::CommitStaged { workspace, token } => {
            let outcome = catalog
                .commit_staged(&CommitStagedRequest {
                    workspace: workspace_id(workspace)?,
                    token: layerfs_history::StageToken::new(*token).map_err(failure)?,
                })
                .map_err(failure)?;
            commit_outcome(outcome)
        }
        HistoryCommand::AddLayer {
            stack,
            branch,
            commit,
            expected_stack_head,
            expected_branch_base,
        } => {
            let outcome = catalog
                .add_layer(&AddLayerRequest {
                    stack: stack_id(stack)?,
                    branch: branch_id(branch)?,
                    commit: commit_id(commit)?,
                    expected_stack_head: layer_id(expected_stack_head)?,
                    expected_branch_base: layer_id(expected_branch_base)?,
                })
                .map_err(failure)?;
            HistoryResult::Published(match outcome {
                AddLayerOutcome::Added(record) => LayerOutcomeWire::Added(layer_wire(&record)),
                AddLayerOutcome::UpToDate { layer } => LayerOutcomeWire::UpToDate {
                    layer: layer.to_bytes(),
                },
                AddLayerOutcome::NoChanges { head } => LayerOutcomeWire::NoChanges {
                    head: head.to_bytes(),
                },
            })
        }
        HistoryCommand::DiscardStage { workspace, token } => {
            let outcome = catalog
                .discard_stage(&DiscardRequest {
                    workspace: workspace_id(workspace)?,
                    token: layerfs_history::StageToken::new(*token).map_err(failure)?,
                })
                .map_err(failure)?;
            HistoryResult::Discarded {
                removed: matches!(outcome, DiscardOutcome::Removed),
            }
        }
        HistoryCommand::ReserveInodes { scope, count } => {
            let reservation: Reservation = catalog
                .reserve_inodes(&ReserveRequest {
                    scope: id(scope),
                    count: *count,
                })
                .map_err(failure)?;
            HistoryResult::Reservation {
                scope: *reservation.scope.as_bytes(),
                start: reservation.start,
                count: reservation.count,
            }
        }
    };
    Ok(Response::History(Box::new(result)))
}

/// Saves one prepared update and freezes exactly one stage for it.
fn stage(
    catalog: &dyn HistoryCatalog,
    store: &Store,
    changes: &PreparedChanges,
    deadline: Instant,
    timer: &TimingScope<'_, Active>,
) -> Result<StageRecord, Failure> {
    let workspace = workspace_id(&changes.workspace)?;
    let branch = branch_id(&changes.branch)?;
    let snapshot = catalog
        .branch_snapshot(branch)
        .map_err(failure)?
        .ok_or(Code::NotFound)?;
    // A stale request is refused before any content work is attempted.
    if changes.expected_head != snapshot.branch.head_commit.map(CommitId::to_bytes)
        || changes.expected_base != snapshot.branch.base_layer.to_bytes()
    {
        return Err(failure(HistoryError::HeadMoved(Box::new(
            layerfs_history::error::MovedState {
                expected_head: optional_commit(&changes.expected_head)?,
                actual_head: snapshot.branch.head_commit,
                expected_base: layer_id(&changes.expected_base)?,
                actual_base: snapshot.branch.base_layer,
            },
        ))));
    }
    // Changing an expected token never rebases content: the construction base
    // must be the captured effective root, and the scope must be the stack's.
    if changes.base != *snapshot.effective_root.as_bytes() {
        return Err(Code::InvalidInput.into());
    }
    if id(&changes.scope) != snapshot.scope {
        return Err(Code::InvalidInput.into());
    }
    let provider = StoreProvider::new(store);
    for inode in &changes.inodes {
        let kind = InodeKind::from_code(changes_kind(inode.kind)?).map_err(content)?;
        history_bootstrap::validate_inode_role(
            &provider,
            kind,
            id(&inode.content),
            id(&inode.metadata),
            timer,
        )?;
    }
    if Instant::now() >= deadline {
        return Err(Code::Deadline.into());
    }
    let mut save = store
        .begin_save(timer.child("history.begin_save"))
        .map_err(storage)?;
    let prepared = PreparedUpdate {
        base: changes.base,
        scope: changes.scope,
        root_serial: changes.root_serial,
        directories: &changes.directories,
        inodes: &changes.inodes,
        new_directories: &changes.new_directories,
        directory_metadata: &changes.directory_metadata,
        new_file_serials: &changes.new_file_serials,
    };
    let built = {
        let mut handoff = SaveHandoff::new(&mut save);
        let result = filesystem::update(&provider, &prepared, &mut handoff, deadline, timer);
        let retained = handoff.take_failure();
        drop(handoff);
        match retained {
            Some(error) => Err(storage(error)),
            None => result,
        }
    };
    let built = built.and_then(|value| {
        if Instant::now() >= deadline {
            Err(Code::Deadline.into())
        } else {
            Ok(value)
        }
    });
    let (root, _) = match built {
        Ok(value) => {
            save.finish(timer.child("history.finish"))
                .map_err(storage)?;
            value
        }
        Err(mut error) => {
            if !error.unknown {
                if let Err(cleanup) = save.abort(timer.child("history.abort")) {
                    let cleanup = storage(cleanup);
                    error.cleanup = Some(cleanup.code);
                    error.unknown |= cleanup.unknown;
                }
            }
            return Err(error);
        }
    };
    catalog
        .stage_changes(&StageRequest {
            workspace,
            branch,
            expected_head: snapshot.branch.head_commit,
            expected_base: snapshot.branch.base_layer,
            expected_root: snapshot.effective_root,
            construction_base_root: id(&changes.base),
            intended_commit_base: snapshot.branch.base_layer,
            candidate_root: id(&root),
            profile: snapshot.profile,
            scope: snapshot.scope,
            generation: changes.generation,
        })
        .map_err(failure)
}

fn changes_kind(kind: u8) -> Result<u8, Failure> {
    match kind {
        1..=3 => Ok(kind),
        _ => Err(Code::InvalidInput.into()),
    }
}

fn commit_outcome(outcome: CommitStagedOutcome) -> layerfs_bridge::contract::HistoryResult {
    HistoryResult::Committed(match outcome {
        CommitStagedOutcome::Committed(record) => {
            CommitOutcomeWire::Committed(commit_wire(&record))
        }
        CommitStagedOutcome::UpToDate { head, root } => CommitOutcomeWire::UpToDate {
            head: head.map(CommitId::to_bytes),
            root: *root.as_bytes(),
        },
    })
}

fn cursor_option(cursor: &[u8]) -> Option<Vec<u8>> {
    (!cursor.is_empty()).then(|| cursor.to_vec())
}

fn name_of(name: &[u8]) -> Result<HistoryName, Failure> {
    let text = std::str::from_utf8(name).map_err(|_| Code::InvalidInput)?;
    HistoryName::new(text).map_err(failure)
}

fn workspace_id(bytes: &[u8; 32]) -> Result<WorkspaceId, Failure> {
    WorkspaceId::from_slice(bytes).map_err(failure)
}

fn stack_id(bytes: &[u8; 17]) -> Result<LayerStackId, Failure> {
    LayerStackId::from_slice(bytes).map_err(failure)
}

fn branch_id(bytes: &[u8; 17]) -> Result<BranchId, Failure> {
    BranchId::from_slice(bytes).map_err(failure)
}

fn commit_id(bytes: &[u8; 33]) -> Result<CommitId, Failure> {
    CommitId::from_slice(bytes).map_err(failure)
}

fn layer_id(bytes: &[u8; 33]) -> Result<LayerId, Failure> {
    LayerId::from_slice(bytes).map_err(failure)
}

fn optional_commit(bytes: &Option<[u8; 33]>) -> Result<Option<CommitId>, Failure> {
    bytes.as_ref().map(commit_id).transpose()
}

fn optional_layer(bytes: &Option<[u8; 33]>) -> Result<Option<LayerId>, Failure> {
    bytes.as_ref().map(layer_id).transpose()
}

fn manifest_of(entries: &[ManifestEntry]) -> Result<NamespaceManifest, Failure> {
    let entries = entries
        .iter()
        .map(|entry| {
            Ok(layerfs_history::ManifestEntry {
                parent: entry.parent,
                name: entry.name.clone(),
                kind: layerfs_history::RecordKind::from_code(entry.kind)
                    .map_err(|_| Code::InvalidInput)?,
                mode: entry.mode,
                mtime_seconds: entry.mtime_seconds,
                mtime_nanoseconds: entry.mtime_nanoseconds,
                content: entry.content.map(|root| id(&root)),
                target: (!entry.target.is_empty()).then(|| entry.target.clone()),
            })
        })
        .collect::<Result<Vec<_>, Failure>>()?;
    Ok(NamespaceManifest { entries })
}

fn stack_wire(record: &LayerStackRecord) -> StackWire {
    StackWire {
        stack: record.id.to_bytes(),
        name: record.name.as_str().as_bytes().to_vec(),
        scope: *record.scope.as_bytes(),
        profile: *record.profile.as_bytes(),
        head_layer: record.head_layer.to_bytes(),
    }
}

fn branch_wire(record: &BranchRecord) -> BranchWire {
    BranchWire {
        branch: record.id.to_bytes(),
        stack: record.stack.to_bytes(),
        name: record.name.as_str().as_bytes().to_vec(),
        base_layer: record.base_layer.to_bytes(),
        head_commit: record.head_commit.map(CommitId::to_bytes),
    }
}

fn snapshot_wire(snapshot: &BranchSnapshot) -> BranchSnapshotWire {
    BranchSnapshotWire {
        branch: branch_wire(&snapshot.branch),
        head_root: snapshot.head_root.map(|root| *root.as_bytes()),
        base_root: *snapshot.base_root.as_bytes(),
        effective_root: *snapshot.effective_root.as_bytes(),
        root_serial: None,
        scope: *snapshot.scope.as_bytes(),
        profile: *snapshot.profile.as_bytes(),
    }
}

fn commit_wire(record: &CommitRecord) -> CommitWire {
    CommitWire {
        commit: record.id.to_bytes(),
        stack: record.stack.to_bytes(),
        root: *record.root.as_bytes(),
        parent: record.parent.map(CommitId::to_bytes),
        base_layer: record.base_layer.to_bytes(),
    }
}

fn layer_wire(record: &LayerRecord) -> LayerWire {
    LayerWire {
        layer: record.id.to_bytes(),
        stack: record.stack.to_bytes(),
        parent: record.parent.map(LayerId::to_bytes),
        root: *record.root.as_bytes(),
        source_branch: record.source_branch.map(BranchId::to_bytes),
        source_commit: record.source_commit.map(CommitId::to_bytes),
    }
}

fn stage_wire(record: &StageRecord) -> StageWire {
    StageWire {
        workspace: record.workspace.to_bytes(),
        token: record.token.value(),
        stack: record.stack.to_bytes(),
        branch: record.branch.to_bytes(),
        expected_head: record.expected_head.map(CommitId::to_bytes),
        expected_base: record.expected_base.to_bytes(),
        expected_root: *record.expected_root.as_bytes(),
        construction_base_root: *record.construction_base_root.as_bytes(),
        intended_commit_base: record.intended_commit_base.to_bytes(),
        candidate_root: *record.candidate_root.as_bytes(),
        profile: *record.profile.as_bytes(),
        scope: *record.scope.as_bytes(),
        generation: record.generation,
    }
}

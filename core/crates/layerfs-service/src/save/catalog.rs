//! Catalog changes after service admission.
use crate::{
    error::{catalog as failure, content, storage},
    read::content::id,
    records::*,
    save::{
        filesystem::{self, PreparedUpdate},
        import::{namespace, scan},
        validation::validate_inode_role,
    },
};
use layerfs_bridge::contract::HistoryResult;
use layerfs_bridge::contract::*;
use layerfs_content::{filesystem::scope_for_seed, object::inode_leaf::InodeKind};
use layerfs_history::*;
use layerfs_storage::{SaveHandoff, Store, StoreProvider};
use layerfs_telemetry::timer::{Active, TimingScope};
use std::{io::Write, path::Path, time::Instant};

/// Runs one mutating history command.
pub(crate) fn command(
    catalog: &dyn HistoryCatalog,
    store: &Store,
    import_root: Option<&Path>,
    command: &HistoryCommand,
    deadline: Instant,
    timer: &TimingScope<'_, Active>,
    output: &mut dyn Write,
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
            let entries: Vec<_> = manifest
                .entries
                .iter()
                .map(namespace::PreparedEntry::from)
                .collect();
            let reservation = catalog
                .reserve_inodes(&ReserveRequest {
                    scope: scope.object(),
                    count: u64::try_from(entries.len()).map_err(|_| Code::Capacity)?,
                })
                .map_err(failure)?;
            let profile = layerfs_content::filesystem::profile_id();
            let provider = StoreProvider::new(store);
            let mut progress = namespace::ImportProgress::disabled(deadline);
            let root = namespace::build_namespace(
                store,
                &provider,
                scope,
                reservation.start,
                entries,
                false,
                &mut progress,
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
        HistoryCommand::ImportNativeDirectory {
            stack,
            name,
            scope_seed,
        } => {
            let source = import_root.ok_or(Code::Unsupported)?;
            let stack = LayerStackId::from_authority(*stack);
            let name = name_of(name)?;
            let scope = scope_for_seed(*scope_seed);
            let mut progress = namespace::ImportProgress::with_output(deadline, output);
            let entries = scan::scan_and_save(source, store, &mut progress, timer)?;
            let reservation = catalog
                .reserve_inodes(&ReserveRequest {
                    scope: scope.object(),
                    count: u64::try_from(entries.len()).map_err(|_| Code::Capacity)?,
                })
                .map_err(failure)?;
            let provider = StoreProvider::new(store);
            let root = namespace::build_namespace(
                store,
                &provider,
                scope,
                reservation.start,
                entries,
                true,
                &mut progress,
                timer,
            )?;
            let record = catalog
                .initialize_layerstack(&StackInitialization {
                    stack,
                    name,
                    scope: scope.object(),
                    profile: layerfs_content::filesystem::profile_id(),
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
        validate_inode_role(
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
        new_symlink_serials: &changes.new_symlink_serials,
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

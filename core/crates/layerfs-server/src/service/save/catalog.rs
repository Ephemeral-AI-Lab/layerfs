//! Catalog changes after typed service admission.
//!
//! Pure catalog commands hold their separate metadata allowance. The legacy
//! import/prepared/composite surfaces keep their whole-request C2 allowance;
//! completed content is retained through later catalog failure. Releasing that
//! allowance between phases requires the versioned complete result owner.
use crate::service::{
    construction::Construction,
    error::{catalog as failure, content, storage},
    read::content::id,
    records::*,
    save::{
        filesystem::{self, PreparedUpdate},
        import::{namespace, scan},
    },
};
use layerfs_bridge::contract::HistoryResult;
use layerfs_bridge::contract::*;
use layerfs_content::filesystem::{
    scope_for_seed,
    state::{StateScope, StateTable},
};
use layerfs_history::*;
use layerfs_storage::{SaveHandoff, Store, StoreProvider};
use layerfs_telemetry::timer::{Active, TimingScope};
use std::{
    io::{Read, Write},
    path::Path,
    time::Instant,
};

/// The body a command may read and the progress stream it writes.
pub(crate) struct Streams<'a> {
    /// The ordered body a prepared command declared, when it declared one.
    pub(crate) input: &'a mut dyn Read,
    /// Where a long command reports its progress.
    pub(crate) output: &'a mut dyn Write,
}

/// Runs one mutating history command.
pub(crate) fn command(
    catalog: &dyn HistoryCatalog,
    store: &Store,
    bindings: (Option<&Path>, &Construction),
    command: &HistoryCommand,
    streams: Streams<'_>,
    deadline: Instant,
    timer: &TimingScope<'_, Active>,
) -> Result<Response, Failure> {
    let (import_root, construction) = bindings;
    let Streams { input, output } = streams;
    let result = match command {
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
                &entries,
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
            let stage = stage(
                catalog,
                store,
                construction,
                changes,
                input,
                deadline,
                timer,
            )?;
            HistoryResult::Stage(stage_wire(&stage))
        }
        HistoryCommand::Commit(changes) => {
            let stage = stage(
                catalog,
                store,
                construction,
                changes,
                input,
                deadline,
                timer,
            )?;
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
    construction: &Construction,
    changes: &PreparedChanges,
    input: &mut dyn Read,
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
    // Every supplied root is checked against the base tree as its row arrives:
    // the rows of this update are a stream, so there is no earlier moment at
    // which the service holds them all to check them here.
    if Instant::now() >= deadline {
        return Err(Code::Deadline.into());
    }
    let mut state = construction.begin(changes)?;
    let state_scope =
        match StateScope::new(state.selection().clone(), 1, StateTable::DirectoryRoots) {
            Ok(scope) => scope,
            Err(error) => {
                let mut error = content(error);
                if let Err(cleanup) = state.release().map_err(storage) {
                    error.cleanup = Some(cleanup.code);
                    error.unknown |= cleanup.unknown;
                }
                return Err(error);
            }
        };
    if Instant::now() >= deadline {
        let mut error = Failure::from(Code::Deadline);
        if let Err(cleanup) = state.release().map_err(storage) {
            error.cleanup = Some(cleanup.code);
            error.unknown |= cleanup.unknown;
        }
        return Err(error);
    }
    let mut save = match store.begin_save(timer.child("history.begin_save")) {
        Ok(save) => save,
        Err(error) => {
            let mut error = storage(error);
            if let Err(cleanup) = state.release().map_err(storage) {
                error.cleanup = Some(cleanup.code);
                error.unknown |= cleanup.unknown;
            }
            return Err(error);
        }
    };
    let mut prepared = PreparedUpdate {
        base: changes.base,
        scope: changes.scope,
        root_serial: changes.root_serial,
        changes,
        body: input,
    };
    let built = {
        let mut handoff = SaveHandoff::new(&mut save);
        let result = {
            let mut adapter = state.adapter();
            filesystem::update(
                &provider,
                &mut prepared,
                &mut handoff,
                &mut adapter,
                &state_scope,
                deadline,
                timer,
            )
        };
        let retained = handoff.take_failure();
        drop(handoff);
        match retained.or_else(|| state.take_failure()) {
            Some(error) => Err(storage(error)),
            None => result,
        }
    };
    // Logical table completion does not refund native disk/FD/engine ownership.
    // The exact session closes/unlinks once before a known Save is finished.
    let built = if state.is_quarantined() {
        // Retention is this exact owner's required disposition, not a failed
        // cleanup attempt. Its capsule/credit survives without destructive I/O.
        let mut error = built.err().unwrap_or_else(|| Code::Unknown.into());
        error.unknown = true;
        Err(error)
    } else {
        match (built, state.release().map_err(storage)) {
            (Ok(value), Ok(())) => Ok(value),
            (Ok(_), Err(cleanup)) => Err(cleanup),
            (Err(error), Ok(())) => Err(error),
            (Err(mut error), Err(cleanup)) => {
                error.cleanup = Some(cleanup.code);
                error.unknown |= cleanup.unknown;
                Err(error)
            }
        }
    };
    drop(state);
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
            // Each owner decides its own custody. A metadata-only scratch
            // Unknown must not hide cleanup of a known unfinished content Save.
            // C2 abort already denies effects for its own quarantine/attempt.
            if let Err(cleanup) = save.abort(timer.child("history.abort")) {
                let cleanup = storage(cleanup);
                error.cleanup.get_or_insert(cleanup.code);
                error.unknown |= cleanup.unknown;
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

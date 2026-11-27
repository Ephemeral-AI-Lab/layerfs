//! Atomic filesystem calls over a private, indexed Workspace. One capture and
//! canonical construction publishes the whole change list; validation failures
//! close only that private overlay. An uncertain Commit keeps its Workspace in
//! the StoreService with its failure.
use super::mutation::Mutation;
use crate::history::lookup::{self as args, Answer, Arg};
use crate::read::{snapshot, visibility::Visibility};
use crate::store::{BindError, BindPhase};
use crate::store_service::NamedBindFailure;
use crate::StoreService;
use layerfs_content::compare::TreeTable;
use layerfs_content::filesystem::FilesystemRootId;
use layerfs_history::{BranchRecord, CommitOutcome, CommitVia, LayerStackRecord, WorkspaceId};
use layerfs_protocol::sdk::{SdkRefusal, SdkValue};
use std::sync::Arc;

pub fn path(value: &str) -> Result<String, SdkRefusal> {
    if value.len() > 4096
        || value.as_bytes().contains(&0)
        || !(value == "/"
            || (value.starts_with('/')
                && !value.ends_with('/')
                && value[1..]
                    .split('/')
                    .all(|s| !s.is_empty() && s != "." && s != "..")))
    {
        return Err(args::invalid(
            "expected an absolute path without empty, dot or parent components",
        ));
    }
    Ok(value.to_owned())
}

/// The changes of one call, pulled while they are applied, and the content
/// of each streamed write among them. Only one change and one piece are held
/// at a time. The source ends only after its whole input was received, so a
/// call whose input stops early is refused and commits nothing.
pub trait Source {
    /// The next change, in the engine's form; `None` after the last one.
    fn next(&mut self) -> Result<Option<SdkValue>, SdkRefusal>;
    /// The next piece of the streamed write last returned; `None` at its end.
    fn piece(&mut self) -> Result<Option<&[u8]>, SdkRefusal>;
}

/// The changes of a file call: listed in its arguments, or streamed.
pub enum Changes<'s, 'a> {
    /// Every change, read before the call.
    Listed(&'s [Arg<Change<'a>>]),
    /// Changes read from `parse`d records of a source as they are applied.
    Streamed {
        /// Where the records and pieces come from.
        source: &'s mut dyn Source,
        /// One record as its change.
        parse: for<'v> fn(&'v SdkValue) -> Arg<Change<'v>>,
    },
}

/// One change of a file call, with checked absolute paths.
pub enum Change<'a> {
    /// Replace the file's bytes, creating it with `mode` (default 0o644).
    /// An existing file keeps its mode unless `mode` is given.
    Write {
        path: String,
        bytes: &'a [u8],
        mode: Option<u32>,
    },
    /// [`Change::Write`] whose bytes follow the change in its source.
    Streamed { path: String, mode: Option<u32> },
    /// Replace the one occurrence of `old` with `new`, or with `all`
    /// every occurrence.
    Edit {
        path: String,
        old: &'a str,
        new: &'a str,
        all: bool,
    },
    /// Create the directory and its missing parents. A created `path` gets
    /// `mode` (default 0o755); missing parents get 0o755, and an existing
    /// directory keeps its mode, as `mkdir -p -m` does.
    Mkdir { path: String, mode: Option<u32> },
    /// Set the entry's permission bits.
    Chmod { path: String, mode: u32 },
    /// Remove the entry; a folder that is not empty only when `recursive`.
    Rm { path: String, recursive: bool },
    /// Copy the entry, recursively.
    Cp { from: String, to: String },
    /// Rename the entry.
    Mv { from: String, to: String },
}

/// Applies `changes` in order to a private Workspace on Branch `branch` of
/// `project` and commits them once, with `message` or the default one. A
/// change the caller refused is refused at its index, after the changes
/// before it. With `expected` (a Commit id or prefix), the head the
/// private Workspace bound to must be that Commit, else the call is
/// refused `HeadMoved` naming the head, before any change; a head that
/// moves after the bind merges as without it.
pub fn commit(
    service: &StoreService,
    project: &str,
    branch: Arg<&str>,
    changes: Changes<'_, '_>,
    message: Option<&str>,
    expected: Option<&str>,
) -> Answer {
    let store = &service.store;
    let (branch_name, workspace) = match (branch, expected) {
        // #818 W2: the bind finds the project and Branch by name with its
        // snapshot, in one read.
        (Ok(name), None) if name != crate::history::MAIN => {
            (name, bind_named(service, project, name)?)
        }
        (branch, expected) => {
            let (branch_name, branch, stack) =
                crate::telemetry::timed("files.resolve", |_| resolve(store, project, branch))?;
            let id = workspace_id()?;
            store.prepare_history_write().map_err(args::history)?;
            let workspace =
                crate::telemetry::timed("files.bind", |timing| service.bind(id, branch.id, timing))
                    .map_err(args::failed)?;
            if let Some(expected) = expected {
                let bound = workspace
                    .snapshot()
                    .map_err(crate::write::mutation::failure);
                let guard = bound.and_then(|base| {
                    let head = crate::removal::node_of(&base);
                    let what = format!("Branch {branch_name}");
                    crate::history::resolve::expect(store, &stack, Some(expected), &what, head)
                });
                if let Err(refusal) = guard {
                    close(service, &workspace)?;
                    return Err(refusal);
                }
            }
            (branch_name, workspace)
        }
    };
    let result = crate::telemetry::timed("files.change", |_| change(&workspace, changes))?;
    if result.as_ref().is_err_and(|e| e.code == "Unknown") {
        return result.map(|_| SdkValue::Null);
    }
    let replaced = *result.as_ref().unwrap_or(&0);
    if let Err(error) = result {
        close(service, &workspace)?;
        return Err(error);
    }
    let outcome = workspace.commit_captured_via(
        &crate::store::CommitOptions {
            force: false,
            message: Some(message.unwrap_or("Apply filesystem changes").to_owned()),
            expected: None,
        },
        CommitVia::Files,
    );
    let outcome = match outcome {
        Ok(outcome) => outcome,
        Err(failure) => {
            if failure.locally_settled
                && failure.local_error.is_none()
                && failure
                    .namespace
                    .as_ref()
                    .is_none_or(|held| !held.retained())
            {
                let code = match &failure.error {
                    crate::store::CommitError::History(
                        layerfs_history::HistoryError::HeadMoved(_),
                    ) => "HeadMoved",
                    crate::store::CommitError::History(layerfs_history::HistoryError::Busy) => {
                        "Busy"
                    }
                    // Spec 5: the project epoch moved (a removal raced this
                    // write) or is gone after compaction; the Commit is
                    // refused with the spec's `Conflict`, nothing written.
                    crate::store::CommitError::History(
                        layerfs_history::HistoryError::Conflict(_),
                    ) => "Conflict",
                    crate::store::CommitError::History(
                        layerfs_history::HistoryError::Capacity(_),
                    ) => "Capacity",
                    crate::store::CommitError::Storage(error)
                    | crate::store::CommitError::Construction { storage: error, .. }
                        if error.is_capacity_refusal() =>
                    {
                        "Capacity"
                    }
                    _ => "Failed",
                };
                let refusal = args::refuse(code, failure.to_string(), SdkValue::Null);
                close(service, &workspace)?;
                return Err(refusal);
            }
            return Err(service.retain(workspace, failure));
        }
    };
    close(service, &workspace)?;
    // The Branch moved since the private binding and changed a path this call
    // changed too: nothing was published, as for a Workspace Commit.
    let Some(history) = outcome.history else {
        let merge = outcome.merge.as_ref();
        let paths = merge.map_or_else(Vec::new, |m| {
            m.conflicts
                .iter()
                .map(|c| SdkValue::text(c.path.clone()))
                .collect()
        });
        return Err(args::refuse(
            "Conflict",
            format!(
                "the head moved and {} path(s) conflict with this call; nothing was committed",
                paths.len()
            ),
            SdkValue::map([
                ("branch", SdkValue::text(branch_name)),
                (
                    "head",
                    SdkValue::opt_text(
                        merge
                            .and_then(|m| m.head)
                            .map(|id| args::hex(id.as_slice())),
                    ),
                ),
                ("conflicts", SdkValue::List(paths)),
            ]),
        ));
    };
    Ok(match history {
        CommitOutcome::Committed { record, .. } => SdkValue::map([
            ("kind", SdkValue::text("Committed")),
            ("branch", SdkValue::text(branch_name)),
            ("head", SdkValue::text(args::hex(record.id.as_slice()))),
            ("commit", args::commit_value(&record, Some(branch_name))),
            ("replaced", SdkValue::Int(replaced as i64)),
        ]),
        CommitOutcome::UpToDate { head, root } => SdkValue::map([
            ("kind", SdkValue::text("UpToDate")),
            ("branch", SdkValue::text(branch_name)),
            (
                "head",
                SdkValue::opt_text(head.map(|id| args::hex(id.as_slice()))),
            ),
            ("root", SdkValue::text(args::hex(root.as_bytes()))),
            ("replaced", SdkValue::Int(replaced as i64)),
        ]),
    })
}

/// The project and Branch by name on the control reader, with the call's
/// refusals in their order: `NoSuchProject`, a refused `branch` argument,
/// `ReadOnly` for `main`, `NoSuchBranch`.
fn resolve<'a>(
    store: &Arc<crate::store::Store>,
    project: &str,
    branch: Arg<&'a str>,
) -> Result<(&'a str, BranchRecord, LayerStackRecord), SdkRefusal> {
    let stack = args::project(store, project)?;
    let branch_name = branch?;
    if branch_name == crate::history::MAIN {
        return Err(args::refuse(
            "ReadOnly",
            "main changes only through publish",
            SdkValue::Null,
        ));
    }
    let branch = args::branch(store, &stack, branch_name)?;
    Ok((branch_name, branch, stack))
}

/// A fresh identity for the call's private Workspace.
fn workspace_id() -> Result<WorkspaceId, SdkRefusal> {
    let key = layerfs_protocol::duplex::generate_keypair().map_err(args::failed)?;
    WorkspaceId::from_authority(key.public).map_err(args::history)
}

/// The private Workspace on Branch `branch` of `project`, bound by name
/// (#818 W2) with the refusals [`resolve`] gives, in its order: a refusal
/// the call meets before the bind reads the project and Branch (no
/// Workspace key, SQLite's deferred writer refused, overlay maintenance
/// stopped or its start failed, call capacity full, a reader that did not
/// open) comes after the lookups, as before, and is `Failed` (the writer's
/// own refusal for the writer) when they pass.
fn bind_named(
    service: &StoreService,
    project: &str,
    branch: &str,
) -> Result<Arc<crate::store::BoundWorkspace>, SdkRefusal> {
    let lookups_first = |refusal: SdkRefusal| {
        let resolved = crate::telemetry::timed("files.resolve", |_| {
            resolve(&service.store, project, Ok(branch))
        });
        resolved.err().unwrap_or(refusal)
    };
    let id = workspace_id().map_err(lookups_first)?;
    // #819: SQLite's deferred writer opens before any write bind; its
    // refusal, like the others before the bind reads, follows the lookups.
    (service.store.prepare_history_write()).map_err(|error| lookups_first(args::history(error)))?;
    let bound = crate::telemetry::timed("files.bind", |timing| {
        service.bind_named(id, project, branch, timing)
    });
    bound.map_err(|failure| match failure {
        NamedBindFailure::Admission(message) => lookups_first(args::failed(message)),
        NamedBindFailure::Bind {
            refusal,
            looked_up: false,
        } => lookups_first(args::failed(refusal)),
        NamedBindFailure::Bind { refusal, .. } => {
            lookup_answer(&refusal, project, branch).unwrap_or_else(|| args::failed(refusal))
        }
    })
}

/// The answer of a named bind's own lookup, as [`resolve`] gives it: no
/// such project or Branch, or its failed read, also under a cleanup failure
/// that followed it (the lookup answered first). `None` for a later step.
fn lookup_answer(
    refusal: &crate::store::BindRefusal,
    project: &str,
    branch: &str,
) -> Option<SdkRefusal> {
    let mut error = &refusal.error;
    while let BindError::Cleanup { original, .. } = error {
        error = original;
    }
    match error {
        BindError::NoSuchProject(_) => Some(args::no_project(project)),
        BindError::NoSuchBranch(_) => Some(args::no_branch(branch)),
        // In phase Snapshot after the lookup began, a read is the lookup's.
        BindError::Read(error) if matches!(refusal.phase, BindPhase::Snapshot) => {
            Some(args::published(error.clone()))
        }
        _ => None,
    }
}

/// Applies `changes` to the private Workspace: how many texts its edits
/// replaced, or the first refused change's refusal, after the changes
/// before it are finished in the overlay.
fn change(
    workspace: &crate::store::BoundWorkspace,
    mut changes: Changes<'_, '_>,
) -> Result<Result<u64, SdkRefusal>, SdkRefusal> {
    // The bound node's raw tree and markers: the private Workspace's overlay
    // already whiteouts a hidden path, so its absence there is ambiguous.
    // The base view distinguishes a hidden path (a `Removed` refusal for
    // `edit` and a cp source) from a genuinely absent one (`NoSuchPath`).
    // `rm` and `mv`-from read only the private overlay, where a hidden path is
    // already a whiteout, so they report it as absent: `NoSuchPath`, this
    // method table's wire spelling of §4.3's `NotFound` for those two calls.
    let base = workspace
        .snapshot()
        .map_err(crate::write::mutation::failure)?;
    let node = crate::removal::node_of(&base);
    // #818 W1: the bound node's markers the bind kept answer without a read,
    // and the base tree opens only when a refusal asks. Without kept markers
    // the call reads both first, as before, so a failed read still refuses
    // every change (SPEC D16). The control reader opens with the tree.
    let tree = std::cell::OnceCell::new();
    let visibility = match workspace.kept_markers(node) {
        Some((serial, markers)) => Visibility::from_markers(serial, markers),
        None => {
            let visibility = Visibility::for_stack(
                &workspace.store,
                base.branch.stack,
                Visibility::node_serial(&workspace.store, node)?,
            )?;
            let _ = tree.set(open_tree(workspace, &base)?);
            visibility
        }
    };
    let removed = |path: &str| -> Result<Option<SdkRefusal>, SdkRefusal> {
        let (reader, tree) = match tree.get() {
            Some(opened) => opened,
            None => {
                let opened = open_tree(workspace, &base)?;
                tree.get_or_init(|| opened)
            }
        };
        let exists = snapshot::entry(reader, tree, path)?.is_some();
        Ok(exists
            .then(|| visibility.hidden(path))
            .flatten()
            .map(|frontier| Visibility::refusal(path, frontier)))
    };
    let mut mutation = Mutation::new(workspace)?;
    let (mut result, mut replaced) = (Ok(()), 0u64);
    for index in 0.. {
        // #818 W6: a listed call's next window of paths is planned before
        // its first change; a failed plan read refuses at that change. A
        // change refused before any read is never planned over
        // (`Mutation::plan`). Streamed changes are not planned: the plan
        // needs the window's paths ahead, which a stream does not give, so
        // each streamed change walks its own paths, as on SQLite.
        let planned = match &changes {
            Changes::Listed(listed)
                if index % super::plan::PLAN_PATHS == 0 && index < listed.len() =>
            {
                let end = listed.len().min(index + super::plan::PLAN_PATHS);
                mutation.plan(&listed[index..end])
            }
            _ => Ok(()),
        };
        let (record, parsed);
        let (change, source): (Result<&Change<'_>, SdkRefusal>, Option<&mut dyn Source>) =
            match &mut changes {
                Changes::Listed(listed) => match listed.get(index) {
                    Some(change) => (change.as_ref().map_err(Clone::clone), None),
                    None => break,
                },
                Changes::Streamed { source, parse } => match source.next() {
                    Ok(Some(value)) => {
                        record = value;
                        parsed = parse(&record);
                        (parsed.as_ref().map_err(Clone::clone), Some(&mut **source))
                    }
                    Ok(None) => break,
                    // a lost input is no change's refusal: nothing commits
                    Err(lost) => {
                        result = Err(lost);
                        break;
                    }
                },
            };
        result = planned
            .and_then(|()| match change? {
                Change::Write { path, bytes, mode } => mutation.write(path, bytes, *mode),
                Change::Streamed { path, mode } => match source {
                    Some(source) => mutation.write_stream(path, *mode, source),
                    None => Err(args::invalid("a streamed write needs its streamed input")),
                },
                Change::Edit {
                    path,
                    old,
                    new,
                    all,
                } => match mutation.edit(path, old, new, *all) {
                    Ok(count) => {
                        replaced += count;
                        Ok(())
                    }
                    Err(refusal) if refusal.code == "NoSuchPath" => match removed(path)? {
                        Some(hidden) => Err(hidden),
                        None => Err(refusal),
                    },
                    Err(refusal) => Err(refusal),
                },
                Change::Mkdir { path, mode } => mutation.mkdir(path, *mode),
                Change::Chmod { path, mode } => mutation.chmod(path, *mode),
                // `rm` treats a hidden path as absent for the same reason.
                Change::Rm { path, recursive } => mutation.remove(path, *recursive),
                Change::Cp { from, to } => match mutation.copy(from, to, index as u64) {
                    Err(refusal) if refusal.code == "NoSuchPath" => match removed(from)? {
                        Some(hidden) => Err(hidden),
                        None => Err(refusal),
                    },
                    other => other,
                },
                // A hidden `from` is absent to the overlay, so `mv`-from answers
                // `NoSuchPath` (see the `NotFound` mapping above); unlike
                // `edit`/`cp`-from it does not consult the base view.
                Change::Mv { from, to } => mutation.rename(from, to),
            })
            .map_err(|mut e| {
                if let SdkValue::Map(fields) = &mut e.detail {
                    fields.push(("index".into(), SdkValue::Int(index as i64)));
                } else {
                    e.detail = SdkValue::map([("index", SdkValue::Int(index as i64))]);
                }
                e
            });
        if result.is_err() {
            break;
        }
    }
    if result.as_ref().is_err_and(|e| e.code == "Unknown") {
        return Ok(result.map(|()| replaced));
    }
    mutation.finish()?;
    Ok(result.map(|()| replaced))
}

/// The bound node's raw tree on the control reader, for the hidden-path check.
fn open_tree(
    workspace: &crate::store::BoundWorkspace,
    base: &layerfs_history::BranchSnapshot,
) -> Result<(Arc<crate::store::StorePorts>, TreeTable), SdkRefusal> {
    let reader = workspace.store.ports(base.scope);
    let tree = TreeTable::open(&*reader, FilesystemRootId(base.effective_root));
    Ok((reader, tree.map_err(args::content)?))
}

/// Closes the call's private Workspace and discards its overlay.
fn close(
    service: &StoreService,
    workspace: &crate::store::BoundWorkspace,
) -> Result<(), SdkRefusal> {
    crate::telemetry::timed("files.close", |_| {
        service.release(workspace).map_err(args::failed)
    })
}

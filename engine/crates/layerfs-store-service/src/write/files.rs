//! Atomic filesystem calls over a private, indexed Workspace. One capture and
//! canonical construction publishes the whole change list; validation failures
//! close only that private overlay. An uncertain Commit keeps its Workspace in
//! the StoreService with its failure.
use super::mutation::Mutation;
use crate::history::lookup::{self as args, Answer, Arg};
use crate::StoreService;
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_history::{CommitOutcome, CommitVia, WorkspaceId};

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

/// One change of a file call, with checked absolute paths.
pub enum Change<'a> {
    /// Replace the file's bytes, creating it with `mode` (default 0o644).
    /// An existing file keeps its mode unless `mode` is given.
    Write {
        path: String,
        bytes: &'a [u8],
        mode: Option<u32>,
    },
    /// Replace the one occurrence of `old` with `new`.
    Edit {
        path: String,
        old: &'a str,
        new: &'a str,
    },
    /// Create the directory and its missing parents. A created `path` gets
    /// `mode` (default 0o755); missing parents get 0o755, and an existing
    /// directory keeps its mode, as `mkdir -p -m` does.
    Mkdir { path: String, mode: Option<u32> },
    /// Set the entry's permission bits.
    Chmod { path: String, mode: u32 },
    /// Remove the entry, recursively.
    Rm { path: String },
    /// Copy the entry, recursively.
    Cp { from: String, to: String },
    /// Rename the entry.
    Mv { from: String, to: String },
}

/// Applies `changes` in order to a private Workspace on Branch `branch` of
/// `project` and commits them once, with `message` or the default one. A
/// change the caller refused is refused at its index, after the changes
/// before it.
pub fn commit(
    service: &StoreService,
    project: &str,
    branch: Arg<&str>,
    changes: &[Arg<Change<'_>>],
    message: Option<&str>,
) -> Answer {
    let store = &service.store;
    let (branch_name, branch) = crate::telemetry::timed("files.resolve", |_| {
        let stack = args::project(store, project)?;
        let branch_name = branch?;
        if branch_name == "main" {
            return Err(args::refuse(
                "ReadOnly",
                "main changes only through publish",
                SdkValue::Null,
            ));
        }
        Ok((branch_name, args::branch(store, &stack, branch_name)?))
    })?;
    let id = WorkspaceId::from_authority(
        layerfs_bridge::duplex::generate_keypair()
            .map_err(args::failed)?
            .public,
    )
    .map_err(args::history)?;
    let workspace =
        crate::telemetry::timed("files.bind", |timing| service.bind(id, branch.id, timing))
            .map_err(args::failed)?;
    let result = crate::telemetry::timed("files.change", |_| change(&workspace, changes))?;
    if result.as_ref().is_err_and(|e| e.code == "Unknown") {
        return result.map(|()| SdkValue::Null);
    }
    if let Err(error) = result {
        close(service, &workspace)?;
        return Err(error);
    }
    let outcome = workspace.commit_captured_via(
        &crate::store::CommitOptions {
            force: false,
            message: Some(message.unwrap_or("Apply filesystem changes").to_owned()),
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
        ]),
        CommitOutcome::UpToDate { head, root } => SdkValue::map([
            ("kind", SdkValue::text("UpToDate")),
            ("branch", SdkValue::text(branch_name)),
            (
                "head",
                SdkValue::opt_text(head.map(|id| args::hex(id.as_slice()))),
            ),
            ("root", SdkValue::text(args::hex(root.as_bytes()))),
        ]),
    })
}

/// Applies `changes` to the private Workspace: the first refused change's
/// refusal, after the changes before it are finished in the overlay.
fn change(
    workspace: &crate::store::BoundWorkspace,
    changes: &[Arg<Change<'_>>],
) -> Result<Result<(), SdkRefusal>, SdkRefusal> {
    let mut mutation = Mutation::new(workspace)?;
    let mut result = Ok(());
    for (index, change) in changes.iter().enumerate() {
        result = (|| match change.as_ref().map_err(Clone::clone)? {
            Change::Write { path, bytes, mode } => mutation.write(path, bytes, *mode),
            Change::Edit { path, old, new } => mutation.edit(path, old, new),
            Change::Mkdir { path, mode } => mutation.mkdir(path, *mode),
            Change::Chmod { path, mode } => mutation.chmod(path, *mode),
            Change::Rm { path } => mutation.remove(path),
            Change::Cp { from, to } => mutation.copy(from, to, index as u64),
            Change::Mv { from, to } => mutation.rename(from, to),
        })()
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
        return Ok(result);
    }
    mutation.finish()?;
    Ok(result)
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

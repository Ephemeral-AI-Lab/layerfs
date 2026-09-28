//! Authorized dispatch of the read-only Workspace view-lease operations.
//!
//! Every view operation runs under the same control slot as the other control
//! operations: a call during an in-flight Commit fails Busy rather than
//! blocking or interleaving. Sequential reads across a finished Commit (pin
//! before `commit`, read after) are the supported order.
use layerfs_bridge::contract::{
    Code, Failure, Operation, Response, WorkspaceViewEntryWire, WorkspaceViewLeaseWire,
    WorkspaceViewListWire, WorkspaceViewReadWire, WorkspaceViewReadlinkWire,
    WorkspaceViewReleaseOutcome, WorkspaceViewReleaseWire, WorkspaceViewStatusWire,
};
use layerfs_workspace::{NodeKind, Workspace, WorkspaceError, VIEW_LEASE_BYTES};
use std::io::Read;
use std::time::Instant;

/// The control-slot operation code every view operation authorizes under.
pub(crate) const VIEW_AUTHORITY: u8 = 64;

/// Reads 32 unguessable bytes from the OS randomness device for one lease
/// token. The registry refuses duplicates, and a token nobody registered is
/// refused before any resolution runs.
fn lease_token() -> Result<[u8; VIEW_LEASE_BYTES], Failure> {
    let mut token = [0_u8; VIEW_LEASE_BYTES];
    token[0] = 1;
    let mut file = std::fs::File::open("/dev/urandom").map_err(|_| Code::Io)?;
    file.read_exact(&mut token[1..]).map_err(|_| Code::Io)?;
    Ok(token)
}

fn entry(node: layerfs_workspace::NodeAttributes) -> WorkspaceViewEntryWire {
    WorkspaceViewEntryWire {
        serial: node.serial,
        kind: match node.kind {
            NodeKind::File => 1,
            NodeKind::Directory => 2,
            NodeKind::Symlink => 3,
        },
        size: node.size,
        references: node.references,
        mode: node.mode,
        mtime_seconds: node.mtime_seconds,
        mtime_nanoseconds: node.mtime_nanoseconds,
    }
}

fn code(error: WorkspaceError) -> Code {
    match error {
        WorkspaceError::Denied | WorkspaceError::ReadOnly => Code::Denied,
        WorkspaceError::NotFound => Code::PathNotFound,
        WorkspaceError::NotDirectory | WorkspaceError::IsDirectory | WorkspaceError::WrongKind => {
            Code::InvalidInput
        }
        WorkspaceError::InvalidInput | WorkspaceError::BadHandle => Code::InvalidInput,
        WorkspaceError::Capacity => Code::Capacity,
        WorkspaceError::Busy | WorkspaceError::Closed => Code::Busy,
        WorkspaceError::Deadline => Code::Deadline,
        WorkspaceError::Unsupported => Code::Unsupported,
        WorkspaceError::Service(failure) => failure.code,
        WorkspaceError::Backing(failure) if failure.kind == std::io::ErrorKind::TimedOut => {
            Code::Deadline
        }
        _ => Code::Io,
    }
}

fn view_token(view: &[u8]) -> Result<[u8; VIEW_LEASE_BYTES], Failure> {
    view.try_into().map_err(|_| Code::InvalidInput.into())
}

/// Dispatches one authorized view operation against the selected Workspace.
pub(crate) fn dispatch(
    workspace: &Workspace,
    incarnation: &[u8; 32],
    operation: &Operation,
    deadline: Instant,
) -> Result<Response, Failure> {
    match operation {
        Operation::WorkspacePinView { .. } => {
            let token = lease_token()?;
            let info = workspace.pin_view(token, deadline).map_err(code)?;
            Ok(Response::WorkspaceViewLease(Box::new(
                WorkspaceViewLeaseWire {
                    workspace: workspace_identity(workspace),
                    incarnation: *incarnation,
                    view: token.to_vec(),
                    root: entry(info.root),
                    generation: info.generation,
                    revision: info.revision,
                    base: info.base,
                },
            )))
        }
        Operation::WorkspaceViewLookup {
            view, parent, name, ..
        } => {
            let token = view_token(view)?;
            let resolved = workspace
                .view_lookup(&token, *parent, name, deadline)
                .map_err(code)?;
            Ok(Response::WorkspaceViewEntry(WorkspaceViewEntryWire {
                serial: resolved.serial,
                kind: match resolved.kind {
                    NodeKind::File => 1,
                    NodeKind::Directory => 2,
                    NodeKind::Symlink => 3,
                },
                size: resolved.size,
                references: resolved.references,
                mode: resolved.mode,
                mtime_seconds: resolved.mtime_seconds,
                mtime_nanoseconds: resolved.mtime_nanoseconds,
            }))
        }
        Operation::WorkspaceViewList {
            view,
            directory,
            after,
            entries,
            ..
        } => {
            let token = view_token(view)?;
            let page = workspace
                .view_list(
                    &token,
                    *directory,
                    after.as_deref(),
                    *entries as usize,
                    deadline,
                )
                .map_err(code)?;
            Ok(Response::WorkspaceViewList(Box::new(
                WorkspaceViewListWire {
                    workspace: workspace_identity(workspace),
                    incarnation: *incarnation,
                    view: view.clone(),
                    entries: page.entries,
                    continuation: page.continuation,
                },
            )))
        }
        Operation::WorkspaceViewRead {
            view,
            file,
            offset,
            bytes,
            ..
        } => {
            let token = view_token(view)?;
            let read = workspace
                .view_read(&token, *file, *offset, *bytes as usize, deadline)
                .map_err(code)?;
            Ok(Response::WorkspaceViewRead(Box::new(
                WorkspaceViewReadWire {
                    workspace: workspace_identity(workspace),
                    incarnation: *incarnation,
                    view: view.clone(),
                    serial: *file,
                    bytes: read.bytes,
                    eof: read.eof,
                    size: read.size,
                },
            )))
        }
        Operation::WorkspaceViewReadlink { view, link, .. } => {
            let token = view_token(view)?;
            let target = workspace
                .view_readlink(&token, *link, deadline)
                .map_err(code)?;
            Ok(Response::WorkspaceViewReadlink(Box::new(
                WorkspaceViewReadlinkWire {
                    workspace: workspace_identity(workspace),
                    incarnation: *incarnation,
                    view: view.clone(),
                    target,
                },
            )))
        }
        Operation::WorkspaceViewStatus { view, .. } => {
            let token = view_token(view)?;
            let status = workspace.view_status(&token).map_err(code)?;
            Ok(Response::WorkspaceViewStatus(Box::new(
                WorkspaceViewStatusWire {
                    workspace: workspace_identity(workspace),
                    incarnation: *incarnation,
                    view: view.clone(),
                    generation: status.generation,
                    revision: status.revision,
                    entries: status.entries as u64,
                    held_leases: status.held_leases as u64,
                },
            )))
        }
        Operation::WorkspaceReleaseView { view, .. } => {
            let token = view_token(view)?;
            let outcome = workspace.release_view(&token).map_err(code)?;
            let outcome = match outcome {
                layerfs_workspace::ViewRelease::Completed => WorkspaceViewReleaseOutcome::Completed,
                layerfs_workspace::ViewRelease::Retained(cause) => {
                    WorkspaceViewReleaseOutcome::Retained(code(cause))
                }
            };
            Ok(Response::WorkspaceViewRelease(Box::new(
                WorkspaceViewReleaseWire {
                    workspace: workspace_identity(workspace),
                    incarnation: *incarnation,
                    view: view.clone(),
                    outcome,
                },
            )))
        }
        _ => Err(Code::Unsupported.into()),
    }
}

fn workspace_identity(workspace: &Workspace) -> Vec<u8> {
    workspace.id().as_bytes().to_vec()
}

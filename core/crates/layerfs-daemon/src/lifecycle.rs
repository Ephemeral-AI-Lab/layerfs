//! The single selected native owner shared by control and process shutdown.
use layerfs_bridge::contract::{
    Code, Failure, Response, WorkspaceAttachOutcome, WorkspaceAttachWire,
    WorkspaceAttachmentProgress, WorkspaceAttachmentState, WorkspaceAttachmentWire,
    WorkspaceStatusWire, PROJECTION_BYTES, PROJECTION_BYTE_LABELS, PROJECTION_CLASSES,
    PROJECTION_CLASS_LABELS, PROJECTION_SIZE_BUCKETS, PROJECTION_SIZE_LABELS,
};
use layerfs_fuse::{MountError, MountFailure, MountHandle};
use layerfs_workspace::{
    AttachOptions, Attachment, AttachmentCleanupProgress, Base, Workspace, WorkspaceAccess,
    WorkspaceError, WorkspaceHost,
};
use std::{io, sync::Mutex, time::Instant};

pub(crate) struct Lifecycle {
    host: WorkspaceHost,
    profile: AttachOptions,
    pub slot: Mutex<Slot>,
}

pub(crate) struct Slot {
    pub selected: Option<Selected>,
    pub mount: Option<MountHandle>,
}

pub(crate) struct Selected {
    pub id: String,
    pub incarnation: [u8; 32],
    // None preserves an exact unresolved/failed selector. WorkspaceHost alone
    // owns failed attachment resources and their semantic state.
    pub workspace: Option<Workspace>,
}

impl Lifecycle {
    pub fn new_idle(host: WorkspaceHost, profile: AttachOptions) -> Self {
        Self {
            host,
            profile,
            slot: Mutex::new(Slot {
                selected: None,
                mount: None,
            }),
        }
    }

    pub fn new(
        host: WorkspaceHost,
        profile: AttachOptions,
        workspace: Option<Workspace>,
        mount: Option<MountHandle>,
    ) -> Self {
        let selected = Some(Selected {
            id: profile.id.clone(),
            incarnation: profile.incarnation,
            workspace,
        });
        Self {
            host,
            profile,
            slot: Mutex::new(Slot { selected, mount }),
        }
    }

    pub fn can_attach(&self, slot: &Slot) -> Result<(), Failure> {
        if slot.mount.is_some() {
            return Err(Code::Busy.into());
        }
        if let Some(selected) = &slot.selected {
            let workspace = selected.workspace.as_ref().ok_or(Code::Busy)?;
            let status = workspace.status().map_err(|error| failure_code(&error))?;
            if !status.closed || status.mounted {
                return Err(Code::Busy.into());
            }
        }
        Ok(())
    }

    pub fn attach(
        &self,
        slot: &mut Slot,
        workspace: &[u8],
        incarnation: [u8; 32],
        deadline: Instant,
    ) -> Result<Response, Failure> {
        let id = std::str::from_utf8(workspace).map_err(|_| Code::InvalidInput)?;
        let mut options = self.profile.clone();
        options.id = id.into();
        options.incarnation = incarnation;
        self.attach_with_options(slot, workspace, incarnation, options, deadline)
    }

    pub fn attach_selected(
        &self,
        slot: &mut Slot,
        workspace: &[u8],
        incarnation: [u8; 32],
        base: Base,
        deadline: Instant,
    ) -> Result<Response, Failure> {
        let id = std::str::from_utf8(workspace).map_err(|_| Code::InvalidInput)?;
        let mut options = self.profile.clone();
        options.id = id.into();
        options.incarnation = incarnation;
        options.base = base;
        self.attach_with_options(slot, workspace, incarnation, options, deadline)
    }

    fn attach_with_options(
        &self,
        slot: &mut Slot,
        workspace: &[u8],
        incarnation: [u8; 32],
        options: AttachOptions,
        deadline: Instant,
    ) -> Result<Response, Failure> {
        let id = std::str::from_utf8(workspace).map_err(|_| Code::InvalidInput)?;
        let mut result = Box::new(WorkspaceAttachWire {
            workspace: workspace.into(),
            incarnation,
            outcome: WorkspaceAttachOutcome::Completed,
        });
        result.validate()?;
        // Keep at most one prior closed capability until exact absence allows
        // restoration. Its existing native allocation charges remain live.
        let prior = slot.selected.replace(Selected {
            id: id.into(),
            incarnation,
            workspace: None,
        });
        match self.host.attach(options, deadline) {
            Ok(attached) => {
                slot.selected
                    .as_mut()
                    .expect("selected before Attach")
                    .workspace = Some(attached);
            }
            Err(error) => {
                if matches!(
                    self.host.attachment(id, incarnation),
                    Err(WorkspaceError::NotFound)
                ) {
                    slot.selected = prior;
                    return Err(failure_code(&error).into());
                }
                // A failed inspection is not proof of absence. Preserve the
                // selector even when cleanup might already have released all.
                result.outcome = WorkspaceAttachOutcome::Retained(failure_code(&error));
            }
        }
        Ok(Response::WorkspaceAttach(result))
    }

    pub fn status(&self, slot: &Slot) -> Result<Response, Failure> {
        let selected = slot.selected.as_ref().ok_or(Code::NotFound)?;
        if let Some(workspace) = &selected.workspace {
            return workspace_status(selected, workspace);
        }
        let state = match self.host.attachment(&selected.id, selected.incarnation) {
            Ok(Attachment::Attached(workspace)) => return workspace_status(selected, &workspace),
            Ok(Attachment::Attaching) => WorkspaceAttachmentState::Attaching,
            Ok(Attachment::Failed(failure)) => WorkspaceAttachmentState::Failed {
                cause: failure_code(&failure.cause),
                cleanup: failure.cleanup.as_ref().map(failure_code),
                progress: match failure.progress {
                    AttachmentCleanupProgress::Running => WorkspaceAttachmentProgress::Running,
                    AttachmentCleanupProgress::Retained {
                        mount_directory,
                        metadata_arena,
                        backing_directory,
                    } => WorkspaceAttachmentProgress::Retained {
                        mount_directory,
                        metadata_arena,
                        backing_directory,
                    },
                },
            },
            Err(error) => return Err(failure_code(&error).into()),
        };
        let result = WorkspaceAttachmentWire {
            workspace: selected.id.as_bytes().into(),
            incarnation: selected.incarnation,
            state,
        };
        result.validate()?;
        Ok(Response::WorkspaceAttachment(Box::new(result)))
    }

    pub fn close(&self, slot: &mut Slot, deadline: Instant) -> Result<(), WorkspaceError> {
        let Some(selected) = &slot.selected else {
            return Ok(());
        };
        if let Some(workspace) = &selected.workspace {
            return close_workspace(workspace, deadline);
        }
        match self.host.attachment(&selected.id, selected.incarnation) {
            Ok(Attachment::Attached(workspace)) => close_workspace(&workspace, deadline)?,
            Ok(Attachment::Failed(_)) => {
                self.host
                    .cleanup_failed_attach(&selected.id, selected.incarnation, deadline)?
            }
            Ok(Attachment::Attaching) => return Err(WorkspaceError::Busy),
            Err(WorkspaceError::NotFound) => {}
            Err(error) => return Err(error),
        }
        slot.selected = None;
        Ok(())
    }

    pub fn shutdown(
        &self,
        deadline: Instant,
        control: Option<&crate::control::Control>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut slot = self.slot.try_lock().map_err(|error| match error {
            std::sync::TryLockError::WouldBlock => Failure::from(Code::Busy),
            std::sync::TryLockError::Poisoned(_) => Failure::from(Code::Io),
        })?;
        if let Some(mount) = slot.mount.as_mut() {
            mount.unmount(deadline)?;
            slot.mount = None;
        }
        self.close(&mut slot, deadline)?;
        // Closure and control admission share this exclusion boundary. A dirty
        // or retained owner must keep its Commit/Status endpoint available.
        if let Some(control) = control {
            control.stop_admission();
        }
        Ok(())
    }
}

/// Maps the Workspace's labeled projection counts into the fixed wire classes.
///
/// A class the Workspace does not report is a contract mismatch, not a zero.
fn projection_counts(
    local: &layerfs_workspace::WorkspaceStatus,
) -> Result<[u64; PROJECTION_CLASSES], Failure> {
    let mut counts = [0u64; PROJECTION_CLASSES];
    for (label, count) in &local.projection_calls {
        let slot = PROJECTION_CLASS_LABELS
            .iter()
            .position(|name| name == label)
            .ok_or(Code::Integrity)?;
        counts[slot] = *count;
    }
    if local.projection_calls.len() != PROJECTION_CLASSES {
        return Err(Code::Integrity.into());
    }
    Ok(counts)
}

/// Maps the Workspace's labeled byte totals into the fixed wire order.
///
/// A label the Workspace does not report is a contract mismatch, not a zero.
fn projection_bytes(
    local: &layerfs_workspace::WorkspaceStatus,
) -> Result<[u64; PROJECTION_BYTES], Failure> {
    let mut totals = [0u64; PROJECTION_BYTES];
    for (label, total) in &local.projection_bytes {
        let slot = PROJECTION_BYTE_LABELS
            .iter()
            .position(|name| name == label)
            .ok_or(Code::Integrity)?;
        totals[slot] = *total;
    }
    if local.projection_bytes.len() != PROJECTION_BYTES {
        return Err(Code::Integrity.into());
    }
    Ok(totals)
}

/// Maps one direction's labeled request-size histogram into the fixed wire order.
fn projection_sizes(
    rows: &[(String, u64)],
    direction: &str,
) -> Result<[u64; PROJECTION_SIZE_BUCKETS], Failure> {
    let mut counts = [0u64; PROJECTION_SIZE_BUCKETS];
    let prefix = format!("{direction}:");
    for (label, count) in rows {
        let bucket = label
            .strip_prefix(prefix.as_str())
            .and_then(|name| {
                PROJECTION_SIZE_LABELS
                    .iter()
                    .position(|entry| *entry == name)
            })
            .ok_or(Code::Integrity)?;
        counts[bucket] = *count;
    }
    if rows.len() != PROJECTION_SIZE_BUCKETS {
        return Err(Code::Integrity.into());
    }
    Ok(counts)
}

fn close_workspace(workspace: &Workspace, deadline: Instant) -> Result<(), WorkspaceError> {
    if workspace.status()?.closed {
        return Ok(());
    }
    match workspace.close_clean_until(deadline) {
        Ok(()) | Err(WorkspaceError::Closed) => Ok(()),
        Err(error) => Err(error),
    }
}

fn workspace_status(selected: &Selected, workspace: &Workspace) -> Result<Response, Failure> {
    let local = workspace.status().map_err(|error| failure_code(&error))?;
    eprintln!(
        "DIAG status id={} upstream={} calls={} bytes={} histogram={}",
        selected.id.as_bytes().iter().map(|b| format!("{b:02x}")).collect::<String>(),
        local.upstream_calls,
        local.projection_calls.len(),
        local.projection_bytes.len(),
        local.projection_histogram.len(),
    );
    let result = WorkspaceStatusWire {
        workspace: selected.id.as_bytes().into(),
        incarnation: selected.incarnation,
        mounted: local.mounted,
        stopping: local.stopping,
        closed: local.closed,
        active_operations: local.active_operations as u64,
        nodes: local.nodes as u64,
        handles: local.handles as u64,
        cookies: local.cookies as u64,
        consumer_accounted_bytes: local.accounted_bytes as u64,
        projection: projection_counts(&local)?,
        projection_bytes: projection_bytes(&local)?,
        projection_read_sizes: projection_sizes(&local.projection_histogram, "read")?,
        projection_write_sizes: projection_sizes(&local.projection_histogram, "write")?,
        upstream_calls: local.upstream_calls,
    };
    result.validate()?;
    if workspace.access_mode() == WorkspaceAccess::LocalEdit {
        crate::control_commit::status(result, local)
    } else {
        Ok(Response::WorkspaceStatus(Box::new(result)))
    }
}

pub(crate) fn failure_code(error: &WorkspaceError) -> Code {
    match error {
        WorkspaceError::InvalidInput => Code::InvalidInput,
        WorkspaceError::Capacity => Code::Capacity,
        WorkspaceError::Busy | WorkspaceError::Closed => Code::Busy,
        WorkspaceError::NotFound => Code::NotFound,
        WorkspaceError::ReadOnly | WorkspaceError::Denied => Code::Denied,
        WorkspaceError::Unsupported => Code::Unsupported,
        WorkspaceError::Deadline => Code::Deadline,
        WorkspaceError::Service(failure) if failure.unknown => Code::Unknown,
        WorkspaceError::Service(failure) => failure.code,
        WorkspaceError::Backing(failure) if failure.kind == io::ErrorKind::TimedOut => {
            Code::Deadline
        }
        _ => Code::Io,
    }
}

pub(crate) fn mount_failure_code(error: &MountError) -> Code {
    match error {
        MountError::Deadline | MountError::Workspace(WorkspaceError::Deadline) => Code::Deadline,
        MountError::Workspace(WorkspaceError::Backing(failure))
            if failure.kind == io::ErrorKind::TimedOut =>
        {
            Code::Deadline
        }
        MountError::Unsupported | MountError::Workspace(WorkspaceError::Unsupported) => {
            Code::Unsupported
        }
        MountError::Workspace(WorkspaceError::Busy | WorkspaceError::Closed) => Code::Busy,
        _ => Code::Io,
    }
}

pub(crate) fn mount(
    workspace: &Workspace,
    deadline: Instant,
) -> Result<MountHandle, Box<MountFailure>> {
    match workspace.access_mode() {
        WorkspaceAccess::ReadOnly => layerfs_fuse::mount(workspace, deadline),
        WorkspaceAccess::LocalEdit => layerfs_fuse::mount_writable(workspace, deadline),
    }
}

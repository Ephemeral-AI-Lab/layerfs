//! Daemon control payloads shared by their distinct response tags.
use super::{response::code, Decoder, Encoder};
use crate::contract::*;

pub(super) fn put_status(e: &mut Encoder, status: &WorkspaceStatusWire) -> Result<(), Failure> {
    status.validate()?;
    e.blob(&status.workspace)?;
    e.put(&status.incarnation)?;
    e.u8(u8::from(status.mounted)
        | (u8::from(status.stopping) << 1)
        | (u8::from(status.closed) << 2))?;
    e.u64(status.active_operations)?;
    e.u64(status.nodes)?;
    e.u64(status.handles)?;
    e.u64(status.cookies)?;
    e.u64(status.consumer_accounted_bytes)?;
    for count in status.projection {
        e.u64(count)?;
    }
    e.u64(status.upstream_calls)?;
    e.u64(status.range_accepted_payload_bytes)?;
    e.u64(status.range_shifted_suffix_bytes)
}
pub(super) fn take_status(d: &mut Decoder<'_>) -> Result<WorkspaceStatusWire, Failure> {
    let workspace = d.blob(WORKSPACE_ID_BYTES)?;
    let incarnation = d.root()?;
    let flags = d.u8()?;
    if flags & !7 != 0 {
        return Err(Code::InvalidInput.into());
    }
    let status = WorkspaceStatusWire {
        workspace,
        incarnation,
        mounted: flags & 1 != 0,
        stopping: flags & 2 != 0,
        closed: flags & 4 != 0,
        active_operations: d.u64()?,
        nodes: d.u64()?,
        handles: d.u64()?,
        cookies: d.u64()?,
        consumer_accounted_bytes: d.u64()?,
        projection: {
            let mut counts = [0u64; PROJECTION_CLASSES];
            for slot in counts.iter_mut() {
                *slot = d.u64()?;
            }
            counts
        },
        upstream_calls: d.u64()?,
        range_accepted_payload_bytes: d.u64()?,
        range_shifted_suffix_bytes: d.u64()?,
    };
    status.validate()?;
    Ok(status)
}
pub(super) fn put_lifecycle(
    e: &mut Encoder,
    result: &WorkspaceLifecycleWire,
) -> Result<(), Failure> {
    result.validate()?;
    e.blob(&result.workspace)?;
    e.put(&result.incarnation)?;
    match result.outcome {
        WorkspaceLifecycleOutcome::Completed => e.u8(0),
        WorkspaceLifecycleOutcome::Retained(code) => {
            e.u8(1)?;
            e.u8(code as u8)
        }
    }
}
pub(super) fn take_lifecycle(d: &mut Decoder<'_>) -> Result<WorkspaceLifecycleWire, Failure> {
    let result = WorkspaceLifecycleWire {
        workspace: d.blob(WORKSPACE_ID_BYTES)?,
        incarnation: d.root()?,
        outcome: match d.u8()? {
            0 => WorkspaceLifecycleOutcome::Completed,
            1 => WorkspaceLifecycleOutcome::Retained(code(d.u8()?)?),
            _ => return Err(Code::InvalidInput.into()),
        },
    };
    result.validate()?;
    Ok(result)
}
pub(super) fn put_attach(e: &mut Encoder, result: &WorkspaceAttachWire) -> Result<(), Failure> {
    result.validate()?;
    e.blob(&result.workspace)?;
    e.put(&result.incarnation)?;
    match result.outcome {
        WorkspaceAttachOutcome::Completed => e.u8(0),
        WorkspaceAttachOutcome::Retained(code) => {
            e.u8(1)?;
            e.u8(code as u8)
        }
    }
}
pub(super) fn take_attach(d: &mut Decoder<'_>) -> Result<WorkspaceAttachWire, Failure> {
    let result = WorkspaceAttachWire {
        workspace: d.blob(WORKSPACE_ID_BYTES)?,
        incarnation: d.root()?,
        outcome: match d.u8()? {
            0 => WorkspaceAttachOutcome::Completed,
            1 => WorkspaceAttachOutcome::Retained(code(d.u8()?)?),
            _ => return Err(Code::InvalidInput.into()),
        },
    };
    result.validate()?;
    Ok(result)
}
pub(super) fn put_attachment(
    e: &mut Encoder,
    status: &WorkspaceAttachmentWire,
) -> Result<(), Failure> {
    status.validate()?;
    e.blob(&status.workspace)?;
    e.put(&status.incarnation)?;
    match status.state {
        WorkspaceAttachmentState::Attaching => e.u8(0)?,
        WorkspaceAttachmentState::Failed {
            cause,
            cleanup,
            progress,
        } => {
            e.u8(1)?;
            e.u8(cause as u8)?;
            e.u8(cleanup.map_or(0, |code| code as u8))?;
            match progress {
                WorkspaceAttachmentProgress::Running => e.u8(0)?,
                WorkspaceAttachmentProgress::Retained {
                    mount_directory,
                    metadata_arena,
                    backing_directory,
                } => {
                    e.u8(1)?;
                    e.u8(u8::from(mount_directory)
                        | (u8::from(metadata_arena) << 1)
                        | (u8::from(backing_directory) << 2))?;
                }
            }
        }
    }
    Ok(())
}
pub(super) fn take_attachment(d: &mut Decoder<'_>) -> Result<WorkspaceAttachmentWire, Failure> {
    let workspace = d.blob(WORKSPACE_ID_BYTES)?;
    let incarnation = d.root()?;
    let state = match d.u8()? {
        0 => WorkspaceAttachmentState::Attaching,
        1 => {
            let cause = code(d.u8()?)?;
            let cleanup = match d.u8()? {
                0 => None,
                value => Some(code(value)?),
            };
            let progress = match d.u8()? {
                0 => WorkspaceAttachmentProgress::Running,
                1 => {
                    let flags = d.u8()?;
                    if flags & !7 != 0 {
                        return Err(Code::InvalidInput.into());
                    }
                    WorkspaceAttachmentProgress::Retained {
                        mount_directory: flags & 1 != 0,
                        metadata_arena: flags & 2 != 0,
                        backing_directory: flags & 4 != 0,
                    }
                }
                _ => return Err(Code::InvalidInput.into()),
            };
            WorkspaceAttachmentState::Failed {
                cause,
                cleanup,
                progress,
            }
        }
        _ => return Err(Code::InvalidInput.into()),
    };
    let status = WorkspaceAttachmentWire {
        workspace,
        incarnation,
        state,
    };
    status.validate()?;
    Ok(status)
}

//! Lossless domain/variant/field encoding, with iterative original/cleanup traversal.
use super::writer::Writer;
use crate::RuntimeError;
use layerfs_bridge::contract::{FrameError, FrameResult};
use layerfs_content::ContentError;
use layerfs_history::{
    error::{HistoryError, Missing, StageDisposition},
    StageRecord,
};
use layerfs_storage::{port::PersistenceError, StorageError};
enum Task<'a> {
    Runtime(&'a RuntimeError),
    Content(&'a ContentError),
    Storage(&'a StorageError),
    History(&'a HistoryError),
    Io(&'a std::io::Error),
    Persistence(&'a PersistenceError),
    Opaque(&'a (dyn std::error::Error + 'static)),
    ChildStart(u8),
    End(usize),
}
fn push<'a>(tasks: &mut Vec<Task<'a>>, task: Task<'a>) -> FrameResult<()> {
    tasks
        .try_reserve(1)
        .map_err(|error| FrameError::Allocation {
            requested_bytes: (tasks.len() + 1) * std::mem::size_of::<Task<'a>>(),
            error,
        })?;
    tasks.push(task);
    Ok(())
}
pub(crate) fn encode(writer: &mut Writer, error: &RuntimeError) -> FrameResult<()> {
    let mut tasks = Vec::new();
    push(&mut tasks, Task::Runtime(error))?;
    while let Some(task) = tasks.pop() {
        let (domain, code) = match &task {
            Task::Runtime(e) => (1, runtime_code(e)),
            Task::Content(e) => (2, content_code(e)),
            Task::Storage(e) => (3, storage_code(e)),
            Task::History(e) => (4, history_code(e)),
            Task::Io(e) => (5, io_code(e.kind())),
            Task::Persistence(e) => (6, persistence_code(e)),
            Task::Opaque(_) => (7, 1),
            Task::End(at) => {
                writer.end_length(*at)?;
                continue;
            }
            Task::ChildStart(tag) => {
                let at = writer.child_start(*tag)?;
                let child = tasks.pop().expect("scheduled error child");
                push(&mut tasks, Task::End(at))?;
                push(&mut tasks, child)?;
                continue;
            }
        };
        let at = writer.node_start(domain, code)?;
        push(&mut tasks, Task::End(at))?;
        match task {
            Task::Runtime(error) => match error {
                RuntimeError::Invalid(label) => writer.text(1, label)?,
                RuntimeError::Content(e) => child(&mut tasks, 1, Task::Content(e))?,
                RuntimeError::Storage(e) => child(&mut tasks, 1, Task::Storage(e))?,
                RuntimeError::History(e) => child(&mut tasks, 1, Task::History(e))?,
                RuntimeError::Reply(e) => child(&mut tasks, 1, Task::Io(e))?,
                _ => (),
            },
            Task::Content(e) => content_fields(writer, e)?,
            Task::Storage(e) => match e {
                StorageError::Content(e) => child(&mut tasks, 1, Task::Content(e))?,
                StorageError::Io(e) => child(&mut tasks, 1, Task::Io(e))?,
                StorageError::ObjectMissing(id)
                | StorageError::Unpublished(id)
                | StorageError::Collision(id) => writer.field(1, 3, id.as_bytes())?,
                StorageError::MissingDependency { object, reference } => {
                    writer.field(1, 3, object.as_bytes())?;
                    writer.field(2, 3, reference.as_bytes())?;
                }
                StorageError::VisibilityCeiling { pack_id, ceiling } => {
                    writer.signed(1, *pack_id)?;
                    writer.signed(2, *ceiling)?;
                }
                StorageError::UninspectedState {
                    ceiling,
                    highest_pack_id,
                } => {
                    writer.signed(1, *ceiling)?;
                    writer.signed(2, *highest_pack_id)?;
                }
                StorageError::UnsupportedPolicy { field } | StorageError::Integrity(field) => {
                    writer.text(1, field)?
                }
                StorageError::CapacityExceeded {
                    what,
                    limit,
                    actual,
                } => {
                    writer.text(1, what)?;
                    writer.number(2, *limit)?;
                    writer.number(3, *actual)?;
                }
                StorageError::UnknownOutcome { original } => {
                    child(&mut tasks, 1, Task::Storage(original))?
                }
                StorageError::CleanupFailed { original, cleanup } => {
                    child(&mut tasks, 2, Task::Storage(cleanup))?;
                    child(&mut tasks, 1, Task::Storage(original))?;
                }
                StorageError::Busy | StorageError::OwnershipUnavailable | StorageError::Aborted => {
                }
            },
            Task::History(e) => match e {
                HistoryError::WithStage { cause, stage } => {
                    match stage {
                        StageDisposition::Absent(workspace) => {
                            writer.number(1, 1)?;
                            writer.field(2, 3, &workspace.to_bytes())?;
                        }
                        StageDisposition::Retained(record) => {
                            writer.number(1, 2)?;
                            stage_field(writer, record)?;
                        }
                        StageDisposition::AcknowledgedUnknown(record) => {
                            writer.number(1, 3)?;
                            stage_field(writer, record)?;
                        }
                    }
                    child(&mut tasks, 3, Task::History(cause))?;
                }
                HistoryError::InvalidInput(s)
                | HistoryError::Unsupported(s)
                | HistoryError::Capacity(s)
                | HistoryError::Integrity(s)
                | HistoryError::NotInHistory(s) => writer.text(1, s)?,
                HistoryError::Missing(kind) => writer.number(
                    1,
                    match kind {
                        Missing::LayerStack => 1,
                        Missing::Branch => 2,
                        Missing::Commit => 3,
                        Missing::Layer => 4,
                        Missing::Stage => 5,
                        Missing::Scope => 6,
                        Missing::Catalog => 7,
                    },
                )?,
                HistoryError::HeadMoved(state) => {
                    optional_id(writer, 1, state.expected_head.map(|id| id.to_bytes()))?;
                    optional_id(writer, 2, state.actual_head.map(|id| id.to_bytes()))?;
                    writer.field(3, 3, &state.expected_base.to_bytes())?;
                    writer.field(4, 3, &state.actual_base.to_bytes())?;
                }
                HistoryError::BaseMismatch {
                    commit_base,
                    branch_base,
                } => {
                    writer.field(1, 3, &commit_base.to_bytes())?;
                    writer.field(2, 3, &branch_base.to_bytes())?;
                }
                HistoryError::StackMoved { expected, actual } => {
                    writer.field(1, 3, &expected.to_bytes())?;
                    writer.field(2, 3, &actual.to_bytes())?;
                }
                HistoryError::StageChanged { expected, actual } => {
                    writer.number(1, expected.value())?;
                    writer.field(
                        2,
                        3,
                        &actual.map(|t| t.value().to_be_bytes()).unwrap_or_default()
                            [..if actual.is_some() { 8 } else { 0 }],
                    )?;
                }
                _ => (),
            },
            Task::Io(e) => {
                writer.text(1, &format!("{:?}", e.kind()))?;
                if let Some(raw) = e.raw_os_error() {
                    writer.signed(2, raw as i64)?;
                }
                writer.text(3, &e.to_string())?;
                if let Some(source) = e.get_ref() {
                    if let Some(persistence) = source.downcast_ref::<PersistenceError>() {
                        child(&mut tasks, 4, Task::Persistence(persistence))?;
                    } else {
                        child(&mut tasks, 4, Task::Opaque(source))?;
                    }
                }
            }
            Task::Persistence(PersistenceError::Refused { status }) => writer.text(1, status)?,
            Task::Opaque(e) => {
                writer.text(1, &e.to_string())?;
                if let Some(source) = e.source() {
                    child(&mut tasks, 2, Task::Opaque(source))?;
                }
            }
            _ => (),
        }
    }
    Ok(())
}
fn child<'a>(tasks: &mut Vec<Task<'a>>, tag: u8, task: Task<'a>) -> FrameResult<()> {
    push(tasks, task)?;
    push(tasks, Task::ChildStart(tag))
}
fn optional_id(writer: &mut Writer, tag: u8, id: Option<[u8; 33]>) -> FrameResult<()> {
    writer.field(tag, 3, id.as_ref().map_or(&[], |bytes| bytes.as_slice()))
}
pub(crate) fn stage_record(writer: &mut Writer, stage: &StageRecord) -> FrameResult<()> {
    writer.raw(&stage.workspace.to_bytes())?;
    writer.u64(stage.token.value())?;
    writer.raw(&stage.stack.to_bytes())?;
    writer.raw(&stage.branch.to_bytes())?;
    writer.byte(u8::from(stage.expected_head.is_some()))?;
    if let Some(id) = stage.expected_head {
        writer.raw(&id.to_bytes())?;
    }
    writer.raw(&stage.expected_base.to_bytes())?;
    writer.raw(stage.expected_root.as_bytes())?;
    writer.raw(stage.construction_base_root.as_bytes())?;
    writer.raw(&stage.intended_commit_base.to_bytes())?;
    writer.raw(stage.candidate_root.as_bytes())?;
    writer.raw(stage.profile.as_bytes())?;
    writer.raw(stage.scope.as_bytes())?;
    writer.u64(stage.generation)
}
fn stage_field(writer: &mut Writer, record: &StageRecord) -> FrameResult<()> {
    let mut count = Writer::count(4096);
    stage_record(&mut count, record)?;
    writer.raw(&[2, 6])?;
    writer.u32(count.len())?;
    stage_record(writer, record)
}
fn runtime_code(e: &RuntimeError) -> u8 {
    match e {
        RuntimeError::Denied => 1,
        RuntimeError::Invalid(_) => 2,
        RuntimeError::AdmissionUnavailable => 3,
        RuntimeError::StaleCapability => 4,
        RuntimeError::AlreadyAttempted => 5,
        RuntimeError::RetainedCustody => 6,
        RuntimeError::Content(_) => 7,
        RuntimeError::Storage(_) => 8,
        RuntimeError::History(_) => 9,
        RuntimeError::Reply(_) => 10,
    }
}
fn storage_code(e: &StorageError) -> u8 {
    match e {
        StorageError::Content(_) => 1,
        StorageError::Io(_) => 2,
        StorageError::ObjectMissing(_) => 3,
        StorageError::Unpublished(_) => 4,
        StorageError::Collision(_) => 5,
        StorageError::MissingDependency { .. } => 6,
        StorageError::VisibilityCeiling { .. } => 7,
        StorageError::OwnershipUnavailable => 8,
        StorageError::UninspectedState { .. } => 9,
        StorageError::UnsupportedPolicy { .. } => 10,
        StorageError::Integrity(_) => 11,
        StorageError::CapacityExceeded { .. } => 12,
        StorageError::UnknownOutcome { .. } => 13,
        StorageError::CleanupFailed { .. } => 14,
        StorageError::Aborted => 15,
        StorageError::Busy => 16,
    }
}
fn history_code(e: &HistoryError) -> u8 {
    match e {
        HistoryError::WithStage { .. } => 1,
        HistoryError::InvalidInput(_) => 2,
        HistoryError::Missing(_) => 3,
        HistoryError::Unsupported(_) => 4,
        HistoryError::Busy => 5,
        HistoryError::OwnershipUnavailable => 6,
        HistoryError::Capacity(_) => 7,
        HistoryError::Integrity(_) => 8,
        HistoryError::HeadMoved(_) => 9,
        HistoryError::BaseMismatch { .. } => 10,
        HistoryError::NotInHistory(_) => 11,
        HistoryError::StackMoved { .. } => 12,
        HistoryError::StageChanged { .. } => 13,
        HistoryError::ContinuityUnavailable => 14,
        HistoryError::UnknownOutcome => 15,
    }
}
fn persistence_code(e: &PersistenceError) -> u8 {
    match e {
        PersistenceError::BackendUnavailable => 1,
        PersistenceError::Missing => 2,
        PersistenceError::Refused { .. } => 3,
        PersistenceError::Malformed => 4,
        PersistenceError::Uncertain => 5,
        PersistenceError::Busy => 6,
    }
}
fn io_code(kind: std::io::ErrorKind) -> u8 {
    use std::io::ErrorKind::*;
    match kind {
        NotFound => 1,
        PermissionDenied => 2,
        ConnectionRefused => 3,
        ConnectionReset => 4,
        ConnectionAborted => 5,
        NotConnected => 6,
        AddrInUse => 7,
        AddrNotAvailable => 8,
        BrokenPipe => 9,
        AlreadyExists => 10,
        WouldBlock => 11,
        InvalidInput => 12,
        InvalidData => 13,
        TimedOut => 14,
        WriteZero => 15,
        Interrupted => 16,
        UnexpectedEof => 17,
        Unsupported => 18,
        OutOfMemory => 19,
        Other => 20,
        _ => 255,
    }
}
fn content_code(e: &ContentError) -> u8 {
    use ContentError::*;
    match e {
        UnsupportedPolicy { .. } => 1,
        LengthOverflow => 2,
        UnexpectedEof => 3,
        TrailingBytes => 4,
        UnsupportedFraming => 5,
        UnsupportedMappingVersion { .. } => 6,
        InvalidMappingTag { .. } => 7,
        WrongLogicalRole => 8,
        InvalidRecord(_) => 9,
        ObjectLimitExceeded { .. } => 10,
        IdentityMismatch => 11,
        MissingObject => 12,
        PathNotFound => 13,
        ProviderFailure { .. } => 14,
        InvalidIdentityLength { .. } => 15,
        InvalidIdentityText => 16,
        InvalidRange { .. } => 17,
        NonCanonicalPagePartition => 18,
        NonCanonicalOrdering => 19,
        LengthMismatch { .. } => 20,
        MappingDepthExceeded => 21,
        Io => 22,
        OutputRejected => 23,
        BoundedCapacityExceeded { .. } => 24,
        InvalidEdit { .. } => 25,
        BatchCardinality { .. } => 26,
        InvalidPath => 27,
        PathLimitExceeded => 28,
        InvalidUtf8 => 29,
        RootMutation => 30,
        UnsupportedProfile { .. } => 31,
        ScopeMismatch { .. } => 32,
        ResourceUnavailable { .. } => 33,
        InvalidOrderingRecord(_) => 34,
        IncompleteOperation => 35,
    }
}
fn content_fields(w: &mut Writer, e: &ContentError) -> FrameResult<()> {
    use ContentError::*;
    match e {
        UnsupportedPolicy { field } => w.text(1, field)?,
        UnsupportedMappingVersion { version } => w.number(1, *version as u64)?,
        InvalidMappingTag { tag } => w.number(1, *tag as u64)?,
        InvalidRecord(s) | InvalidOrderingRecord(s) => w.text(1, s)?,
        ObjectLimitExceeded { limit, actual } => {
            w.number(1, *limit as u64)?;
            w.number(2, *actual as u64)?;
        }
        ProviderFailure { what }
        | InvalidEdit { what }
        | UnsupportedProfile { what }
        | ScopeMismatch { what }
        | ResourceUnavailable { what } => w.text(1, what)?,
        InvalidIdentityLength { expected, actual } => {
            w.number(1, *expected as u64)?;
            w.number(2, *actual as u64)?;
        }
        InvalidRange { start, end, length } => {
            w.number(1, *start)?;
            w.number(2, *end)?;
            w.number(3, *length)?;
        }
        LengthMismatch { expected, actual } => {
            w.number(1, *expected)?;
            w.number(2, *actual)?;
        }
        BoundedCapacityExceeded {
            what,
            limit,
            actual,
        } => {
            w.text(1, what)?;
            w.number(2, *limit)?;
            w.number(3, *actual)?;
        }
        BatchCardinality {
            requested,
            returned,
        } => {
            w.number(1, *requested as u64)?;
            w.number(2, *returned as u64)?;
        }
        _ => (),
    }
    Ok(())
}

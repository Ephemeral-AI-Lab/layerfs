use layerfs_sdk::{
    ExecutionId, OperationId, OperationReceipt, OutputStream, WorkspaceCommitResult,
    WorkspaceEndResult, WorkspaceSession,
};
use std::fmt;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommandSummary(pub String);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OperationPhase {
    Planned,
    Running,
    Draining,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProgressValue {
    pub completed: u64,
    pub total: Option<u64>,
}

#[derive(Clone, Debug)]
pub enum CommandResult {
    Database {
        role: String,
        location: String,
    },
    Id {
        kind: String,
        id: String,
    },
    Reference {
        outcome: String,
        id: String,
    },
    InitializedLayer {
        history_id: String,
        layer_id: String,
        root_id: String,
    },
    CreatedStack {
        history_id: String,
        stack_id: String,
        root_id: String,
    },
    Workspace(WorkspaceSession),
    WorkspaceCommit(WorkspaceCommitResult),
    WorkspaceEnd(WorkspaceEndResult),
    View {
        scope: crate::ViewScope,
        snapshot: crate::ViewSnapshot,
    },
    Unit,
}

#[derive(Clone, Debug)]
#[allow(clippy::large_enum_variant)]
pub enum CliEvent {
    Started {
        operation_id: OperationId,
        command: CommandSummary,
    },
    Progress {
        operation_id: OperationId,
        phase: OperationPhase,
        progress: ProgressValue,
        elapsed_ns: u64,
    },
    Output {
        execution_id: ExecutionId,
        sequence: u64,
        stream: OutputStream,
        bytes: Vec<u8>,
    },
    Snapshot {
        scope: crate::ViewScope,
        snapshot: crate::ViewSnapshot,
    },
    Finished {
        operation_id: OperationId,
        result: CliResult<CommandResult>,
        receipt: OperationReceipt,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CliError {
    Parse(String),
    Context(String),
    Invalid(String),
    NotFound,
    Conflict,
    HeadMoved,
    WrongHistory,
    ReadOnly,
    WorkspaceBusy,
    WorkspaceDirty,
    Interrupted,
    Integrity,
    Io(String),
}

pub type CliResult<T> = Result<T, CliError>;

impl fmt::Display for CliError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl std::error::Error for CliError {}

pub(crate) fn put_event(
    output: &mut crate::control::WireWriter,
    event: &CliEvent,
) -> CliResult<()> {
    match event {
        CliEvent::Started {
            operation_id,
            command,
        } => {
            output.byte(0);
            output.string(&operation_id.to_string())?;
            output.string(&command.0)?;
        }
        CliEvent::Progress {
            operation_id,
            phase,
            progress,
            elapsed_ns,
        } => {
            output.byte(1);
            output.string(&operation_id.to_string())?;
            output.byte(match phase {
                OperationPhase::Planned => 0,
                OperationPhase::Running => 1,
                OperationPhase::Draining => 2,
            });
            output.u64(progress.completed);
            put_optional_u64(output, progress.total);
            output.u64(*elapsed_ns);
        }
        CliEvent::Output {
            execution_id,
            sequence,
            stream,
            bytes,
        } => {
            output.byte(2);
            output.string(&execution_id.to_string())?;
            output.u64(*sequence);
            output.byte(match stream {
                OutputStream::Stdout => 0,
                OutputStream::Stderr => 1,
            });
            output.bytes(bytes)?;
        }
        CliEvent::Snapshot { scope, snapshot } => {
            output.byte(3);
            crate::query::put_scope(output, *scope);
            crate::query::put_snapshot(output, snapshot)?;
        }
        CliEvent::Finished {
            operation_id,
            result,
            receipt,
        } => {
            output.byte(4);
            output.string(&operation_id.to_string())?;
            output.bool(result.is_ok());
            match result {
                Ok(result) => put_result(output, result)?,
                Err(error) => put_error(output, error)?,
            }
            put_receipt(output, receipt)?;
        }
    }
    Ok(())
}

pub(crate) fn get_event(input: &mut crate::control::WireReader<'_>) -> CliResult<CliEvent> {
    Ok(match input.byte()? {
        0 => CliEvent::Started {
            operation_id: parse(&input.string()?, "operation ID")?,
            command: CommandSummary(input.string()?),
        },
        1 => CliEvent::Progress {
            operation_id: parse(&input.string()?, "operation ID")?,
            phase: match input.byte()? {
                0 => OperationPhase::Planned,
                1 => OperationPhase::Running,
                2 => OperationPhase::Draining,
                _ => return Err(CliError::Context("operation phase".to_owned())),
            },
            progress: ProgressValue {
                completed: input.u64()?,
                total: get_optional_u64(input)?,
            },
            elapsed_ns: input.u64()?,
        },
        2 => CliEvent::Output {
            execution_id: parse(&input.string()?, "execution ID")?,
            sequence: input.u64()?,
            stream: match input.byte()? {
                0 => OutputStream::Stdout,
                1 => OutputStream::Stderr,
                _ => return Err(CliError::Context("output stream".to_owned())),
            },
            bytes: input.bytes()?.to_vec(),
        },
        3 => CliEvent::Snapshot {
            scope: crate::query::get_scope(input)?,
            snapshot: crate::query::get_snapshot(input)?,
        },
        4 => {
            let operation_id = parse(&input.string()?, "operation ID")?;
            let result = if input.bool()? {
                Ok(get_result(input)?)
            } else {
                Err(get_error(input)?)
            };
            CliEvent::Finished {
                operation_id,
                result,
                receipt: get_receipt(input)?,
            }
        }
        _ => return Err(CliError::Context("CLI event".to_owned())),
    })
}

fn put_result(output: &mut crate::control::WireWriter, result: &CommandResult) -> CliResult<()> {
    match result {
        CommandResult::Database { role, location } => {
            output.byte(0);
            output.string(role)?;
            output.string(location)?;
        }
        CommandResult::Id { kind, id } => {
            output.byte(1);
            output.string(kind)?;
            output.string(id)?;
        }
        CommandResult::Reference { outcome, id } => {
            output.byte(2);
            output.string(outcome)?;
            output.string(id)?;
        }
        CommandResult::InitializedLayer {
            history_id,
            layer_id,
            root_id,
        } => {
            output.byte(3);
            output.string(history_id)?;
            output.string(layer_id)?;
            output.string(root_id)?;
        }
        CommandResult::CreatedStack {
            history_id,
            stack_id,
            root_id,
        } => {
            output.byte(4);
            output.string(history_id)?;
            output.string(stack_id)?;
            output.string(root_id)?;
        }
        CommandResult::Workspace(value) => {
            output.byte(5);
            crate::query::put_workspace_session(output, value)?;
        }
        CommandResult::WorkspaceCommit(value) => {
            output.byte(6);
            put_workspace_commit(output, value)?;
        }
        CommandResult::WorkspaceEnd(value) => {
            output.byte(7);
            output.string(&value.session_id.to_string())?;
            output.bool(value.discarded);
        }
        CommandResult::View { scope, snapshot } => {
            output.byte(8);
            crate::query::put_scope(output, *scope);
            crate::query::put_snapshot(output, snapshot)?;
        }
        CommandResult::Unit => output.byte(9),
    }
    Ok(())
}

fn get_result(input: &mut crate::control::WireReader<'_>) -> CliResult<CommandResult> {
    Ok(match input.byte()? {
        0 => CommandResult::Database {
            role: input.string()?,
            location: input.string()?,
        },
        1 => CommandResult::Id {
            kind: input.string()?,
            id: input.string()?,
        },
        2 => CommandResult::Reference {
            outcome: input.string()?,
            id: input.string()?,
        },
        3 => CommandResult::InitializedLayer {
            history_id: input.string()?,
            layer_id: input.string()?,
            root_id: input.string()?,
        },
        4 => CommandResult::CreatedStack {
            history_id: input.string()?,
            stack_id: input.string()?,
            root_id: input.string()?,
        },
        5 => CommandResult::Workspace(crate::query::get_workspace_session(input)?),
        6 => CommandResult::WorkspaceCommit(get_workspace_commit(input)?),
        7 => CommandResult::WorkspaceEnd(WorkspaceEndResult {
            session_id: parse(&input.string()?, "workspace ID")?,
            discarded: input.bool()?,
        }),
        8 => CommandResult::View {
            scope: crate::query::get_scope(input)?,
            snapshot: crate::query::get_snapshot(input)?,
        },
        9 => CommandResult::Unit,
        _ => return Err(CliError::Context("command result".to_owned())),
    })
}

fn put_workspace_commit(
    output: &mut crate::control::WireWriter,
    value: &WorkspaceCommitResult,
) -> CliResult<()> {
    match value {
        WorkspaceCommitResult::Created {
            previous_head,
            commit_id,
        } => {
            output.byte(0);
            output.string(&previous_head.to_string())?;
            output.string(&commit_id.to_string())?;
        }
        WorkspaceCommitResult::UpToDate { head } => {
            output.byte(1);
            output.string(&head.to_string())?;
        }
        WorkspaceCommitResult::HeadMoved { expected, actual } => {
            output.byte(2);
            output.string(&expected.to_string())?;
            output.string(&actual.to_string())?;
        }
    }
    Ok(())
}

fn get_workspace_commit(
    input: &mut crate::control::WireReader<'_>,
) -> CliResult<WorkspaceCommitResult> {
    Ok(match input.byte()? {
        0 => WorkspaceCommitResult::Created {
            previous_head: parse(&input.string()?, "commit ID")?,
            commit_id: parse(&input.string()?, "commit ID")?,
        },
        1 => WorkspaceCommitResult::UpToDate {
            head: parse(&input.string()?, "commit ID")?,
        },
        2 => WorkspaceCommitResult::HeadMoved {
            expected: parse(&input.string()?, "commit ID")?,
            actual: parse(&input.string()?, "commit ID")?,
        },
        _ => return Err(CliError::Context("workspace commit result".to_owned())),
    })
}

pub(crate) fn put_error(
    output: &mut crate::control::WireWriter,
    error: &CliError,
) -> CliResult<()> {
    let (kind, detail) = match error {
        CliError::Parse(value) => (0, Some(value.as_str())),
        CliError::Context(value) => (1, Some(value.as_str())),
        CliError::Invalid(value) => (2, Some(value.as_str())),
        CliError::NotFound => (3, None),
        CliError::Conflict => (4, None),
        CliError::HeadMoved => (5, None),
        CliError::WrongHistory => (6, None),
        CliError::ReadOnly => (7, None),
        CliError::WorkspaceBusy => (8, None),
        CliError::WorkspaceDirty => (9, None),
        CliError::Interrupted => (10, None),
        CliError::Integrity => (11, None),
        CliError::Io(value) => (12, Some(value.as_str())),
    };
    output.byte(kind);
    if let Some(detail) = detail {
        output.string(detail)?;
    }
    Ok(())
}

pub(crate) fn get_error(input: &mut crate::control::WireReader<'_>) -> CliResult<CliError> {
    Ok(match input.byte()? {
        0 => CliError::Parse(input.string()?),
        1 => CliError::Context(input.string()?),
        2 => CliError::Invalid(input.string()?),
        3 => CliError::NotFound,
        4 => CliError::Conflict,
        5 => CliError::HeadMoved,
        6 => CliError::WrongHistory,
        7 => CliError::ReadOnly,
        8 => CliError::WorkspaceBusy,
        9 => CliError::WorkspaceDirty,
        10 => CliError::Interrupted,
        11 => CliError::Integrity,
        12 => CliError::Io(input.string()?),
        _ => return Err(CliError::Context("CLI error".to_owned())),
    })
}

pub(crate) fn put_receipt(
    output: &mut crate::control::WireWriter,
    value: &OperationReceipt,
) -> CliResult<()> {
    output.string(&value.id.to_string())?;
    output.string(&value.name)?;
    output.byte(match value.outcome {
        layerfs_sdk::OperationOutcome::Succeeded => 0,
        layerfs_sdk::OperationOutcome::Failed => 1,
        layerfs_sdk::OperationOutcome::Interrupted => 2,
    });
    output.u64(value.queued_ns);
    output.u64(value.service_ns);
    put_count(output, value.fragments.len())?;
    for fragment in &value.fragments {
        output.u32(fragment.process_id);
        output.u64(fragment.started_ns);
        output.u64(fragment.elapsed_ns);
    }
    put_count(output, value.storage.len())?;
    for receipt in &value.storage {
        put_storage_receipt(output, receipt)?;
    }
    Ok(())
}

pub(crate) fn get_receipt(
    input: &mut crate::control::WireReader<'_>,
) -> CliResult<OperationReceipt> {
    Ok(OperationReceipt {
        id: parse(&input.string()?, "operation ID")?,
        name: input.string()?,
        outcome: match input.byte()? {
            0 => layerfs_sdk::OperationOutcome::Succeeded,
            1 => layerfs_sdk::OperationOutcome::Failed,
            2 => layerfs_sdk::OperationOutcome::Interrupted,
            _ => return Err(CliError::Context("operation outcome".to_owned())),
        },
        queued_ns: input.u64()?,
        service_ns: input.u64()?,
        fragments: (0..input.count()?)
            .map(|_| {
                Ok(layerfs_sdk::TimingFragment {
                    process_id: input.u32()?,
                    started_ns: input.u64()?,
                    elapsed_ns: input.u64()?,
                })
            })
            .collect::<CliResult<Vec<_>>>()?,
        storage: (0..input.count()?)
            .map(|_| get_storage_receipt(input))
            .collect::<CliResult<Vec<_>>>()?,
    })
}

fn put_storage_receipt(
    output: &mut crate::control::WireWriter,
    value: &layerfs_sdk::StorageReceipt,
) -> CliResult<()> {
    match value {
        layerfs_sdk::StorageReceipt::Local(value) => {
            output.byte(0);
            put_local_objects(output, value.objects);
            put_admission_map(output, &value.facts)?;
            put_database(output, value.database);
        }
        layerfs_sdk::StorageReceipt::Transfer(value) => {
            output.byte(1);
            put_transfer_set(output, value.objects.set);
            output.u64(value.objects.known_subtrees_pruned);
            put_transfer_map(output, &value.facts)?;
            put_database(output, value.database);
            let transport = value.transport;
            output.u64(transport.object_membership_pages);
            output.u64(transport.typed_membership_pages);
            output.u64(transport.request_reply_turns);
            output.u64(transport.one_way_payload_batches);
            output.u64(transport.command_frames);
            output.u64(transport.payload_frames);
            output.u64(transport.reply_frames);
            output.u64(transport.peak_buffer_bytes);
        }
    }
    Ok(())
}

fn get_storage_receipt(
    input: &mut crate::control::WireReader<'_>,
) -> CliResult<layerfs_sdk::StorageReceipt> {
    Ok(match input.byte()? {
        0 => layerfs_sdk::StorageReceipt::Local(layerfs_sdk::LocalAdmissionReceipt {
            objects: get_local_objects(input)?,
            facts: get_admission_map(input)?,
            database: get_database(input)?,
        }),
        1 => layerfs_sdk::StorageReceipt::Transfer(layerfs_sdk::TransferReceipt {
            objects: layerfs_sdk::ObjectTransferReceipt {
                set: get_transfer_set(input)?,
                known_subtrees_pruned: input.u64()?,
            },
            facts: get_transfer_map(input)?,
            database: get_database(input)?,
            transport: layerfs_sdk::TransportReceipt {
                object_membership_pages: input.u64()?,
                typed_membership_pages: input.u64()?,
                request_reply_turns: input.u64()?,
                one_way_payload_batches: input.u64()?,
                command_frames: input.u64()?,
                payload_frames: input.u64()?,
                reply_frames: input.u64()?,
                peak_buffer_bytes: input.u64()?,
            },
        }),
        _ => return Err(CliError::Context("storage receipt".to_owned())),
    })
}

fn put_admission_map(
    output: &mut crate::control::WireWriter,
    values: &std::collections::BTreeMap<layerfs_sdk::FactKind, layerfs_sdk::AdmissionSetReceipt>,
) -> CliResult<()> {
    put_count(output, values.len())?;
    for (kind, value) in values {
        crate::query::put_fact_kind(output, *kind);
        put_admission_set(output, *value);
    }
    Ok(())
}

fn get_admission_map(
    input: &mut crate::control::WireReader<'_>,
) -> CliResult<std::collections::BTreeMap<layerfs_sdk::FactKind, layerfs_sdk::AdmissionSetReceipt>>
{
    (0..input.count()?)
        .map(|_| {
            Ok((
                crate::query::get_fact_kind(input)?,
                get_admission_set(input)?,
            ))
        })
        .collect()
}

fn put_transfer_map(
    output: &mut crate::control::WireWriter,
    values: &std::collections::BTreeMap<layerfs_sdk::FactKind, layerfs_sdk::TransferSetReceipt>,
) -> CliResult<()> {
    put_count(output, values.len())?;
    for (kind, value) in values {
        crate::query::put_fact_kind(output, *kind);
        put_transfer_set(output, *value);
    }
    Ok(())
}

fn get_transfer_map(
    input: &mut crate::control::WireReader<'_>,
) -> CliResult<std::collections::BTreeMap<layerfs_sdk::FactKind, layerfs_sdk::TransferSetReceipt>> {
    (0..input.count()?)
        .map(|_| {
            Ok((
                crate::query::get_fact_kind(input)?,
                get_transfer_set(input)?,
            ))
        })
        .collect()
}

fn put_local_objects(
    output: &mut crate::control::WireWriter,
    value: layerfs_sdk::LocalObjectReceipt,
) {
    output.u64(value.candidate_ids);
    output.u64(value.candidate_bytes);
    output.u64(value.inserted_ids);
    output.u64(value.inserted_bytes);
    output.u64(value.reused_ids);
    output.u64(value.reused_bytes);
}

fn get_local_objects(
    input: &mut crate::control::WireReader<'_>,
) -> CliResult<layerfs_sdk::LocalObjectReceipt> {
    Ok(layerfs_sdk::LocalObjectReceipt {
        candidate_ids: input.u64()?,
        candidate_bytes: input.u64()?,
        inserted_ids: input.u64()?,
        inserted_bytes: input.u64()?,
        reused_ids: input.u64()?,
        reused_bytes: input.u64()?,
    })
}

fn put_admission_set(
    output: &mut crate::control::WireWriter,
    value: layerfs_sdk::AdmissionSetReceipt,
) {
    output.u64(value.inserted_ids);
    output.u64(value.inserted_bytes);
    output.u64(value.raced_existing_ids);
    output.u64(value.raced_existing_bytes);
}

fn get_admission_set(
    input: &mut crate::control::WireReader<'_>,
) -> CliResult<layerfs_sdk::AdmissionSetReceipt> {
    Ok(layerfs_sdk::AdmissionSetReceipt {
        inserted_ids: input.u64()?,
        inserted_bytes: input.u64()?,
        raced_existing_ids: input.u64()?,
        raced_existing_bytes: input.u64()?,
    })
}

fn put_transfer_set(
    output: &mut crate::control::WireWriter,
    value: layerfs_sdk::TransferSetReceipt,
) {
    output.u64(value.announced_ids);
    output.u64(value.missing_ids);
    output.u64(value.sent_ids);
    output.u64(value.sent_bytes);
    output.u64(value.inserted_ids);
    output.u64(value.inserted_bytes);
    output.u64(value.raced_existing_ids);
    output.u64(value.raced_existing_bytes);
}

fn get_transfer_set(
    input: &mut crate::control::WireReader<'_>,
) -> CliResult<layerfs_sdk::TransferSetReceipt> {
    Ok(layerfs_sdk::TransferSetReceipt {
        announced_ids: input.u64()?,
        missing_ids: input.u64()?,
        sent_ids: input.u64()?,
        sent_bytes: input.u64()?,
        inserted_ids: input.u64()?,
        inserted_bytes: input.u64()?,
        raced_existing_ids: input.u64()?,
        raced_existing_bytes: input.u64()?,
    })
}

fn put_database(output: &mut crate::control::WireWriter, value: layerfs_sdk::DatabaseReceipt) {
    output.u64(value.write_transactions);
    output.u64(value.rollback_transactions);
    output.u64(value.object_admission_transactions);
    output.u64(value.fact_admission_transactions);
    output.u64(value.visibility_transactions);
    output.u64(value.commit_sync_elapsed_ns);
}

fn get_database(
    input: &mut crate::control::WireReader<'_>,
) -> CliResult<layerfs_sdk::DatabaseReceipt> {
    Ok(layerfs_sdk::DatabaseReceipt {
        write_transactions: input.u64()?,
        rollback_transactions: input.u64()?,
        object_admission_transactions: input.u64()?,
        fact_admission_transactions: input.u64()?,
        visibility_transactions: input.u64()?,
        commit_sync_elapsed_ns: input.u64()?,
    })
}

fn put_optional_u64(output: &mut crate::control::WireWriter, value: Option<u64>) {
    output.bool(value.is_some());
    if let Some(value) = value {
        output.u64(value);
    }
}

fn get_optional_u64(input: &mut crate::control::WireReader<'_>) -> CliResult<Option<u64>> {
    input.bool()?.then(|| input.u64()).transpose()
}

fn put_count(output: &mut crate::control::WireWriter, value: usize) -> CliResult<()> {
    output.u32(
        value
            .try_into()
            .map_err(|_| CliError::Context("collection length".to_owned()))?,
    );
    Ok(())
}

fn parse<T: std::str::FromStr>(value: &str, name: &str) -> CliResult<T> {
    value
        .parse()
        .map_err(|_| CliError::Context(name.to_owned()))
}

pub(crate) fn put_monitor_scope(
    output: &mut crate::control::WireWriter,
    scope: layerfs_sdk::MonitorScope,
) -> CliResult<()> {
    use layerfs_sdk::MonitorScope;
    match scope {
        MonitorScope::Databases => output.byte(0),
        MonitorScope::Dedup { route } => {
            output.byte(1);
            crate::query::put_optional_string(
                output,
                route.as_ref().map(ToString::to_string).as_deref(),
            )?;
        }
        MonitorScope::Workspace(id) => {
            output.byte(2);
            crate::query::put_optional_string(
                output,
                id.as_ref().map(ToString::to_string).as_deref(),
            )?;
        }
        MonitorScope::Branch(id) => {
            output.byte(3);
            output.string(&id.to_string())?;
        }
        MonitorScope::Operation(id) => {
            output.byte(4);
            crate::query::put_optional_string(
                output,
                id.as_ref().map(ToString::to_string).as_deref(),
            )?;
        }
        MonitorScope::Process => output.byte(5),
    }
    Ok(())
}

pub(crate) fn get_monitor_scope(
    input: &mut crate::control::WireReader<'_>,
) -> CliResult<layerfs_sdk::MonitorScope> {
    use layerfs_sdk::MonitorScope;
    Ok(match input.byte()? {
        0 => MonitorScope::Databases,
        1 => MonitorScope::Dedup {
            route: crate::query::get_optional_string(input)?
                .map(|value| parse(&value, "route ID"))
                .transpose()?,
        },
        2 => MonitorScope::Workspace(
            crate::query::get_optional_string(input)?
                .map(|value| parse(&value, "workspace ID"))
                .transpose()?,
        ),
        3 => MonitorScope::Branch(parse(&input.string()?, "branch ID")?),
        4 => MonitorScope::Operation(
            crate::query::get_optional_string(input)?
                .map(|value| parse(&value, "operation ID"))
                .transpose()?,
        ),
        5 => MonitorScope::Process,
        _ => return Err(CliError::Context("monitor scope".to_owned())),
    })
}

pub(crate) fn put_monitor(
    output: &mut crate::control::WireWriter,
    value: &crate::MonitorView,
) -> CliResult<()> {
    use crate::MonitorView;
    match value {
        MonitorView::Databases(values) => {
            output.byte(0);
            crate::query::put_count(output, values.len())?;
            for value in values {
                output.string(&value.role)?;
                output.string(&value.location)?;
                output.u64(value.database_bytes);
                output.u64(value.wal_bytes);
                output.u64(value.shm_bytes);
            }
        }
        MonitorView::Dedup(values) => {
            output.byte(1);
            crate::query::put_count(output, values.len())?;
            for value in values {
                output.string(&value.route)?;
                output.u64(value.route_cas_bytes);
                output.u64(value.union_cas_bytes);
                output.u64(value.cross_store_placement_bytes);
                output.f64(value.placement_factor);
                crate::query::put_count(output, value.placements.len())?;
                for placement in &value.placements {
                    output.string(&placement.role)?;
                    output.u64(placement.object_count);
                    output.u64(placement.encoded_bytes);
                }
            }
        }
        MonitorView::Workspaces(values) => {
            output.byte(2);
            crate::query::put_count(output, values.len())?;
            for value in values {
                crate::query::put_workspace_summary(output, value)?;
            }
        }
        MonitorView::Branch(value) => {
            output.byte(3);
            output.bool(value.is_some());
            if let Some(value) = value {
                crate::query::put_store_fact(output, value)?;
            }
        }
        MonitorView::Operations(values) => {
            output.byte(4);
            crate::query::put_count(output, values.len())?;
            for value in values {
                put_receipt(output, value)?;
            }
        }
        MonitorView::Process {
            process_id,
            resident_bytes,
            available_parallelism,
        } => {
            output.byte(5);
            output.u32(*process_id);
            crate::query::put_optional_u64(output, *resident_bytes);
            output.u64(
                (*available_parallelism)
                    .try_into()
                    .map_err(|_| CliError::Context("parallelism".to_owned()))?,
            );
        }
    }
    Ok(())
}

pub(crate) fn get_monitor(
    input: &mut crate::control::WireReader<'_>,
) -> CliResult<crate::MonitorView> {
    use crate::{DatabaseView, DedupView, MonitorView, PlacementView};
    Ok(match input.byte()? {
        0 => MonitorView::Databases(
            (0..input.count()?)
                .map(|_| {
                    Ok(DatabaseView {
                        role: input.string()?,
                        location: input.string()?,
                        database_bytes: input.u64()?,
                        wal_bytes: input.u64()?,
                        shm_bytes: input.u64()?,
                    })
                })
                .collect::<CliResult<Vec<_>>>()?,
        ),
        1 => MonitorView::Dedup(
            (0..input.count()?)
                .map(|_| {
                    Ok(DedupView {
                        route: input.string()?,
                        route_cas_bytes: input.u64()?,
                        union_cas_bytes: input.u64()?,
                        cross_store_placement_bytes: input.u64()?,
                        placement_factor: input.f64()?,
                        placements: (0..input.count()?)
                            .map(|_| {
                                Ok(PlacementView {
                                    role: input.string()?,
                                    object_count: input.u64()?,
                                    encoded_bytes: input.u64()?,
                                })
                            })
                            .collect::<CliResult<Vec<_>>>()?,
                    })
                })
                .collect::<CliResult<Vec<_>>>()?,
        ),
        2 => MonitorView::Workspaces(
            (0..input.count()?)
                .map(|_| crate::query::get_workspace_summary(input))
                .collect::<CliResult<Vec<_>>>()?,
        ),
        3 => MonitorView::Branch(
            input
                .bool()?
                .then(|| crate::query::get_store_fact(input))
                .transpose()?,
        ),
        4 => MonitorView::Operations(
            (0..input.count()?)
                .map(|_| get_receipt(input))
                .collect::<CliResult<Vec<_>>>()?,
        ),
        5 => MonitorView::Process {
            process_id: input.u32()?,
            resident_bytes: crate::query::get_optional_u64(input)?,
            available_parallelism: input
                .u64()?
                .try_into()
                .map_err(|_| CliError::Context("parallelism".to_owned()))?,
        },
        _ => return Err(CliError::Context("monitor snapshot".to_owned())),
    })
}

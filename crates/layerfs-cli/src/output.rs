use crate::{
    CliEvent, CliResult, CommandResult, MonitorView, StoreFact, StoreSnapshot, ViewSnapshot,
};
use layerfs_sdk::{OperationReceipt, OutputStream, WorkspaceCommitResult, WorkspacePlacement};
use std::fmt::Write as _;
use std::io::Write;

pub(crate) fn render(event: &CliEvent, json: bool, output: &mut impl Write) -> CliResult<()> {
    if json {
        writeln!(output, "{}", json_event(event)).map_err(io)
    } else {
        human_event(event, output)
    }
}

fn human_event(event: &CliEvent, output: &mut impl Write) -> CliResult<()> {
    match event {
        CliEvent::Started {
            operation_id,
            command,
        } => writeln!(output, "STARTED {operation_id} {}", command.0).map_err(io),
        CliEvent::Progress {
            operation_id,
            phase,
            progress,
            elapsed_ns,
        } => writeln!(
            output,
            "PROGRESS {operation_id} {phase:?} {} {} {elapsed_ns}",
            progress.completed,
            progress
                .total
                .map(|value| value.to_string())
                .unwrap_or_else(|| "-".to_owned())
        )
        .map_err(io),
        CliEvent::Output {
            execution_id,
            sequence,
            stream,
            bytes,
        } => writeln!(
            output,
            "OUTPUT {execution_id} {sequence} {stream:?} {}",
            hex(bytes)
        )
        .map_err(io),
        CliEvent::Snapshot { scope, snapshot } => human_snapshot(*scope, snapshot, output),
        CliEvent::Finished {
            operation_id,
            result,
            receipt,
        } => {
            match result {
                Ok(result) => writeln!(output, "FINISHED {}", human_result(result)).map_err(io)?,
                Err(error) => {
                    writeln!(output, "FAILED {} detail={}", error_code(error), error).map_err(io)?
                }
            }
            writeln!(
                output,
                "RECEIPT operation={operation_id} service_ns={}",
                receipt.service_ns
            )
            .map_err(io)
        }
    }
}

fn human_result(result: &CommandResult) -> String {
    match result {
        CommandResult::Database { role, location } => format!("CONNECTED {role} {location}"),
        CommandResult::Id { kind, id } => format!("CREATED {kind} {id}"),
        CommandResult::Reference { outcome, id } => format!("{outcome} {id}"),
        CommandResult::InitializedLayer { layer_id, .. } => {
            format!("CREATED layer {layer_id}")
        }
        CommandResult::CreatedStack { stack_id, .. } => format!("CREATED stack {stack_id}"),
        CommandResult::Workspace(workspace) => format!("CREATED workspace {}", workspace.id),
        CommandResult::WorkspaceCommit(result) => match result {
            WorkspaceCommitResult::Created {
                previous_head,
                commit_id,
            } => format!("CREATED commit {commit_id} previous={previous_head}"),
            WorkspaceCommitResult::UpToDate { head } => format!("UP_TO_DATE {head}"),
            WorkspaceCommitResult::HeadMoved { expected, actual } => {
                format!("HEAD_MOVED expected={expected} actual={actual}")
            }
        },
        CommandResult::WorkspaceEnd(result) => {
            format!("ENDED {} discarded={}", result.session_id, result.discarded)
        }
        CommandResult::View { .. } | CommandResult::Unit => "OK".to_owned(),
    }
}

fn human_snapshot(
    scope: crate::ViewScope,
    snapshot: &ViewSnapshot,
    output: &mut impl Write,
) -> CliResult<()> {
    writeln!(output, "SNAPSHOT {scope:?}").map_err(io)?;
    match snapshot {
        ViewSnapshot::Topology(entries) => entries.iter().try_for_each(|entry| {
            writeln!(
                output,
                "{} {} parent={} active={}",
                entry.role,
                entry.location,
                entry.parent.as_deref().unwrap_or("-"),
                entry.active
            )
            .map_err(io)
        }),
        ViewSnapshot::Workspaces(values) => values.iter().try_for_each(|value| {
            writeln!(
                output,
                "{} branch={} head={} state={:?} dirty={}",
                value.id, value.branch_id, value.pinned_head, value.state, value.dirty
            )
            .map_err(io)
        }),
        ViewSnapshot::Workspace(value) => {
            writeln!(
                output,
                "{} branch={} head={} state={:?} generation={} placement={} projection={:?}",
                value.session.id,
                value.session.branch_id,
                value.session.pinned_head,
                value.session.state,
                value.mutation_generation,
                placement(&value.session.placement),
                value.session.projection
            )
            .map_err(io)?;
            value.executions.iter().try_for_each(|execution| {
                writeln!(
                    output,
                    "execution={} running={} receipt={:?}",
                    execution.id, execution.running, execution.receipt
                )
                .map_err(io)
            })
        }
        ViewSnapshot::WorkspaceDiff(value) => writeln!(
            output,
            "{} dirty={} generation={}",
            value.session_id, value.dirty, value.mutation_generation
        )
        .map_err(io),
        ViewSnapshot::Output(page) => {
            for chunk in &page.chunks {
                writeln!(
                    output,
                    "OUTPUT {} {:?} {}",
                    chunk.sequence,
                    chunk.stream,
                    String::from_utf8_lossy(&chunk.bytes)
                )
                .map_err(io)?;
            }
            writeln!(
                output,
                "OUTPUT_PAGE next={} truncated={} exited={} receipt={:?}",
                page.next_sequence, page.truncated, page.exited, page.receipt
            )
            .map_err(io)
        }
        ViewSnapshot::Monitor(value) => human_monitor(value, output),
        ViewSnapshot::Store(value) => human_store(value, output),
    }
}

fn human_store(value: &StoreSnapshot, output: &mut impl Write) -> CliResult<()> {
    match value {
        StoreSnapshot::Page { facts, next } => {
            for fact in facts {
                writeln!(output, "{}", human_store_fact(fact)).map_err(io)?;
            }
            writeln!(output, "PAGE next={}", next.is_some()).map_err(io)
        }
        StoreSnapshot::Fact(Some(fact)) => {
            writeln!(output, "{}", human_store_fact(fact)).map_err(io)
        }
        StoreSnapshot::Fact(None) => writeln!(output, "NOT_FOUND").map_err(io),
        StoreSnapshot::CommitDiff(changes) => changes.iter().try_for_each(|change| {
            writeln!(
                output,
                "inode={} before={:?} after={:?}",
                change.inode, change.before, change.after
            )
            .map_err(io)
        }),
    }
}

fn human_store_fact(fact: &StoreFact) -> String {
    let mut value = format!("{:?} {}", fact.kind, fact.id).to_lowercase();
    for (name, field) in &fact.fields {
        write!(value, " {name}={field}").unwrap();
    }
    value
}

fn human_monitor(value: &MonitorView, output: &mut impl Write) -> CliResult<()> {
    match value {
        MonitorView::Databases(values) => values.iter().try_for_each(|value| {
            writeln!(
                output,
                "{} {} db={} wal={} shm={}",
                value.role, value.location, value.database_bytes, value.wal_bytes, value.shm_bytes
            )
            .map_err(io)
        }),
        MonitorView::Dedup(values) => values.iter().try_for_each(|value| {
            writeln!(
                output,
                "route={} route_cas={} union_cas={} cross_store_placement={} placement={}",
                value.route,
                value.route_cas_bytes,
                value.union_cas_bytes,
                value.cross_store_placement_bytes,
                value.placement_factor
            )
            .map_err(io)
        }),
        MonitorView::Workspaces(values) => values.iter().try_for_each(|value| {
            writeln!(
                output,
                "{} branch={} state={:?} dirty={}",
                value.id, value.branch_id, value.state, value.dirty
            )
            .map_err(io)
        }),
        MonitorView::Branch(value) => writeln!(output, "branch={value:?}").map_err(io),
        MonitorView::Operations(values) => values
            .iter()
            .try_for_each(|value| writeln!(output, "{}", human_receipt(value)).map_err(io)),
        MonitorView::Process {
            process_id,
            resident_bytes,
            available_parallelism,
        } => writeln!(
            output,
            "pid={process_id} rss={} parallelism={available_parallelism}",
            resident_bytes
                .map(|value| value.to_string())
                .unwrap_or_else(|| "unavailable".to_owned())
        )
        .map_err(io),
    }
}

fn human_receipt(value: &OperationReceipt) -> String {
    format!(
        "{} outcome={:?} queued_ns={} service_ns={} fragments={}",
        value.id,
        value.outcome,
        value.queued_ns,
        value.service_ns,
        value.fragments.len()
    )
}

fn json_event(event: &CliEvent) -> String {
    let mut text = format!("{{\"schema_version\":{}", crate::JSON_SCHEMA_VERSION);
    match event {
        CliEvent::Started {
            operation_id,
            command,
        } => write!(
            text,
            ",\"event\":\"started\",\"operation_id\":{},\"command\":{}",
            json_string(&operation_id.to_string()),
            json_string(&command.0)
        )
        .unwrap(),
        CliEvent::Progress {
            operation_id,
            phase,
            progress,
            elapsed_ns,
        } => write!(
            text,
            ",\"event\":\"progress\",\"operation_id\":{},\"phase\":{},\"completed\":{},\"total\":{},\"elapsed_ns\":{}",
            json_string(&operation_id.to_string()),
            json_string(&format!("{phase:?}")),
            progress.completed,
            progress.total.map(|value| value.to_string()).unwrap_or_else(|| "null".to_owned()),
            elapsed_ns
        )
        .unwrap(),
        CliEvent::Output {
            execution_id,
            sequence,
            stream,
            bytes,
        } => write!(
            text,
            ",\"event\":\"output\",\"execution_id\":{},\"sequence\":{},\"stream\":{},\"bytes_hex\":{}",
            json_string(&execution_id.to_string()),
            sequence,
            json_string(&format!("{stream:?}").to_lowercase()),
            json_string(&hex(bytes))
        )
        .unwrap(),
        CliEvent::Snapshot { scope, snapshot } => write!(
            text,
            ",\"event\":\"snapshot\",\"scope\":{},\"snapshot\":{}",
            json_string(&format!("{scope:?}").to_lowercase()),
            json_snapshot(snapshot)
        )
        .unwrap(),
        CliEvent::Finished {
            operation_id,
            result,
            receipt,
        } => {
            write!(
                text,
                ",\"event\":\"finished\",\"operation_id\":{},\"result\":{},\"receipt\":{}",
                json_string(&operation_id.to_string()),
                match result {
                    Ok(result) => json_result(result),
                    Err(error) => format!(
                        "{{\"ok\":false,\"code\":{},\"detail\":{}}}",
                        json_string(error_code(error)),
                        json_string(&error.to_string())
                    ),
                },
                receipt.to_json()
            )
            .unwrap();
        }
    }
    text.push('}');
    text
}

fn json_result(result: &CommandResult) -> String {
    match result {
        CommandResult::Database { role, location } => format!(
            "{{\"ok\":true,\"outcome\":\"CONNECTED\",\"role\":{},\"location\":{}}}",
            json_string(role),
            json_string(location)
        ),
        CommandResult::Id { kind, id } => format!(
            "{{\"ok\":true,\"outcome\":\"CREATED\",\"kind\":{},\"id\":{}}}",
            json_string(kind),
            json_string(id)
        ),
        CommandResult::Reference { outcome, id } => format!(
            "{{\"ok\":true,\"outcome\":{},\"id\":{}}}",
            json_string(outcome),
            json_string(id)
        ),
        CommandResult::InitializedLayer {
            history_id,
            layer_id,
            root_id,
        } => format!(
            "{{\"ok\":true,\"outcome\":\"CREATED\",\"kind\":\"layer\",\"id\":{},\"history_id\":{},\"root_id\":{}}}",
            json_string(layer_id),
            json_string(history_id),
            json_string(root_id)
        ),
        CommandResult::CreatedStack {
            history_id,
            stack_id,
            root_id,
        } => format!(
            "{{\"ok\":true,\"outcome\":\"CREATED\",\"kind\":\"stack\",\"id\":{},\"history_id\":{},\"root_id\":{}}}",
            json_string(stack_id),
            json_string(history_id),
            json_string(root_id)
        ),
        CommandResult::Workspace(workspace) => format!(
            "{{\"ok\":true,\"outcome\":\"CREATED\",\"kind\":\"workspace\",\"id\":{}}}",
            json_string(&workspace.id.to_string())
        ),
        CommandResult::WorkspaceCommit(result) => match result {
            WorkspaceCommitResult::Created {
                previous_head,
                commit_id,
            } => format!(
                "{{\"ok\":true,\"outcome\":\"CREATED\",\"kind\":\"commit\",\"id\":{},\"previous_head\":{}}}",
                json_string(&commit_id.to_string()),
                json_string(&previous_head.to_string())
            ),
            WorkspaceCommitResult::UpToDate { head } => format!(
                "{{\"ok\":true,\"outcome\":\"UP_TO_DATE\",\"head\":{}}}",
                json_string(&head.to_string())
            ),
            WorkspaceCommitResult::HeadMoved { expected, actual } => format!(
                "{{\"ok\":true,\"outcome\":\"HEAD_MOVED\",\"expected\":{},\"actual\":{}}}",
                json_string(&expected.to_string()),
                json_string(&actual.to_string())
            ),
        },
        CommandResult::WorkspaceEnd(result) => format!(
            "{{\"ok\":true,\"outcome\":\"ENDED\",\"id\":{},\"discarded\":{}}}",
            json_string(&result.session_id.to_string()),
            result.discarded
        ),
        CommandResult::View { .. } | CommandResult::Unit => {
            "{\"ok\":true,\"outcome\":\"OK\"}".to_owned()
        }
    }
}

fn json_snapshot(snapshot: &ViewSnapshot) -> String {
    match snapshot {
        ViewSnapshot::Topology(values) => json_array(values.iter().map(|value| {
            format!(
                "{{\"role\":{},\"location\":{},\"parent\":{},\"active\":{}}}",
                json_string(&value.role),
                json_string(&value.location),
                value
                    .parent
                    .as_ref()
                    .map(|value| json_string(value))
                    .unwrap_or_else(|| "null".to_owned()),
                value.active
            )
        })),
        ViewSnapshot::Workspaces(values) => json_array(values.iter().map(|value| {
            format!(
                "{{\"id\":{},\"branch_id\":{},\"pinned_head\":{},\"state\":{},\"dirty\":{}}}",
                json_string(&value.id.to_string()),
                json_string(&value.branch_id.to_string()),
                json_string(&value.pinned_head.to_string()),
                json_string(&format!("{:?}", value.state).to_lowercase()),
                value.dirty
            )
        })),
        ViewSnapshot::Workspace(value) => format!(
            "{{\"id\":{},\"branch_id\":{},\"pinned_head\":{},\"state\":{},\"mutation_generation\":{},\"placement\":{},\"projection\":{},\"executions\":{}}}",
            json_string(&value.session.id.to_string()),
            json_string(&value.session.branch_id.to_string()),
            json_string(&value.session.pinned_head.to_string()),
            json_string(&format!("{:?}", value.session.state).to_lowercase()),
            value.mutation_generation,
            json_string(&placement(&value.session.placement)),
            json_string(&format!("{:?}", value.session.projection).to_lowercase()),
            json_array(value.executions.iter().map(|execution| format!(
                "{{\"id\":{},\"running\":{}}}",
                json_string(&execution.id.to_string()),
                execution.running
            )))
        ),
        ViewSnapshot::WorkspaceDiff(value) => format!(
            "{{\"session_id\":{},\"dirty\":{},\"mutation_generation\":{}}}",
            json_string(&value.session_id.to_string()),
            value.dirty,
            value.mutation_generation
        ),
        ViewSnapshot::Output(page) => format!(
            "{{\"chunks\":{},\"next_sequence\":{},\"truncated\":{},\"exited\":{}}}",
            json_array(page.chunks.iter().map(|chunk| format!(
                "{{\"sequence\":{},\"stream\":{},\"bytes_hex\":{}}}",
                chunk.sequence,
                json_string(match chunk.stream { OutputStream::Stdout => "stdout", OutputStream::Stderr => "stderr" }),
                json_string(&hex(&chunk.bytes))
            ))),
            page.next_sequence,
            page.truncated,
            page.exited
        ),
        ViewSnapshot::Monitor(value) => json_monitor(value),
        ViewSnapshot::Store(value) => json_store(value),
    }
}

fn json_store(value: &StoreSnapshot) -> String {
    match value {
        StoreSnapshot::Page { facts, next } => format!(
            "{{\"facts\":{},\"next\":{}}}",
            json_array(facts.iter().map(json_store_fact)),
            next.as_ref()
                .map(|cursor| json_string(&cursor.to_string()))
                .unwrap_or_else(|| "null".to_owned())
        ),
        StoreSnapshot::Fact(value) => value
            .as_ref()
            .map(json_store_fact)
            .unwrap_or_else(|| "null".to_owned()),
        StoreSnapshot::CommitDiff(values) => json_array(values.iter().map(|value| {
            format!(
                "{{\"inode\":{},\"before\":{},\"after\":{}}}",
                json_string(&value.inode),
                value
                    .before
                    .as_ref()
                    .map(|id| json_string(id))
                    .unwrap_or_else(|| "null".to_owned()),
                value
                    .after
                    .as_ref()
                    .map(|id| json_string(id))
                    .unwrap_or_else(|| "null".to_owned())
            )
        })),
    }
}

fn json_store_fact(fact: &StoreFact) -> String {
    let fields = fact
        .fields
        .iter()
        .map(|(name, value)| format!("{}:{}", json_string(name), json_string(value)))
        .collect::<Vec<_>>()
        .join(",");
    format!(
        "{{\"kind\":{},\"id\":{},\"fields\":{{{fields}}}}}",
        json_string(&format!("{:?}", fact.kind).to_lowercase()),
        json_string(&fact.id),
    )
}

fn json_monitor(value: &MonitorView) -> String {
    match value {
        MonitorView::Databases(values) => json_array(values.iter().map(|value| {
            format!(
                "{{\"role\":{},\"location\":{},\"database_bytes\":{},\"wal_bytes\":{},\"shm_bytes\":{}}}",
                json_string(&value.role),
                json_string(&value.location),
                value.database_bytes,
                value.wal_bytes,
                value.shm_bytes
            )
        })),
        MonitorView::Dedup(values) => json_array(values.iter().map(|value| {
            format!(
                "{{\"route\":{},\"route_cas_bytes\":{},\"union_cas_bytes\":{},\"cross_store_placement_bytes\":{},\"placement_factor\":{},\"placements\":{}}}",
                json_string(&value.route),
                value.route_cas_bytes,
                value.union_cas_bytes,
                value.cross_store_placement_bytes,
                value.placement_factor,
                json_array(value.placements.iter().map(|placement| format!(
                    "{{\"role\":{},\"object_count\":{},\"encoded_bytes\":{}}}",
                    json_string(&placement.role), placement.object_count, placement.encoded_bytes
                )))
            )
        })),
        MonitorView::Workspaces(values) => json_array(values.iter().map(|value| {
            format!(
                "{{\"id\":{},\"branch_id\":{},\"state\":{},\"dirty\":{}}}",
                json_string(&value.id.to_string()),
                json_string(&value.branch_id.to_string()),
                json_string(&format!("{:?}", value.state).to_lowercase()),
                value.dirty
            )
        })),
        MonitorView::Branch(value) => value
            .as_ref()
            .map(json_store_fact)
            .unwrap_or_else(|| "null".to_owned()),
        MonitorView::Operations(values) => {
            json_array(values.iter().map(OperationReceipt::to_json))
        }
        MonitorView::Process {
            process_id,
            resident_bytes,
            available_parallelism,
        } => format!(
            "{{\"process_id\":{},\"resident_bytes\":{},\"available_parallelism\":{}}}",
            process_id,
            resident_bytes
                .map(|value| value.to_string())
                .unwrap_or_else(|| "null".to_owned()),
            available_parallelism
        ),
    }
}

fn placement(value: &WorkspacePlacement) -> String {
    match value {
        WorkspacePlacement::Host { root } => format!("host:{}", root.display()),
        WorkspacePlacement::Container { container_id, root } => {
            format!("container:{}:{}", container_id.0, root.display())
        }
    }
}

fn json_array(values: impl IntoIterator<Item = String>) -> String {
    format!("[{}]", values.into_iter().collect::<Vec<_>>().join(","))
}

fn json_string(value: &str) -> String {
    let mut output = String::from("\"");
    for character in value.chars() {
        match character {
            '"' => output.push_str("\\\""),
            '\\' => output.push_str("\\\\"),
            '\n' => output.push_str("\\n"),
            '\r' => output.push_str("\\r"),
            '\t' => output.push_str("\\t"),
            value if value.is_control() => write!(output, "\\u{:04x}", value as u32).unwrap(),
            value => output.push(value),
        }
    }
    output.push('"');
    output
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().fold(String::new(), |mut output, byte| {
        write!(output, "{byte:02x}").unwrap();
        output
    })
}

fn error_code(error: &crate::CliError) -> &'static str {
    match error {
        crate::CliError::Parse(_) => "PARSE_ERROR",
        crate::CliError::Context(_) | crate::CliError::Io(_) => "CONTEXT_ERROR",
        crate::CliError::Invalid(_) => "INVALID",
        crate::CliError::NotFound => "NOT_FOUND",
        crate::CliError::Conflict => "CONFLICT",
        crate::CliError::HeadMoved => "HEAD_MOVED",
        crate::CliError::WrongHistory => "WRONG_HISTORY",
        crate::CliError::ReadOnly => "READ_ONLY",
        crate::CliError::WorkspaceBusy => "WORKSPACE_BUSY",
        crate::CliError::WorkspaceDirty => "WORKSPACE_DIRTY",
        crate::CliError::Interrupted => "INTERRUPTED",
        crate::CliError::Integrity => "INTEGRITY_ERROR",
    }
}

fn io(error: std::io::Error) -> crate::CliError {
    crate::CliError::Io(error.to_string())
}

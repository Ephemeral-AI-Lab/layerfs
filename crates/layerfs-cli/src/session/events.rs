use super::empty_receipt;
use crate::fixture::MockState;
use crate::{CliError, CliEvent, CommandResult, FinishedStatus, OperationReceipt, OperationState};

pub(super) fn normalize_receipt(
    result: &Result<CommandResult, CliError>,
    receipt: &mut OperationReceipt,
) {
    let no_change = match result {
        Err(_) => true,
        Ok(
            CommandResult::Pull(value)
            | CommandResult::Push(value)
            | CommandResult::Add(value)
            | CommandResult::Workspace(value),
        ) => value.contains("Already") || value.contains("UpToDate") || value == "NoChanges",
        Ok(CommandResult::Diff(_) | CommandResult::Monitor(_) | CommandResult::Query(_)) => true,
        Ok(CommandResult::Context(value)) => !value.starts_with("Created"),
        Ok(CommandResult::NeedsResolution { .. }) => true,
        Ok(CommandResult::Fork { .. }) => false,
    };
    if no_change {
        let elapsed_ms = receipt.elapsed_ms;
        *receipt = empty_receipt();
        receipt.elapsed_ms = elapsed_ms;
    }
}

pub(super) fn record_operation_event(state: &mut MockState, event: &CliEvent) {
    let Some(operation) = state
        .operations
        .iter_mut()
        .find(|operation| &operation.id == event.operation_id())
    else {
        return;
    };
    let message = match event {
        CliEvent::Started { command, .. } => {
            operation.phase = "started".into();
            format!("Started {command}")
        }
        CliEvent::Progress {
            phase,
            completed,
            total,
            ..
        } => {
            operation.phase = phase.clone();
            operation.completed = *completed;
            operation.total = *total;
            format!("Progress {phase} {completed}/{total}")
        }
        CliEvent::Output { line, .. } => format!("Output {line}"),
        CliEvent::Snapshot { scope, .. } => format!("Snapshot {scope}"),
        CliEvent::Finished {
            status, receipt, ..
        } => {
            operation.phase = "finished".into();
            operation.completed = operation.total;
            operation.state = match status {
                FinishedStatus::Succeeded => OperationState::Succeeded,
                FinishedStatus::Failed => OperationState::Failed,
                FinishedStatus::Interrupted => OperationState::Interrupted,
            };
            operation.receipt = receipt.clone();
            format!("Finished {status:?}")
        }
    };
    if operation.events.len() == 32 {
        operation.events.remove(0);
    }
    operation.events.push(message);
}

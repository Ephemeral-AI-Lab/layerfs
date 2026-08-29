use crate::{CliError, CliEvent, CliResult, Command, CommandResult, ViewScope, ViewSnapshot};
use layerfs_sdk::{OperationId, OperationOutcome, OperationReceipt, TimingFragment};

impl crate::host::Host {
    pub(crate) fn register_operation(
        &self,
        id: OperationId,
    ) -> CliResult<std::sync::Arc<std::sync::atomic::AtomicBool>> {
        let cancellation = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let mut operations = self
            .operations
            .lock()
            .map_err(|_| CliError::Context("host operations".to_owned()))?;
        if operations.contains_key(&id) {
            return Err(CliError::Conflict);
        }
        operations.insert(id, cancellation.clone());
        Ok(cancellation)
    }

    pub(crate) fn interrupt_operation(&self, id: OperationId) -> CliResult<()> {
        let operations = self
            .operations
            .lock()
            .map_err(|_| CliError::Context("host operations".to_owned()))?;
        operations
            .get(&id)
            .ok_or(CliError::NotFound)?
            .store(true, std::sync::atomic::Ordering::Release);
        Ok(())
    }

    pub(crate) fn finish_operation(&self, id: OperationId) {
        if let Ok(mut operations) = self.operations.lock() {
            operations.remove(&id);
        }
    }

    pub(crate) fn execute(
        &self,
        id: OperationId,
        command: Command,
        interrupted: &std::sync::atomic::AtomicBool,
        emit: &mut dyn FnMut(CliEvent) -> bool,
    ) -> (OperationId, CliResult<CommandResult>, OperationReceipt) {
        let started = std::time::Instant::now();
        self.monitor.begin_operation();
        let mut result = if interrupted.load(std::sync::atomic::Ordering::Acquire) {
            Err(CliError::Interrupted)
        } else {
            match &command {
                Command::Workspace {
                    command:
                        crate::WorkspaceCommand::Output {
                            execution_id,
                            follow: true,
                        },
                } => self.follow_output(execution_id, interrupted, emit),
                _ => self.dispatch(command.clone()),
            }
        };
        if interrupted.load(std::sync::atomic::Ordering::Acquire) {
            result = Err(CliError::Interrupted);
        }
        let service_ns = started.elapsed().as_nanos().min(u128::from(u64::MAX)) as u64;
        let receipt = OperationReceipt {
            id,
            name: crate::host::summary(&command).0,
            outcome: match &result {
                Ok(_) => OperationOutcome::Succeeded,
                Err(CliError::Interrupted) => OperationOutcome::Interrupted,
                Err(_) => OperationOutcome::Failed,
            },
            queued_ns: 0,
            service_ns,
            fragments: vec![TimingFragment {
                process_id: std::process::id(),
                started_ns: 0,
                elapsed_ns: service_ns,
            }],
            storage: self.monitor.finish_operation(),
        };
        let _ = self.monitor.record(receipt.clone());
        (id, result, receipt)
    }

    fn follow_output(
        &self,
        execution_id: &str,
        interrupted: &std::sync::atomic::AtomicBool,
        emit: &mut dyn FnMut(CliEvent) -> bool,
    ) -> CliResult<CommandResult> {
        let execution_id = execution_id
            .parse()
            .map_err(|_| CliError::Invalid("execution ID".to_owned()))?;
        let reader = self
            .workspaces
            .output(execution_id)
            .map_err(crate::host::workspace)?;
        let mut after = 0;
        loop {
            if interrupted.load(std::sync::atomic::Ordering::Acquire) {
                return Err(CliError::Interrupted);
            }
            let page = reader.read(after, true).map_err(crate::host::workspace)?;
            for chunk in &page.chunks {
                if !emit(CliEvent::Output {
                    execution_id,
                    sequence: chunk.sequence,
                    stream: chunk.stream,
                    bytes: chunk.bytes.clone(),
                }) {
                    return Err(CliError::Interrupted);
                }
            }
            after = page.next_sequence;
            if page.exited {
                return Ok(CommandResult::View {
                    scope: ViewScope::Output,
                    snapshot: ViewSnapshot::Output(page),
                });
            }
        }
    }
}

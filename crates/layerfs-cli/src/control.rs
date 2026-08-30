use crate::{
    CliError, CliEvent, CliResult, Command, CommandPlan, CommandResult, Completion, ViewQuery,
    ViewSnapshot,
};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

pub struct CliSession {
    context: PathBuf,
    client: Arc<Mutex<Option<layerfs_sdk::Client>>>,
}

pub struct OperationHandle {
    events: Mutex<std::sync::mpsc::Receiver<CliEvent>>,
    interrupted: Arc<AtomicBool>,
}

impl CliSession {
    pub fn open(context_location: impl AsRef<Path>) -> CliResult<Self> {
        Ok(Self {
            context: context_location.as_ref().to_owned(),
            client: Arc::new(Mutex::new(None)),
        })
    }

    pub fn parse_line(input: &str) -> CliResult<Command> {
        crate::parse::line(input)
    }

    pub fn plan(&self, command: &Command) -> CliResult<CommandPlan> {
        let client = if matches!(command, Command::Db { .. } | Command::Context { .. }) {
            None
        } else {
            Some(self.client()?)
        };
        crate::plan::plan(command, client.as_ref())
    }

    pub fn execute(&self, command: Command) -> CliResult<OperationHandle> {
        if matches!(
            command,
            Command::Context {
                command: crate::ContextCommand::Use { .. }
            }
        ) {
            *self
                .client
                .lock()
                .map_err(|_| CliError::Context("Client cache".to_owned()))? = None;
        }
        let client = if matches!(command, Command::Db { .. } | Command::Context { .. }) {
            Ok(None)
        } else {
            self.client().map(Some)
        };
        let summary = crate::CommandSummary {
            name: crate::plan::plan(&command, client.as_ref().ok().and_then(Option::as_ref))
                .map(|plan| plan.summary)
                .unwrap_or_else(|_| format!("{command:?}")),
        };
        let context = self.context.clone();
        let interrupted = Arc::new(AtomicBool::new(false));
        let worker_interrupted = interrupted.clone();
        let (sender, events) = std::sync::mpsc::sync_channel(1);
        std::thread::spawn(move || {
            let error_context = summary.name.clone();
            if sender.send(CliEvent::Started(summary)).is_err() {
                return;
            }
            let mut emit = |event| {
                if worker_interrupted.load(Ordering::Acquire) {
                    return Err(CliError::Interrupted);
                }
                sender.send(event).map_err(|_| CliError::Interrupted)
            };
            let result = match client {
                Ok(client) => {
                    crate::execute::run(&context, client, command, error_context.clone(), &mut emit)
                }
                Err(source) => Err(CliError::Operation {
                    context: error_context,
                    source: Box::new(source),
                }),
            };
            let _ = sender.send(CliEvent::Finished(result));
        });
        Ok(OperationHandle {
            events: Mutex::new(events),
            interrupted,
        })
    }

    pub fn complete(&self, input: &str, cursor: usize) -> CliResult<Vec<Completion>> {
        let client = self.client().ok();
        crate::completion::complete(input, cursor, client.as_ref())
    }

    pub fn snapshot(&self, query: ViewQuery) -> CliResult<ViewSnapshot> {
        let client = self.client()?;
        Ok(ViewSnapshot(client.query(query.into())?))
    }

    fn client(&self) -> CliResult<layerfs_sdk::Client> {
        let mut client = self
            .client
            .lock()
            .map_err(|_| CliError::Context("Client cache".to_owned()))?;
        if client.is_none() {
            *client = Some(crate::context::client(&self.context)?);
        }
        Ok(client.as_ref().expect("installed Client").clone())
    }
}

impl OperationHandle {
    pub fn interrupt(&self) -> CliResult<()> {
        self.interrupted.store(true, Ordering::Release);
        Ok(())
    }

    pub fn next_event(&self) -> CliResult<Option<CliEvent>> {
        if self.interrupted.load(Ordering::Acquire) {
            return Err(CliError::Interrupted);
        }
        self.events
            .lock()
            .map_err(|_| CliError::Context("operation events".to_owned()))
            .map(|events| events.recv().ok())
    }

    pub fn try_next_event(&self) -> CliResult<Option<CliEvent>> {
        if self.interrupted.load(Ordering::Acquire) {
            return Err(CliError::Interrupted);
        }
        self.events
            .lock()
            .map_err(|_| CliError::Context("operation events".to_owned()))
            .map(|events| events.try_recv().ok())
    }
}

pub fn invoke(
    context: impl AsRef<Path>,
    arguments: Vec<std::ffi::OsString>,
    _persistent: bool,
    output: &mut impl std::io::Write,
) -> CliResult<i32> {
    let parsed = crate::parse::cli(arguments)?;
    match parsed {
        crate::parse::CliArgv::Display(text) => {
            writeln!(output, "{text}")?;
            Ok(0)
        }
        crate::parse::CliArgv::Command(command, json) => {
            let session = CliSession::open(context)?;
            let handle = session.execute(command)?;
            let mut code = 0;
            while let Some(event) = handle.next_event()? {
                if matches!(event, CliEvent::Finished(Err(_))) {
                    code = 1;
                }
                crate::output::render(&event, json, output)?;
            }
            Ok(code)
        }
    }
}

#[allow(dead_code)]
fn _result_type(_: CommandResult) {}

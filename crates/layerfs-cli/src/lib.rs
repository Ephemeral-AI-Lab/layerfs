#![forbid(unsafe_code)]

use layerfs_sdk::{
    BranchId, Client, CommitId, ContainerCreate, ContainerError, ContainerId, ContainerLimits,
    ContainerManager, CreateWorkspaceSession, DiffRequest, EndWorkspaceMode, EntityName,
    ExecutionId, LayerId, LayerStackInitialization, LayerStackStore, LocalForkSource, NonEmpty,
    Query, QueryKind, ResolveChoice, WorkspaceId, WorkspacePlacement, WorkspaceProjection,
};
use std::collections::{BTreeSet, VecDeque};
use std::ffi::OsString;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

#[cfg(unix)]
mod runtime;

pub const HELP: &str = "\
LayerFS — branchable local agent workspaces\n\
\n\
USAGE:\n\
  layerfs [--json] <command>\n\
\n\
CONTAINERS:\n\
  container create --name NAME --image IMAGE [--memory-mib N] [--cpus N] [--pids-limit N]\n\
  container start NAME\n\
  container status NAME\n\
  container stop NAME\n\
  container remove NAME\n\
\n\
WORKSPACES:\n\
  workspace create BRANCH --at PATH [--container NAME] [--projection fuse|materialize]\n\
  workspace exec WORKSPACE -- PROGRAM [ARG...]\n\
  workspace shell WORKSPACE\n\
  workspace output EXECUTION [--follow]\n\
  workspace stop EXECUTION\n\
  workspace conflicts WORKSPACE [--after CURSOR]\n\
  workspace resolve WORKSPACE CONFLICT --branch|--layer|--working-tree\n\
  workspace commit WORKSPACE\n\
  workspace end WORKSPACE [--discard]\n\
\n\
Run the LayerStack, Branch, Monitor, Query, Database, and Context commands documented in README.md.\n";

pub type CliResult<T> = std::result::Result<T, CliError>;

#[derive(Debug)]
pub enum CliError {
    Parse(&'static str),
    Sdk(layerfs_sdk::SdkError),
    Store(layerfs_sdk::StoreError),
    Workspace(layerfs_sdk::WorkspaceError),
    Container(ContainerError),
    Io(std::io::Error),
    Context,
}

impl std::fmt::Display for CliError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl std::error::Error for CliError {}

impl From<layerfs_sdk::SdkError> for CliError {
    fn from(value: layerfs_sdk::SdkError) -> Self {
        Self::Sdk(value)
    }
}

impl From<std::io::Error> for CliError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<layerfs_sdk::StoreError> for CliError {
    fn from(value: layerfs_sdk::StoreError) -> Self {
        Self::Store(value)
    }
}

impl From<layerfs_sdk::WorkspaceError> for CliError {
    fn from(value: layerfs_sdk::WorkspaceError) -> Self {
        Self::Workspace(value)
    }
}

impl From<ContainerError> for CliError {
    fn from(value: ContainerError) -> Self {
        Self::Container(value)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Command {
    DbCreate(PathBuf),
    DbConnect(PathBuf),
    ContextUse(PathBuf),
    ContextShow,
    ContainerCreate(ContainerCreate),
    ContainerStart(String),
    ContainerStatus(String),
    ContainerStop(String),
    ContainerRemove(String),
    LayerStackInit {
        name: EntityName,
        source: LayerStackInitialization,
    },
    LayerStackDiff {
        from: LayerId,
        to: LayerId,
    },
    LayerStackAdd(BranchId),
    BranchFork {
        name: EntityName,
        source: LocalForkSource,
    },
    BranchDiffCommits {
        branch_id: BranchId,
        from: CommitId,
        to: CommitId,
    },
    BranchDiffLayer {
        branch_id: BranchId,
        layer_id: LayerId,
    },
    WorkspaceCreate(CreateWorkspaceSession),
    WorkspaceExec {
        workspace_id: WorkspaceId,
        argv: NonEmpty<Vec<OsString>>,
    },
    WorkspaceShell(WorkspaceId),
    WorkspaceOutput {
        execution_id: ExecutionId,
        follow: bool,
    },
    WorkspaceStop(ExecutionId),
    WorkspaceConflicts {
        workspace_id: WorkspaceId,
        after: Option<layerfs_sdk::ConflictCursor>,
    },
    WorkspaceResolve {
        workspace_id: WorkspaceId,
        conflict_id: layerfs_sdk::ConflictId,
        choice: ResolveChoice,
    },
    WorkspaceCommit(WorkspaceId),
    WorkspaceEnd {
        workspace_id: WorkspaceId,
        mode: EndWorkspaceMode,
    },
    MonitorSnapshot,
    MonitorAnalyzeDedup,
    Query(Query),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CliEvent {
    Started,
    Completed(String),
    Failed(String),
}

pub struct OperationHandle(Mutex<VecDeque<CliEvent>>);

impl OperationHandle {
    pub fn next_event(&self) -> CliResult<Option<CliEvent>> {
        Ok(self.0.lock().map_err(|_| CliError::Context)?.pop_front())
    }
}

pub struct CliSession {
    context: PathBuf,
    state: Mutex<SessionState>,
}

struct SessionState {
    store: Option<PathBuf>,
    client: Option<Client>,
    container: Option<(String, layerfs_sdk::ContainerBinding)>,
}

impl CliSession {
    pub fn open(context: impl AsRef<Path>) -> CliResult<Self> {
        let context = context.as_ref().to_owned();
        let store = if context.is_file() {
            let value = std::fs::read_to_string(&context)?;
            value
                .strip_prefix("store=")
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(PathBuf::from)
        } else {
            None
        };
        Ok(Self {
            context,
            state: Mutex::new(SessionState {
                store,
                client: None,
                container: None,
            }),
        })
    }

    pub fn parse_line(line: &str) -> CliResult<Command> {
        parse(line.split_whitespace().map(OsString::from).collect())
    }

    pub fn parse(arguments: Vec<OsString>) -> CliResult<Command> {
        parse(arguments)
    }

    pub fn execute(&self, command: Command) -> CliResult<OperationHandle> {
        let result = self.execute_inner(command);
        let mut events = VecDeque::from([CliEvent::Started]);
        match result {
            Ok(output) => events.push_back(CliEvent::Completed(output)),
            Err(error) => events.push_back(CliEvent::Failed(error.to_string())),
        }
        Ok(OperationHandle(Mutex::new(events)))
    }

    fn execute_inner(&self, command: Command) -> CliResult<String> {
        match command {
            Command::DbCreate(path) => {
                LayerStackStore::create(&path)?;
                Ok(format!("created {}", path.display()))
            }
            Command::DbConnect(path) => {
                LayerStackStore::connect(&path)?;
                Ok(format!("connected {}", path.display()))
            }
            Command::ContextUse(path) => {
                LayerStackStore::connect(&path)?;
                if let Some(parent) = self.context.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                std::fs::write(&self.context, format!("store={}\n", path.display()))?;
                let mut state = self.state.lock().map_err(|_| CliError::Context)?;
                state.store = Some(path.clone());
                state.client = None;
                state.container = None;
                Ok(format!("store={}", path.display()))
            }
            Command::ContextShow => {
                let state = self.state.lock().map_err(|_| CliError::Context)?;
                Ok(state
                    .store
                    .as_ref()
                    .map(|path| format!("store={}", path.display()))
                    .unwrap_or_else(|| "store=<unset>".to_owned()))
            }
            Command::ContainerCreate(request) => {
                let created = self.containers()?.create(request)?;
                Ok(format!("{} {}", created.name, created.id.0))
            }
            Command::ContainerStart(container) => {
                if self
                    .state
                    .lock()
                    .map_err(|_| CliError::Context)?
                    .container
                    .is_some()
                {
                    return Err(CliError::Context);
                }
                let running = self.containers()?.start(&container)?;
                let output = format!("{} {} {}", running.name, running.id.0, running.endpoint);
                let binding = running.binding();
                let mut state = self.state.lock().map_err(|_| CliError::Context)?;
                if state.client.is_some() {
                    return Err(CliError::Context);
                }
                state.container = Some((running.name, binding));
                Ok(output)
            }
            Command::ContainerStatus(container) => {
                let status = self.containers()?.status(&container)?;
                Ok(format!(
                    "{} {} image={} running={} privileged={} fuse={} sys_admin={} binds={} memory={} nano_cpus={} pids={}",
                    status.name,
                    status.id.0,
                    status.image,
                    status.running,
                    status.privileged,
                    status.fuse_device,
                    status.sys_admin,
                    status.host_binds,
                    status.memory_bytes,
                    status.nano_cpus,
                    status.pids
                ))
            }
            Command::ContainerStop(container) => {
                let mut state = self.state.lock().map_err(|_| CliError::Context)?;
                if let Some((name, binding)) = &state.container {
                    if container != *name && container != binding.container_id().0 {
                        return Err(CliError::Context);
                    }
                }
                if let Some(client) = &state.client {
                    if client.active_workspace_count()? != 0
                        || client.active_execution_count()? != 0
                    {
                        return Err(CliError::Context);
                    }
                }
                state.client = None;
                state.container = None;
                drop(state);
                let status = self.containers()?.stop(&container)?;
                Ok(format!("{} stopped={}", status.name, !status.running))
            }
            Command::ContainerRemove(container) => {
                self.containers()?.remove(&container)?;
                Ok(format!("removed {container}"))
            }
            Command::WorkspaceCreate(mut request) => {
                self.resolve_container(&mut request)?;
                execute_sdk(&self.client()?, Command::WorkspaceCreate(request))
            }
            command => execute_sdk(&self.client()?, command),
        }
    }

    fn containers(&self) -> CliResult<ContainerManager> {
        let mut path = self.context.as_os_str().to_os_string();
        path.push(".runtime");
        Ok(ContainerManager::open(PathBuf::from(path))?)
    }

    fn resolve_container(&self, request: &mut CreateWorkspaceSession) -> CliResult<()> {
        let WorkspacePlacement::Container { container_id, .. } = &mut request.placement else {
            return Ok(());
        };
        let state = self.state.lock().map_err(|_| CliError::Context)?;
        let Some((name, binding)) = &state.container else {
            return Err(CliError::Context);
        };
        if container_id.0 != *name && container_id != binding.container_id() {
            return Err(CliError::Context);
        }
        *container_id = binding.container_id().clone();
        Ok(())
    }

    fn client(&self) -> CliResult<Client> {
        let mut state = self.state.lock().map_err(|_| CliError::Context)?;
        if let Some(client) = &state.client {
            return Ok(client.clone());
        }
        let path = state.store.clone().ok_or(CliError::Context)?;
        let store = Arc::new(LayerStackStore::connect(path)?);
        let client = match state.container.as_ref() {
            Some((_, binding)) => Client::connect_with_container(store, binding.clone())?,
            None => Client::connect(store)?,
        };
        state.client = Some(client.clone());
        Ok(client)
    }

    pub(crate) fn idle(&self) -> CliResult<bool> {
        let state = self.state.lock().map_err(|_| CliError::Context)?;
        match &state.client {
            Some(client) => {
                Ok(client.active_workspace_count()? == 0 && client.active_execution_count()? == 0)
            }
            None => Ok(true),
        }
    }
}

pub fn invoke(
    context: impl AsRef<Path>,
    arguments: Vec<OsString>,
    json: bool,
    output: &mut dyn Write,
) -> CliResult<i32> {
    let session = CliSession::open(context)?;
    invoke_session(&session, arguments, json, output)
}

fn invoke_session(
    session: &CliSession,
    arguments: Vec<OsString>,
    json: bool,
    output: &mut dyn Write,
) -> CliResult<i32> {
    let command = match CliSession::parse(arguments) {
        Ok(command) => command,
        Err(error) => {
            writeln!(output, "FAILED {error}")?;
            return Ok(2);
        }
    };
    let handle = session.execute(command)?;
    let mut failed = false;
    while let Some(event) = handle.next_event()? {
        match event {
            CliEvent::Started => {}
            CliEvent::Completed(value) => {
                if json {
                    writeln!(
                        output,
                        "{{\"schema_version\":4,\"result\":\"{}\"}}",
                        escape_json(&value)
                    )?;
                } else {
                    writeln!(output, "{value}")?;
                }
            }
            CliEvent::Failed(error) => {
                failed = true;
                writeln!(output, "FAILED {error}")?;
            }
        }
    }
    Ok(i32::from(failed))
}

#[cfg(unix)]
pub fn invoke_managed(
    context: impl AsRef<Path>,
    arguments: Vec<OsString>,
    json: bool,
    output: &mut dyn Write,
) -> CliResult<i32> {
    runtime::dispatch(context.as_ref(), arguments, json, output)
}

#[cfg(not(unix))]
pub fn invoke_managed(
    context: impl AsRef<Path>,
    arguments: Vec<OsString>,
    json: bool,
    output: &mut dyn Write,
) -> CliResult<i32> {
    invoke(context, arguments, json, output)
}

#[cfg(unix)]
pub fn serve_context_owner(context: PathBuf) -> CliResult<()> {
    runtime::serve(context)
}

pub fn default_context_location() -> PathBuf {
    std::env::var_os("LAYERFS_CONTEXT")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(".layerfs-context"))
}

fn execute_sdk(client: &Client, command: Command) -> CliResult<String> {
    Ok(match command {
        Command::LayerStackInit { name, source } => {
            format!("{:?}", client.initialize_layerstack(name, source)?)
        }
        Command::LayerStackDiff { from, to } => diff_output(client.diff(DiffRequest::Layers {
            from_layer_id: from,
            to_layer_id: to,
        })?)?,
        Command::LayerStackAdd(branch_id) => format!("{:?}", client.add_layer(branch_id)?),
        Command::BranchFork { name, source } => format!("{}", client.fork_branch(name, source)?),
        Command::BranchDiffCommits {
            branch_id,
            from,
            to,
        } => diff_output(client.diff(DiffRequest::BranchCommits {
            branch_id,
            from_commit_id: from,
            to_commit_id: to,
        })?)?,
        Command::BranchDiffLayer {
            branch_id,
            layer_id,
        } => diff_output(client.diff(DiffRequest::BranchLayer {
            branch_id,
            layer_id,
        })?)?,
        Command::WorkspaceCreate(request) => {
            client.create_workspace_session(request)?.id.to_string()
        }
        Command::WorkspaceExec { workspace_id, argv } => client
            .exec_workspace_session(workspace_id, argv)?
            .id
            .to_string(),
        Command::WorkspaceShell(workspace_id) => {
            client.shell_workspace_session(workspace_id)?.id.to_string()
        }
        Command::WorkspaceOutput {
            execution_id,
            follow,
        } => {
            let reader = client.workspace_output(execution_id)?;
            if !follow {
                format!("{:?}", reader.read(0, false)?)
            } else {
                let mut after = 0;
                let mut pages = Vec::new();
                loop {
                    let page = reader.read(after, true)?;
                    after = page.next_sequence;
                    let exited = page.exited;
                    pages.push(page);
                    if exited {
                        break;
                    }
                }
                format!("{pages:?}")
            }
        }
        Command::WorkspaceStop(execution_id) => {
            client.stop_workspace_execution(execution_id)?;
            "stopped".to_owned()
        }
        Command::WorkspaceConflicts {
            workspace_id,
            after,
        } => format!("{:?}", client.workspace_conflicts(workspace_id, after)?),
        Command::WorkspaceResolve {
            workspace_id,
            conflict_id,
            choice,
        } => format!(
            "{:?}",
            client.resolve_workspace_conflict(workspace_id, conflict_id, choice)?
        ),
        Command::WorkspaceCommit(workspace_id) => {
            format!("{:?}", client.commit_workspace_session(workspace_id)?)
        }
        Command::WorkspaceEnd { workspace_id, mode } => {
            format!("{:?}", client.end_workspace_session(workspace_id, mode)?)
        }
        Command::MonitorSnapshot => format!("{:?}", client.monitor_snapshot()?),
        Command::MonitorAnalyzeDedup => format!("{:?}", client.analyze_dedup()?),
        Command::Query(query) => {
            let mut current = Some(query);
            let mut items = Vec::new();
            while let Some(query) = current {
                let page = client.query(query.clone())?;
                current = page.clone().into_next_query(&query);
                items.extend(page.items);
            }
            format!("{items:?}")
        }
        Command::DbCreate(_)
        | Command::DbConnect(_)
        | Command::ContextUse(_)
        | Command::ContextShow
        | Command::ContainerCreate(_)
        | Command::ContainerStart(_)
        | Command::ContainerStatus(_)
        | Command::ContainerStop(_)
        | Command::ContainerRemove(_) => return Err(CliError::Parse("command routing")),
    })
}

fn diff_output(handle: layerfs_sdk::OperationHandle) -> CliResult<String> {
    let mut entries = Vec::new();
    while let Some(page) = handle.next_diff_page()? {
        entries.extend(page.entries);
    }
    Ok(format!("{entries:?}"))
}

fn parse(arguments: Vec<OsString>) -> CliResult<Command> {
    let args = arguments
        .iter()
        .map(|value| value.to_str().ok_or(CliError::Parse("UTF-8 argument")))
        .collect::<CliResult<Vec<_>>>()?;
    match args.as_slice() {
        ["db", "create", path] => Ok(Command::DbCreate(PathBuf::from(path))),
        ["db", "connect", path] => Ok(Command::DbConnect(PathBuf::from(path))),
        ["context", "use", "--store", path] => Ok(Command::ContextUse(PathBuf::from(path))),
        ["context", "show"] => Ok(Command::ContextShow),
        ["container", "create", rest @ ..] => parse_container_create(rest),
        ["container", "start", container] => Ok(Command::ContainerStart((*container).to_owned())),
        ["container", "status", container] => Ok(Command::ContainerStatus((*container).to_owned())),
        ["container", "stop", container] => Ok(Command::ContainerStop((*container).to_owned())),
        ["container", "remove", container] => Ok(Command::ContainerRemove((*container).to_owned())),
        ["layerstack", "init", rest @ ..] => parse_layerstack_init(rest),
        ["layerstack", "diff", "--from", from, "--to", to] => Ok(Command::LayerStackDiff {
            from: from.parse().map_err(|_| CliError::Parse("LayerId"))?,
            to: to.parse().map_err(|_| CliError::Parse("LayerId"))?,
        }),
        ["layerstack", "add", branch] => Ok(Command::LayerStackAdd(
            branch.parse().map_err(|_| CliError::Parse("BranchId"))?,
        )),
        ["branch", "fork", rest @ ..] => parse_branch_fork(rest),
        ["branch", "diff", "--branch", branch, "--from", from, "--to", to] => {
            Ok(Command::BranchDiffCommits {
                branch_id: branch.parse().map_err(|_| CliError::Parse("BranchId"))?,
                from: from.parse().map_err(|_| CliError::Parse("CommitId"))?,
                to: to.parse().map_err(|_| CliError::Parse("CommitId"))?,
            })
        }
        ["branch", "diff", "--branch", branch, "--layer", layer] => Ok(Command::BranchDiffLayer {
            branch_id: branch.parse().map_err(|_| CliError::Parse("BranchId"))?,
            layer_id: layer.parse().map_err(|_| CliError::Parse("LayerId"))?,
        }),
        ["workspace", "create", rest @ ..] => parse_workspace_create(rest),
        ["workspace", "exec", workspace, "--", argv @ ..] => Ok(Command::WorkspaceExec {
            workspace_id: workspace
                .parse()
                .map_err(|_| CliError::Parse("WorkspaceId"))?,
            argv: NonEmpty::new(argv.iter().map(OsString::from).collect())
                .map_err(|_| CliError::Parse("argv"))?,
        }),
        ["workspace", "shell", workspace] => Ok(Command::WorkspaceShell(
            workspace
                .parse()
                .map_err(|_| CliError::Parse("WorkspaceId"))?,
        )),
        ["workspace", "output", execution] => Ok(Command::WorkspaceOutput {
            execution_id: execution
                .parse()
                .map_err(|_| CliError::Parse("ExecutionId"))?,
            follow: false,
        }),
        ["workspace", "output", execution, "--follow"] => Ok(Command::WorkspaceOutput {
            execution_id: execution
                .parse()
                .map_err(|_| CliError::Parse("ExecutionId"))?,
            follow: true,
        }),
        ["workspace", "stop", execution] => Ok(Command::WorkspaceStop(
            execution
                .parse()
                .map_err(|_| CliError::Parse("ExecutionId"))?,
        )),
        ["workspace", "conflicts", workspace] => Ok(Command::WorkspaceConflicts {
            workspace_id: workspace
                .parse()
                .map_err(|_| CliError::Parse("WorkspaceId"))?,
            after: None,
        }),
        ["workspace", "conflicts", workspace, "--after", after] => {
            Ok(Command::WorkspaceConflicts {
                workspace_id: workspace
                    .parse()
                    .map_err(|_| CliError::Parse("WorkspaceId"))?,
                after: Some(
                    after
                        .parse()
                        .map_err(|_| CliError::Parse("Conflict cursor"))?,
                ),
            })
        }
        ["workspace", "resolve", workspace, conflict, choice] => Ok(Command::WorkspaceResolve {
            workspace_id: workspace
                .parse()
                .map_err(|_| CliError::Parse("WorkspaceId"))?,
            conflict_id: conflict
                .parse()
                .map_err(|_| CliError::Parse("ConflictId"))?,
            choice: match *choice {
                "--branch" => ResolveChoice::Branch,
                "--layer" => ResolveChoice::Layer,
                "--working-tree" => ResolveChoice::WorkingTree,
                _ => return Err(CliError::Parse("resolution choice")),
            },
        }),
        ["workspace", "commit", workspace] => Ok(Command::WorkspaceCommit(
            workspace
                .parse()
                .map_err(|_| CliError::Parse("WorkspaceId"))?,
        )),
        ["workspace", "end", workspace] => Ok(Command::WorkspaceEnd {
            workspace_id: workspace
                .parse()
                .map_err(|_| CliError::Parse("WorkspaceId"))?,
            mode: EndWorkspaceMode::Clean,
        }),
        ["workspace", "end", workspace, "--discard"] => Ok(Command::WorkspaceEnd {
            workspace_id: workspace
                .parse()
                .map_err(|_| CliError::Parse("WorkspaceId"))?,
            mode: EndWorkspaceMode::Discard,
        }),
        ["monitor", "snapshot"] => Ok(Command::MonitorSnapshot),
        ["monitor", "analyze-dedup"] => Ok(Command::MonitorAnalyzeDedup),
        ["query", rest @ ..] => parse_query(rest),
        _ => Err(CliError::Parse("command")),
    }
}

fn parse_container_create(args: &[&str]) -> CliResult<Command> {
    if args.len() < 4 || args.len() > 10 || args.len() % 2 != 0 {
        return Err(CliError::Parse("container create options"));
    }
    let mut keys = BTreeSet::new();
    for pair in args.chunks_exact(2) {
        if !matches!(
            pair[0],
            "--name" | "--image" | "--memory-mib" | "--cpus" | "--pids-limit"
        ) || !keys.insert(pair[0])
        {
            return Err(CliError::Parse("container create options"));
        }
    }
    let name = option(args, "--name").ok_or(CliError::Parse("--name"))?;
    let image = option(args, "--image").ok_or(CliError::Parse("--image"))?;
    let memory_mib = option(args, "--memory-mib")
        .map(str::parse)
        .transpose()
        .map_err(|_| CliError::Parse("--memory-mib"))?
        .unwrap_or(512_u64);
    let cpus = option(args, "--cpus")
        .map(str::parse)
        .transpose()
        .map_err(|_| CliError::Parse("--cpus"))?
        .unwrap_or(2_u16);
    let pids = option(args, "--pids-limit")
        .map(str::parse)
        .transpose()
        .map_err(|_| CliError::Parse("--pids-limit"))?
        .unwrap_or(512_u32);
    Ok(Command::ContainerCreate(ContainerCreate {
        name: name.to_owned(),
        image: image.to_owned(),
        limits: ContainerLimits {
            memory_bytes: memory_mib
                .checked_mul(1024 * 1024)
                .ok_or(CliError::Parse("--memory-mib"))?,
            cpus,
            pids,
        },
    }))
}

fn parse_layerstack_init(args: &[&str]) -> CliResult<Command> {
    let name = option(args, "--name").ok_or(CliError::Parse("--name"))?;
    let source = if args.contains(&"--empty") {
        if args.len() != 3 || !args.contains(&"--name") || !args.contains(&"--empty") {
            return Err(CliError::Parse("LayerStack source"));
        }
        LayerStackInitialization::Empty
    } else {
        let directory = args
            .last()
            .filter(|value| **value != name && !value.starts_with("--"))
            .ok_or(CliError::Parse("LayerStack source"))?;
        LayerStackInitialization::Directory(PathBuf::from(directory))
    };
    Ok(Command::LayerStackInit {
        name: EntityName::new(name).map_err(|_| CliError::Parse("name"))?,
        source,
    })
}

fn parse_branch_fork(args: &[&str]) -> CliResult<Command> {
    let name = EntityName::new(option(args, "--name").ok_or(CliError::Parse("--name"))?)
        .map_err(|_| CliError::Parse("name"))?;
    let source = match (
        option(args, "--layer"),
        option(args, "--branch"),
        option(args, "--commit"),
    ) {
        (Some(layer), None, None) => LocalForkSource::Layer {
            layer_id: layer.parse().map_err(|_| CliError::Parse("LayerId"))?,
        },
        (None, Some(branch), Some(commit)) => LocalForkSource::Branch {
            branch_id: branch.parse().map_err(|_| CliError::Parse("BranchId"))?,
            commit_id: commit.parse().map_err(|_| CliError::Parse("CommitId"))?,
        },
        _ => return Err(CliError::Parse("Fork source")),
    };
    Ok(Command::BranchFork { name, source })
}

fn parse_workspace_create(args: &[&str]) -> CliResult<Command> {
    let branch_id = args
        .first()
        .ok_or(CliError::Parse("BranchId"))?
        .parse()
        .map_err(|_| CliError::Parse("BranchId"))?;
    let root = PathBuf::from(option(args, "--at").ok_or(CliError::Parse("--at"))?);
    let placement = option(args, "--container").map_or_else(
        || WorkspacePlacement::Host { root: root.clone() },
        |id| WorkspacePlacement::Container {
            container_id: ContainerId(id.to_owned()),
            root: root.clone(),
        },
    );
    let projection = match option(args, "--projection") {
        Some("fuse") => Some(WorkspaceProjection::Fuse),
        Some("materialize") => Some(WorkspaceProjection::Materialize),
        Some(_) => return Err(CliError::Parse("projection")),
        None => None,
    };
    Ok(Command::WorkspaceCreate(CreateWorkspaceSession {
        branch_id,
        placement,
        projection,
    }))
}

fn parse_query(args: &[&str]) -> CliResult<Command> {
    let kind = match args.first().copied() {
        Some("layerstacks") => QueryKind::LayerStacks,
        Some("layers") => QueryKind::Layers,
        Some("branches") => QueryKind::Branches,
        Some("commits") => QueryKind::Commits,
        Some("workspaces") => QueryKind::Workspaces,
        Some("monitor") => QueryKind::Monitor,
        _ => return Err(CliError::Parse("query kind")),
    };
    let mut query = Query::new(kind);
    if let Some(id) = option(args, "--layerstack") {
        query = query.in_layer_stack(id.parse().map_err(|_| CliError::Parse("LayerStackId"))?);
    }
    Ok(Command::Query(query))
}

fn option<'a>(args: &'a [&str], name: &str) -> Option<&'a str> {
    args.windows(2)
        .find_map(|pair| (pair[0] == name).then_some(pair[1]))
}

fn escape_json(value: &str) -> String {
    value
        .chars()
        .flat_map(|character| match character {
            '\\' => "\\\\".chars().collect::<Vec<_>>(),
            '"' => "\\\"".chars().collect(),
            '\n' => "\\n".chars().collect(),
            '\r' => "\\r".chars().collect(),
            '\t' => "\\t".chars().collect(),
            character if character.is_control() => '�'.to_string().chars().collect(),
            character => vec![character],
        })
        .collect()
}

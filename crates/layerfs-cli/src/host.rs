use crate::command::{
    BranchCommand, DbCommand, LayerCommand, MonitorCommand, Projection, StackCommand, StoreRole,
    WorkspaceCommand,
};
use crate::context::{ContextPaths, SavedBranch, SavedContext};
use crate::{
    CliError, CliResult, Command, CommandPlan, CommandResult, CommandSummary, CommitDiffEntry,
    StoreQuery, StoreScope, StoreSnapshot, TopologyEntry, ViewQuery, ViewScope, ViewSnapshot,
};
use layerfs_sdk::{
    BranchCommit, BranchSource, Client, EndWorkspaceMode, LayerInitialization, LayerSource,
    Monitor, MonitorScope, MonitoredRoute, NonEmpty, OperationId, Query, QueryResult, QueryScope,
    StoreLocation, WorkspacePlacement, WorkspaceProjection, Workspaces,
};
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::{Arc, Mutex};

pub(crate) struct Host {
    paths: ContextPaths,
    state: Mutex<State>,
    pub(crate) workspaces: Arc<Workspaces>,
    pub(crate) monitor: Arc<Monitor>,
    pub(crate) operations:
        Mutex<std::collections::BTreeMap<OperationId, Arc<std::sync::atomic::AtomicBool>>>,
}

struct State {
    saved: SavedContext,
    client: Client,
    cursors: std::collections::VecDeque<(String, layerfs_sdk::QueryCursor)>,
}

impl Host {
    pub(crate) fn load(paths: ContextPaths) -> CliResult<Arc<Self>> {
        paths.prepare()?;
        let saved = SavedContext::load(&paths.context)?;
        let mut client = Client::new();
        if let Some(layer) = &saved.layer {
            client
                .connect_layer(StoreLocation::local(layer))
                .map_err(sdk)?;
            for stack in &saved.stacks {
                client
                    .connect_stack(StoreLocation::local(stack))
                    .map_err(sdk)?;
            }
            for branch in &saved.branches {
                select_stack(&mut client, branch.parent_stack.as_deref())?;
                client
                    .connect_branch(StoreLocation::local(&branch.location))
                    .map_err(sdk)?;
            }
            select_stack(&mut client, saved.active_stack.as_deref())?;
            if let Some(branch) = &saved.active_branch {
                let id = branch_connection(&client, branch)?.id;
                client.use_branch(id).map_err(sdk)?;
            }
        }
        let (workspaces, monitor) = if client.context().is_ok() {
            client.subsystems(&paths.runtime).map_err(sdk)?
        } else {
            let workspaces =
                Arc::new(Workspaces::new(paths.runtime.join("workspaces"), []).map_err(workspace)?);
            let monitor = Arc::new(
                Monitor::new(paths.runtime.join("monitor"), [], workspaces.clone())
                    .map_err(monitor)?,
            );
            (workspaces, monitor)
        };
        Ok(Arc::new(Self {
            paths,
            state: Mutex::new(State {
                saved,
                client,
                cursors: std::collections::VecDeque::new(),
            }),
            workspaces,
            monitor,
            operations: Mutex::new(std::collections::BTreeMap::new()),
        }))
    }

    pub(crate) fn plan(&self, command: &Command) -> CliResult<CommandPlan> {
        let state = self
            .state
            .lock()
            .map_err(|_| CliError::Context("host state".to_owned()))?;
        let mut route = Vec::new();
        if let Some(layer) = &state.saved.layer {
            route.push(layer.to_string_lossy().into_owned());
        }
        if let Some(stack) = &state.saved.active_stack {
            route.push(stack.to_string_lossy().into_owned());
        }
        if let Some(branch) = &state.saved.active_branch {
            route.push(branch.to_string_lossy().into_owned());
        }
        Ok(CommandPlan {
            command: summary(command),
            effect: crate::plan::effect(command),
            route,
            confirmation_required: matches!(
                command,
                Command::Workspace {
                    command: WorkspaceCommand::End { discard: true, .. }
                }
            ),
        })
    }

    pub(crate) fn snapshot(&self, query: ViewQuery) -> CliResult<ViewSnapshot> {
        match query {
            ViewQuery::Topology => self.topology(),
            ViewQuery::Workspaces => self
                .workspaces
                .sessions()
                .map(ViewSnapshot::Workspaces)
                .map_err(workspace),
            ViewQuery::Workspace(id) => self
                .workspaces
                .session(id)
                .map(ViewSnapshot::Workspace)
                .map_err(workspace),
            ViewQuery::WorkspaceDiff(id) => self
                .workspaces
                .diff(id)
                .map(ViewSnapshot::WorkspaceDiff)
                .map_err(workspace),
            ViewQuery::Output {
                execution_id,
                after,
                follow,
            } => self
                .workspaces
                .output(execution_id)
                .and_then(|reader| reader.read(after, follow))
                .map(ViewSnapshot::Output)
                .map_err(workspace),
            ViewQuery::Monitor(scope) => self
                .monitor
                .snapshot(scope)
                .map(|snapshot| ViewSnapshot::Monitor(crate::query::monitor_view(snapshot)))
                .map_err(monitor),
            ViewQuery::Store(query) => self.store_snapshot(query),
        }
    }

    pub(crate) fn dispatch(&self, command: Command) -> CliResult<CommandResult> {
        match command {
            Command::Db { command } => self.database(command),
            Command::Layer { command } => self.layer(command),
            Command::Stack { command } => self.stack(command),
            Command::Branch { command } => self.branch(command),
            Command::Workspace { command } => self.workspace(command),
            Command::Monitor { command } => self.monitor(command),
        }
    }

    fn database(&self, command: DbCommand) -> CliResult<CommandResult> {
        if matches!(command, DbCommand::List) {
            return self.view(ViewScope::Topology, ViewQuery::Topology);
        }
        let mut state = self
            .state
            .lock()
            .map_err(|_| CliError::Context("host state".to_owned()))?;
        match command {
            DbCommand::Create { role, location } => {
                self.open_database(&mut state, role, absolute(&location)?, true)?;
                Ok(CommandResult::Database {
                    role: format!("{role:?}").to_lowercase(),
                    location: location.to_string_lossy().into_owned(),
                })
            }
            DbCommand::Connect { role, location } => {
                self.open_database(&mut state, role, absolute(&location)?, false)?;
                Ok(CommandResult::Database {
                    role: format!("{role:?}").to_lowercase(),
                    location: location.to_string_lossy().into_owned(),
                })
            }
            DbCommand::Use { location } => {
                let location = absolute(&location)?;
                let stack_id = state
                    .client
                    .context()
                    .map_err(sdk)?
                    .stacks
                    .iter()
                    .find(|stack| stack.location.path() == location)
                    .map(|stack| stack.id);
                if let Some(stack_id) = stack_id {
                    state.client.use_stack(Some(stack_id)).map_err(sdk)?;
                    state.saved.active_stack = Some(location);
                    state.saved.active_branch = None;
                } else {
                    let branch_id = branch_connection(&state.client, &location)?.id;
                    state.client.use_branch(branch_id).map_err(sdk)?;
                    state.saved.active_branch = Some(location);
                }
                state.saved.save(&self.paths.context)?;
                Ok(CommandResult::Unit)
            }
            DbCommand::Disconnect { location } => {
                let location = absolute(&location)?;
                let branch = branch_connection(&state.client, &location).ok().cloned();
                if let Some(branch) = branch {
                    let route_id = route(&state, &branch)?.id;
                    self.workspaces
                        .detach_branch_store(&location)
                        .map_err(workspace)?;
                    self.monitor.detach_route(route_id).map_err(monitor)?;
                    state.client.disconnect_branch(branch.id).map_err(sdk)?;
                    state
                        .saved
                        .branches
                        .retain(|value| value.location != location);
                    if state.saved.active_branch.as_ref() == Some(&location) {
                        state.saved.active_branch = None;
                    }
                } else if let Some(stack_id) = state
                    .client
                    .context()
                    .map_err(sdk)?
                    .stacks
                    .iter()
                    .find(|stack| stack.location.path() == location)
                    .map(|stack| stack.id)
                {
                    state.client.disconnect_stack(stack_id).map_err(sdk)?;
                    state.saved.stacks.retain(|value| value != &location);
                    if state.saved.active_stack.as_ref() == Some(&location) {
                        state.saved.active_stack = None;
                    }
                } else if state.client.context().map_err(sdk)?.layer.location.path() == location {
                    state.client.disconnect_layer().map_err(sdk)?;
                    state.saved.layer = None;
                } else {
                    return Err(CliError::NotFound);
                }
                state.saved.save(&self.paths.context)?;
                Ok(CommandResult::Unit)
            }
            DbCommand::List => unreachable!(),
        }
    }

    fn open_database(
        &self,
        state: &mut State,
        role: StoreRole,
        location: PathBuf,
        create: bool,
    ) -> CliResult<()> {
        match role {
            StoreRole::Layer => {
                if create {
                    state
                        .client
                        .create_layer(StoreLocation::local(&location))
                        .map_err(sdk)?;
                } else {
                    state
                        .client
                        .connect_layer(StoreLocation::local(&location))
                        .map_err(sdk)?;
                }
                state.saved.layer = Some(location);
            }
            StoreRole::Stack => {
                if create {
                    state
                        .client
                        .create_stack(StoreLocation::local(&location))
                        .map_err(sdk)?;
                } else {
                    state
                        .client
                        .connect_stack(StoreLocation::local(&location))
                        .map_err(sdk)?;
                }
                state.saved.stacks.push(location.clone());
                state.saved.active_stack = Some(location);
                state.saved.active_branch = None;
            }
            StoreRole::Branch => {
                let id = if create {
                    state
                        .client
                        .create_branch(StoreLocation::local(&location))
                        .map_err(sdk)?
                } else {
                    state
                        .client
                        .connect_branch(StoreLocation::local(&location))
                        .map_err(sdk)?
                };
                let branch = state
                    .client
                    .context()
                    .map_err(sdk)?
                    .branches
                    .iter()
                    .find(|branch| branch.id == id)
                    .ok_or(CliError::NotFound)?;
                self.workspaces
                    .attach_branch_store(branch.store.clone())
                    .map_err(workspace)?;
                self.monitor
                    .attach_route(route(state, branch)?)
                    .map_err(monitor)?;
                state.saved.branches.push(SavedBranch {
                    location: location.clone(),
                    parent_stack: state.saved.active_stack.clone(),
                });
                state.saved.active_branch = Some(location);
            }
        }
        state.saved.save(&self.paths.context)
    }

    fn layer(&self, command: LayerCommand) -> CliResult<CommandResult> {
        let state = self.state.lock().map_err(|_| CliError::Integrity)?;
        let context = state.client.context().map_err(sdk)?;
        match command {
            LayerCommand::Init(request) => {
                let source = if request.empty {
                    LayerInitialization::Empty
                } else {
                    LayerInitialization::Directory(
                        request
                            .directory
                            .ok_or_else(|| CliError::Invalid("directory".to_owned()))?,
                    )
                };
                let (history, layer) = context.layer.store.initialize(source).map_err(storage)?;
                Ok(CommandResult::InitializedLayer {
                    history_id: history.id.to_string(),
                    layer_id: layer.id.to_string(),
                    root_id: layer.root_id.to_string(),
                })
            }
            LayerCommand::Pull { layer_id } => {
                let stack = active_stack(context)?;
                reference(stack.store.pull_layer(parse(&layer_id)?).map_err(storage)?)
            }
            LayerCommand::Add { source } => created(
                context
                    .layer
                    .store
                    .add_layer(layer_source(&source)?)
                    .map_err(storage)?
                    .result_id,
            ),
            LayerCommand::List => store_page(
                &state.client,
                QueryScope::Layer,
                layerfs_sdk::FactKind::Layer,
            ),
            LayerCommand::Show { id } => {
                store_fact(&state.client, QueryScope::Layer, layer_fact(&id)?)
            }
        }
    }

    fn stack(&self, command: StackCommand) -> CliResult<CommandResult> {
        let state = self.state.lock().map_err(|_| CliError::Integrity)?;
        let stack = active_stack(state.client.context().map_err(sdk)?)?;
        match command {
            StackCommand::Create { layer_id } => {
                let (history, stack_record) = stack
                    .store
                    .create_stack(parse(&layer_id)?)
                    .map_err(storage)?;
                Ok(CommandResult::CreatedStack {
                    history_id: history.id.to_string(),
                    stack_id: stack_record.id.to_string(),
                    root_id: stack_record.root_id.to_string(),
                })
            }
            StackCommand::Pull { stack_id } => {
                reference(stack.store.pull_stack(parse(&stack_id)?).map_err(storage)?)
            }
            StackCommand::Add { source } => created(
                stack
                    .store
                    .add_stack(branch_commit(&source)?)
                    .map_err(storage)?
                    .result_id,
            ),
            StackCommand::Push { stack_id } => {
                reference(stack.store.push_stack(parse(&stack_id)?).map_err(storage)?)
            }
            StackCommand::List => store_page(
                &state.client,
                QueryScope::Stack(stack.id),
                layerfs_sdk::FactKind::Stack,
            ),
            StackCommand::Show { id } => {
                store_fact(&state.client, QueryScope::Stack(stack.id), stack_fact(&id)?)
            }
        }
    }

    fn branch(&self, command: BranchCommand) -> CliResult<CommandResult> {
        let state = self.state.lock().map_err(|_| CliError::Integrity)?;
        let branch = active_branch(state.client.context().map_err(sdk)?)?;
        match command {
            BranchCommand::Create { source } => {
                let branch = branch
                    .store
                    .create_branch(branch_source(&source)?)
                    .map_err(storage)?;
                Ok(CommandResult::Id {
                    kind: "branch".to_owned(),
                    id: branch.id.to_string(),
                })
            }
            BranchCommand::Merge {
                source_branch_id,
                target_branch_id,
            } => merged(
                branch
                    .store
                    .merge(parse(&source_branch_id)?, parse(&target_branch_id)?)
                    .map_err(storage)?,
            ),
            BranchCommand::Pull { branch_id } => reference(
                branch
                    .store
                    .pull_branch(parse(&branch_id)?)
                    .map_err(storage)?
                    .1,
            ),
            BranchCommand::Push { branch_id } => reference(
                branch
                    .store
                    .push_branch(parse(&branch_id)?)
                    .map_err(storage)?,
            ),
            BranchCommand::PullCommits { branch_id } => Ok(CommandResult::Id {
                kind: "commit".to_owned(),
                id: branch
                    .store
                    .pull_commits(parse(&branch_id)?)
                    .map_err(storage)?
                    .to_string(),
            }),
            BranchCommand::List => store_page(
                &state.client,
                QueryScope::Branch(branch.id),
                layerfs_sdk::FactKind::Branch,
            ),
            BranchCommand::Show { id } => store_fact(
                &state.client,
                QueryScope::Branch(branch.id),
                branch_fact(&id)?,
            ),
            BranchCommand::Diff { left, right } => store_view(
                &state.client,
                Query::CommitDiff {
                    connection: branch.id,
                    left: parse(&left)?,
                    right: parse(&right)?,
                },
            ),
        }
    }

    fn workspace(&self, command: WorkspaceCommand) -> CliResult<CommandResult> {
        match command {
            WorkspaceCommand::Create {
                branch_id,
                root,
                container,
                projection,
            } => {
                let placement = match container {
                    Some(container) => WorkspacePlacement::Container {
                        container_id: layerfs_sdk::ContainerId(container),
                        root,
                    },
                    None => WorkspacePlacement::Host { root },
                };
                let projection = projection.map(|projection| match projection {
                    Projection::Fuse => WorkspaceProjection::Fuse,
                    Projection::Materialize => WorkspaceProjection::Materialize,
                });
                self.workspaces
                    .create_workspace_session(layerfs_sdk::CreateWorkspaceSession {
                        branch_id: parse(&branch_id)?,
                        placement,
                        projection,
                    })
                    .map(CommandResult::Workspace)
                    .map_err(workspace)
            }
            WorkspaceCommand::Exec { workspace_id, argv } => {
                let execution = self
                    .workspaces
                    .exec(
                        parse(&workspace_id)?,
                        NonEmpty::new(argv).map_err(workspace)?,
                    )
                    .map_err(workspace)?;
                Ok(CommandResult::Id {
                    kind: "execution".to_owned(),
                    id: execution.id.to_string(),
                })
            }
            WorkspaceCommand::Shell { workspace_id } => {
                let execution = self
                    .workspaces
                    .shell(parse(&workspace_id)?)
                    .map_err(workspace)?;
                Ok(CommandResult::Id {
                    kind: "execution".to_owned(),
                    id: execution.id.to_string(),
                })
            }
            WorkspaceCommand::Stop { execution_id } => {
                self.workspaces
                    .stop(parse(&execution_id)?)
                    .map_err(workspace)?;
                Ok(CommandResult::Unit)
            }
            WorkspaceCommand::Commit { workspace_id } => self
                .workspaces
                .commit_workspace_session(parse(&workspace_id)?)
                .map(CommandResult::WorkspaceCommit)
                .map_err(workspace),
            WorkspaceCommand::End {
                workspace_id,
                discard,
            } => self
                .workspaces
                .end_workspace_session(
                    parse(&workspace_id)?,
                    if discard {
                        EndWorkspaceMode::Discard
                    } else {
                        EndWorkspaceMode::Clean
                    },
                )
                .map(CommandResult::WorkspaceEnd)
                .map_err(workspace),
            WorkspaceCommand::Output {
                execution_id,
                follow,
            } => self.view(
                ViewScope::Output,
                ViewQuery::Output {
                    execution_id: parse(&execution_id)?,
                    after: 0,
                    follow,
                },
            ),
            WorkspaceCommand::List => self.view(ViewScope::Workspaces, ViewQuery::Workspaces),
            WorkspaceCommand::Show { workspace_id } => self.view(
                ViewScope::Workspace,
                ViewQuery::Workspace(parse(&workspace_id)?),
            ),
            WorkspaceCommand::Diff { workspace_id } => self.view(
                ViewScope::Workspace,
                ViewQuery::WorkspaceDiff(parse(&workspace_id)?),
            ),
        }
    }

    fn monitor(&self, command: MonitorCommand) -> CliResult<CommandResult> {
        let scope = match command {
            MonitorCommand::Db => MonitorScope::Databases,
            MonitorCommand::Dedup { route, analyze } => {
                if analyze {
                    let route = route
                        .as_ref()
                        .ok_or_else(|| CliError::Invalid("route".to_owned()))?;
                    self.monitor
                        .analyze_dedup(parse_route(route)?)
                        .map_err(monitor)?;
                }
                MonitorScope::Dedup {
                    route: route.map(|route| parse_route(&route)).transpose()?,
                }
            }
            MonitorCommand::Workspace { workspace_id } => {
                MonitorScope::Workspace(workspace_id.map(|id| parse(&id)).transpose()?)
            }
            MonitorCommand::Branch { branch_id } => MonitorScope::Branch(parse(&branch_id)?),
            MonitorCommand::Operation { operation_id } => {
                MonitorScope::Operation(operation_id.map(|id| parse_operation(&id)).transpose()?)
            }
            MonitorCommand::Process => MonitorScope::Process,
        };
        self.view(ViewScope::Monitor, ViewQuery::Monitor(scope))
    }

    fn topology(&self) -> CliResult<ViewSnapshot> {
        let state = self.state.lock().map_err(|_| CliError::Integrity)?;
        let context = state.client.context().map_err(sdk)?;
        let mut entries = vec![TopologyEntry {
            role: "layer".to_owned(),
            location: context.layer.location.path().to_string_lossy().into_owned(),
            parent: None,
            active: true,
        }];
        entries.extend(context.stacks.iter().map(|stack| TopologyEntry {
            role: "stack".to_owned(),
            location: stack.location.path().to_string_lossy().into_owned(),
            parent: Some(context.layer.location.path().to_string_lossy().into_owned()),
            active: context.active_stack == Some(stack.id),
        }));
        entries.extend(context.branches.iter().map(|branch| {
            TopologyEntry {
                role: "branch".to_owned(),
                location: branch.location.path().to_string_lossy().into_owned(),
                parent: Some(match branch.parent {
                    layerfs_sdk::BranchParent::Layer(_) => {
                        context.layer.location.path().to_string_lossy().into_owned()
                    }
                    layerfs_sdk::BranchParent::Stack(id) => context
                        .stacks
                        .iter()
                        .find(|stack| stack.id == id)
                        .map(|stack| stack.location.path().to_string_lossy().into_owned())
                        .unwrap_or_default(),
                }),
                active: context.active_branch == Some(branch.id),
            }
        }));
        Ok(ViewSnapshot::Topology(entries))
    }

    fn store_snapshot(&self, query: StoreQuery) -> CliResult<ViewSnapshot> {
        let mut state = self.state.lock().map_err(|_| CliError::Integrity)?;
        let query = match query {
            StoreQuery::Page {
                scope,
                kind,
                after,
                limit,
            } => Query::Page {
                scope: store_scope(&state.client, scope)?,
                kind,
                after: after
                    .map(|cursor| {
                        state
                            .cursors
                            .iter()
                            .find(|(key, _)| key == &cursor)
                            .map(|(_, value)| value.clone())
                            .ok_or_else(|| CliError::Invalid("page cursor".to_owned()))
                    })
                    .transpose()?,
                limit,
            },
            StoreQuery::Fact { scope, id } => Query::Fact {
                scope: store_scope(&state.client, scope)?,
                id,
            },
            StoreQuery::CommitDiff {
                branch,
                left,
                right,
            } => Query::CommitDiff {
                connection: branch_connection(&state.client, &absolute(&branch)?)?.id,
                left,
                right,
            },
        };
        let result = state.client.query(query).map_err(sdk)?;
        let next = match &result {
            QueryResult::Page(page) => page.next.clone(),
            _ => None,
        };
        let snapshot = store_snapshot(result)?;
        if let Some(value) = next {
            let cursor = value.to_string();
            state.cursors.retain(|(key, _)| key != &cursor);
            state.cursors.push_back((cursor, value));
            if state.cursors.len() > 1024 {
                state.cursors.pop_front();
            }
        }
        Ok(ViewSnapshot::Store(snapshot))
    }

    fn view(&self, scope: ViewScope, query: ViewQuery) -> CliResult<CommandResult> {
        Ok(CommandResult::View {
            scope,
            snapshot: self.snapshot(query)?,
        })
    }
}

fn store_page(
    client: &Client,
    scope: QueryScope,
    kind: layerfs_sdk::FactKind,
) -> CliResult<CommandResult> {
    store_view(
        client,
        Query::Page {
            scope,
            kind,
            after: None,
            limit: 128,
        },
    )
}

fn store_fact(
    client: &Client,
    scope: QueryScope,
    id: layerfs_sdk::FactId,
) -> CliResult<CommandResult> {
    store_view(client, Query::Fact { scope, id })
}

fn store_view(client: &Client, query: Query) -> CliResult<CommandResult> {
    Ok(CommandResult::View {
        scope: ViewScope::Store,
        snapshot: ViewSnapshot::Store(store_snapshot(client.query(query).map_err(sdk)?)?),
    })
}

fn store_scope(client: &Client, scope: StoreScope) -> CliResult<QueryScope> {
    let context = client.context().map_err(sdk)?;
    Ok(match scope {
        StoreScope::Layer => QueryScope::Layer,
        StoreScope::Stack(path) => {
            let path = absolute(&path)?;
            QueryScope::Stack(
                context
                    .stacks
                    .iter()
                    .find(|connection| connection.location.path() == path)
                    .map(|connection| connection.id)
                    .ok_or(CliError::NotFound)?,
            )
        }
        StoreScope::Branch(path) => {
            QueryScope::Branch(branch_connection(client, &absolute(&path)?)?.id)
        }
    })
}

fn store_snapshot(result: layerfs_sdk::QueryResult) -> CliResult<StoreSnapshot> {
    Ok(match result {
        QueryResult::Page(page) => StoreSnapshot::Page {
            facts: page
                .facts
                .into_iter()
                .map(crate::query::store_fact)
                .collect(),
            next: page.next.map(|cursor| cursor.to_string()),
        },
        QueryResult::Fact(fact) => StoreSnapshot::Fact(fact.map(crate::query::store_fact)),
        QueryResult::CommitDiff(values) => StoreSnapshot::CommitDiff(
            values
                .into_iter()
                .map(|value| CommitDiffEntry {
                    inode: format!("{:?}", value.inode),
                    before: value.before.map(|id| id.to_string()),
                    after: value.after.map(|id| id.to_string()),
                })
                .collect(),
        ),
        QueryResult::Topology(_) => {
            return Err(CliError::Invalid("use topology snapshot".to_owned()))
        }
    })
}

pub(crate) fn summary(command: &Command) -> CommandSummary {
    CommandSummary(format!("{command:?}"))
}

fn active_stack(
    context: &layerfs_sdk::ConnectionContext,
) -> CliResult<&layerfs_sdk::StackConnection> {
    let id = context.active_stack.ok_or(CliError::NotFound)?;
    context
        .stacks
        .iter()
        .find(|stack| stack.id == id)
        .ok_or(CliError::NotFound)
}

fn active_branch(
    context: &layerfs_sdk::ConnectionContext,
) -> CliResult<&layerfs_sdk::BranchConnection> {
    let id = context.active_branch.ok_or(CliError::NotFound)?;
    context
        .branches
        .iter()
        .find(|branch| branch.id == id)
        .ok_or(CliError::NotFound)
}

fn branch_connection<'a>(
    client: &'a Client,
    path: &Path,
) -> CliResult<&'a layerfs_sdk::BranchConnection> {
    client
        .context()
        .map_err(sdk)?
        .branches
        .iter()
        .find(|branch| branch.location.path() == path)
        .ok_or(CliError::NotFound)
}

fn select_stack(client: &mut Client, path: Option<&Path>) -> CliResult<()> {
    let id = path
        .map(|path| {
            client
                .context()
                .map_err(sdk)?
                .stacks
                .iter()
                .find(|stack| stack.location.path() == path)
                .map(|stack| stack.id)
                .ok_or(CliError::NotFound)
        })
        .transpose()?;
    client.use_stack(id).map_err(sdk)
}

fn route(state: &State, branch: &layerfs_sdk::BranchConnection) -> CliResult<MonitoredRoute> {
    let context = state.client.context().map_err(sdk)?;
    let stack = match branch.parent {
        layerfs_sdk::BranchParent::Layer(_) => None,
        layerfs_sdk::BranchParent::Stack(id) => context
            .stacks
            .iter()
            .find(|stack| stack.id == id)
            .map(|stack| stack.store.clone()),
    };
    Ok(MonitoredRoute::new(
        branch.store.clone(),
        stack,
        context.layer.store.clone(),
    ))
}

fn layer_source(value: &str) -> CliResult<LayerSource> {
    if value.contains('@') {
        branch_commit(value).map(LayerSource::BranchCommit)
    } else {
        parse(value).map(LayerSource::Stack)
    }
}

fn layer_fact(value: &str) -> CliResult<layerfs_sdk::FactId> {
    if value.len() == 34 && value.starts_with("20") {
        parse(value).map(layerfs_sdk::FactId::LayerHistory)
    } else {
        parse(value).map(layerfs_sdk::FactId::Layer)
    }
}

fn stack_fact(value: &str) -> CliResult<layerfs_sdk::FactId> {
    if value.len() == 98 && value.starts_with("21") {
        parse(value).map(layerfs_sdk::FactId::StackHistory)
    } else {
        parse(value).map(layerfs_sdk::FactId::Stack)
    }
}

fn branch_fact(value: &str) -> CliResult<layerfs_sdk::FactId> {
    if let Some((_, commit)) = value.split_once('@') {
        parse(commit).map(layerfs_sdk::FactId::Commit)
    } else {
        parse(value).map(layerfs_sdk::FactId::Branch)
    }
}

fn branch_source(value: &str) -> CliResult<BranchSource> {
    if value.contains('@') {
        return branch_commit(value).map(BranchSource::Commit);
    }
    if value.len() == 66 && value.starts_with("32") {
        parse(value).map(BranchSource::Layer)
    } else {
        parse(value).map(BranchSource::Stack)
    }
}

fn branch_commit(value: &str) -> CliResult<BranchCommit> {
    let (branch_id, commit_id) = value
        .split_once('@')
        .ok_or_else(|| CliError::Invalid("Branch Commit".to_owned()))?;
    Ok(BranchCommit {
        branch_id: parse(branch_id)?,
        commit_id: parse(commit_id)?,
    })
}

fn parse<T>(value: &str) -> CliResult<T>
where
    T: FromStr,
    T::Err: std::fmt::Debug,
{
    value
        .parse()
        .map_err(|_| CliError::Invalid("typed ID".to_owned()))
}

fn parse_route(value: &str) -> CliResult<layerfs_sdk::BranchStoreId> {
    parse(value)
}

fn parse_operation(value: &str) -> CliResult<OperationId> {
    parse(value)
}

fn reference<T: std::fmt::Display>(value: layerfs_sdk::RefOutcome<T>) -> CliResult<CommandResult> {
    let (outcome, id) = match value {
        layerfs_sdk::RefOutcome::Created(id) => ("CREATED", id),
        layerfs_sdk::RefOutcome::FastForwarded(id) => ("FAST_FORWARDED", id),
        layerfs_sdk::RefOutcome::UpToDate(id) => ("UP_TO_DATE", id),
    };
    Ok(CommandResult::Reference {
        outcome: outcome.to_owned(),
        id: id.to_string(),
    })
}

fn created(value: impl std::fmt::Display) -> CliResult<CommandResult> {
    Ok(CommandResult::Reference {
        outcome: "CREATED".to_owned(),
        id: value.to_string(),
    })
}

fn merged(value: layerfs_sdk::MergeOutcome) -> CliResult<CommandResult> {
    match value {
        layerfs_sdk::MergeOutcome::UpToDate(id) => reference(layerfs_sdk::RefOutcome::UpToDate(id)),
        layerfs_sdk::MergeOutcome::FastForwarded(id) => {
            reference(layerfs_sdk::RefOutcome::FastForwarded(id))
        }
        layerfs_sdk::MergeOutcome::Merged(id) => created(id),
    }
}

fn absolute(path: &Path) -> CliResult<PathBuf> {
    if path.is_absolute() {
        Ok(path.to_owned())
    } else {
        Ok(std::env::current_dir()
            .map_err(|error| CliError::Io(error.to_string()))?
            .join(path))
    }
}

fn sdk(error: layerfs_sdk::SdkError) -> CliError {
    CliError::Context(error.to_string())
}
fn storage(error: layerfs_sdk::StorageError) -> CliError {
    match error {
        layerfs_sdk::StorageError::Conflict(_) => CliError::Conflict,
        layerfs_sdk::StorageError::CommitHeadMoved(_) => CliError::HeadMoved,
        layerfs_sdk::StorageError::WrongLayerHistory(_)
        | layerfs_sdk::StorageError::WrongStackHistory(_) => CliError::WrongHistory,
        layerfs_sdk::StorageError::ReadOnlyStackHistory(_) => CliError::ReadOnly,
        layerfs_sdk::StorageError::NotFound(_) => CliError::NotFound,
        layerfs_sdk::StorageError::Integrity(_) => CliError::Integrity,
        error => CliError::Context(error.to_string()),
    }
}
pub(crate) fn workspace(error: layerfs_sdk::WorkspaceError) -> CliError {
    match error {
        layerfs_sdk::WorkspaceError::WorkspaceBusy => CliError::WorkspaceBusy,
        layerfs_sdk::WorkspaceError::WorkspaceDirty => CliError::WorkspaceDirty,
        layerfs_sdk::WorkspaceError::ReadOnly => CliError::ReadOnly,
        layerfs_sdk::WorkspaceError::NotFound => CliError::NotFound,
        error => CliError::Context(error.to_string()),
    }
}
fn monitor(error: layerfs_sdk::MonitorError) -> CliError {
    CliError::Context(error.to_string())
}

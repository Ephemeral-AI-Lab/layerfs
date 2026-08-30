use crate::command::{CommandKind, WorkspaceAnchor};
use crate::database::Databases;
use crate::fixture::{
    layer_for, trailing_number, BranchRecord, LayerRecord, MockState, ProjectRecord,
};
use crate::model::{
    field, BranchOrigin, BranchRelation, CliError, CliEvent, CliResult, CommandEffect, CommandPlan,
    CommandResult, Completion, FinishedStatus, OperationReceipt, OperationState, OperationView,
    RemotePlacement, ViewQuery, ViewSnapshot,
};
use crate::{BranchId, Command, EntityName, LayerId, LayerStackId, OperationId};
use std::collections::VecDeque;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

mod events;
mod reconcile;
use crate::snapshot::{changes_snapshot, explicit_diff_snapshot, files_snapshot};
use crate::workspace::{canonical_tree, create_workspace, seed_tree, workspace_action};
use events::{empty_receipt, normalize_receipt, record_operation_event};
use reconcile::create_reconciliation_workspace;

#[derive(Clone)]
pub struct CliSession {
    state: Arc<Mutex<MockState>>,
    databases: Arc<Databases>,
    next_operation: Arc<AtomicU64>,
}

pub struct OperationHandle {
    id: OperationId,
    state: Arc<Mutex<MockState>>,
    databases: Arc<Databases>,
    pending: Option<Command>,
    events: VecDeque<Option<CliEvent>>,
    terminal: bool,
}

impl CliSession {
    pub fn open(context_location: impl AsRef<Path>) -> CliResult<Self> {
        let fixture = matches!(
            context_location.as_ref().to_str(),
            Some("mock" | "mock-empty")
        );
        let empty = context_location.as_ref() != Path::new("mock");
        let databases = Arc::new(Databases::open(context_location.as_ref(), empty)?);
        let mut state = if fixture {
            if empty {
                MockState::empty()
            } else {
                MockState::demo()
            }
        } else {
            match databases.load_state() {
                Ok(state) => state,
                Err(CliError::NotFound(_)) => MockState::empty(),
                Err(error) => return Err(error),
            }
        };
        if fixture {
            databases.decorate_storage(&mut state)?;
        }
        Ok(Self {
            state: Arc::new(Mutex::new(state)),
            databases,
            next_operation: Arc::new(AtomicU64::new(100)),
        })
    }

    pub fn parse_line(input: &str) -> CliResult<Command> {
        Command::parse(input)
    }

    pub fn complete(&self, input: &str, cursor: usize) -> CliResult<Vec<Completion>> {
        if cursor > input.len() || !input.is_char_boundary(cursor) {
            return Err(CliError::Parse("cursor is not a UTF-8 boundary".into()));
        }
        let start = input[..cursor]
            .rfind(char::is_whitespace)
            .map_or(0, |index| index + 1);
        let prefix = &input[start..cursor];
        let state = self.lock()?;
        let mut values = vec![
            ("layerstack", "LayerStack operations"),
            ("db", "Create or validate one real SQLite Store"),
            ("context", "Select or show one Store pair"),
            ("branch", "Branch operations"),
            ("workspace", "Workspace operations"),
            ("monitor", "Monitor operations"),
            ("query", "Read-only snapshots"),
            ("pull", "Pull through an exact boundary"),
            ("fork", "Create a local named Branch"),
            ("push", "Publish a locally owned suffix"),
            ("diff", "Compare exact snapshots"),
            ("--through", "Exact upper boundary"),
            ("--reference", "Local-first parent-backed serving"),
            ("--replica", "Offline-complete serving"),
            ("--name", "Immutable entity name"),
            ("--layerstack", "LayerStackStore location"),
            ("--branch", "BranchStore location"),
            ("--parent", "Immutable parent LayerStackStore"),
        ]
        .into_iter()
        .map(|(value, description)| (value.to_owned(), description.to_owned()))
        .collect::<Vec<_>>();
        values.extend(
            state
                .projects
                .iter()
                .map(|project| (project.name.to_string(), format!("project {}", project.id))),
        );
        values.extend(state.branches.iter().map(|branch| {
            let project = state
                .projects
                .iter()
                .find(|project| project.id == branch.project_id)
                .expect("fixture project");
            (
                branch.id.to_string(),
                format!("{}/{}", project.name, branch.name),
            )
        }));
        values.extend(
            state
                .layers
                .iter()
                .map(|layer| (layer.id.to_string(), format!("Layer {}", layer.number))),
        );
        values.extend(state.workspaces.iter().map(|workspace| {
            (
                workspace.id.to_string(),
                format!(
                    "Workspace {}/{}",
                    workspace.project_name, workspace.branch_name
                ),
            )
        }));
        values.sort();
        values.dedup_by(|left, right| left.0 == right.0);
        Ok(values
            .into_iter()
            .filter(|(value, _)| value.starts_with(prefix))
            .map(|(value, description)| Completion {
                start,
                end: cursor,
                value,
                description,
            })
            .collect())
    }

    pub fn plan(&self, command: &Command) -> CliResult<CommandPlan> {
        let state = self.lock()?;
        plan(&state, command)
    }

    pub fn execute(&self, command: Command) -> CliResult<OperationHandle> {
        let plan = self.plan(&command)?;
        let number = self.next_operation.fetch_add(1, Ordering::Relaxed);
        let id = OperationId::new(format!("OP-{number}"));
        let total: u64 = match command.kind {
            CommandKind::LayerStackPull { .. } => 5,
            CommandKind::BranchPull { .. } => 6,
            CommandKind::Diff(_) => 3,
            _ => 4,
        };
        let transfer = matches!(
            command.kind,
            CommandKind::LayerStackPull {
                placement: RemotePlacement::Replica,
                ..
            } | CommandKind::BranchPull {
                placement: RemotePlacement::Replica,
                ..
            } | CommandKind::BranchPush { .. }
        );
        let fact_write = store_write(&command.kind);
        let receipt = OperationReceipt {
            facts_announced: if fact_write { total } else { 0 },
            facts_missing: if fact_write {
                total.saturating_sub(2)
            } else {
                0
            },
            facts_inserted: if fact_write {
                total.saturating_sub(2)
            } else {
                0
            },
            objects_announced: if transfer { 18_421 } else { 0 },
            objects_missing: if transfer { 1_204 } else { 0 },
            objects_sent: if transfer { 1_204 } else { 0 },
            objects_inserted: if transfer { 1_198 } else { 0 },
            objects_raced: if transfer { 6 } else { 0 },
            elapsed_ms: 84,
            elapsed_micros: 84_000,
        };
        let work_phase = if transfer {
            "transferring missing objects"
        } else {
            "applying semantic result"
        };
        let events = VecDeque::from([
            Some(CliEvent::Started {
                operation_id: id.clone(),
                command: command.raw.clone(),
            }),
            None,
            Some(CliEvent::Progress {
                operation_id: id.clone(),
                phase: "enumerating history".into(),
                completed: 1,
                total,
                elapsed_ms: 8,
            }),
            None,
            Some(CliEvent::Progress {
                operation_id: id.clone(),
                phase: work_phase.into(),
                completed: total.saturating_sub(1),
                total,
                elapsed_ms: 47,
            }),
            None,
            Some(CliEvent::Output {
                operation_id: id.clone(),
                line: plan.summary.clone(),
            }),
            None,
            Some(CliEvent::Snapshot {
                operation_id: id.clone(),
                scope: affected_scope(&command),
            }),
            None,
            Some(CliEvent::Finished {
                operation_id: id.clone(),
                status: FinishedStatus::Succeeded,
                result: Ok(CommandResult::Query("pending".into())),
                receipt,
            }),
        ]);
        self.lock()?.operations.insert(
            0,
            OperationView {
                id: id.clone(),
                title: command.raw.clone(),
                project: None,
                phase: "queued".into(),
                completed: 0,
                total,
                state: OperationState::Running,
                receipt: empty_receipt(),
                events: Vec::new(),
            },
        );
        Ok(OperationHandle {
            id,
            state: self.state.clone(),
            databases: self.databases.clone(),
            pending: Some(command),
            events,
            terminal: false,
        })
    }

    pub fn snapshot(&self, query: ViewQuery) -> CliResult<ViewSnapshot> {
        let state = self.lock()?;
        match query {
            ViewQuery::Projects(page) => Ok(ViewSnapshot::Projects(state.project_summaries(&page))),
            ViewQuery::Project { id, page } => state
                .project_snapshot(&id, &page)
                .map(ViewSnapshot::Project)
                .ok_or_else(|| CliError::NotFound(id.to_string())),
            ViewQuery::Branch { id, page } => {
                let branch = state
                    .branch(&id)
                    .ok_or_else(|| CliError::NotFound(id.to_string()))?;
                let project = state
                    .project_snapshot(&branch.project_id, &page)
                    .ok_or_else(|| CliError::NotFound(branch.project_id.to_string()))?;
                Ok(ViewSnapshot::Branch(project, id))
            }
            ViewQuery::Workspaces { project, page } => Ok(ViewSnapshot::Workspaces(
                state.workspace_snapshot(project.as_ref(), &page),
            )),
            ViewQuery::Activity(page) => Ok(ViewSnapshot::Activity(state.activity_snapshot(&page))),
            ViewQuery::Files { target, page } => {
                files_snapshot(&state, target, &page).map(ViewSnapshot::Files)
            }
            ViewQuery::Changes { target, page } => {
                changes_snapshot(&state, target, &page).map(ViewSnapshot::Changes)
            }
            ViewQuery::Diff { request, page } => {
                validate_diff(&state, &request)?;
                explicit_diff_snapshot(&state, &request, &page).map(ViewSnapshot::Diff)
            }
        }
    }

    fn lock(&self) -> CliResult<std::sync::MutexGuard<'_, MockState>> {
        self.state
            .lock()
            .map_err(|_| CliError::Integrity("mock state lock".into()))
    }
}

impl OperationHandle {
    pub fn id(&self) -> &OperationId {
        &self.id
    }

    pub fn interrupt(&mut self) -> CliResult<()> {
        if self.terminal {
            return Ok(());
        }
        self.pending = None;
        self.events.clear();
        self.events.push_back(Some(CliEvent::Finished {
            operation_id: self.id.clone(),
            status: FinishedStatus::Interrupted,
            result: Err(CliError::Interrupted),
            receipt: empty_receipt(),
        }));
        Ok(())
    }

    pub fn try_next_event(&mut self) -> CliResult<Option<CliEvent>> {
        let Some(event) = self.events.pop_front() else {
            return Ok(None);
        };
        let Some(mut event) = event else {
            return Ok(None);
        };
        if let CliEvent::Finished {
            status,
            result,
            receipt,
            operation_id: _,
        } = &mut event
        {
            if *status == FinishedStatus::Succeeded {
                if let Some(command) = self.pending.take() {
                    let mut state = self
                        .state
                        .lock()
                        .map_err(|_| CliError::Integrity("mock state lock".into()))?;
                    let mut candidate = state.clone();
                    *result = apply(&mut candidate, command, &self.databases);
                    if let Some(micros) = candidate.last_operation_elapsed_micros.take() {
                        receipt.elapsed_micros = micros;
                        receipt.elapsed_ms = micros.saturating_add(999) / 1_000;
                    }
                    if result.is_ok() {
                        *state = candidate;
                    }
                    if result.is_err() {
                        *status = FinishedStatus::Failed;
                    }
                }
            }
            normalize_receipt(result, receipt);
            self.terminal = true;
        }
        let mut state = self
            .state
            .lock()
            .map_err(|_| CliError::Integrity("mock state lock".into()))?;
        record_operation_event(&mut state, &event);
        Ok(Some(event))
    }

    pub fn next_event(&mut self) -> CliResult<Option<CliEvent>> {
        loop {
            let pending = !self.events.is_empty();
            match self.try_next_event()? {
                Some(event) => return Ok(Some(event)),
                None if pending => continue,
                None => return Ok(None),
            }
        }
    }
}

fn plan(state: &MockState, command: &Command) -> CliResult<CommandPlan> {
    let mut fields = Vec::new();
    let mut consequences = Vec::new();
    let (title, effect, summary) = match &command.kind {
        CommandKind::LayerStackPull { through, placement } => {
            let layer = state
                .find_layer(through)
                .ok_or_else(|| CliError::NotFound(through.clone()))?;
            let project = state
                .projects
                .iter()
                .find(|project| project.id == layer.project_id)
                .expect("fixture project");
            if !project.authority_available {
                return Err(CliError::AuthorityUnavailable(project.name.to_string()));
            }
            fields.push(field(
                "Project",
                format!("{} ({})", project.name, project.id),
            ));
            fields.push(field(
                "Through",
                format!("{} / L{}", layer.id, layer.number),
            ));
            fields.push(field(
                "Current boundary",
                project
                    .work_layers
                    .map(|number| format!("L{number}"))
                    .unwrap_or_else(|| "NOT PULLED".into()),
            ));
            fields.push(field(
                "Current mode",
                project
                    .mode
                    .map(|mode| mode.to_string())
                    .unwrap_or_else(|| "—".into()),
            ));
            fields.push(field("Requested mode", placement.to_string()));
            fields.push(field(
                "Missing suffix",
                layer
                    .number
                    .saturating_sub(project.work_layers.unwrap_or(0))
                    .to_string(),
            ));
            fields.push(field(
                "Complete roots",
                format!("through L{}", project.complete_roots),
            ));
            consequences.push("Acquire the complete Layer prefix through this boundary".into());
            if *placement == RemotePlacement::Replica {
                consequences.push("Verify every historical Layer root locally".into());
            }
            (
                "Pull LayerStack through boundary",
                CommandEffect::Mutate,
                format!("{} through L{} as {placement}", project.name, layer.number),
            )
        }
        CommandKind::BranchPull {
            branch,
            through,
            placement,
        } => {
            let branch = state
                .find_branch(branch)
                .ok_or_else(|| CliError::NotFound(branch.clone()))?;
            let project = state
                .projects
                .iter()
                .find(|project| project.id == branch.project_id)
                .expect("fixture project");
            if !project.authority_available {
                return Err(CliError::AuthorityUnavailable(project.name.to_string()));
            }
            let number = commit_number(state, branch, through)?;
            fields.push(field("Branch", format!("{} ({})", branch.name, branch.id)));
            fields.push(field(
                "Current boundary",
                branch
                    .work_head
                    .map(|number| format!("C{number}"))
                    .unwrap_or_else(|| "NOT PULLED".into()),
            ));
            fields.push(field("Current relation", branch.relation.to_string()));
            fields.push(field("Through", format!("C{number}")));
            fields.push(field("Requested mode", placement.to_string()));
            consequences.push("Acquire complete visible Commit ancestry".into());
            if *placement == RemotePlacement::Replica {
                consequences.push("Verify every visible Commit root locally".into());
            }
            (
                "Pull Branch through boundary",
                CommandEffect::Mutate,
                format!("{} through C{number} as {placement}", branch.name),
            )
        }
        CommandKind::BranchForkLayer { name, layer } => {
            let layer = state
                .find_layer(layer)
                .ok_or_else(|| CliError::NotFound(layer.clone()))?;
            let project = state
                .projects
                .iter()
                .find(|project| project.id == layer.project_id)
                .expect("fixture project");
            if project
                .work_layers
                .is_none_or(|boundary| layer.number > boundary)
            {
                return Err(CliError::NotPulled(layer.id.to_string()));
            }
            if state.relation_name_exists(&layer.project_id, name) {
                return Err(CliError::NameConflict(name.to_string()));
            }
            fields.push(field("Name", name.to_string()));
            fields.push(field("Source Layer", layer.id.to_string()));
            consequences.extend([
                "Remote calls: 0".into(),
                "Canonical objects copied: 0".into(),
            ]);
            (
                "Fork local Branch",
                CommandEffect::Mutate,
                format!("create {name} from {}", layer.id),
            )
        }
        CommandKind::BranchForkCommit {
            name,
            branch,
            commit,
        } => {
            let source = state
                .find_branch(branch)
                .ok_or_else(|| CliError::NotFound(branch.clone()))?;
            let number = commit_number(state, source, commit)?;
            if !commit_is_local(source, number, commit) {
                return Err(CliError::NotPulled(format!("{}/C{number}", source.name)));
            }
            if state.relation_name_exists(&source.project_id, name) {
                return Err(CliError::NameConflict(name.to_string()));
            }
            fields.push(field("Name", name.to_string()));
            fields.push(field("Source", format!("{}/C{number}", source.name)));
            consequences.extend([
                "Remote calls: 0".into(),
                "Canonical objects copied: 0".into(),
            ]);
            (
                "Fork local Branch",
                CommandEffect::Mutate,
                format!("create {name} from {}/C{number}", source.name),
            )
        }
        CommandKind::WorkspaceCreate {
            anchor,
            path,
            container,
            projection,
        } => {
            let (branch, anchor_label) = validate_workspace_anchor(state, anchor)?;
            if state
                .workspaces
                .iter()
                .any(|workspace| workspace.branch_id == branch.id)
            {
                return Err(CliError::WorkspaceBusy(branch.name.to_string()));
            }
            fields.push(field(
                "Target Branch",
                format!("{} ({})", branch.name, branch.id),
            ));
            fields.push(field("Anchor", anchor_label));
            fields.push(field("Path", path.clone()));
            fields.push(field("Projection", projection.clone()));
            fields.push(field(
                "Placement",
                container
                    .as_ref()
                    .map(|id| format!("container {id}"))
                    .unwrap_or_else(|| "host".into()),
            ));
            consequences.push("Create one ephemeral writable COW session".into());
            (
                "Create Workspace",
                CommandEffect::Mutate,
                format!("Workspace for {}", branch.name),
            )
        }
        CommandKind::BranchPush { branch } => {
            let branch = state
                .find_branch(branch)
                .ok_or_else(|| CliError::NotFound(branch.clone()))?;
            if !matches!(
                branch.relation,
                BranchRelation::LocalOnly
                    | BranchRelation::LocalCurrent
                    | BranchRelation::LocalPushAhead { .. }
            ) {
                return Err(CliError::HeadMoved(branch.name.to_string()));
            }
            fields.push(field("Branch", format!("{} ({})", branch.name, branch.id)));
            fields.push(field(
                "Authority head",
                branch
                    .authority_head
                    .map(|head| format!("C{head}"))
                    .unwrap_or_else(|| "—".into()),
            ));
            fields.push(field(
                "Work head",
                branch
                    .work_head
                    .map(|head| format!("C{head}"))
                    .unwrap_or_else(|| "—".into()),
            ));
            consequences.push("Transfer only the locally owned suffix".into());
            (
                "Push local Branch",
                CommandEffect::Mutate,
                format!("publish {}", branch.name),
            )
        }
        CommandKind::LayerStackAdd { branch } => {
            let branch = state
                .find_branch(branch)
                .ok_or_else(|| CliError::NotFound(branch.clone()))?;
            let preflight = add_preflight(state, branch)?;
            fields.push(field(
                "Source Branch",
                format!("{} ({})", branch.name, branch.id),
            ));
            let project = state
                .projects
                .iter()
                .find(|project| project.id == branch.project_id)
                .expect("fixture project");
            fields.push(field(
                "Current Layer",
                format!("L{}", project.authority_layers),
            ));
            fields.push(field(
                "Result Layer",
                match &preflight {
                    AddPreflight::Already(layer) => {
                        format!("{layer} · already accepted")
                    }
                    AddPreflight::Ready => format!("L{}", project.authority_layers + 1),
                    AddPreflight::NeedsResolution { current, .. } => {
                        format!("resolution Workspace against {current}")
                    }
                },
            ));
            consequences.push(match preflight {
                AddPreflight::Already(_) => {
                    "No write: this exact Branch head is already accepted".into()
                }
                AddPreflight::Ready => "Accept the exact pushed head as the next Layer".into(),
                AddPreflight::NeedsResolution { old, current } => {
                    format!("Create an ephemeral reconciliation Workspace: {old} → {current}")
                }
            });
            (
                "Add Branch as Layer",
                CommandEffect::Mutate,
                format!("accept {}", branch.name),
            )
        }
        CommandKind::WorkspaceAction { action, target, .. } => (
            "Workspace operation",
            if matches!(action.as_str(), "exec" | "shell") {
                CommandEffect::Execute
            } else if matches!(action.as_str(), "output" | "conflicts") {
                CommandEffect::Read
            } else {
                CommandEffect::Mutate
            },
            format!("{action} {target}"),
        ),
        CommandKind::Diff(request) => {
            validate_diff(state, request)?;
            let (name, from, to) = request.labels();
            fields.extend([field("From", from), field("To", to)]);
            ("Open Diff", CommandEffect::Read, name)
        }
        CommandKind::Monitor { action } => {
            ("Monitor observation", CommandEffect::Read, action.clone())
        }
        CommandKind::LayerStackInit { name, source } => {
            fields.extend([
                field("Name", name.to_string()),
                field("Source", source.clone()),
            ]);
            (
                "Initialize named LayerStack",
                CommandEffect::Mutate,
                format!("create project {name}"),
            )
        }
        CommandKind::DbCreate { role, location, .. } => (
            "Create Store",
            CommandEffect::Mutate,
            format!("create {role:?} at {location}"),
        ),
        CommandKind::DbConnect { role, location, .. } => (
            "Connect Store",
            CommandEffect::Read,
            format!("connect {role:?} at {location}"),
        ),
        CommandKind::ContextUse { layerstack, branch } => (
            "Use Store pair",
            CommandEffect::Mutate,
            format!("LayerStackStore {layerstack}; BranchStore {branch}"),
        ),
        CommandKind::ContextShow => (
            "Show Store pair",
            CommandEffect::Read,
            "show active context".into(),
        ),
        CommandKind::ReadOnly { family, action } => (
            "Read-only command",
            CommandEffect::Read,
            format!("{family} {action}"),
        ),
    };
    Ok(CommandPlan {
        title: title.into(),
        effect,
        summary,
        fields,
        consequences,
        confirmation_required: effect != CommandEffect::Read,
    })
}

fn apply(
    state: &mut MockState,
    command: Command,
    databases: &Databases,
) -> Result<CommandResult, CliError> {
    if let Some(result) = databases.apply_control(state, &command.kind) {
        return result;
    }
    let write_store = store_write(&command.kind);
    let commit_target = match &command.kind {
        CommandKind::WorkspaceAction { action, target, .. } if action == "commit" => {
            Some(target.clone())
        }
        _ => None,
    };
    let before = commit_target
        .as_ref()
        .map(|_| databases.branch_storage_bytes())
        .transpose()?;
    let result = apply_state(state, command, databases)?;
    if let Some(target) = commit_target.as_deref() {
        if state
            .workspaces
            .iter()
            .find(|workspace| workspace.id.as_str() == target)
            .is_some_and(|workspace| workspace.published_commit.is_some())
        {
            databases.publish_workspace_commit(state, target)?;
        }
    } else if write_store {
        databases.sync(state)?;
    }
    if let (Some(target), Some(before)) = (commit_target, before) {
        let growth = databases.branch_storage_bytes()?.saturating_sub(before);
        if let Some(workspace) = state
            .workspaces
            .iter_mut()
            .find(|workspace| workspace.id.as_str() == target)
        {
            workspace.storage.sqlite_growth_bytes = growth;
            if let Some(receipt) = &mut workspace.commit_receipt {
                receipt.sqlite_growth_bytes = growth;
            }
        }
    }
    if write_store {
        databases.decorate_storage(state)?;
    }
    Ok(result)
}

fn store_write(command: &CommandKind) -> bool {
    match command {
        CommandKind::WorkspaceAction { action, .. } => action == "commit",
        CommandKind::LayerStackPull { .. }
        | CommandKind::BranchPull { .. }
        | CommandKind::BranchForkLayer { .. }
        | CommandKind::BranchForkCommit { .. }
        | CommandKind::BranchPush { .. }
        | CommandKind::LayerStackAdd { .. }
        | CommandKind::LayerStackInit { .. } => true,
        _ => false,
    }
}

fn apply_state(
    state: &mut MockState,
    command: Command,
    databases: &Databases,
) -> Result<CommandResult, CliError> {
    match command.kind {
        CommandKind::LayerStackPull { through, placement } => {
            let layer = state
                .find_layer(&through)
                .cloned()
                .ok_or_else(|| CliError::NotFound(through.clone()))?;
            let project = state
                .projects
                .iter_mut()
                .find(|project| project.id == layer.project_id)
                .expect("fixture project");
            if project
                .work_layers
                .is_some_and(|current| layer.number < current)
            {
                return Ok(CommandResult::Pull(format!(
                    "AlreadyContained through L{}",
                    project.work_layers.unwrap_or_default()
                )));
            }
            let previous = project.work_layers;
            let previous_mode = project.mode;
            project.work_layers = Some(layer.number);
            project.mode = Some(placement);
            if placement == RemotePlacement::Replica {
                project.complete_roots = project.complete_roots.max(layer.number);
            }
            let outcome = match (previous, previous_mode) {
                (None, _) => "Created",
                (Some(value), _) if value < layer.number => "Advanced",
                (_, Some(mode)) if mode != placement => "ModeChanged",
                _ => "UpToDate",
            };
            Ok(CommandResult::Pull(format!(
                "{outcome} through L{}",
                layer.number
            )))
        }
        CommandKind::BranchPull {
            branch,
            through,
            placement,
        } => {
            let id = state
                .find_branch(&branch)
                .map(|record| record.id.clone())
                .ok_or_else(|| CliError::NotFound(branch.clone()))?;
            let number = {
                let record = state.branch(&id).expect("resolved Branch");
                commit_number(state, record, &through)?
            };
            let record = state.branch_mut(&id).expect("resolved Branch");
            if matches!(
                record.relation,
                BranchRelation::LocalOnly
                    | BranchRelation::LocalCurrent
                    | BranchRelation::LocalPushAhead { .. }
                    | BranchRelation::AuthorityAhead { .. }
                    | BranchRelation::Diverged
            ) {
                return Err(CliError::ReadOnly(
                    "cannot Pull into locally owned Branch".into(),
                ));
            }
            let current = record.work_head;
            if current.is_some_and(|current| number < current) {
                return Ok(CommandResult::Pull(format!(
                    "AlreadyContained through C{}",
                    current.unwrap_or_default()
                )));
            }
            let mode_changed = match &record.relation {
                BranchRelation::RemoteCurrent { mode }
                | BranchRelation::RemotePullBehind { mode, .. } => *mode != placement,
                _ => false,
            };
            record.work_head = Some(number);
            if placement == RemotePlacement::Replica {
                record.remote_complete_through =
                    Some(record.remote_complete_through.unwrap_or(0).max(number));
            }
            for commit in &mut record.commits {
                commit.work = commit.number <= number;
            }
            record.relation = match record.authority_head {
                Some(head) if head > number => BranchRelation::RemotePullBehind {
                    mode: placement,
                    commits: head - number,
                },
                _ => BranchRelation::RemoteCurrent { mode: placement },
            };
            let outcome = match (current, mode_changed) {
                (Some(current), false) if current == number => "UpToDate",
                (Some(current), true) if current == number => "ModeChanged",
                _ => "Advanced",
            };
            Ok(CommandResult::Pull(format!("{outcome} through C{number}")))
        }
        CommandKind::BranchForkLayer { name, layer } => {
            let layer = state
                .find_layer(&layer)
                .cloned()
                .ok_or_else(|| CliError::NotFound(layer.clone()))?;
            let project = state
                .projects
                .iter()
                .find(|project| project.id == layer.project_id)
                .expect("fixture project");
            if project
                .work_layers
                .is_none_or(|boundary| layer.number > boundary)
            {
                return Err(CliError::NotPulled(layer.id.to_string()));
            }
            create_branch(state, name, BranchOrigin::Layer(layer.id), layer.project_id)
        }
        CommandKind::BranchForkCommit {
            name,
            branch,
            commit,
        } => {
            let source = state
                .find_branch(&branch)
                .cloned()
                .ok_or_else(|| CliError::NotFound(branch.clone()))?;
            let number = commit_number(state, &source, &commit)?;
            if !commit_is_local(&source, number, &commit) {
                return Err(CliError::NotPulled(format!("{}/C{number}", source.name)));
            }
            let selected = resolve_commit_id(&source, &commit)
                .ok_or_else(|| CliError::NotFound(commit.clone()))?;
            create_branch(
                state,
                name,
                BranchOrigin::Commit(source.id.clone(), selected),
                source.project_id,
            )
        }
        CommandKind::BranchPush { branch } => {
            let id = state
                .find_branch(&branch)
                .map(|record| record.id.clone())
                .ok_or_else(|| CliError::NotFound(branch.clone()))?;
            let record = state.branch_mut(&id).expect("resolved Branch");
            if matches!(
                record.relation,
                BranchRelation::AuthorityAhead { .. } | BranchRelation::Diverged
            ) {
                return Err(CliError::HeadMoved(record.name.to_string()));
            }
            let Some(head) = record.work_head else {
                return Ok(CommandResult::Push("NoChanges".into()));
            };
            let outcome = if record.authority_head.is_none() {
                "Created"
            } else if record.authority_head == Some(head) {
                "UpToDate"
            } else {
                "Advanced"
            };
            record.authority_head = Some(head);
            record.relation = BranchRelation::LocalCurrent;
            for commit in &mut record.commits {
                commit.authority = commit.number <= head;
            }
            Ok(CommandResult::Push(format!("{outcome} C{head}")))
        }
        CommandKind::LayerStackAdd { branch } => add_layer(state, &branch),
        CommandKind::WorkspaceCreate {
            anchor,
            path,
            container,
            projection,
        } => create_workspace(state, anchor, path, container, projection),
        CommandKind::WorkspaceAction {
            action,
            target,
            arguments,
        } => workspace_action(state, databases, &action, &target, &arguments),
        CommandKind::Diff(request) => {
            let (title, _, _) = request.labels();
            Ok(CommandResult::Diff(title))
        }
        CommandKind::Monitor { action } => Ok(CommandResult::Monitor(action)),
        CommandKind::LayerStackInit { name, .. } => {
            if state.projects.iter().any(|project| project.name == name) {
                return Err(CliError::NameConflict(name.to_string()));
            }
            let id = LayerStackId::new(format!("S-MOCK-{}", state.projects.len() + 1));
            state.projects.push(ProjectRecord {
                id: id.clone(),
                name: name.clone(),
                authority_layers: 1,
                work_layers: None,
                mode: None,
                complete_roots: 0,
                authority_available: true,
                observed: "now".into(),
            });
            let layer_id = LayerId::new(format!("L-{id}-01"));
            let tree = seed_tree(layer_id.as_str());
            let (root, objects) = canonical_tree(&tree);
            state.layers.push(LayerRecord {
                id: layer_id,
                project_id: id.clone(),
                number: 1,
                source: None,
                root,
                tree,
                objects,
            });
            Ok(CommandResult::Initialized(format!("Created {name} ({id})")))
        }
        CommandKind::ReadOnly { family, action } => {
            Ok(CommandResult::Query(format!("{family} {action}")))
        }
        CommandKind::DbCreate { .. }
        | CommandKind::DbConnect { .. }
        | CommandKind::ContextUse { .. }
        | CommandKind::ContextShow => unreachable!("control command handled before mock state"),
    }
}

fn create_branch(
    state: &mut MockState,
    name: EntityName,
    origin: BranchOrigin,
    project_id: LayerStackId,
) -> Result<CommandResult, CliError> {
    if state.relation_name_exists(&project_id, &name) {
        return Err(CliError::NameConflict(name.to_string()));
    }
    let id = BranchId::new(format!("B-local-{}", state.next_branch));
    state.next_branch += 1;
    let boundary_commit = match &origin {
        BranchOrigin::Commit(_, commit) => Some(commit.clone()),
        BranchOrigin::Layer(_) => None,
    };
    let origin_label = match &origin {
        BranchOrigin::Layer(layer) => layer.to_string(),
        BranchOrigin::Commit(branch, commit) => format!("{branch}/{commit}"),
    };
    let effective_base = match &origin {
        BranchOrigin::Layer(layer) => Some(layer.clone()),
        BranchOrigin::Commit(_, _) => None,
    };
    state.branches.push(BranchRecord {
        id: id.clone(),
        project_id,
        name: name.clone(),
        origin,
        authority_head: None,
        work_head: None,
        remote_complete_through: None,
        boundary_commit,
        effective_base,
        relation: BranchRelation::LocalOnly,
        commits: Vec::new(),
    });
    Ok(CommandResult::Fork {
        branch_id: id,
        name,
        origin: origin_label,
        remote_calls: 0,
        objects_copied: 0,
    })
}

fn add_layer(state: &mut MockState, branch_value: &str) -> Result<CommandResult, CliError> {
    let branch = state
        .find_branch(branch_value)
        .cloned()
        .ok_or_else(|| CliError::NotFound(branch_value.into()))?;
    let preflight = add_preflight(state, &branch)?;
    let commit = branch
        .work_head
        .and_then(|number| branch.commit_id(number))
        .ok_or_else(|| CliError::Integrity("Add source Commit".into()))?;
    match preflight {
        AddPreflight::Already(layer) => {
            return Ok(CommandResult::Add(format!("AlreadyAccepted {layer}")));
        }
        AddPreflight::NeedsResolution { old, current } => {
            return create_reconciliation_workspace(state, &branch, old, current);
        }
        AddPreflight::Ready => {}
    }
    let accepted = branch
        .commits
        .iter()
        .find(|item| item.id == commit)
        .cloned()
        .ok_or_else(|| CliError::Integrity("Add source Commit".into()))?;
    let project = state
        .projects
        .iter_mut()
        .find(|project| project.id == branch.project_id)
        .expect("fixture project");
    project.authority_layers += 1;
    let number = project.authority_layers;
    let id = layer_for(project, number);
    state.layers.push(LayerRecord {
        id: id.clone(),
        project_id: project.id.clone(),
        number,
        source: Some((branch.id.clone(), commit.clone())),
        root: accepted.root,
        tree: accepted.tree,
        objects: accepted.objects,
    });
    if let Some(record) = state.branch_mut(&branch.id) {
        if let Some(item) = record.commits.iter_mut().find(|item| item.id == commit) {
            item.accepted_layer = Some(id.clone());
        }
    }
    Ok(CommandResult::Add(format!(
        "Added {id} from {}/{}",
        branch.name, commit
    )))
}

enum AddPreflight {
    Already(LayerId),
    Ready,
    NeedsResolution { old: LayerId, current: LayerId },
}

fn add_preflight(state: &MockState, branch: &BranchRecord) -> CliResult<AddPreflight> {
    if branch.authority_head != branch.work_head || branch.work_head.is_none() {
        return Err(CliError::NotPulled("Push Branch before Add".into()));
    }
    let head = (
        branch.id.clone(),
        branch
            .work_head
            .and_then(|number| branch.commit_id(number))
            .ok_or_else(|| CliError::Integrity("Add Branch head".into()))?,
    );
    if let Some(layer) = state
        .layers
        .iter()
        .find(|layer| layer.source.as_ref() == Some(&head))
    {
        return Ok(AddPreflight::Already(layer.id.clone()));
    }
    let base = state.branch_base_layer(branch);
    let project = state
        .projects
        .iter()
        .find(|project| project.id == branch.project_id)
        .expect("fixture project");
    let current = state
        .layers
        .iter()
        .find(|layer| layer.project_id == project.id && layer.number == project.authority_layers)
        .map(|layer| layer.id.clone())
        .expect("authority head Layer");
    if project.work_layers != Some(project.authority_layers) {
        return Err(CliError::NotPulled(format!(
            "Pull current Layer {current} before Add"
        )));
    }
    if base != current {
        return Ok(AddPreflight::NeedsResolution { old: base, current });
    }
    Ok(AddPreflight::Ready)
}

fn validate_workspace_anchor<'a>(
    state: &'a MockState,
    anchor: &WorkspaceAnchor,
) -> CliResult<(&'a BranchRecord, String)> {
    match anchor {
        WorkspaceAnchor::Commit { branch, commit } => {
            let branch = state
                .find_branch(branch)
                .ok_or_else(|| CliError::NotFound(branch.clone()))?;
            if !matches!(
                branch.relation,
                BranchRelation::LocalOnly
                    | BranchRelation::LocalCurrent
                    | BranchRelation::LocalPushAhead { .. }
            ) {
                return Err(CliError::ReadOnly("Fork the remote Commit first".into()));
            }
            let number = commit_number(state, branch, commit)?;
            let current = branch.work_head.or_else(|| {
                branch
                    .boundary_commit
                    .as_ref()
                    .and_then(|id| trailing_number(id.as_str()))
            });
            if current != Some(number) {
                return Err(CliError::HeadMoved(
                    "historical Commit: Fork a new Branch first".into(),
                ));
            }
            Ok((branch, format!("C{number}")))
        }
        WorkspaceAnchor::InitialLayer { branch, layer } => {
            let branch = state
                .find_branch(branch)
                .ok_or_else(|| CliError::NotFound(branch.clone()))?;
            if branch.work_head.is_some() || branch.boundary_commit.is_some() {
                return Err(CliError::HeadMoved("Branch already has a Commit".into()));
            }
            let layer = state
                .find_layer(layer)
                .ok_or_else(|| CliError::NotFound(layer.clone()))?;
            if layer.project_id != branch.project_id {
                return Err(CliError::Integrity("LayerStack mismatch".into()));
            }
            if branch.origin != BranchOrigin::Layer(layer.id.clone()) {
                return Err(CliError::Integrity(
                    "initial Workspace must use the Branch origin Layer".into(),
                ));
            }
            Ok((branch, layer.id.to_string()))
        }
    }
}

fn commit_number(state: &MockState, branch: &BranchRecord, value: &str) -> CliResult<u16> {
    branch
        .commits
        .iter()
        .find(|commit| commit.id.as_str() == value || format!("C{}", commit.number) == value)
        .map(|commit| commit.number)
        .or_else(|| {
            branch
                .boundary_commit
                .as_ref()
                .filter(|commit| commit.as_str() == value)
                .and_then(|commit| {
                    state
                        .branches
                        .iter()
                        .flat_map(|branch| &branch.commits)
                        .find(|candidate| &candidate.id == commit)
                        .map(|candidate| candidate.number)
                        .or_else(|| trailing_number(commit.as_str()))
                })
        })
        .ok_or_else(|| CliError::NotFound(format!("Commit {value} in {}", branch.name)))
}

fn commit_is_local(branch: &BranchRecord, number: u16, value: &str) -> bool {
    branch
        .commits
        .iter()
        .any(|commit| commit.number == number && commit.work)
        || branch_scope_is_present(branch)
            && branch
                .boundary_commit
                .as_ref()
                .is_some_and(|commit| commit.as_str() == value)
}

fn resolve_commit_id(branch: &BranchRecord, value: &str) -> Option<crate::CommitId> {
    branch
        .commits
        .iter()
        .find(|commit| commit.id.as_str() == value || format!("C{}", commit.number) == value)
        .map(|commit| commit.id.clone())
        .or_else(|| {
            branch
                .boundary_commit
                .as_ref()
                .filter(|commit| commit.as_str() == value)
                .cloned()
        })
}

fn branch_scope_is_present(branch: &BranchRecord) -> bool {
    !matches!(
        branch.relation,
        BranchRelation::AuthorityOnly | BranchRelation::Integrity
    )
}

fn validate_diff(state: &MockState, request: &crate::DiffRequest) -> CliResult<()> {
    match request {
        crate::DiffRequest::Layers { from, to } => {
            let from = state
                .find_layer(from)
                .ok_or_else(|| CliError::NotFound(from.clone()))?;
            let to = state
                .find_layer(to)
                .ok_or_else(|| CliError::NotFound(to.clone()))?;
            if from.project_id != to.project_id {
                return Err(CliError::Integrity("LayerStack mismatch".into()));
            }
        }
        crate::DiffRequest::BranchCommits { branch, from, to } => {
            let branch = state
                .find_branch(branch)
                .ok_or_else(|| CliError::NotFound(branch.clone()))?;
            for value in [from, to] {
                let number = commit_number(state, branch, value)?;
                if !commit_is_local(branch, number, value) {
                    return Err(CliError::NotPulled(value.clone()));
                }
            }
        }
        crate::DiffRequest::BranchLayer { branch, layer } => {
            let branch = state
                .find_branch(branch)
                .ok_or_else(|| CliError::NotFound(branch.clone()))?;
            let layer = state
                .find_layer(layer)
                .ok_or_else(|| CliError::NotFound(layer.clone()))?;
            if branch.project_id != layer.project_id {
                return Err(CliError::Integrity("LayerStack mismatch".into()));
            }
            let project = state
                .projects
                .iter()
                .find(|project| project.id == layer.project_id)
                .expect("fixture project");
            if project
                .work_layers
                .is_none_or(|boundary| layer.number > boundary)
                || branch.work_head.is_none() && branch.boundary_commit.is_none()
            {
                return Err(CliError::NotPulled(layer.id.to_string()));
            }
        }
    }
    Ok(())
}

fn affected_scope(command: &Command) -> String {
    match command.kind {
        CommandKind::LayerStackPull { .. } | CommandKind::LayerStackAdd { .. } => "project",
        CommandKind::BranchPull { .. }
        | CommandKind::BranchForkLayer { .. }
        | CommandKind::BranchForkCommit { .. }
        | CommandKind::BranchPush { .. } => "branch",
        CommandKind::WorkspaceCreate { .. } | CommandKind::WorkspaceAction { .. } => "workspace",
        CommandKind::Diff(_) => "diff",
        CommandKind::Monitor { .. } => "activity",
        CommandKind::LayerStackInit { .. }
        | CommandKind::ReadOnly { .. }
        | CommandKind::DbCreate { .. }
        | CommandKind::DbConnect { .. }
        | CommandKind::ContextUse { .. }
        | CommandKind::ContextShow => "projects",
    }
    .into()
}

#[cfg(test)]
mod tests;

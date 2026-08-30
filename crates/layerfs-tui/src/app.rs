use crate::{explorer::ExplorerState, workspace::WorkspaceTab};
use crossterm::event::{KeyCode, KeyEvent};
use layerfs_cli::{
    ActivitySnapshot, BranchId, BranchOrigin, BranchView, CliEvent, CliResult, CliSession, Command,
    CommandPlan, CommitId, DiffRequest, DiffSnapshot, FinishedStatus, LayerId, LayerStackId,
    OperationHandle, OperationId, PageRequest, ProjectSnapshot, ProjectSummary, ViewQuery,
    ViewSnapshot, WorkspaceId, WorkspaceSnapshot, WorkspaceState,
};
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::Path;

mod keys;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ActivityTab {
    Operations,
    Storage,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Route {
    Projects,
    Project(LayerStackId),
    Branch(LayerStackId, BranchId),
    Workspaces(Option<LayerStackId>),
    Workspace(WorkspaceId),
    Activity(ActivityTab),
    Diff(DiffRequest),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Overlay {
    None,
    Command,
    Search,
    Help,
    Plan,
}

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub enum GraphTarget {
    Branch(BranchId),
    Commit(BranchId, CommitId),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GraphRow {
    pub depth: u16,
    pub marker: &'static str,
    pub label: String,
    pub detail: String,
    pub target: GraphTarget,
}

pub struct App {
    pub session: CliSession,
    pub route: Route,
    history: Vec<Route>,
    pub overlay: Overlay,
    pub focus: usize,
    pub compact_pane: usize,
    pub operation_drawer: bool,
    pub should_quit: bool,
    pub command: String,
    pub command_cursor: usize,
    pub completion_selected: usize,
    pub search: String,
    pub completions: Vec<layerfs_cli::Completion>,
    pub plan: Option<(Command, CommandPlan)>,
    pub error: Option<String>,
    pub event_log: VecDeque<String>,
    pub active_operation: Option<OperationHandle>,
    active_command: Option<Command>,
    pub operation_title: String,
    pub operation_progress: Option<(u64, u64, String)>,
    pub projects: Vec<ProjectSummary>,
    pub project: Option<ProjectSnapshot>,
    pub workspaces: WorkspaceSnapshot,
    pub activity: ActivitySnapshot,
    pub diff: Option<DiffSnapshot>,
    pub selected_project: Option<LayerStackId>,
    pub selected_layer: Option<LayerId>,
    pub selected_graph: Option<GraphTarget>,
    pub explorer: ExplorerState,
    pub selected_workspace: Option<WorkspaceId>,
    pub workspace_tab: WorkspaceTab,
    pub selected_workspace_path: Option<String>,
    pub selected_operation: Option<OperationId>,
    pub selected_diff_path: Option<String>,
    expanded: HashSet<BranchId>,
}

impl App {
    pub fn demo() -> Self {
        Self::open("mock").expect("mock session")
    }

    pub fn open(context_location: impl AsRef<Path>) -> CliResult<Self> {
        let session = CliSession::open(context_location)?;
        let projects = match session.snapshot(ViewQuery::Projects(page()))? {
            ViewSnapshot::Projects(page) => page.items,
            _ => unreachable!(),
        };
        let selected_project = projects.first().map(|project| project.id.clone());
        let project = selected_project.as_ref().and_then(|id| {
            match session
                .snapshot(ViewQuery::Project {
                    id: id.clone(),
                    page: page(),
                })
                .ok()?
            {
                ViewSnapshot::Project(snapshot) => Some(snapshot),
                _ => None,
            }
        });
        let selected_layer = project.as_ref().and_then(|snapshot| {
            snapshot
                .layers
                .items
                .iter()
                .find(|layer| layer.number == 15)
                .or_else(|| snapshot.layers.items.last())
                .map(|layer| layer.id.clone())
        });
        let workspaces = match session.snapshot(ViewQuery::Workspaces {
            project: None,
            page: page(),
        })? {
            ViewSnapshot::Workspaces(snapshot) => snapshot,
            _ => unreachable!(),
        };
        let activity = match session.snapshot(ViewQuery::Activity(page()))? {
            ViewSnapshot::Activity(snapshot) => snapshot,
            _ => unreachable!(),
        };
        let explorer = ExplorerState::new(selected_layer.clone());
        let mut app = Self {
            session,
            route: Route::Projects,
            history: Vec::new(),
            overlay: Overlay::None,
            focus: 0,
            compact_pane: 0,
            operation_drawer: false,
            should_quit: false,
            command: String::new(),
            command_cursor: 0,
            completion_selected: 0,
            search: String::new(),
            completions: Vec::new(),
            plan: None,
            error: None,
            event_log: VecDeque::with_capacity(32),
            active_operation: None,
            active_command: None,
            operation_title: "idle".into(),
            operation_progress: None,
            projects,
            project,
            workspaces,
            activity,
            diff: None,
            selected_project,
            selected_layer,
            selected_graph: None,
            explorer,
            selected_workspace: None,
            workspace_tab: WorkspaceTab::Overview,
            selected_workspace_path: None,
            selected_operation: None,
            selected_diff_path: None,
            expanded: HashSet::new(),
        };
        if let Some(project) = &app.project {
            app.expanded.extend(
                project
                    .branches
                    .items
                    .iter()
                    .map(|branch| branch.id.clone()),
            );
        }
        app.selected_workspace = app
            .workspaces
            .workspaces
            .items
            .first()
            .map(|w| w.id.clone());
        app.selected_operation = app.activity.operations.items.first().map(|o| o.id.clone());
        app.sync_graph_selection();
        Ok(app)
    }

    pub fn empty_demo() -> Self {
        let mut app = Self::demo();
        app.session = CliSession::open("mock-empty").expect("empty mock session");
        app.projects.clear();
        app.project = None;
        app.workspaces = match app
            .session
            .snapshot(ViewQuery::Workspaces {
                project: None,
                page: page(),
            })
            .expect("empty Workspaces")
        {
            ViewSnapshot::Workspaces(snapshot) => snapshot,
            _ => unreachable!(),
        };
        app.activity = match app
            .session
            .snapshot(ViewQuery::Activity(page()))
            .expect("empty Activity")
        {
            ViewSnapshot::Activity(snapshot) => snapshot,
            _ => unreachable!(),
        };
        app.selected_project = None;
        app.selected_layer = None;
        app.selected_graph = None;
        app.explorer = ExplorerState::new(None);
        app.selected_workspace = None;
        app.selected_operation = None;
        app
    }

    pub fn set_demo_route(&mut self, name: &str) {
        match name {
            "projects" => self.route = Route::Projects,
            "topology" | "project" => {
                if let Some(id) = self.selected_project.clone() {
                    self.route = Route::Project(id);
                    self.activate_selected_layer();
                }
            }
            "branch" => {
                if let Some(project) = self.selected_project.clone() {
                    self.route = Route::Branch(project, BranchId::from("B-main"));
                    self.selected_graph = Some(GraphTarget::Commit(
                        BranchId::from("B-main"),
                        CommitId::from("C-B-main-35"),
                    ));
                    self.activate_selected_graph();
                }
            }
            "workspaces" => self.route = Route::Workspaces(self.selected_project.clone()),
            "workspace" => {
                if let Some(id) = self.selected_workspace.clone() {
                    self.route = Route::Workspace(id);
                }
            }
            "activity" | "operations" => self.route = Route::Activity(ActivityTab::Operations),
            "storage" => self.route = Route::Activity(ActivityTab::Storage),
            "reconciliation" => {
                let pull = CliSession::parse_line("layerstack pull --through L-A-19 --reference")
                    .expect("mock Pull");
                self.start_operation(pull);
                for _ in 0..16 {
                    self.tick();
                }
                let command =
                    CliSession::parse_line("layerstack add B-local-sync").expect("mock Add");
                self.start_operation(command);
                for _ in 0..16 {
                    self.tick();
                }
            }
            "diff" => self.open_diff(DiffRequest::BranchCommits {
                branch: "B-search-a".into(),
                from: "C-B-search-a-05".into(),
                to: "C-B-search-a-08".into(),
            }),
            _ => {}
        }
        self.focus = 0;
        self.compact_pane = 0;
    }

    pub fn handle_key(&mut self, key: KeyEvent) {
        if self.error.is_some() {
            if matches!(key.code, KeyCode::Esc | KeyCode::Enter) {
                self.error = None;
            }
            return;
        }
        self.error = None;
        match self.overlay {
            Overlay::Command => self.command_key(key),
            Overlay::Search => self.search_key(key),
            Overlay::Help => {
                if matches!(key.code, KeyCode::Esc | KeyCode::Char('?')) {
                    self.overlay = Overlay::None;
                }
            }
            Overlay::Plan => self.plan_key(key),
            Overlay::None => self.normal_key(key),
        }
    }

    pub fn tick(&mut self) {
        let mut finished = false;
        for _ in 0..2 {
            let Some(handle) = self.active_operation.as_mut() else {
                break;
            };
            match handle.try_next_event() {
                Ok(Some(event)) => {
                    finished |= matches!(event, CliEvent::Finished { .. });
                    self.apply_event(event);
                }
                Ok(None) => break,
                Err(error) => {
                    self.error = Some(error.to_string());
                    finished = true;
                    break;
                }
            }
        }
        if finished {
            self.active_operation = None;
            self.active_command = None;
            self.operation_progress = None;
            self.refresh_all();
        }
    }

    pub(crate) fn interrupt_active(&mut self) -> bool {
        let Some(handle) = self.active_operation.as_mut() else {
            return false;
        };
        if let Err(error) = handle.interrupt() {
            self.error = Some(error.to_string());
        }
        true
    }

    pub fn graph_rows(&self) -> Vec<GraphRow> {
        let Some(snapshot) = self.project.as_ref() else {
            return Vec::new();
        };
        let selected_layer = self
            .selected_layer
            .as_ref()
            .or_else(|| snapshot.layers.items.last().map(|layer| &layer.id));
        let Some(selected_layer) = selected_layer else {
            return Vec::new();
        };
        let branches = snapshot
            .branches
            .items
            .iter()
            .map(|branch| (branch.id.clone(), branch))
            .collect::<HashMap<_, _>>();
        let mut rows = Vec::new();
        let roots = snapshot
            .branches
            .items
            .iter()
            .filter(|branch| branch.origin == BranchOrigin::Layer(selected_layer.clone()))
            .collect::<Vec<_>>();
        for branch in roots {
            self.flatten_branch(branch, &branches, 0, false, &mut rows);
        }
        rows
    }

    pub fn focused_branch_rows(&self, branch_id: &BranchId) -> Vec<GraphRow> {
        let Some(snapshot) = self.project.as_ref() else {
            return Vec::new();
        };
        let branches = snapshot
            .branches
            .items
            .iter()
            .map(|branch| (branch.id.clone(), branch))
            .collect::<HashMap<_, _>>();
        let mut rows = Vec::new();
        if let Some(branch) = branches.get(branch_id) {
            self.flatten_branch(branch, &branches, 0, true, &mut rows);
        }
        rows
    }

    pub fn selected_branch(&self) -> Option<&BranchView> {
        let id = match self.selected_graph.as_ref()? {
            GraphTarget::Branch(id) | GraphTarget::Commit(id, _) => id,
        };
        self.project
            .as_ref()?
            .branches
            .items
            .iter()
            .find(|branch| &branch.id == id)
    }

    fn flatten_branch<'a>(
        &self,
        branch: &'a BranchView,
        branches: &HashMap<BranchId, &'a BranchView>,
        depth: u16,
        include_inherited: bool,
        rows: &mut Vec<GraphRow>,
    ) {
        let origin = match &branch.origin {
            BranchOrigin::Layer(layer) => self
                .project
                .as_ref()
                .and_then(|project| {
                    project
                        .layers
                        .items
                        .iter()
                        .find(|candidate| candidate.id == *layer)
                })
                .map(|layer| format!("from L{}", layer.number))
                .unwrap_or_else(|| format!("from {layer}")),
            BranchOrigin::Commit(_, commit) => {
                let number = commit
                    .as_str()
                    .rsplit('-')
                    .next()
                    .unwrap_or("?")
                    .trim_start_matches('0');
                format!("fork @ C{number}")
            }
        };
        rows.push(GraphRow {
            depth,
            marker: "[B]",
            label: branch.name.to_string(),
            detail: format!("{} · {origin}", branch.relation),
            target: GraphTarget::Branch(branch.id.clone()),
        });
        if !self.expanded.contains(&branch.id) {
            return;
        }
        let commits = important_commits(branch);
        for commit in commits
            .into_iter()
            .filter(|commit| include_inherited || !commit.inherited)
        {
            let mut flags = Vec::new();
            if commit.inherited {
                flags.push("INHERITED BOUNDARY".into());
            } else if commit.owned {
                flags.push("OWNED".into());
            }
            if commit.authority {
                flags.push("AUTH".into());
            }
            if commit.work {
                flags.push("WORK".into());
            }
            if commit.authority_head {
                flags.push("AUTH HEAD".into());
            }
            if commit.work_head {
                flags.push("WORK HEAD".into());
            }
            for workspace_id in &commit.workspaces {
                if let Some(workspace) = self
                    .workspaces
                    .workspaces
                    .items
                    .iter()
                    .find(|workspace| &workspace.id == workspace_id)
                {
                    flags.push(
                        match workspace.state {
                            WorkspaceState::Clean => "W",
                            WorkspaceState::Dirty => "W*",
                            WorkspaceState::Running => "W▶",
                            WorkspaceState::Busy => "BUSY",
                            WorkspaceState::HeadMoved => "W!",
                            WorkspaceState::ReadOnly => "W read-only",
                        }
                        .into(),
                    );
                }
            }
            if let Some(layer) = &commit.accepted_layer {
                let label = self
                    .project
                    .as_ref()
                    .and_then(|project| project.layers.items.iter().find(|item| &item.id == layer))
                    .map_or_else(|| layer.to_string(), |layer| format!("L{}", layer.number));
                flags.push(format!("→ {label}"));
            }
            rows.push(GraphRow {
                depth: depth + 1,
                marker: "[C]",
                label: format!("C{}", commit.number),
                detail: flags.join(" · "),
                target: GraphTarget::Commit(branch.id.clone(), commit.id.clone()),
            });
            let children = branches
                .values()
                .filter(|child| {
                    child.origin == BranchOrigin::Commit(branch.id.clone(), commit.id.clone())
                })
                .copied()
                .collect::<Vec<_>>();
            for child in children {
                self.flatten_branch(child, branches, depth + 2, false, rows);
            }
        }
    }

    fn command_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => {
                self.overlay = Overlay::None;
                self.command.clear();
                self.command_cursor = 0;
                self.completions.clear();
            }
            KeyCode::Enter => match CliSession::parse_line(&self.command)
                .and_then(|command| self.session.plan(&command).map(|plan| (command, plan)))
            {
                Ok((command, plan)) if plan.confirmation_required => {
                    self.plan = Some((command, plan));
                    self.overlay = Overlay::Plan;
                }
                Ok((command, _)) => {
                    self.overlay = Overlay::None;
                    self.start_operation(command);
                }
                Err(error) => self.error = Some(error.to_string()),
            },
            KeyCode::Tab => {
                if let Some(completion) = self.completions.get(self.completion_selected).cloned() {
                    self.command
                        .replace_range(completion.start..completion.end, &completion.value);
                    self.command_cursor = completion.start.saturating_add(completion.value.len());
                    self.update_completions();
                }
            }
            KeyCode::Backspace => {
                if self.command_cursor > 0 {
                    let start = previous_boundary(&self.command, self.command_cursor);
                    self.command.replace_range(start..self.command_cursor, "");
                    self.command_cursor = start;
                }
                self.update_completions();
            }
            KeyCode::Delete => {
                let end = next_boundary(&self.command, self.command_cursor);
                self.command.replace_range(self.command_cursor..end, "");
                self.update_completions();
            }
            KeyCode::Left => {
                self.command_cursor = previous_boundary(&self.command, self.command_cursor)
            }
            KeyCode::Right => {
                self.command_cursor = next_boundary(&self.command, self.command_cursor)
            }
            KeyCode::Home => self.command_cursor = 0,
            KeyCode::End => self.command_cursor = self.command.len(),
            KeyCode::Up if !self.completions.is_empty() => {
                self.completion_selected = (self.completion_selected + self.completions.len() - 1)
                    % self.completions.len();
            }
            KeyCode::Down if !self.completions.is_empty() => {
                self.completion_selected = (self.completion_selected + 1) % self.completions.len();
            }
            KeyCode::Char(character) => {
                self.command.insert(self.command_cursor, character);
                self.command_cursor += character.len_utf8();
                self.update_completions();
            }
            _ => {}
        }
    }

    fn search_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => {
                self.search.clear();
                self.overlay = Overlay::None;
            }
            KeyCode::Enter => {
                self.apply_search();
                self.overlay = Overlay::None;
            }
            KeyCode::Backspace => {
                self.search.pop();
            }
            KeyCode::Char(character) => self.search.push(character),
            _ => {}
        }
    }

    fn plan_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => {
                self.plan = None;
                self.overlay = Overlay::None;
            }
            KeyCode::Enter => {
                if let Some((command, _)) = self.plan.take() {
                    self.overlay = Overlay::None;
                    self.start_operation(command);
                }
            }
            _ => {}
        }
    }

    fn start_operation(&mut self, command: Command) {
        if self.active_operation.is_some() {
            self.error = Some("an operation is already running; interrupt or wait".into());
            return;
        }
        match self.session.execute(command.clone()) {
            Ok(handle) => {
                self.operation_title = handle.id().to_string();
                self.active_command = Some(command);
                self.active_operation = Some(handle);
            }
            Err(error) => self.error = Some(error.to_string()),
        }
    }

    fn apply_event(&mut self, event: CliEvent) {
        match event {
            CliEvent::Started { command, .. } => {
                self.operation_title = command.clone();
                self.push_event(format!("Started {command}"));
            }
            CliEvent::Progress {
                phase,
                completed,
                total,
                ..
            } => {
                self.operation_progress = Some((completed, total, phase.clone()));
                self.push_event(format!("Progress {phase} {completed}/{total}"));
            }
            CliEvent::Output { line, .. } => self.push_event(format!("Output {line}")),
            CliEvent::Snapshot { scope, .. } => {
                self.push_event(format!("Snapshot {scope}"));
                self.refresh_activity();
            }
            CliEvent::Finished { status, result, .. } => {
                let route_after = (status == FinishedStatus::Succeeded)
                    .then(|| self.active_command.clone())
                    .flatten();
                let resolution = match &result {
                    Ok(layerfs_cli::CommandResult::NeedsResolution { workspace_id, .. }) => {
                        Some(workspace_id.clone())
                    }
                    _ => None,
                };
                let message = match result {
                    Ok(layerfs_cli::CommandResult::NeedsResolution {
                        workspace_id,
                        conflict_count,
                        ..
                    }) => format!(
                        "Finished {status:?}: reconcile in {workspace_id} · {conflict_count} conflicts"
                    ),
                    Ok(value) => format!("Finished {status:?}: {value:?}"),
                    Err(error) => format!("Finished {status:?}: {error}"),
                };
                if status != FinishedStatus::Succeeded {
                    self.error = Some(message.clone());
                }
                self.operation_title = message.clone();
                self.push_event(message);
                if let Some(command) = route_after {
                    self.route_after_command(command);
                }
                if let Some(workspace) = resolution {
                    self.selected_workspace = Some(workspace);
                    self.route = Route::Workspaces(self.selected_project.clone());
                    self.focus = 1;
                    self.compact_pane = 1;
                }
            }
        }
    }

    fn push_event(&mut self, event: String) {
        if self.event_log.len() == 32 {
            self.event_log.pop_front();
        }
        self.event_log.push_back(event);
    }

    fn go(&mut self, route: Route) {
        if self.route != route {
            self.history.push(self.route.clone());
            self.route = route;
            self.focus = 0;
            self.compact_pane = 0;
            self.refresh_route();
        }
    }

    fn back(&mut self) {
        if let Some(route) = self.history.pop() {
            self.route = route;
            self.focus = 0;
            self.compact_pane = 0;
            self.refresh_route();
        }
    }

    fn open_selected(&mut self) {
        match &self.route {
            Route::Projects => {
                if let Some(id) = self.selected_project.clone() {
                    self.go(Route::Project(id));
                }
            }
            Route::Project(_) => self.open_explorer_selection(),
            Route::Workspaces(_) => {
                if let Some(workspace) = self.selected_workspace.as_ref() {
                    self.go(Route::Workspace(workspace.clone()));
                }
            }
            Route::Activity(_) => {}
            Route::Branch(_, _) => self.open_explorer_selection(),
            Route::Workspace(_)
                if self.focus == 0
                    && matches!(
                        self.workspace_tab,
                        WorkspaceTab::Files | WorkspaceTab::Changes | WorkspaceTab::Runs
                    ) =>
            {
                self.focus = 1;
                self.compact_pane = 1;
            }
            Route::Workspace(_) | Route::Diff(_) => {}
        }
    }

    fn move_selection(&mut self, delta: isize) {
        match &self.route {
            Route::Projects => {
                if self.focus != 0 {
                    return;
                }
                self.selected_project = move_id(
                    &self
                        .projects
                        .iter()
                        .map(|p| p.id.clone())
                        .collect::<Vec<_>>(),
                    self.selected_project.as_ref(),
                    delta,
                );
            }
            Route::Project(_) => {
                if self.explorer.mode != crate::ExplorerMode::Topology {
                    self.move_explorer_path(delta);
                } else if self.focus == 0 {
                    if let Some(project) = &self.project {
                        self.selected_layer = move_id(
                            &project
                                .layers
                                .items
                                .iter()
                                .rev()
                                .map(|layer| layer.id.clone())
                                .collect::<Vec<_>>(),
                            self.selected_layer.as_ref(),
                            delta,
                        );
                        self.sync_graph_selection();
                        self.activate_selected_layer();
                    }
                } else if self.focus == 1 {
                    let rows = self.graph_rows();
                    self.selected_graph = move_target(&rows, self.selected_graph.as_ref(), delta);
                    self.activate_selected_graph();
                }
            }
            Route::Branch(_, branch) => {
                if self.explorer.mode != crate::ExplorerMode::Topology {
                    self.move_explorer_path(delta);
                } else if self.focus == 0 {
                    let rows = self.focused_branch_rows(branch);
                    self.selected_graph = move_target(&rows, self.selected_graph.as_ref(), delta);
                    self.activate_selected_graph();
                }
            }
            Route::Workspaces(_) => {
                if self.focus != 0 {
                    return;
                }
                self.selected_workspace = move_id(
                    &self
                        .workspaces
                        .workspaces
                        .items
                        .iter()
                        .map(|workspace| workspace.id.clone())
                        .collect::<Vec<_>>(),
                    self.selected_workspace.as_ref(),
                    delta,
                );
            }
            Route::Workspace(_) => {
                if self.focus != 0 {
                    return;
                }
                let values = self.workspace_items();
                self.selected_workspace_path =
                    move_id(&values, self.selected_workspace_path.as_ref(), delta);
            }
            Route::Activity(ActivityTab::Operations) if self.focus == 0 => {
                self.selected_operation = move_id(
                    &self
                        .activity
                        .operations
                        .items
                        .iter()
                        .map(|operation| operation.id.clone())
                        .collect::<Vec<_>>(),
                    self.selected_operation.as_ref(),
                    delta,
                );
            }
            Route::Activity(_) => {}
            Route::Diff(_) => {
                if self.focus != 0 {
                    return;
                }
                let load_more = delta > 0
                    && self.diff.as_ref().is_some_and(|diff| {
                        diff.entries.next.is_some()
                            && self.selected_diff_path.as_ref()
                                == diff.entries.items.last().map(|entry| &entry.path)
                    });
                if load_more {
                    self.load_more_diff();
                }
                if let Some(diff) = &self.diff {
                    self.selected_diff_path = move_id(
                        &diff
                            .entries
                            .items
                            .iter()
                            .map(|entry| entry.path.clone())
                            .collect::<Vec<_>>(),
                        self.selected_diff_path.as_ref(),
                        delta,
                    );
                }
            }
        }
    }

    fn focus_count(&self) -> usize {
        match self.route {
            Route::Projects | Route::Workspaces(_) | Route::Activity(_) => 2,
            Route::Project(_) | Route::Workspace(_) | Route::Diff(_) => 3,
            Route::Branch(_, _) if self.explorer.mode == crate::ExplorerMode::Topology => 2,
            Route::Branch(_, _) => 3,
        }
    }

    fn refresh_route(&mut self) {
        match self.route.clone() {
            Route::Projects => self.refresh_projects(),
            Route::Project(id) | Route::Branch(id, _) => self.refresh_project(id),
            Route::Workspaces(project) => self.refresh_workspaces(project),
            Route::Workspace(_) => self.refresh_workspaces(None),
            Route::Activity(_) => self.refresh_activity(),
            Route::Diff(request) => self.refresh_diff(request),
        }
    }

    fn refresh_all(&mut self) {
        self.refresh_projects();
        if let Some(project) = self.selected_project.clone() {
            self.refresh_project(project);
        }
        self.refresh_workspaces(None);
        self.refresh_activity();
        if let Route::Diff(request) = self.route.clone() {
            self.refresh_diff(request);
        }
        if matches!(self.route, Route::Project(_) | Route::Branch(_, _))
            && self.explorer.mode != crate::ExplorerMode::Topology
        {
            self.refresh_explorer();
        }
    }

    fn refresh_projects(&mut self) {
        match self.session.snapshot(ViewQuery::Projects(page())) {
            Ok(ViewSnapshot::Projects(page)) => {
                self.projects = page.items;
                if self
                    .selected_project
                    .as_ref()
                    .is_none_or(|id| !self.projects.iter().any(|p| &p.id == id))
                {
                    self.selected_project = self.projects.first().map(|p| p.id.clone());
                }
            }
            Ok(_) => {}
            Err(error) => self.error = Some(error.to_string()),
        }
    }

    fn refresh_project(&mut self, id: LayerStackId) {
        match self.session.snapshot(ViewQuery::Project {
            id: id.clone(),
            page: page(),
        }) {
            Ok(ViewSnapshot::Project(snapshot)) => {
                self.project = Some(snapshot);
                self.selected_project = Some(id);
                if self.selected_layer.as_ref().is_none_or(|layer| {
                    !self
                        .project
                        .as_ref()
                        .is_some_and(|project| project.layers.items.iter().any(|v| &v.id == layer))
                }) {
                    self.selected_layer = self
                        .project
                        .as_ref()
                        .and_then(|p| p.layers.items.last().map(|v| v.id.clone()));
                }
                self.sync_graph_selection();
                self.sync_explorer_subject();
                if let Some(project) = &self.project {
                    self.expanded
                        .retain(|id| project.branches.items.iter().any(|branch| &branch.id == id));
                }
            }
            Ok(_) => {}
            Err(error) => self.error = Some(error.to_string()),
        }
    }

    fn refresh_workspaces(&mut self, project: Option<LayerStackId>) {
        match self.session.snapshot(ViewQuery::Workspaces {
            project,
            page: page(),
        }) {
            Ok(ViewSnapshot::Workspaces(snapshot)) => {
                self.workspaces = snapshot;
                if self
                    .selected_workspace
                    .as_ref()
                    .is_none_or(|id| !self.workspaces.workspaces.items.iter().any(|w| &w.id == id))
                {
                    self.selected_workspace = self
                        .workspaces
                        .workspaces
                        .items
                        .first()
                        .map(|w| w.id.clone());
                }
                let items = self.workspace_items();
                if self
                    .selected_workspace_path
                    .as_ref()
                    .is_none_or(|selected| !items.contains(selected))
                {
                    self.selected_workspace_path = items.first().cloned();
                }
            }
            Ok(_) => {}
            Err(error) => self.error = Some(error.to_string()),
        }
    }

    fn refresh_activity(&mut self) {
        match self.session.snapshot(ViewQuery::Activity(page())) {
            Ok(ViewSnapshot::Activity(snapshot)) => {
                self.activity = snapshot;
                if self.selected_operation.as_ref().is_none_or(|id| {
                    !self
                        .activity
                        .operations
                        .items
                        .iter()
                        .any(|item| &item.id == id)
                }) {
                    self.selected_operation = self
                        .activity
                        .operations
                        .items
                        .first()
                        .map(|item| item.id.clone());
                }
            }
            Ok(_) => {}
            Err(error) => self.error = Some(error.to_string()),
        }
    }

    fn refresh_diff(&mut self, request: DiffRequest) {
        match self.session.snapshot(ViewQuery::Diff {
            request: request.clone(),
            page: PageRequest::first(128),
        }) {
            Ok(ViewSnapshot::Diff(snapshot)) => {
                if self.selected_diff_path.as_ref().is_none_or(|path| {
                    !snapshot
                        .entries
                        .items
                        .iter()
                        .any(|entry| &entry.path == path)
                }) {
                    self.selected_diff_path = snapshot
                        .entries
                        .items
                        .first()
                        .map(|entry| entry.path.clone());
                }
                self.diff = Some(snapshot);
                self.route = Route::Diff(request);
            }
            Ok(_) => {}
            Err(error) => self.error = Some(error.to_string()),
        }
    }

    fn load_more_diff(&mut self) {
        let Some(request) = (match &self.route {
            Route::Diff(request) => Some(request.clone()),
            _ => None,
        }) else {
            return;
        };
        let Some(cursor) = self
            .diff
            .as_ref()
            .and_then(|diff| diff.entries.next.clone())
        else {
            return;
        };
        match self.session.snapshot(ViewQuery::Diff {
            request,
            page: PageRequest {
                after: Some(cursor),
                limit: 128,
            },
        }) {
            Ok(ViewSnapshot::Diff(mut next)) => {
                if let Some(diff) = &mut self.diff {
                    diff.entries.items.append(&mut next.entries.items);
                    diff.entries.next = next.entries.next;
                }
            }
            Ok(_) => {}
            Err(error) => self.error = Some(error.to_string()),
        }
    }

    fn sync_graph_selection(&mut self) {
        let rows = self.graph_rows();
        if self
            .selected_graph
            .as_ref()
            .is_none_or(|target| !rows.iter().any(|row| &row.target == target))
        {
            self.selected_graph = rows.first().map(|row| row.target.clone());
        }
    }

    fn prefill_fork(&mut self) {
        let command = match self.explorer.subject.clone() {
            Some(crate::ActiveSubject::Layer(layer)) => {
                Some(format!("branch fork --name new-rollout --layer {layer}"))
            }
            Some(crate::ActiveSubject::Commit(branch, commit)) => Some(format!(
                "branch fork --name new-rollout --branch {branch} --commit {commit}"
            )),
            Some(crate::ActiveSubject::Branch(branch)) => self
                .project
                .as_ref()
                .and_then(|project| project.branches.items.iter().find(|item| item.id == branch))
                .and_then(|item| item.work_head.as_ref().or(item.authority_head.as_ref()))
                .map(|commit| {
                    format!("branch fork --name new-rollout --branch {branch} --commit {commit}")
                }),
            None => None,
        };
        if let Some(command) = command {
            self.command = command;
            self.command_cursor = self.command.len();
            self.overlay = Overlay::Command;
            self.update_completions();
        }
    }

    fn prefill_workspace(&mut self) {
        let command = match self.explorer.subject.clone() {
            Some(crate::ActiveSubject::Commit(branch_id, commit_id)) => self
                .project
                .as_ref()
                .and_then(|project| {
                    project
                        .branches
                        .items
                        .iter()
                        .find(|branch| branch.id == branch_id)
                })
                .filter(|branch| {
                    branch.commits.iter().any(|commit| {
                        commit.id == commit_id
                            && commit
                                .actions
                                .contains(&layerfs_cli::SemanticAction::Workspace)
                    })
                })
                .map(|_| {
                    format!(
                        "workspace create --branch {branch_id} --commit {commit_id} --at /tmp/layerfs-workspace"
                    )
                }),
            Some(crate::ActiveSubject::Branch(branch_id)) => self
                .project
                .as_ref()
                .and_then(|project| {
                    project
                        .branches
                        .items
                        .iter()
                        .find(|branch| branch.id == branch_id)
                })
                .filter(|branch| {
                    branch
                        .actions
                        .contains(&layerfs_cli::SemanticAction::Workspace)
                })
                .and_then(|branch| match &branch.origin {
                    BranchOrigin::Layer(layer) => Some(format!(
                        "workspace create --branch {branch_id} --initial-layer {layer} --at /tmp/layerfs-workspace"
                    )),
                    BranchOrigin::Commit(_, _) => None,
                }),
            Some(crate::ActiveSubject::Layer(_)) | None => None,
        };
        if let Some(command) = command {
            self.command = command;
            self.command_cursor = self.command.len();
            self.overlay = Overlay::Command;
            self.update_completions();
        } else {
            self.error =
                Some("Workspace requires a local Branch head or a new local Branch base".into());
        }
    }

    fn open_diff(&mut self, request: DiffRequest) {
        self.history.push(self.route.clone());
        self.refresh_diff(request);
        self.focus = 0;
        self.compact_pane = 0;
    }

    fn route_after_command(&mut self, command: Command) {
        match command.kind {
            layerfs_cli::CommandKind::Diff(request) => self.open_diff(request),
            layerfs_cli::CommandKind::Monitor { .. } => {
                self.route = Route::Activity(ActivityTab::Storage);
                self.focus = 1;
                self.compact_pane = 1;
            }
            layerfs_cli::CommandKind::WorkspaceAction { action, .. }
                if matches!(action.as_str(), "output" | "conflicts") =>
            {
                self.route = Route::Workspaces(self.selected_project.clone());
                self.focus = 1;
                self.compact_pane = 1;
            }
            layerfs_cli::CommandKind::WorkspaceAction { action, .. } if action == "end" => {
                self.route = Route::Workspaces(self.selected_project.clone());
                self.focus = 0;
                self.compact_pane = 0;
            }
            layerfs_cli::CommandKind::ReadOnly { family, action }
                if family == "query" && action.starts_with("projects") =>
            {
                self.route = Route::Projects;
                self.focus = 0;
                self.compact_pane = 0;
            }
            _ => {}
        }
    }

    fn update_completions(&mut self) {
        self.completions = self
            .session
            .complete(&self.command, self.command_cursor)
            .unwrap_or_default();
        self.completion_selected = 0;
    }

    fn prefill_workspace_bash(&mut self) {
        let Some(workspace) = self.selected_workspace.as_ref() else {
            return;
        };
        self.command = format!("workspace exec {workspace} -- /bin/bash -lc \"\"");
        self.command_cursor = self.command.len().saturating_sub(1);
        self.overlay = Overlay::Command;
        self.update_completions();
    }

    fn plan_workspace_command(&mut self, action: &str, discard: bool) {
        let Some(workspace) = self.selected_workspace.as_ref() else {
            return;
        };
        if action == "end"
            && !discard
            && self.selected_workspace_view().is_some_and(|view| {
                matches!(
                    view.state,
                    WorkspaceState::Dirty | WorkspaceState::HeadMoved
                )
            })
        {
            self.error =
                Some("Workspace has an uncommitted final delta; use D to Discard & End".into());
            return;
        }
        let line = format!(
            "workspace {action} {workspace}{}",
            if discard { " --discard" } else { "" }
        );
        match CliSession::parse_line(&line)
            .and_then(|command| self.session.plan(&command).map(|plan| (command, plan)))
        {
            Ok(value) => {
                self.plan = Some(value);
                self.overlay = Overlay::Plan;
            }
            Err(error) => self.error = Some(error.to_string()),
        }
    }

    pub fn paste_command(&mut self, value: &str) {
        for character in value.chars().filter(|value| !matches!(value, '\n' | '\r')) {
            self.command.insert(self.command_cursor, character);
            self.command_cursor += character.len_utf8();
        }
        self.update_completions();
    }

    pub fn quit_cleanly(&mut self) {
        if let Some(operation) = self.active_operation.as_mut() {
            if let Err(error) = operation.interrupt() {
                self.error = Some(error.to_string());
            }
            for _ in 0..4 {
                self.tick();
                if self.active_operation.is_none() {
                    break;
                }
            }
        }
        self.should_quit = true;
    }

    fn selected_branch_id(&self) -> Option<BranchId> {
        match self.selected_graph.as_ref()? {
            GraphTarget::Branch(branch) | GraphTarget::Commit(branch, _) => Some(branch.clone()),
        }
    }

    fn set_expanded(&mut self, expanded: bool) {
        if let Some(branch) = self.selected_branch_id() {
            if expanded {
                self.expanded.insert(branch);
            } else {
                self.expanded.remove(&branch);
                self.selected_graph = Some(GraphTarget::Branch(branch));
            }
        }
    }

    fn toggle_expanded(&mut self) {
        if let Some(branch) = self.selected_branch_id() {
            if self.expanded.contains(&branch) {
                self.set_expanded(false);
            } else {
                self.set_expanded(true);
            }
        }
    }
}

fn important_commits(branch: &BranchView) -> Vec<&layerfs_cli::CommitView> {
    if branch.commits.len() <= 12 {
        return branch.commits.iter().collect();
    }
    branch
        .commits
        .iter()
        .filter(|commit| {
            commit.number == 1
                || commit.authority_head
                || commit.work_head
                || commit.child_branches > 0
                || !commit.workspaces.is_empty()
                || commit.accepted_layer.is_some()
                || commit.number + 8 >= branch.commits.len() as u16
        })
        .collect()
}

fn move_id<T: Clone + PartialEq>(items: &[T], selected: Option<&T>, delta: isize) -> Option<T> {
    if items.is_empty() {
        return None;
    }
    let current = selected
        .and_then(|selected| items.iter().position(|item| item == selected))
        .unwrap_or(0);
    let next = (current as isize + delta).clamp(0, items.len() as isize - 1) as usize;
    items.get(next).cloned()
}

fn move_target(
    rows: &[GraphRow],
    selected: Option<&GraphTarget>,
    delta: isize,
) -> Option<GraphTarget> {
    move_id(
        &rows
            .iter()
            .map(|row| row.target.clone())
            .collect::<Vec<_>>(),
        selected,
        delta,
    )
}

fn page() -> PageRequest {
    PageRequest::first(64)
}

fn previous_boundary(value: &str, cursor: usize) -> usize {
    value[..cursor]
        .char_indices()
        .last()
        .map_or(0, |(index, _)| index)
}

fn next_boundary(value: &str, cursor: usize) -> usize {
    value[cursor..]
        .char_indices()
        .nth(1)
        .map_or(value.len(), |(index, _)| cursor + index)
}

#[cfg(test)]
mod tests;

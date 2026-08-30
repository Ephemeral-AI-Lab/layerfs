use layerfs_cli::{
    BranchId, CommitId, DiffChange, DiffEntryView, DiffSnapshot, FilePreview, FilesSnapshot,
    LayerId, PageRequest, RouteTarget, ViewQuery, ViewSnapshot, WorkspaceFileKind,
    WorkspaceFileView, WorkspaceId,
};
use ratatui::{
    layout::Rect,
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};
use std::collections::HashSet;

use crate::{
    app::{App, GraphTarget},
    format::{bytes, display_id, truncate},
    render::{list_panel, panel, panel_at_scroll, three_panes},
    theme::Theme,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExplorerMode {
    Topology,
    Files,
    Changes,
}

impl ExplorerMode {
    pub const ALL: [Self; 3] = [Self::Topology, Self::Files, Self::Changes];

    pub const fn label(self) -> &'static str {
        match self {
            Self::Topology => "Topology",
            Self::Files => "Files",
            Self::Changes => "Changes",
        }
    }

    pub fn next(self, delta: isize) -> Self {
        let current = Self::ALL.iter().position(|mode| *mode == self).unwrap_or(0) as isize;
        Self::ALL[(current + delta).rem_euclid(Self::ALL.len() as isize) as usize]
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ActiveSubject {
    Layer(LayerId),
    Branch(BranchId),
    Commit(BranchId, CommitId),
}

impl ActiveSubject {
    pub fn target(&self) -> RouteTarget {
        match self {
            Self::Layer(id) => RouteTarget::Layer(id.clone()),
            Self::Branch(id) => RouteTarget::Branch(id.clone()),
            Self::Commit(branch, commit) => RouteTarget::Commit(branch.clone(), commit.clone()),
        }
    }
}

pub struct ExplorerState {
    pub subject: Option<ActiveSubject>,
    pub mode: ExplorerMode,
    pub files: Option<FilesSnapshot>,
    pub changes: Option<DiffSnapshot>,
    pub selected_path: Option<String>,
    pub expanded_dirs: HashSet<String>,
    pub content_scroll: u16,
}

impl ExplorerState {
    pub fn new(layer: Option<LayerId>) -> Self {
        Self {
            subject: layer.map(ActiveSubject::Layer),
            mode: ExplorerMode::Topology,
            files: None,
            changes: None,
            selected_path: None,
            expanded_dirs: HashSet::new(),
            content_scroll: 0,
        }
    }

    pub fn set_subject(&mut self, subject: ActiveSubject) {
        if self.subject.as_ref() != Some(&subject) {
            self.subject = Some(subject);
            self.files = None;
            self.changes = None;
            self.selected_path = None;
            self.expanded_dirs.clear();
            self.content_scroll = 0;
        }
    }

    pub fn clear_content(&mut self) {
        self.files = None;
        self.changes = None;
        self.selected_path = None;
        self.expanded_dirs.clear();
        self.content_scroll = 0;
    }

    pub fn workspace_target(id: WorkspaceId) -> RouteTarget {
        RouteTarget::Workspace(id)
    }
}

impl App {
    pub(crate) fn sync_explorer_subject(&mut self) {
        let valid = self.explorer.subject.as_ref().is_some_and(|subject| {
            self.project.as_ref().is_some_and(|project| match subject {
                ActiveSubject::Layer(id) => {
                    project.layers.items.iter().any(|layer| &layer.id == id)
                }
                ActiveSubject::Branch(id) => {
                    project.branches.items.iter().any(|branch| &branch.id == id)
                }
                ActiveSubject::Commit(branch_id, commit_id) => project
                    .branches
                    .items
                    .iter()
                    .find(|branch| &branch.id == branch_id)
                    .is_some_and(|branch| {
                        branch.commits.iter().any(|commit| &commit.id == commit_id)
                    }),
            })
        });
        if !valid {
            self.activate_selected_layer();
        }
    }

    pub(crate) fn activate_selected_layer(&mut self) {
        if let Some(layer) = self.selected_layer.clone() {
            self.explorer.set_subject(ActiveSubject::Layer(layer));
        }
    }

    pub(crate) fn activate_selected_graph(&mut self) {
        if let Some(target) = self.selected_graph.clone() {
            let subject = match target {
                GraphTarget::Branch(id) => ActiveSubject::Branch(id),
                GraphTarget::Commit(branch, commit) => ActiveSubject::Commit(branch, commit),
            };
            self.explorer.set_subject(subject);
        }
    }

    pub(crate) fn set_explorer_mode(&mut self, mode: ExplorerMode) {
        self.explorer.mode = mode;
        self.explorer.content_scroll = 0;
        self.focus = 0;
        self.compact_pane = 0;
        self.refresh_explorer();
    }

    pub(crate) fn cycle_explorer_mode(&mut self, delta: isize) {
        self.set_explorer_mode(self.explorer.mode.next(delta));
    }

    pub(crate) fn refresh_explorer(&mut self) {
        let Some(target) = self.explorer.subject.as_ref().map(ActiveSubject::target) else {
            self.explorer.clear_content();
            return;
        };
        let result = match self.explorer.mode {
            ExplorerMode::Topology => return,
            ExplorerMode::Files => self.session.snapshot(ViewQuery::Files {
                target,
                page: PageRequest::first(128),
            }),
            ExplorerMode::Changes => self.session.snapshot(ViewQuery::Changes {
                target,
                page: PageRequest::first(128),
            }),
        };
        match result {
            Ok(ViewSnapshot::Files(snapshot)) => {
                let first_load = self.explorer.files.is_none();
                self.explorer.selected_path = retained_path(
                    self.explorer.selected_path.as_ref(),
                    snapshot.files.items.iter().map(|file| &file.path),
                );
                if first_load {
                    self.explorer.expanded_dirs = snapshot
                        .files
                        .items
                        .iter()
                        .filter(|file| file.kind == WorkspaceFileKind::Directory)
                        .map(|file| file.path.clone())
                        .collect();
                } else {
                    self.explorer.expanded_dirs.retain(|path| {
                        snapshot.files.items.iter().any(|file| {
                            &file.path == path && file.kind == WorkspaceFileKind::Directory
                        })
                    });
                }
                self.explorer.files = Some(snapshot);
                self.explorer.changes = None;
            }
            Ok(ViewSnapshot::Changes(snapshot)) => {
                self.explorer.selected_path = retained_path(
                    self.explorer.selected_path.as_ref(),
                    snapshot.entries.items.iter().map(|entry| &entry.path),
                );
                self.explorer.changes = Some(snapshot);
                self.explorer.files = None;
            }
            Ok(_) => {}
            Err(error) => self.error = Some(error.to_string()),
        }
    }

    pub(crate) fn explorer_paths(&self) -> Vec<String> {
        match self.explorer.mode {
            ExplorerMode::Topology => Vec::new(),
            ExplorerMode::Files => self
                .explorer
                .files
                .as_ref()
                .map(|snapshot| {
                    snapshot
                        .files
                        .items
                        .iter()
                        .filter(|file| path_visible(&file.path, &self.explorer.expanded_dirs))
                        .map(|file| file.path.clone())
                        .collect()
                })
                .unwrap_or_default(),
            ExplorerMode::Changes => self
                .explorer
                .changes
                .as_ref()
                .map(|snapshot| {
                    snapshot
                        .entries
                        .items
                        .iter()
                        .map(|entry| entry.path.clone())
                        .collect()
                })
                .unwrap_or_default(),
        }
    }

    pub(crate) fn move_explorer_path(&mut self, delta: isize) {
        if self.focus == 1 {
            self.explorer.content_scroll = if delta.is_negative() {
                self.explorer
                    .content_scroll
                    .saturating_sub(delta.unsigned_abs() as u16)
            } else {
                self.explorer.content_scroll.saturating_add(delta as u16)
            };
            return;
        }
        if self.focus != 0 {
            return;
        }
        let paths = self.explorer_paths();
        self.explorer.selected_path =
            move_item(&paths, self.explorer.selected_path.as_ref(), delta);
    }

    pub(crate) fn toggle_explorer_directory(&mut self, expanded: bool) {
        let Some(path) = self.explorer.selected_path.clone() else {
            return;
        };
        let is_directory = self.explorer.files.as_ref().is_some_and(|snapshot| {
            snapshot
                .files
                .items
                .iter()
                .any(|file| file.path == path && file.kind == WorkspaceFileKind::Directory)
        });
        if !is_directory {
            return;
        }
        if expanded {
            self.explorer.expanded_dirs.insert(path);
        } else {
            self.explorer.expanded_dirs.remove(&path);
        }
    }

    pub(crate) fn open_explorer_selection(&mut self) {
        match self.explorer.mode {
            ExplorerMode::Topology => self.set_explorer_mode(ExplorerMode::Files),
            ExplorerMode::Changes => {
                self.set_explorer_mode(ExplorerMode::Files);
                self.focus = 1;
                self.compact_pane = 1;
            }
            ExplorerMode::Files if self.focus == 0 => {
                let directory = self.explorer.files.as_ref().is_some_and(|snapshot| {
                    snapshot.files.items.iter().any(|file| {
                        self.explorer.selected_path.as_ref() == Some(&file.path)
                            && file.kind == WorkspaceFileKind::Directory
                    })
                });
                if directory {
                    let expanded = self
                        .explorer
                        .selected_path
                        .as_ref()
                        .is_some_and(|path| self.explorer.expanded_dirs.contains(path));
                    self.toggle_explorer_directory(!expanded);
                } else {
                    self.focus = 1;
                    self.compact_pane = 1;
                }
            }
            ExplorerMode::Files => {}
        }
    }
}

pub(crate) fn tabs(frame: &mut Frame, area: Rect, app: &App, theme: Theme) {
    let mut spans = vec![Span::styled(
        format!(" {}  ", subject_label(app)),
        theme.title(),
    )];
    for mode in ExplorerMode::ALL {
        let label = format!(" {} ", mode.label());
        spans.push(Span::styled(
            label,
            if mode == app.explorer.mode {
                theme.selected()
            } else {
                theme.muted()
            },
        ));
    }
    spans.push(Span::styled("  [ prev mode · ] next mode", theme.muted()));
    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

pub(crate) fn navigation(frame: &mut Frame, area: Rect, app: &App, theme: Theme) {
    let (name, total) = match app.explorer.mode {
        ExplorerMode::Topology => match app.route {
            crate::Route::Branch(_, _) => (["GRAPH", "INSPECTOR"][app.focus.min(1)], 2),
            _ => (["LAYERS", "GRAPH", "INSPECTOR"][app.focus.min(2)], 3),
        },
        ExplorerMode::Files => (["FILE TREE", "CONTENT", "INSPECTOR"][app.focus.min(2)], 3),
        ExplorerMode::Changes => (
            ["CHANGED PATHS", "BEFORE / AFTER", "INSPECTOR"][app.focus.min(2)],
            3,
        ),
    };
    let back = if app.explorer.mode == ExplorerMode::Topology {
        "Projects"
    } else {
        "Topology"
    };
    if area.width < 100 {
        frame.render_widget(
            Paragraph::new(format!(
                " FOCUS {}/{} · {name}  [Tab] pane  [Esc] {back}  [?] Help",
                app.focus + 1,
                total
            )),
            area,
        );
        return;
    }
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                format!(" FOCUS {}/{} · {name}  ", app.focus + 1, total),
                theme.focus(),
            ),
            Span::raw("[Tab] next pane  [Shift+Tab] previous  "),
            Span::styled(format!("[Esc/Backspace] {back}  [?] help"), theme.muted()),
        ])),
        area,
    );
}

pub(crate) fn draw(frame: &mut Frame, area: Rect, app: &App, theme: Theme) {
    match app.explorer.mode {
        ExplorerMode::Files => draw_files(frame, area, app, theme),
        ExplorerMode::Changes => draw_changes(frame, area, app, theme),
        ExplorerMode::Topology => {}
    }
}

fn draw_files(frame: &mut Frame, area: Rect, app: &App, theme: Theme) {
    let panes = three_panes(area, app.compact_pane);
    let snapshot = app.explorer.files.as_ref();
    let selected = snapshot.and_then(|snapshot| selected_file(app, snapshot));
    if let Some(left) = panes[0] {
        let paths = app.explorer_paths();
        let lines = snapshot
            .map(|snapshot| {
                paths
                    .iter()
                    .filter_map(|path| snapshot.files.items.iter().find(|file| &file.path == path))
                    .map(|file| file_line(app, file, theme))
                    .collect()
            })
            .unwrap_or_else(no_data);
        let selected_index = paths
            .iter()
            .position(|path| app.explorer.selected_path.as_ref() == Some(path));
        list_panel(
            frame,
            left,
            " FILE TREE ",
            app.focus == 0,
            selected_index,
            lines,
            theme,
        );
    }
    if let Some(center) = panes[1] {
        panel_at_scroll(
            frame,
            center,
            &selected
                .map(|file| format!(" CONTENT · {} ", truncate(&file.path, 36)))
                .unwrap_or_else(|| " CONTENT ".into()),
            app.focus == 1,
            app.explorer.content_scroll,
            selected.map(preview_lines).unwrap_or_else(no_data),
            theme,
        );
    }
    if let Some(right) = panes[2] {
        panel(
            frame,
            right,
            " FILE INSPECTOR ",
            app.focus == 2,
            file_inspector(snapshot, selected),
            theme,
        );
    }
}

fn draw_changes(frame: &mut Frame, area: Rect, app: &App, theme: Theme) {
    let panes = three_panes(area, app.compact_pane);
    let snapshot = app.explorer.changes.as_ref();
    let selected = snapshot.and_then(|snapshot| selected_change(app, snapshot));
    if let Some(left) = panes[0] {
        let lines = snapshot
            .map(|snapshot| {
                snapshot
                    .entries
                    .items
                    .iter()
                    .map(|entry| change_line(app, entry, left.width.saturating_sub(4), theme))
                    .collect()
            })
            .unwrap_or_else(no_data);
        let selected_index = snapshot.and_then(|snapshot| {
            snapshot
                .entries
                .items
                .iter()
                .position(|entry| app.explorer.selected_path.as_ref() == Some(&entry.path))
        });
        list_panel(
            frame,
            left,
            " CHANGED PATHS ",
            app.focus == 0,
            selected_index,
            lines,
            theme,
        );
    }
    if let Some(center) = panes[1] {
        panel_at_scroll(
            frame,
            center,
            &selected
                .map(|entry| format!(" BEFORE → AFTER · {} ", truncate(&entry.path, 30)))
                .unwrap_or_else(|| " BEFORE → AFTER ".into()),
            app.focus == 1,
            app.explorer.content_scroll,
            selected.map(change_content).unwrap_or_else(no_data),
            theme,
        );
    }
    if let Some(right) = panes[2] {
        panel(
            frame,
            right,
            " DELTA INSPECTOR ",
            app.focus == 2,
            change_inspector(snapshot, selected),
            theme,
        );
    }
}

fn file_line(app: &App, file: &WorkspaceFileView, theme: Theme) -> Line<'static> {
    let active = app.explorer.selected_path.as_ref() == Some(&file.path);
    let selected = active && app.focus == 0;
    let depth = file.path.matches('/').count();
    let name = file.path.rsplit('/').next().unwrap_or(&file.path);
    let marker = match file.kind {
        WorkspaceFileKind::Directory if app.explorer.expanded_dirs.contains(&file.path) => "▾",
        WorkspaceFileKind::Directory => "▸",
        WorkspaceFileKind::File => "·",
    };
    Line::styled(
        format!(
            "{}{}{} {}  {}",
            if selected {
                "> "
            } else if active {
                "• "
            } else {
                "  "
            },
            "  ".repeat(depth),
            marker,
            name,
            bytes(file.bytes)
        ),
        if selected {
            theme.selected()
        } else {
            Default::default()
        },
    )
}

fn change_line(app: &App, entry: &DiffEntryView, width: u16, theme: Theme) -> Line<'static> {
    let active = app.explorer.selected_path.as_ref() == Some(&entry.path);
    let selected = active && app.focus == 0;
    let marker = match entry.change {
        DiffChange::Add => "+",
        DiffChange::Remove => "−",
        DiffChange::Modify => "~",
    };
    Line::styled(
        truncate(
            &format!(
                "{}{} {}  {}",
                if selected {
                    ">"
                } else if active {
                    "•"
                } else {
                    " "
                },
                marker,
                entry.path,
                entry.aspects.join(", ")
            ),
            usize::from(width),
        ),
        if selected {
            theme.selected()
        } else {
            Default::default()
        },
    )
}

fn preview_lines(file: &WorkspaceFileView) -> Vec<Line<'static>> {
    match &file.preview {
        FilePreview::None => vec![Line::from("Directory")],
        FilePreview::Binary => vec![Line::from("Binary file · preview unavailable")],
        FilePreview::Text(value) => text_lines(value),
        FilePreview::Truncated(value) => {
            let mut lines = text_lines(value);
            lines.push(Line::from("… preview truncated"));
            lines
        }
    }
}

fn change_content(entry: &DiffEntryView) -> Vec<Line<'static>> {
    let mut lines = vec![Line::from("BEFORE")];
    lines.extend(text_lines(entry.before.as_deref().unwrap_or("∅")));
    lines.push(Line::from(""));
    lines.push(Line::from("AFTER"));
    lines.extend(text_lines(entry.after.as_deref().unwrap_or("∅")));
    lines
}

fn file_inspector(
    snapshot: Option<&FilesSnapshot>,
    file: Option<&WorkspaceFileView>,
) -> Vec<Line<'static>> {
    let (Some(snapshot), Some(file)) = (snapshot, file) else {
        return no_data();
    };
    vec![
        Line::from(file.path.clone()),
        Line::from(""),
        Line::from(format!("Kind        {:?}", file.kind)),
        Line::from(format!("Logical     {}", bytes(file.bytes))),
        Line::from(format!(
            "Allocated   {}",
            file.allocated_bytes
                .map(bytes)
                .unwrap_or_else(|| "unavailable".into())
        )),
        Line::from(format!("Root        {}", display_id(&snapshot.root))),
        Line::from(format!(
            "Generation  {}",
            snapshot
                .generation
                .map(|value| value.to_string())
                .unwrap_or_else(|| "immutable".into())
        )),
        Line::from(format!("Resolved    {}", target_label(&snapshot.resolved))),
    ]
}

fn change_inspector(
    snapshot: Option<&DiffSnapshot>,
    entry: Option<&DiffEntryView>,
) -> Vec<Line<'static>> {
    let Some(snapshot) = snapshot else {
        return no_data();
    };
    let mut lines = vec![
        Line::from("Exact final-state delta"),
        Line::from(format!(
            "From  {}",
            snapshot
                .from_target
                .as_ref()
                .map(target_label)
                .unwrap_or_else(|| "empty filesystem".into())
        )),
        Line::from(format!("To    {}", target_label(&snapshot.to_target))),
        Line::from(""),
        Line::from(format!(
            "+{}  ~{}  −{}",
            snapshot.summary.added, snapshot.summary.modified, snapshot.summary.removed
        )),
        Line::from(format!(
            "Bytes {} → {}",
            bytes(snapshot.summary.before_bytes),
            bytes(snapshot.summary.after_bytes)
        )),
        Line::from(format!(
            "Root  {} → {}",
            snapshot
                .from_root
                .as_ref()
                .map(display_id)
                .unwrap_or_else(|| "empty".into()),
            display_id(&snapshot.to_root)
        )),
    ];
    if let Some(entry) = entry {
        lines.extend([
            Line::from(""),
            Line::from(entry.path.clone()),
            Line::from(format!("Change   {:?}", entry.change)),
            Line::from(format!("Aspects  {}", entry.aspects.join(", "))),
        ]);
    }
    lines
}

fn selected_file<'a>(app: &App, snapshot: &'a FilesSnapshot) -> Option<&'a WorkspaceFileView> {
    let path = app.explorer.selected_path.as_ref()?;
    snapshot.files.items.iter().find(|file| &file.path == path)
}

fn selected_change<'a>(app: &App, snapshot: &'a DiffSnapshot) -> Option<&'a DiffEntryView> {
    let path = app.explorer.selected_path.as_ref()?;
    snapshot
        .entries
        .items
        .iter()
        .find(|entry| &entry.path == path)
}

fn subject_label(app: &App) -> String {
    match app.explorer.subject.as_ref() {
        Some(ActiveSubject::Layer(id)) => app
            .project
            .as_ref()
            .and_then(|project| project.layers.items.iter().find(|layer| &layer.id == id))
            .map(|layer| format!("Layer L{}", layer.number))
            .unwrap_or_else(|| format!("Layer {}", display_id(id))),
        Some(ActiveSubject::Branch(id)) => app
            .project
            .as_ref()
            .and_then(|project| {
                project
                    .branches
                    .items
                    .iter()
                    .find(|branch| &branch.id == id)
            })
            .map(|branch| format!("Branch {}", branch.name))
            .unwrap_or_else(|| format!("Branch {}", display_id(id))),
        Some(ActiveSubject::Commit(branch, commit)) => app
            .project
            .as_ref()
            .and_then(|project| {
                project
                    .branches
                    .items
                    .iter()
                    .find(|item| &item.id == branch)
            })
            .and_then(|branch| {
                branch
                    .commits
                    .iter()
                    .find(|item| &item.id == commit)
                    .map(|item| format!("Commit {}@C{}", branch.name, item.number))
            })
            .unwrap_or_else(|| format!("Commit {}", display_id(commit))),
        None => "No subject".into(),
    }
}

fn target_label(target: &RouteTarget) -> String {
    match target {
        RouteTarget::Project(id) => format!("Project {}", display_id(id)),
        RouteTarget::Layer(id) => format!("Layer {}", display_id(id)),
        RouteTarget::Branch(id) => format!("Branch {}", display_id(id)),
        RouteTarget::Commit(_, commit) => format!("Commit {}", display_id(commit)),
        RouteTarget::Workspace(id) => format!("Workspace {}", display_id(id)),
        RouteTarget::Operation(id) => format!("Operation {}", display_id(id)),
    }
}

fn text_lines(value: &str) -> Vec<Line<'static>> {
    value
        .lines()
        .map(|line| Line::from(line.to_owned()))
        .collect()
}

pub(crate) fn no_data() -> Vec<Line<'static>> {
    vec![Line::from("No data")]
}

fn retained_path<'a>(
    selected: Option<&String>,
    paths: impl Iterator<Item = &'a String>,
) -> Option<String> {
    let paths = paths.cloned().collect::<Vec<_>>();
    selected
        .filter(|selected| paths.contains(*selected))
        .cloned()
        .or_else(|| paths.first().cloned())
}

fn path_visible(path: &str, expanded: &HashSet<String>) -> bool {
    let mut parent = path.rsplit_once('/').map(|(parent, _)| parent);
    while let Some(value) = parent {
        if !expanded.contains(value) {
            return false;
        }
        parent = value.rsplit_once('/').map(|(parent, _)| parent);
    }
    true
}

fn move_item<T: Clone + PartialEq>(items: &[T], selected: Option<&T>, delta: isize) -> Option<T> {
    if items.is_empty() {
        return None;
    }
    let current = selected
        .and_then(|selected| items.iter().position(|item| item == selected))
        .unwrap_or(0);
    let next = (current as isize + delta).clamp(0, items.len() as isize - 1) as usize;
    items.get(next).cloned()
}

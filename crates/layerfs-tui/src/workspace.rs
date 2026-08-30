use crate::{theme::Theme, App, Route};
use layerfs_cli::{
    DiffChange, FilePreview, WorkspaceFileKind, WorkspaceFileView, WorkspaceRunView,
    WorkspaceState, WorkspaceView,
};
use ratatui::{
    layout::{Constraint, Layout, Rect},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
    Frame,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorkspaceTab {
    Overview,
    Files,
    Changes,
    Runs,
    Storage,
}

impl WorkspaceTab {
    pub const ALL: [Self; 5] = [
        Self::Overview,
        Self::Files,
        Self::Changes,
        Self::Runs,
        Self::Storage,
    ];

    pub const fn label(self) -> &'static str {
        match self {
            Self::Overview => "Overview",
            Self::Files => "Files",
            Self::Changes => "Changes",
            Self::Runs => "Runs",
            Self::Storage => "Storage",
        }
    }

    pub(crate) fn next(self, delta: isize) -> Self {
        let current = Self::ALL.iter().position(|tab| *tab == self).unwrap_or(0) as isize;
        Self::ALL[(current + delta).rem_euclid(Self::ALL.len() as isize) as usize]
    }
}

pub(crate) fn draw(frame: &mut Frame, area: Rect, app: &App, theme: Theme) {
    let rows = Layout::vertical([Constraint::Length(1), Constraint::Min(4)]).split(area);
    tabs(frame, rows[0], app, theme);
    let Some(workspace) = selected(app) else {
        pane(
            frame,
            rows[1],
            " WORKSPACE ",
            true,
            vec![Line::from(
                "Workspace no longer exists. Press Esc to return.",
            )],
            theme,
        );
        return;
    };
    let panes = body_panes(rows[1], app.compact_pane);
    let sections = match app.workspace_tab {
        WorkspaceTab::Overview => overview(workspace),
        WorkspaceTab::Files => files(workspace, app.selected_workspace_path.as_deref()),
        WorkspaceTab::Changes => changes(workspace, app.selected_workspace_path.as_deref()),
        WorkspaceTab::Runs => runs(workspace, app.selected_workspace_path.as_deref()),
        WorkspaceTab::Storage => storage(workspace),
    };
    for (index, (area, (title, lines))) in panes.into_iter().zip(sections).enumerate() {
        if let Some(area) = area {
            pane(frame, area, title, app.focus == index, lines, theme);
        }
    }
}

fn selected(app: &App) -> Option<&WorkspaceView> {
    app.selected_workspace_view()
}

impl App {
    pub(crate) fn selected_workspace_view(&self) -> Option<&WorkspaceView> {
        let id = match &self.route {
            Route::Workspace(id) => id,
            _ => self.selected_workspace.as_ref()?,
        };
        self.workspaces
            .workspaces
            .items
            .iter()
            .find(|workspace| &workspace.id == id)
    }

    pub(crate) fn workspace_items(&self) -> Vec<String> {
        let Some(workspace) = self.selected_workspace_view() else {
            return Vec::new();
        };
        match self.workspace_tab {
            WorkspaceTab::Files => workspace
                .files
                .iter()
                .map(|file| file.path.clone())
                .collect(),
            WorkspaceTab::Changes => workspace
                .changes
                .iter()
                .map(|change| change.path.clone())
                .collect(),
            WorkspaceTab::Runs => workspace
                .runs
                .iter()
                .map(|run| run.execution_id.to_string())
                .collect(),
            WorkspaceTab::Overview | WorkspaceTab::Storage => Vec::new(),
        }
    }

    pub(crate) fn sync_workspace_item(&mut self) {
        let items = self.workspace_items();
        if self
            .selected_workspace_path
            .as_ref()
            .is_none_or(|selected| !items.contains(selected))
        {
            self.selected_workspace_path = items.first().cloned();
        }
    }
}

fn tabs(frame: &mut Frame, area: Rect, app: &App, theme: Theme) {
    let spans = WorkspaceTab::ALL
        .into_iter()
        .flat_map(|tab| {
            let active = tab == app.workspace_tab;
            [
                Span::styled(
                    format!(" {} ", tab.label()),
                    if active {
                        theme.selected()
                    } else {
                        theme.muted()
                    },
                ),
                Span::raw(" "),
            ]
        })
        .collect::<Vec<_>>();
    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

type Section = (&'static str, Vec<Line<'static>>);

fn overview(workspace: &WorkspaceView) -> [Section; 3] {
    let lifecycle = vec![
        Line::from("SESSION ACTIVITY — ephemeral; removed on End"),
        Line::from(""),
        Line::from(format!(
            "✓ Created       {}",
            duration(workspace.timing.create_micros)
        )),
        Line::from(format!(
            "✓ Bash ×{:<3}      {} total",
            workspace.runs.len(),
            duration(workspace.timing.bash_total_micros)
        )),
        Line::from(format!(
            "{} Final state  generation {} · {} paths",
            if workspace.changed_paths == 0 {
                "✓"
            } else {
                "●"
            },
            workspace.generation,
            workspace.changed_paths
        )),
        Line::from(if workspace.commit_receipt.is_some() {
            "✓ Commit Final State"
        } else {
            "○ Commit Final State"
        }),
        Line::from("○ End Workspace"),
    ];
    let anchor = workspace
        .anchor_commit
        .as_ref()
        .map(|id| format!("Commit {id}"))
        .or_else(|| {
            workspace
                .anchor_layer
                .as_ref()
                .map(|id| format!("initial Layer {id}"))
        })
        .unwrap_or_else(|| "invalid".into());
    let details = vec![
        Line::from(format!("Workspace       {}", workspace.id)),
        Line::from(format!("Project         {}", workspace.project_name)),
        Line::from(format!(
            "Target Branch   {} ({})",
            workspace.branch_name, workspace.branch_id
        )),
        Line::from(format!("Pinned anchor   {anchor}")),
        Line::from(format!("Anchor root     {}", workspace.anchor_root)),
        Line::from(format!(
            "Expected head   {}",
            optional(workspace.expected_branch_head.as_ref())
        )),
        Line::from(format!("State           {}", workspace.state)),
        Line::from(format!(
            "Projection      {} · {}",
            workspace.projection, workspace.placement
        )),
        Line::from(format!("Mount           {}", clean(&workspace.mount))),
        Line::from(""),
        Line::from("Only final filesystem state becomes the Commit."),
        Line::from("Bash, order, output, and timing stay ephemeral."),
    ];
    let mut inspector = action_lines(workspace);
    if let Some(receipt) = &workspace.commit_receipt {
        inspector.extend([
            Line::from(""),
            Line::from("LATEST COMMIT"),
            Line::from(format!("Commit          {}", receipt.commit_id)),
            Line::from(format!("Root            {}", receipt.root)),
            Line::from(format!("Frozen gen      {}", receipt.generation)),
            Line::from(format!(
                "Total           {}",
                duration(receipt.total_micros)
            )),
        ]);
    }
    [
        (" LIFECYCLE ", lifecycle),
        (" FINAL STATE ", details),
        (" IDENTITY + ACTIONS ", inspector),
    ]
}

fn files(workspace: &WorkspaceView, selected: Option<&str>) -> [Section; 3] {
    let selected = select_file(workspace, selected);
    let navigator = workspace
        .files
        .iter()
        .map(|file| {
            let depth = file.path.matches('/').count();
            let active = selected.is_some_and(|value| value.path == file.path);
            let marker = match file.kind {
                WorkspaceFileKind::Directory => "▸",
                WorkspaceFileKind::File => change_marker(workspace, &file.path),
            };
            Line::from(format!(
                "{}{}{} {}  {}",
                if active { "> " } else { "  " },
                "  ".repeat(depth),
                marker,
                clean(&file.path),
                bytes(file.bytes)
            ))
        })
        .collect();
    let content = selected.map(preview_lines).unwrap_or_else(no_data);
    let inspector = selected
        .map(|file| {
            vec![
                Line::from(clean(&file.path)),
                Line::from(format!("Kind            {:?}", file.kind)),
                Line::from(format!("Logical         {}", bytes(file.bytes))),
                Line::from(format!(
                    "Allocated       {}",
                    file.allocated_bytes
                        .map(bytes)
                        .unwrap_or_else(|| "unavailable".into())
                )),
                Line::from(format!("Generation      {}", workspace.generation)),
                Line::from(format!(
                    "Change          {}",
                    change_marker(workspace, &file.path)
                )),
                Line::from(""),
                Line::from("[d] show this path in Changes"),
            ]
        })
        .unwrap_or_else(no_data);
    [
        (" FILES ", navigator),
        (" BOUNDED PREVIEW ", content),
        (" FILE INSPECTOR ", inspector),
    ]
}

fn changes(workspace: &WorkspaceView, selected: Option<&str>) -> [Section; 3] {
    let selected = workspace
        .changes
        .iter()
        .find(|change| selected == Some(change.path.as_str()))
        .or_else(|| workspace.changes.first());
    let navigator = workspace
        .changes
        .iter()
        .map(|change| {
            Line::from(format!(
                "{}{} {}",
                if selected.is_some_and(|value| value.path == change.path) {
                    "> "
                } else {
                    "  "
                },
                diff_marker(&change.change),
                clean(&change.path)
            ))
        })
        .collect();
    let content = selected
        .map(|change| {
            vec![
                Line::from(format!("CHANGES vs {}", anchor_label(workspace))),
                Line::from(""),
                Line::from(format!("{} vs immutable anchor", clean(&change.path))),
                Line::from(""),
                Line::from(format!(
                    "BEFORE │ {}",
                    clean(change.before.as_deref().unwrap_or("—"))
                )),
                Line::from(format!(
                    "AFTER  │ {}",
                    clean(change.after.as_deref().unwrap_or("—"))
                )),
                Line::from(""),
                Line::from("Final-state diff only; no Bash provenance."),
            ]
        })
        .unwrap_or_else(no_data);
    let inspector = selected
        .map(|change| {
            vec![
                Line::from(format!("Kind            {:?}", change.change)),
                Line::from(format!("Aspects         {}", change.aspects.join(", "))),
                Line::from(format!("Generation      {}", workspace.generation)),
                Line::from(format!("Anchor root     {}", workspace.anchor_root)),
            ]
        })
        .unwrap_or_else(no_data);
    [
        (" CHANGED PATHS ", navigator),
        (" BEFORE / AFTER ", content),
        (" CHANGE INSPECTOR ", inspector),
    ]
}

fn runs(workspace: &WorkspaceView, selected: Option<&str>) -> [Section; 3] {
    let selected = workspace
        .runs
        .iter()
        .find(|run| selected == Some(run.execution_id.as_str()))
        .or_else(|| workspace.runs.last());
    let navigator = workspace
        .runs
        .iter()
        .enumerate()
        .map(|(index, run)| {
            Line::from(format!(
                "{}#{:<3} exit {:<3} {:>9}  {}",
                if selected.is_some_and(|value| value.execution_id == run.execution_id) {
                    "> "
                } else {
                    "  "
                },
                index + 1,
                run.exit_code,
                duration(run.elapsed_micros),
                truncate(&clean(&run.script), 30)
            ))
        })
        .collect();
    let content = selected
        .map(|run| output_lines(run, 80))
        .unwrap_or_else(no_data);
    let inspector = selected
        .map(|run| {
            vec![
                Line::from(format!("Execution       {}", run.execution_id)),
                Line::from(format!("Elapsed         {}", duration(run.elapsed_micros))),
                Line::from(format!("Exit            {}", run.exit_code)),
                Line::from(format!("Output          {}", bytes(run.output_bytes))),
                Line::from(format!("Final state     {}", workspace.state)),
                Line::from(""),
                Line::from("Output and script are not Commit identity."),
            ]
        })
        .unwrap_or_else(no_data);
    [
        (" BASH RUNS ", navigator),
        (" RETAINED OUTPUT ", content),
        (" RUN INSPECTOR ", inspector),
    ]
}

fn storage(workspace: &WorkspaceView) -> [Section; 3] {
    let value = &workspace.storage;
    let cow = vec![
        Line::from("COW DELTA (MODEL) — not production FUSE allocation"),
        Line::from(""),
        Line::from(format!(
            "Logical final view       {}",
            bytes(value.logical_bytes)
        )),
        Line::from(format!(
            "Mock materialized alloc  {}",
            bytes(value.materialized_allocated_bytes)
        )),
        Line::from(format!(
            "COW delta model          {}",
            bytes(value.cow_delta_bytes)
        )),
        Line::from(format!(
            "Base reused              {}",
            bytes(value.base_reused_bytes)
        )),
        Line::from(format!(
            "Reuse                    {}",
            percent(value.base_reused_bytes, value.logical_bytes)
        )),
    ];
    let dedup = vec![
        Line::from("COMMIT DEDUP — BranchStore canonical objects"),
        Line::from(""),
        Line::from(format!(
            "Candidate objects        {}",
            value.candidate_objects
        )),
        Line::from(format!(
            "Candidate bytes          {}",
            bytes(value.candidate_bytes)
        )),
        Line::from(format!(
            "Inserted objects         {}",
            value.inserted_objects
        )),
        Line::from(format!(
            "Inserted bytes           {}",
            bytes(value.inserted_bytes)
        )),
        Line::from(format!("Reused objects           {}", value.reused_objects)),
        Line::from(format!(
            "Reused bytes             {}",
            bytes(value.reused_bytes)
        )),
        Line::from(format!(
            "Dedup saved              {}",
            percent(value.reused_bytes, value.candidate_bytes)
        )),
        Line::from(format!(
            "SQLite file growth       {}",
            bytes(value.sqlite_growth_bytes)
        )),
    ];
    let timing = vec![
        Line::from("TIMING"),
        Line::from(""),
        Line::from(format!(
            "Create              {}",
            duration(workspace.timing.create_micros)
        )),
        Line::from(format!(
            "Bash last           {}",
            duration(workspace.timing.bash_last_micros)
        )),
        Line::from(format!(
            "Bash total          {}",
            duration(workspace.timing.bash_total_micros)
        )),
        Line::from(format!(
            "Capture             {}",
            duration_or_dash(workspace.timing.capture_micros)
        )),
        Line::from(format!(
            "Admission           {}",
            duration_or_dash(workspace.timing.admission_micros)
        )),
        Line::from(format!(
            "Branch publish      {}",
            duration_or_dash(workspace.timing.publish_micros)
        )),
        Line::from(format!(
            "Commit total        {}",
            duration_or_dash(workspace.timing.commit_total_micros)
        )),
        Line::from(""),
        Line::from("End time is retained in Activity after cleanup."),
    ];
    [
        (" WORKSPACE STORAGE ", cow),
        (" COMMIT DEDUP ", dedup),
        (" TIMING ", timing),
    ]
}

fn action_lines(workspace: &WorkspaceView) -> Vec<Line<'static>> {
    let actions = match workspace.state {
        WorkspaceState::Clean => "[x] Run Bash  [c] Commit  [e] End",
        WorkspaceState::Dirty => "[x] Run Bash  [c] Commit  [D] Discard & End",
        WorkspaceState::Running | WorkspaceState::Busy => "Shell active; Commit and End disabled",
        WorkspaceState::HeadMoved => "Inspect retained delta; [D] Discard & End",
        WorkspaceState::ReadOnly => "[e] End Workspace",
    };
    vec![
        Line::from(format!("State           {}", workspace.state)),
        Line::from(format!(
            "Published       {}",
            optional(workspace.published_commit.as_ref())
        )),
        Line::from(""),
        Line::from(actions),
        Line::from(""),
        Line::from("Esc/back/quit never Commit or End."),
    ]
}

fn body_panes(area: Rect, active: usize) -> [Option<Rect>; 3] {
    if area.width < 100 {
        let mut panes = [None, None, None];
        panes[active % 3] = Some(area);
        panes
    } else {
        let columns = Layout::horizontal([
            Constraint::Percentage(27),
            Constraint::Percentage(48),
            Constraint::Percentage(25),
        ])
        .split(area);
        [Some(columns[0]), Some(columns[1]), Some(columns[2])]
    }
}

fn pane(
    frame: &mut Frame,
    area: Rect,
    title: &str,
    focused: bool,
    lines: Vec<Line<'static>>,
    theme: Theme,
) {
    let block = Block::default()
        .title(title)
        .title_style(if focused {
            theme.focus()
        } else {
            theme.muted()
        })
        .borders(Borders::ALL)
        .border_style(if focused {
            theme.focus()
        } else {
            theme.muted()
        });
    frame.render_widget(
        Paragraph::new(lines)
            .block(block)
            .wrap(Wrap { trim: false }),
        area,
    );
}

fn select_file<'a>(
    workspace: &'a WorkspaceView,
    selected: Option<&str>,
) -> Option<&'a WorkspaceFileView> {
    workspace
        .files
        .iter()
        .find(|file| selected == Some(file.path.as_str()))
        .or_else(|| {
            workspace
                .files
                .iter()
                .find(|file| file.kind == WorkspaceFileKind::File)
        })
}

fn preview_lines(file: &WorkspaceFileView) -> Vec<Line<'static>> {
    let mut lines = vec![Line::from(clean(&file.path)), Line::from("")];
    match &file.preview {
        FilePreview::None => lines.push(Line::from("Directory; select a file to read.")),
        FilePreview::Binary => lines.push(Line::from("Binary preview unavailable.")),
        FilePreview::Text(value) => lines.extend(text_lines(value, false)),
        FilePreview::Truncated(value) => {
            lines.push(Line::from("Preview truncated to the bounded frontend cap."));
            lines.extend(text_lines(value, false));
        }
    }
    lines
}

fn output_lines(run: &WorkspaceRunView, width: usize) -> Vec<Line<'static>> {
    let mut lines = vec![
        Line::from(format!("$ bash -lc {}", clean(&run.script))),
        Line::from(""),
    ];
    lines.extend(output_stream("OUT", &run.stdout, width));
    lines.extend(output_stream("ERR", &run.stderr, width));
    lines
}

fn output_stream(prefix: &str, value: &str, width: usize) -> Vec<Line<'static>> {
    value
        .lines()
        .filter(|line| !line.is_empty())
        .take(128)
        .map(|line| Line::from(format!("{prefix} │ {}", truncate(&clean(line), width))))
        .collect()
}

fn text_lines(value: &str, keep_empty: bool) -> Vec<Line<'static>> {
    value
        .lines()
        .filter(|line| keep_empty || !line.is_empty())
        .take(128)
        .map(|line| Line::from(clean(line)))
        .collect()
}

fn change_marker(workspace: &WorkspaceView, path: &str) -> &'static str {
    workspace
        .changes
        .iter()
        .find(|change| change.path == path)
        .map(|change| diff_marker(&change.change))
        .unwrap_or(" ")
}

fn diff_marker(change: &DiffChange) -> &'static str {
    match change {
        DiffChange::Add => "+",
        DiffChange::Remove => "-",
        DiffChange::Modify => "~",
    }
}

fn optional(value: Option<&impl ToString>) -> String {
    value.map(ToString::to_string).unwrap_or_else(|| "—".into())
}

fn anchor_label(workspace: &WorkspaceView) -> String {
    workspace
        .anchor_commit
        .as_ref()
        .map(|id| format!("Commit {id}"))
        .or_else(|| {
            workspace
                .anchor_layer
                .as_ref()
                .map(|id| format!("initial Layer {id}"))
        })
        .unwrap_or_else(|| "invalid anchor".into())
}

fn duration(micros: u64) -> String {
    if micros >= 1_000_000 {
        format!("{:.2} s", micros as f64 / 1_000_000.0)
    } else if micros >= 1_000 {
        format!("{:.2} ms", micros as f64 / 1_000.0)
    } else {
        format!("{micros} µs")
    }
}

fn duration_or_dash(micros: u64) -> String {
    if micros == 0 {
        "—".into()
    } else {
        duration(micros)
    }
}

fn bytes(value: u64) -> String {
    if value >= 1_048_576 {
        format!("{:.1} MiB", value as f64 / 1_048_576.0)
    } else if value >= 1024 {
        format!("{:.1} KiB", value as f64 / 1024.0)
    } else {
        format!("{value} B")
    }
}

fn percent(part: u64, total: u64) -> String {
    if total == 0 {
        "—".into()
    } else {
        format!("{:.1}%", part as f64 * 100.0 / total as f64)
    }
}

fn clean(value: &str) -> String {
    value
        .chars()
        .map(|character| match character {
            '\n' | '\r' => ' ',
            value if value == '\t' || !value.is_control() => value,
            _ => '�',
        })
        .collect()
}

fn truncate(value: &str, width: usize) -> String {
    if value.chars().count() <= width {
        value.into()
    } else {
        value
            .chars()
            .take(width.saturating_sub(1))
            .chain(std::iter::once('…'))
            .collect()
    }
}

fn no_data() -> Vec<Line<'static>> {
    vec![Line::from("No data")]
}

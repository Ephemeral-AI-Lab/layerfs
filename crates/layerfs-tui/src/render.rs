use crate::{
    app::{ActivityTab, App, GraphTarget, Overlay, Route},
    explorer::{no_data as empty_lines, ActiveSubject, ExplorerMode},
    format::{action_labels, bytes, display_id, short_number, truncate},
    theme::Theme,
};
use layerfs_cli::{
    BranchId, BranchRelation, DiffChange, OperationView, ProjectRelation, ProjectSummary,
    SemanticAction, WorkspaceState, WorkspaceView,
};
use ratatui::{
    layout::{Alignment, Constraint, Flex, Layout, Rect},
    style::Style,
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Gauge, Padding, Paragraph, Wrap},
    Frame,
};

const MIN_WIDTH: u16 = 80;
const MIN_HEIGHT: u16 = 24;

pub(crate) fn draw(frame: &mut Frame, app: &App, theme: Theme) {
    let area = frame.area();
    if area.width < MIN_WIDTH || area.height < MIN_HEIGHT {
        frame.render_widget(
            Paragraph::new(format!(
                "LayerFS needs at least {MIN_WIDTH}x{MIN_HEIGHT}.\nCurrent terminal: {}x{}\n\nThe standalone mock CLI remains available:\n  layerfs query projects",
                area.width, area.height
            ))
            .alignment(Alignment::Center)
            .wrap(Wrap { trim: true }),
            area,
        );
        return;
    }

    let operation_height = if app.operation_drawer { 6 } else { 1 };
    let rows = Layout::vertical([
        Constraint::Length(2),
        Constraint::Length(1),
        Constraint::Min(8),
        Constraint::Length(operation_height),
        Constraint::Length(1),
    ])
    .split(area);
    header(frame, rows[0], app, theme);
    breadcrumb(frame, rows[1], app, theme);
    match &app.route {
        Route::Projects => projects(frame, rows[2], app, theme),
        Route::Project(_) => topology(frame, rows[2], app, theme),
        Route::Branch(_, branch_id) => branch(frame, rows[2], app, branch_id, theme),
        Route::Workspaces(_) => workspaces(frame, rows[2], app, theme),
        Route::Workspace(_) => crate::workspace::draw(frame, rows[2], app, theme),
        Route::Activity(tab) => activity(frame, rows[2], app, *tab, theme),
        Route::Diff(_) => diff(frame, rows[2], app, theme),
    }
    operation(frame, rows[3], app, theme);
    footer(frame, rows[4], app, theme);
    overlay(frame, app, theme);
}

fn header(frame: &mut Frame, area: Rect, app: &App, theme: Theme) {
    let selected = if matches!(app.route, Route::Projects) {
        app.selected_project
            .as_ref()
            .and_then(|id| app.projects.iter().find(|project| &project.id == id))
    } else {
        app.project.as_ref().map(|snapshot| &snapshot.project)
    };
    let project = selected
        .map(|project| format!("  selected {}", project.name))
        .unwrap_or_default();
    let authority =
        if selected.is_some_and(|project| project.relation == ProjectRelation::AuthorityUnknown) {
            "AUTH unavailable"
        } else {
            "AUTH connected"
        };
    let first = if area.width < 100 {
        format!(" LayerFS · SQLite V2 · LayerStackStore {authority} · BranchStore connected")
    } else {
        format!(" LayerFS  context SQLite-V2  {authority}  WORK connected{project}")
    };
    let second = if area.width < 100 {
        selected
            .map(compact_project_summary)
            .unwrap_or_else(|| " No LayerStack selected".into())
    } else {
        " LayerStackStore  authority   BranchStore  local work   observed from Stores".into()
    };
    frame.render_widget(
        Paragraph::new(vec![Line::styled(first, theme.title()), Line::from(second)]),
        area,
    );
}

fn breadcrumb(frame: &mut Frame, area: Rect, app: &App, theme: Theme) {
    if area.width < 100 {
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(
                    format!(" {} · {} ", route_name(app), compact_pane_name(app)),
                    theme.focus(),
                ),
                Span::styled("Tab pane · ? help", theme.muted()),
            ])),
            area,
        );
        return;
    }
    let current = match &app.route {
        Route::Projects => "Projects".into(),
        Route::Project(_) => format!(
            "Projects  /  {}  /  {}",
            project_name(app),
            app.explorer.mode.label()
        ),
        Route::Branch(_, _) => format!(
            "{}  /  {}",
            branch_breadcrumb(app),
            app.explorer.mode.label()
        ),
        Route::Workspaces(_) => format!("{}  /  Workspaces", project_name(app)),
        Route::Workspace(id) => format!(
            "{}  /  Workspaces  /  {}  /  {}",
            project_name(app),
            id,
            app.workspace_tab.label()
        ),
        Route::Activity(ActivityTab::Operations) => "Activity  /  Operations".into(),
        Route::Activity(ActivityTab::Storage) => "Activity  /  Storage".into(),
        Route::Diff(_) => "Diff".into(),
    };
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::raw(" "),
            Span::styled(current, theme.focus()),
            Span::styled(
                "    [1] Projects  [2] Topology  [3] Workspaces  [4] Activity",
                theme.muted(),
            ),
        ])),
        area,
    );
}

fn projects(frame: &mut Frame, area: Rect, app: &App, theme: Theme) {
    let selected = app
        .selected_project
        .as_ref()
        .and_then(|id| app.projects.iter().find(|project| &project.id == id));
    let panes = two_panes(area, app.compact_pane);
    if let Some(left) = panes.0 {
        let projects = filtered_projects(app);
        let selected_index = projects
            .iter()
            .position(|project| app.selected_project.as_ref() == Some(&project.id))
            .map(|index| index + 1);
        let mut lines = vec![Line::styled(
            if left.width < 100 {
                "NAME          AUTH  WORK          BRANCHES  WORKSPACES"
            } else {
                "NAME          AUTH  WORK PLACEMENT       BRANCHES       WORKSPACES"
            },
            theme.muted(),
        )];
        if projects.is_empty() {
            lines = vec![
                Line::from("No LayerStacks exist in this context."),
                Line::from(""),
                Line::from(": layerstack init --name <name> --empty"),
                Line::from(": layerstack init --name <name> <directory>"),
            ];
        }
        for project in projects {
            let selected = app.selected_project.as_ref() == Some(&project.id);
            let row = if left.width < 100 {
                format!(
                    "{}{:<13} L{:<3} {:<13} R{} L{} W{} W*{} W▶{} BUSY{} W!{}",
                    if selected { ">" } else { " " },
                    project.name,
                    project.authority_number,
                    project.relation,
                    project.remote_branches,
                    project.local_branches,
                    project.workspaces,
                    project.dirty_workspaces,
                    project.running_workspaces,
                    project.busy_workspaces,
                    project.retained_workspaces
                )
            } else {
                format!(
                    "{}{:<13} L{:<4} {:<20} REM {:<2} LOC {:<2} W {} W* {} W▶ {} BUSY {} W! {}",
                    if selected { ">" } else { " " },
                    project.name,
                    project.authority_number,
                    project.relation,
                    project.remote_branches,
                    project.local_branches,
                    project.workspaces,
                    project.dirty_workspaces,
                    project.running_workspaces,
                    project.busy_workspaces,
                    project.retained_workspaces
                )
            };
            lines.push(Line::styled(row, row_style(selected, theme)));
        }
        list_panel(
            frame,
            left,
            " PROJECTS ",
            app.focus == 0,
            selected_index,
            lines,
            theme,
        );
    }
    if let Some(right) = panes.1 {
        panel(
            frame,
            right,
            " SELECTED PROJECT ",
            app.focus == 1,
            selected.map(project_detail).unwrap_or_else(empty_lines),
            theme,
        );
    }
}

fn explorer_body(frame: &mut Frame, area: Rect, app: &App, theme: Theme) -> Rect {
    let rows = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Min(1),
    ])
    .split(area);
    crate::explorer::tabs(frame, rows[0], app, theme);
    crate::explorer::navigation(frame, rows[1], app, theme);
    rows[2]
}

fn topology(frame: &mut Frame, area: Rect, app: &App, theme: Theme) {
    let area = explorer_body(frame, area, app, theme);
    if app.explorer.mode != ExplorerMode::Topology {
        crate::explorer::draw(frame, area, app, theme);
        return;
    }
    let panes = three_panes(area, app.compact_pane);
    if let Some(left) = panes[0] {
        let layers = app
            .project
            .as_ref()
            .map(|snapshot| snapshot.layers.items.iter().rev().collect::<Vec<_>>())
            .unwrap_or_default();
        let selected_index = layers
            .iter()
            .position(|layer| app.selected_layer.as_ref() == Some(&layer.id));
        let lines = layers
            .into_iter()
            .map(|layer| {
                let active =
                    app.explorer.subject.as_ref() == Some(&ActiveSubject::Layer(layer.id.clone()));
                let selected = active && app.focus == 0;
                let source = layer
                    .source
                    .as_ref()
                    .map(|source| source_label(app, source))
                    .unwrap_or_else(|| "genesis".into());
                Line::styled(
                    truncate(
                        &format!(
                            "{}L{:<3} {:<9} {:<10} ← {}",
                            if selected {
                                ">"
                            } else if active {
                                "•"
                            } else {
                                " "
                            },
                            layer.number,
                            if layer.authority_head {
                                "AUTH HEAD"
                            } else {
                                "AUTH"
                            },
                            layer_work_label(app, layer),
                            source
                        ),
                        usize::from(left.width.saturating_sub(4)),
                    ),
                    row_style(selected, theme),
                )
            })
            .collect();
        list_panel(
            frame,
            left,
            " AUTHORITY LAYERS ",
            app.focus == 0,
            selected_index,
            lines,
            theme,
        );
    }
    if let Some(center) = panes[1] {
        let rows = app.graph_rows();
        let selected_index = graph_selected_index(app, &rows);
        let lines = graph_lines(app, rows, center.width.saturating_sub(4), theme);
        list_panel(
            frame,
            center,
            " AUTHORITY GRAPH + WORK OVERLAY ",
            app.focus == 1,
            selected_index,
            lines,
            theme,
        );
    }
    if let Some(right) = panes[2] {
        panel(
            frame,
            right,
            " INSPECTOR ",
            app.focus == 2,
            inspector(app),
            theme,
        );
    }
}

fn branch(frame: &mut Frame, area: Rect, app: &App, branch_id: &BranchId, theme: Theme) {
    let area = explorer_body(frame, area, app, theme);
    if app.explorer.mode != ExplorerMode::Topology {
        crate::explorer::draw(frame, area, app, theme);
        return;
    }
    let panes = two_panes(area, app.compact_pane);
    if let Some(left) = panes.0 {
        let rows = app.focused_branch_rows(branch_id);
        let selected_index = graph_selected_index(app, &rows);
        let lines = graph_lines(app, rows, left.width.saturating_sub(4), theme);
        list_panel(
            frame,
            left,
            " BRANCH HISTORY + MCTS ROLLOUT TREE ",
            app.focus == 0,
            selected_index,
            lines,
            theme,
        );
    }
    if let Some(right) = panes.1 {
        panel(
            frame,
            right,
            " INSPECTOR ",
            app.focus == 1,
            inspector(app),
            theme,
        );
    }
}

fn graph_lines(
    app: &App,
    rows: Vec<crate::app::GraphRow>,
    width: u16,
    theme: Theme,
) -> Vec<Line<'static>> {
    rows.into_iter()
        .filter(|row| search_match(app, &format!("{} {}", row.label, row.detail)))
        .map(|row| {
            let active = match (&app.explorer.subject, &row.target) {
                (Some(ActiveSubject::Branch(active)), GraphTarget::Branch(row)) => active == row,
                (
                    Some(ActiveSubject::Commit(active_branch, active_commit)),
                    GraphTarget::Commit(row_branch, row_commit),
                ) => active_branch == row_branch && active_commit == row_commit,
                _ => false,
            };
            let selected = active
                && match app.route {
                    Route::Project(_) => app.focus == 1,
                    Route::Branch(_, _) => app.focus == 0,
                    _ => false,
                };
            Line::styled(
                truncate(
                    &format!(
                        "{}{}├─ {} {}  {}",
                        if selected {
                            ">"
                        } else if active {
                            "•"
                        } else {
                            " "
                        },
                        "│ ".repeat(row.depth as usize),
                        row.marker,
                        row.label,
                        row.detail
                    ),
                    usize::from(width),
                ),
                row_style(selected, theme),
            )
        })
        .collect()
}

fn workspaces(frame: &mut Frame, area: Rect, app: &App, theme: Theme) {
    let selected = selected_workspace(app);
    let panes = two_panes(area, app.compact_pane);
    if let Some(left) = panes.0 {
        let workspaces = app
            .workspaces
            .workspaces
            .items
            .iter()
            .filter(|workspace| search_match(app, &workspace.id.to_string()))
            .collect::<Vec<_>>();
        let selected_index = workspaces
            .iter()
            .position(|workspace| app.selected_workspace.as_ref() == Some(&workspace.id));
        let lines = workspaces
            .into_iter()
            .map(|workspace| {
                let anchor = workspace
                    .anchor_commit
                    .as_ref()
                    .map(|commit| format!("C{}", short_number(commit.as_str())))
                    .or_else(|| {
                        workspace
                            .anchor_layer
                            .as_ref()
                            .map(|layer| format!("L{}", short_number(layer.as_str())))
                    })
                    .unwrap_or_else(|| "—".into());
                Line::styled(
                    format!(
                        "{}{}  {:<12} {:<11} {}@{}  Δ{}",
                        if app.selected_workspace.as_ref() == Some(&workspace.id) {
                            ">"
                        } else {
                            " "
                        },
                        workspace.id,
                        workspace.project_name,
                        workspace.state,
                        workspace.branch_name,
                        anchor,
                        workspace.changed_paths
                    ),
                    row_style(
                        app.selected_workspace.as_ref() == Some(&workspace.id),
                        theme,
                    ),
                )
            })
            .collect();
        list_panel(
            frame,
            left,
            " WORKSPACES ",
            app.focus == 0,
            selected_index,
            lines,
            theme,
        );
    }
    if let Some(right) = panes.1 {
        panel(
            frame,
            right,
            " WORKSPACE DETAIL + RETAINED OUTPUT ",
            app.focus == 1,
            selected.map(workspace_detail).unwrap_or_else(empty_lines),
            theme,
        );
    }
}

fn activity(frame: &mut Frame, area: Rect, app: &App, tab: ActivityTab, theme: Theme) {
    match tab {
        ActivityTab::Operations => operations(frame, area, app, theme),
        ActivityTab::Storage => storage(frame, area, app, theme),
    }
}

fn operations(frame: &mut Frame, area: Rect, app: &App, theme: Theme) {
    let selected = selected_operation(app);
    let panes = two_panes(area, app.compact_pane);
    if let Some(left) = panes.0 {
        let operations = app.activity.operations.items.iter().collect::<Vec<_>>();
        let selected_index = operations
            .iter()
            .position(|operation| app.selected_operation.as_ref() == Some(&operation.id));
        let lines = operations
            .into_iter()
            .map(|operation| {
                Line::styled(
                    format!(
                        "{}{}  {:<11} {:<12} {}/{}  {}",
                        if app.selected_operation.as_ref() == Some(&operation.id) {
                            ">"
                        } else {
                            " "
                        },
                        operation.id,
                        operation.state,
                        operation.phase,
                        operation.completed,
                        operation.total,
                        operation.title
                    ),
                    row_style(
                        app.selected_operation.as_ref() == Some(&operation.id),
                        theme,
                    ),
                )
            })
            .collect();
        list_panel(
            frame,
            left,
            " OPERATIONS ",
            app.focus == 0,
            selected_index,
            lines,
            theme,
        );
    }
    if let Some(right) = panes.1 {
        panel(
            frame,
            right,
            " RECEIPT + EVENTS ",
            app.focus == 1,
            selected.map(operation_detail).unwrap_or_else(empty_lines),
            theme,
        );
    }
}

fn storage(frame: &mut Frame, area: Rect, app: &App, theme: Theme) {
    let storage = &app.activity.storage;
    let panes = two_panes(area, app.compact_pane);
    if let Some(left) = panes.0 {
        panel(
            frame,
            left,
            " STORE INVENTORY ",
            app.focus == 0,
            vec![
                Line::from(format!(
                    " LayerStackStore    {}",
                    storage.layerstack_store_id
                )),
                Line::from(format!(" BranchStore        {}", storage.branch_store_id)),
                Line::from(""),
                Line::from(format!(" Projects           {}", storage.projects)),
                Line::from(format!(" Layers             {}", storage.layers)),
                Line::from(format!(
                    " Authority Branches {}",
                    storage.authority_branches
                )),
                Line::from(format!(" Remote Branches    {}", storage.remote_branches)),
                Line::from(format!(" Local Branches     {}", storage.local_branches)),
                Line::from(format!(" Replica roots      {}", storage.replica_roots)),
                Line::from(format!(" Reference scopes   {}", storage.reference_scopes)),
            ],
            theme,
        );
    }
    if let Some(right) = panes.1 {
        panel(
            frame,
            right,
            " EXPLICIT DEDUP ANALYSIS ",
            app.focus == 1,
            vec![
                Line::styled("Frontend snapshot · no ad-hoc Store reads", theme.success()),
                Line::from(""),
                Line::from(format!(" Shared objects    {}", storage.shared_objects)),
                Line::from(format!(
                    " Authority bytes   {}",
                    bytes(storage.authority_bytes)
                )),
                Line::from(format!(
                    " Branch bytes      {}",
                    bytes(storage.branch_bytes)
                )),
                Line::from(format!(
                    " Unique bytes      {}",
                    bytes(storage.unique_bytes)
                )),
                Line::from(""),
                Line::styled(
                    if storage.analysis_available {
                        "analysis available · : monitor analyze-dedup"
                    } else {
                        "analysis unavailable"
                    },
                    theme.info(),
                ),
            ],
            theme,
        );
    }
}

fn diff(frame: &mut Frame, area: Rect, app: &App, theme: Theme) {
    let panes = three_panes(area, app.compact_pane);
    let selected = app.diff.as_ref().and_then(|snapshot| {
        app.selected_diff_path.as_ref().and_then(|path| {
            snapshot
                .entries
                .items
                .iter()
                .find(|entry| &entry.path == path)
        })
    });
    if let Some(left) = panes[0] {
        if let Some(snapshot) = &app.diff {
            let visible = usize::from(left.height.saturating_sub(4)).max(1);
            let selected_index = snapshot
                .entries
                .items
                .iter()
                .position(|entry| app.selected_diff_path.as_ref() == Some(&entry.path))
                .unwrap_or(0);
            let scroll = selected_index.saturating_add(1).saturating_sub(visible);
            let end = (scroll + visible).min(snapshot.entries.items.len());
            let mut lines = vec![
                Line::styled(snapshot.title.clone(), theme.title()),
                Line::from(format!("{}  →  {}", snapshot.from, snapshot.to)),
            ];
            lines.extend(snapshot.entries.items[scroll..end].iter().map(|entry| {
                let marker = match entry.change {
                    DiffChange::Add => "+",
                    DiffChange::Remove => "-",
                    DiffChange::Modify => "~",
                };
                let selected = app.selected_diff_path.as_ref() == Some(&entry.path);
                let value = format!(
                    "{}{marker} {}  {}",
                    if selected { ">" } else { " " },
                    entry.path,
                    entry.aspects.join(", ")
                );
                Line::styled(
                    truncate(&value, usize::from(left.width.saturating_sub(4))),
                    row_style(selected, theme),
                )
            }));
            let more = if snapshot.entries.next.is_some() {
                " · more"
            } else {
                ""
            };
            panel(
                frame,
                left,
                &format!(
                    " CHANGED PATHS · paths {}–{end} of {}{more} ",
                    scroll + 1,
                    snapshot.entries.items.len()
                ),
                app.focus == 0,
                lines,
                theme,
            );
        } else {
            panel(
                frame,
                left,
                " CHANGED PATHS ",
                app.focus == 0,
                empty_lines(),
                theme,
            );
        }
    }
    if let Some(center) = panes[1] {
        let lines = selected
            .map(|entry| {
                vec![
                    Line::styled(" BEFORE", theme.error()),
                    Line::from(entry.before.as_deref().unwrap_or("∅").to_owned()),
                    Line::from(""),
                    Line::styled(" AFTER", theme.success()),
                    Line::from(entry.after.as_deref().unwrap_or("∅").to_owned()),
                ]
            })
            .unwrap_or_else(empty_lines);
        panel(frame, center, " CONTENT ", app.focus == 1, lines, theme);
    }
    if let Some(right) = panes[2] {
        let lines = selected
            .map(|entry| {
                vec![
                    Line::styled(entry.path.clone(), theme.title()),
                    Line::from(format!("Change       {:?}", entry.change)),
                    Line::from(format!("Aspects      {}", entry.aspects.join(", "))),
                    Line::from(""),
                    Line::from("Final-state diff only"),
                    Line::from("No tool-operation history"),
                ]
            })
            .unwrap_or_else(empty_lines);
        panel(frame, right, " DETAILS ", app.focus == 2, lines, theme);
    }
}

fn operation(frame: &mut Frame, area: Rect, app: &App, theme: Theme) {
    if app.operation_drawer {
        let mut lines = vec![Line::styled(
            format!(" operation: {}", app.operation_title),
            theme.title(),
        )];
        lines.extend(
            app.event_log
                .iter()
                .rev()
                .take(4)
                .rev()
                .map(|event| Line::from(format!("  {event}"))),
        );
        frame.render_widget(
            Paragraph::new(lines).block(Block::default().borders(Borders::TOP)),
            area,
        );
    } else if let Some((done, total, phase)) = &app.operation_progress {
        let ratio = if *total == 0 {
            0.0
        } else {
            *done as f64 / *total as f64
        };
        frame.render_widget(
            Gauge::default()
                .ratio(ratio.clamp(0.0, 1.0))
                .label(format!(
                    "{phase} {done}/{total}   [o] details  [x] interrupt"
                ))
                .gauge_style(theme.info()),
            area,
        );
    } else {
        frame.render_widget(
            Paragraph::new(format!(
                " operation: {}                                      [o] details",
                app.operation_title
            ))
            .style(theme.muted()),
            area,
        );
    }
}

fn footer(frame: &mut Frame, area: Rect, app: &App, theme: Theme) {
    let line = match app.overlay {
        Overlay::Command => {
            let mut command = app.command.clone();
            command.insert(app.command_cursor, '│');
            format!(": {command}   [Enter] run  [Tab] complete  [Esc] cancel")
        }
        Overlay::Search => format!("/{}   [Enter] apply  [Esc] cancel", app.search),
        _ if app.active_operation.is_some() => {
            " [x] interrupt operation  [o] details  [q] blocked while running".into()
        }
        _ => match app.route {
            Route::Projects => {
                " [Enter] open  [/] search  [r] refresh  [:] command  [?] help  [q] quit"
                    .into()
            }
            Route::Project(_) | Route::Branch(_, _) => match app.explorer.mode {
                ExplorerMode::Topology => " [Esc] Projects  [Tab] next pane  [j/k] move  [Enter] Files  [f] Fork  [?] Help".into(),
                ExplorerMode::Files => " [Esc] Topology  [Tab] next pane  [j/k] move/scroll  [Enter] open  [h/l] fold  [?] Help".into(),
                ExplorerMode::Changes => " [Esc] Topology  [Tab] next pane  [j/k] move/scroll  [Enter] Files  [?] Help".into(),
            },
            Route::Workspaces(_) => " [Esc] back  [j/k] move  [Enter] Workspace  [:] command  [?] Help".into(),
            Route::Workspace(_) => " [Esc] Workspaces  [Tab] next pane  [ previous tab · ] next tab  [x] Bash  [c] Commit".into(),
            Route::Activity(_) => " [Esc] back  [ previous view · ] next view  [Tab] pane  [j/k] move  [?] Help".into(),
            Route::Diff(_) => " [Esc] back  [Tab] next pane  [j/k] move  [:] command  [?] Help".into(),
        },
    };
    frame.render_widget(Paragraph::new(line).style(theme.muted()), area);
}

fn overlay(frame: &mut Frame, app: &App, theme: Theme) {
    if app.overlay == Overlay::Command && !app.completions.is_empty() {
        let area = popup(frame.area(), 58, 8, 2);
        let start = app
            .completion_selected
            .saturating_sub(5)
            .min(app.completions.len().saturating_sub(6));
        let lines = app
            .completions
            .iter()
            .enumerate()
            .skip(start)
            .take(6)
            .map(|(index, completion)| {
                Line::from(vec![
                    Span::styled(
                        format!(" {:<24}", completion.value),
                        if index == app.completion_selected {
                            theme.selected()
                        } else {
                            theme.info()
                        },
                    ),
                    Span::raw(completion.description.clone()),
                ])
            })
            .collect();
        frame.render_widget(Clear, area);
        panel(
            frame,
            area,
            " COMPLETIONS · Tab accepts ",
            true,
            lines,
            theme,
        );
    }
    if app.overlay == Overlay::Plan {
        let area = popup(frame.area(), 66, 18, 0);
        let lines = app
            .plan
            .as_ref()
            .map(|(_, plan)| {
                let mut lines = vec![
                    Line::styled(plan.title.clone(), theme.title()),
                    Line::from(plan.summary.clone()),
                    Line::from(""),
                ];
                lines.extend(
                    plan.fields
                        .iter()
                        .map(|field| Line::from(format!(" {:<18} {}", field.label, field.value))),
                );
                lines.push(Line::from(""));
                lines.extend(
                    plan.consequences
                        .iter()
                        .map(|value| Line::styled(format!(" • {value}"), theme.warning())),
                );
                lines.push(Line::from(""));
                lines.push(Line::styled(" Enter confirms · Esc cancels", theme.focus()));
                lines
            })
            .unwrap_or_else(empty_lines);
        frame.render_widget(Clear, area);
        panel(frame, area, " COMMAND PLAN ", true, lines, theme);
    }
    if app.overlay == Overlay::Help {
        let area = popup(frame.area(), 72, 20, 0);
        frame.render_widget(Clear, area);
        panel(
            frame,
            area,
            " HELP ",
            true,
            vec![
                Line::styled(
                    format!(
                        "Current view · {} · {}",
                        route_name(app),
                        compact_pane_name(app)
                    ),
                    theme.title(),
                ),
                Line::from("  1 Projects · 2 Topology · 3 Workspaces · 4 Activity"),
                Line::from("  Tab / Shift+Tab: next / previous pane"),
                Line::from("  j/k or arrows: move, scroll, fold · Enter: open"),
                Line::from("  Esc or Backspace: Files → Topology → Projects"),
                Line::from("  [ previous mode · ] next mode"),
                Line::from(""),
                Line::styled("Actions", theme.title()),
                Line::from("  f Fork · w Workspace · d Changes · r Refresh"),
                Line::from("  : command · / search · o operations · ? close Help"),
                Line::from(""),
                Line::styled("Signals", theme.title()),
                Line::from("  AUTH authority · WORK local view · REF reference · REP replica"),
                Line::from("  PULL +N behind · PUSH +N ahead · W* dirty · W▶ running"),
                Line::from(""),
                Line::from("  ? or Esc closes help"),
            ],
            theme,
        );
    }
    if let Some(error) = &app.error {
        let area = popup(frame.area(), 72, 6, 0);
        frame.render_widget(Clear, area);
        frame.render_widget(
            Paragraph::new(format!(" {error}\n\n Enter or Esc dismisses"))
                .style(theme.error())
                .block(Block::default().borders(Borders::ALL).title(" ERROR ")),
            area,
        );
    }
}

fn inspector(app: &App) -> Vec<Line<'static>> {
    if let Some(ActiveSubject::Layer(active)) = app.explorer.subject.as_ref() {
        if let Some(layer) = app.project.as_ref().and_then(|project| {
            project
                .layers
                .items
                .iter()
                .find(|layer| &layer.id == active)
        }) {
            return vec![
                Line::from(format!("Layer L{}", layer.number)),
                Line::from(format!("Id            {}", display_id(&layer.id))),
                Line::from(format!("Root          {}", layer.root)),
                Line::from(format!("Coverage      {}", layer.coverage)),
                Line::from(format!("Authority     {}", layer.authority)),
                Line::from(format!("Work          {}", layer.work)),
                Line::from(format!("Direct Branches {}", layer.direct_branches)),
                Line::from(""),
                Line::from(if layer.work {
                    "Actions: fork named Branch · diff"
                } else {
                    "Actions: pull through this Layer"
                }),
            ];
        }
    }
    match app.explorer.subject.as_ref() {
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
            .map(|branch| {
                let actions = action_labels(&branch.actions);
                vec![
                    Line::from(format!("Branch {}", branch.name)),
                    Line::from(format!("Id            {}", display_id(&branch.id))),
                    Line::from(format!("Relation      {}", branch.relation)),
                    Line::from(format!(
                        "Authority     {}",
                        optional_id(branch.authority_head.as_ref())
                    )),
                    Line::from(format!(
                        "Work          {}",
                        optional_id(branch.work_head.as_ref())
                    )),
                    Line::from(format!("Children      {}", branch.direct_children)),
                    Line::from(format!("Descendants   {}", branch.descendant_count)),
                    Line::from(format!("Workspaces    {}", branch.workspace_count)),
                    Line::from(format!(
                        "Replica roots {}",
                        optional_id(branch.remote_complete_through.as_ref())
                    )),
                    Line::from(format!(
                        "Visible closure {}",
                        if branch.visible_roots_complete {
                            "complete"
                        } else {
                            "parent-backed"
                        }
                    )),
                    Line::from(""),
                    Line::from(format!("Actions: {actions}")),
                ]
            })
            .unwrap_or_else(empty_lines),
        Some(ActiveSubject::Commit(branch_id, commit_id)) => app
            .project
            .as_ref()
            .and_then(|project| {
                project
                    .branches
                    .items
                    .iter()
                    .find(|branch| &branch.id == branch_id)
                    .and_then(|branch| {
                        branch
                            .commits
                            .iter()
                            .find(|commit| &commit.id == commit_id)
                            .map(|commit| (branch, commit))
                    })
            })
            .map(|(branch, commit)| {
                let mut actions = action_labels(&commit.actions);
                if !commit.actions.contains(&SemanticAction::Workspace) {
                    let reason = if !commit.work {
                        "pull before local use"
                    } else if matches!(
                        branch.relation,
                        BranchRelation::AuthorityAhead { .. } | BranchRelation::Diverged
                    ) {
                        "Workspace unavailable: HeadMoved"
                    } else if matches!(
                        branch.relation,
                        BranchRelation::LocalOnly
                            | BranchRelation::LocalCurrent
                            | BranchRelation::LocalPushAhead { .. }
                    ) && branch.work_head.as_ref() != Some(&commit.id)
                    {
                        "historical Commit: Fork first"
                    } else if !commit.workspaces.is_empty() {
                        "Workspace lease unavailable"
                    } else {
                        "Workspace unavailable: Fork first"
                    };
                    actions.push_str(" · ");
                    actions.push_str(reason);
                }
                vec![
                    Line::from(format!("Commit C{}", commit.number)),
                    Line::from(format!("Branch        {}", branch.name)),
                    Line::from(format!("CommitId      {}", display_id(&commit.id))),
                    Line::from(format!("Base Layer    {}", display_id(&commit.base_layer))),
                    Line::from(format!("Root          {}", commit.root)),
                    Line::from(format!("Child Branches {}", commit.child_branches)),
                    Line::from(format!("Workspaces    {}", commit.workspaces.len())),
                    Line::from(format!(
                        "Accepted      {}",
                        optional_id(commit.accepted_layer.as_ref())
                    )),
                    Line::from(""),
                    Line::from(format!("Actions: {actions}")),
                ]
            })
            .unwrap_or_else(empty_lines),
        Some(ActiveSubject::Layer(_)) => empty_lines(),
        None => app
            .project
            .as_ref()
            .map(|snapshot| project_detail(&snapshot.project))
            .unwrap_or_else(empty_lines),
    }
}

fn project_detail(project: &ProjectSummary) -> Vec<Line<'static>> {
    vec![
        Line::from(project.name.to_string()),
        Line::from(display_id(&project.id)),
        Line::from(""),
        Line::from(format!("Authority head      L{}", project.authority_number)),
        Line::from(format!("Work placement      {}", project.relation)),
        Line::from(format!(
            "Work boundary       {}",
            optional_id(project.work_boundary.as_ref())
        )),
        Line::from(format!(
            "Complete roots      {}/{}",
            project.complete_roots,
            project.work_number.unwrap_or(0)
        )),
        Line::from(""),
        Line::from(format!("Remote Branches     {}", project.remote_branches)),
        Line::from(format!("Local Branches      {}", project.local_branches)),
        Line::from(format!("Workspaces          {}", project.workspaces)),
        Line::from(format!("Dirty Workspaces    {}", project.dirty_workspaces)),
        Line::from(format!(
            "Running / Busy      {} / {}",
            project.running_workspaces, project.busy_workspaces
        )),
        Line::from(format!(
            "Retained W!         {}",
            project.retained_workspaces
        )),
        Line::from(format!("Observed            {}", project.observed)),
    ]
}

fn workspace_detail(workspace: &WorkspaceView) -> Vec<Line<'static>> {
    let anchor = match (&workspace.anchor_commit, &workspace.anchor_layer) {
        (Some(commit), Some(layer)) => format!("Commit {commit} vs current Layer {layer}"),
        (Some(commit), None) => format!("Commit {commit}"),
        (None, Some(layer)) => format!("initial Layer {layer}"),
        (None, None) => "invalid".into(),
    };
    let actions = match workspace.state {
        WorkspaceState::Clean => "exec · shell · commit · end",
        WorkspaceState::Dirty => "exec · shell · commit · end --discard",
        WorkspaceState::Running | WorkspaceState::Busy => "output · stop · inspect",
        WorkspaceState::HeadMoved => "conflicts · inspect delta · end --discard",
        WorkspaceState::ReadOnly => "inspect · output · end",
    };
    let mut lines = vec![
        Line::from(format!("Workspace     {}", workspace.id)),
        Line::from(format!("Project       {}", workspace.project_name)),
        Line::from(format!("Branch        {}", workspace.branch_name)),
        Line::from(format!("Relation      {}", workspace.branch_relation)),
        Line::from(format!("Anchor        {anchor}")),
        Line::from(format!("State         {}", workspace.state)),
        Line::from(format!("Projection    {}", workspace.projection)),
        Line::from(format!("Placement     {}", workspace.placement)),
        Line::from(format!("Mount         {}", truncate(&workspace.mount, 60))),
        Line::from(format!("Changed paths {}", workspace.changed_paths)),
        Line::from(format!("Output bytes  {}", workspace.output_bytes)),
        Line::from(format!("Actions       {actions}")),
    ];
    if !workspace.conflicts.is_empty() {
        lines.push(Line::from(""));
        lines.push(Line::from("Unresolved conflicts"));
        lines.extend(workspace.conflicts.iter().map(|conflict| {
            Line::from(format!(
                "  {}  {} · {}",
                conflict.id, conflict.path, conflict.kind
            ))
        }));
    }
    lines.push(Line::from(""));
    lines.push(Line::from("Retained output"));
    lines.extend(
        workspace
            .output
            .iter()
            .map(|line| Line::from(format!("  {line}"))),
    );
    lines
}

fn operation_detail(operation: &OperationView) -> Vec<Line<'static>> {
    let receipt = &operation.receipt;
    let mut lines = vec![
        Line::from(format!("Operation   {}", operation.id)),
        Line::from(format!("State       {}", operation.state)),
        Line::from(format!("Phase       {}", operation.phase)),
        Line::from(format!(
            "Progress    {}/{}",
            operation.completed, operation.total
        )),
        Line::from(format!("Elapsed     {} ms", receipt.elapsed_ms)),
        Line::from(""),
        Line::from(format!(
            "Facts       {} announced · {} missing · {} inserted",
            receipt.facts_announced, receipt.facts_missing, receipt.facts_inserted
        )),
        Line::from(format!(
            "Objects     {} announced · {} missing · {} sent",
            receipt.objects_announced, receipt.objects_missing, receipt.objects_sent
        )),
        Line::from(format!(
            "Admission   {} inserted · {} raced-existing",
            receipt.objects_inserted, receipt.objects_raced
        )),
        Line::from(""),
        Line::from("Events"),
    ];
    lines.extend(
        operation
            .events
            .iter()
            .map(|event| Line::from(format!("  {event}"))),
    );
    lines
}

pub(crate) fn panel(
    frame: &mut Frame,
    area: Rect,
    title: &str,
    focused: bool,
    lines: Vec<Line<'static>>,
    theme: Theme,
) {
    panel_at_scroll(frame, area, title, focused, 0, lines, theme);
}

pub(crate) fn list_panel(
    frame: &mut Frame,
    area: Rect,
    title: &str,
    focused: bool,
    selected: Option<usize>,
    lines: Vec<Line<'static>>,
    theme: Theme,
) {
    let visible = usize::from(area.height.saturating_sub(2)).max(1);
    let scroll = selected
        .map(|selected| selected.saturating_add(1).saturating_sub(visible))
        .unwrap_or(0);
    let end = (scroll + visible).min(lines.len());
    let title = if lines.len() > visible {
        format!("{title} {}–{end}/{} ", scroll + 1, lines.len())
    } else {
        title.to_owned()
    };
    panel_at_scroll(frame, area, &title, focused, scroll as u16, lines, theme);
}

pub(crate) fn panel_at_scroll(
    frame: &mut Frame,
    area: Rect,
    title: &str,
    focused: bool,
    scroll: u16,
    lines: Vec<Line<'static>>,
    theme: Theme,
) {
    let title = if focused {
        format!("▶{title}")
    } else {
        title.to_owned()
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(if focused {
            theme.focus()
        } else {
            theme.muted()
        })
        .title(Span::styled(
            title,
            if focused {
                theme.focus()
            } else {
                Style::default()
            },
        ))
        .padding(Padding::horizontal(1));
    frame.render_widget(Clear, area);
    frame.render_widget(
        Paragraph::new(lines)
            .scroll((scroll, 0))
            .wrap(Wrap { trim: false })
            .block(block),
        area,
    );
}

fn two_panes(area: Rect, compact: usize) -> (Option<Rect>, Option<Rect>) {
    if area.width < 100 {
        if compact % 2 == 0 {
            (Some(area), None)
        } else {
            (None, Some(area))
        }
    } else {
        let columns = Layout::horizontal([Constraint::Percentage(58), Constraint::Percentage(42)])
            .split(area);
        (Some(columns[0]), Some(columns[1]))
    }
}

pub(crate) fn three_panes(area: Rect, compact: usize) -> [Option<Rect>; 3] {
    if area.width < 100 {
        let mut result = [None, None, None];
        result[compact % 3] = Some(area);
        result
    } else if area.width < 140 {
        let columns = Layout::horizontal([Constraint::Percentage(38), Constraint::Percentage(62)])
            .split(area);
        match compact % 3 {
            2 => [Some(columns[0]), None, Some(columns[1])],
            _ => [Some(columns[0]), Some(columns[1]), None],
        }
    } else {
        let columns = Layout::horizontal([
            Constraint::Max(44),
            Constraint::Min(56),
            Constraint::Max(42),
        ])
        .split(area);
        [Some(columns[0]), Some(columns[1]), Some(columns[2])]
    }
}

fn popup(area: Rect, width: u16, height: u16, bottom_offset: u16) -> Rect {
    let width = width.min(area.width.saturating_sub(4));
    let height = height.min(area.height.saturating_sub(4));
    let vertical = Layout::vertical([Constraint::Length(height)])
        .flex(Flex::Center)
        .split(area);
    let horizontal = Layout::horizontal([Constraint::Length(width)])
        .flex(Flex::Center)
        .split(vertical[0]);
    let mut result = horizontal[0];
    result.y = result.y.saturating_sub(bottom_offset);
    result
}

fn selected_workspace(app: &App) -> Option<&WorkspaceView> {
    let id = app.selected_workspace.as_ref()?;
    app.workspaces
        .workspaces
        .items
        .iter()
        .find(|view| &view.id == id)
}

fn selected_operation(app: &App) -> Option<&OperationView> {
    let id = app.selected_operation.as_ref()?;
    app.activity
        .operations
        .items
        .iter()
        .find(|view| &view.id == id)
}

fn filtered_projects(app: &App) -> Vec<&ProjectSummary> {
    app.projects
        .iter()
        .filter(|project| search_match(app, project.name.as_str()))
        .collect()
}

fn search_match(app: &App, value: &str) -> bool {
    app.search.is_empty()
        || value
            .to_ascii_lowercase()
            .contains(&app.search.to_ascii_lowercase())
}

fn graph_selected_index(app: &App, rows: &[crate::app::GraphRow]) -> Option<usize> {
    rows.iter()
        .filter(|row| search_match(app, &format!("{} {}", row.label, row.detail)))
        .position(|row| app.selected_graph.as_ref() == Some(&row.target))
}

fn source_label(app: &App, source: &(BranchId, layerfs_cli::CommitId)) -> String {
    let branch = app.project.as_ref().and_then(|project| {
        project
            .branches
            .items
            .iter()
            .find(|branch| branch.id == source.0)
    });
    let Some(branch) = branch else {
        return format!("{}/{}", display_id(&source.0), display_id(&source.1));
    };
    let commit = branch
        .commits
        .iter()
        .find(|commit| commit.id == source.1)
        .map(|commit| format!("C{}", commit.number))
        .unwrap_or_else(|| display_id(&source.1));
    format!("{}/{commit}", branch.name)
}

fn layer_work_label(app: &App, layer: &layerfs_cli::LayerView) -> String {
    if !layer.work {
        return "WORK— NEW".into();
    }
    if !layer.work_boundary {
        return "WORK".into();
    }
    let mode = app
        .project
        .as_ref()
        .and_then(|project| match &project.project.relation {
            ProjectRelation::Current { mode } | ProjectRelation::PullBehind { mode, .. } => {
                Some(*mode)
            }
            _ => None,
        });
    mode.map(|mode| format!("WORK {mode}"))
        .unwrap_or_else(|| "WORK".into())
}

fn row_style(selected: bool, theme: Theme) -> Style {
    if selected {
        theme.selected()
    } else {
        Style::default()
    }
}

fn project_name(app: &App) -> String {
    app.project
        .as_ref()
        .map(|snapshot| snapshot.project.name.to_string())
        .unwrap_or_else(|| "all projects".into())
}

fn compact_project_summary(project: &ProjectSummary) -> String {
    let work = match &project.relation {
        ProjectRelation::Current { mode } => {
            format!("{mode}→L{} SYNC", project.work_number.unwrap_or_default())
        }
        ProjectRelation::PullBehind { mode, layers } => format!(
            "{mode}→L{} PULL+{layers}",
            project.work_number.unwrap_or_default()
        ),
        relation => relation.to_string(),
    };
    format!(
        " {} · AUTH L{} · WORK {} · REM{} LOC{} · W{} W*{}",
        project.name,
        project.authority_number,
        work,
        project.remote_branches,
        project.local_branches,
        project.workspaces,
        project.dirty_workspaces
    )
}

fn route_name(app: &App) -> &'static str {
    match app.route {
        Route::Projects => "Projects",
        Route::Project(_) | Route::Branch(_, _) => app.explorer.mode.label(),
        Route::Workspaces(_) => "Workspaces",
        Route::Workspace(_) => "Workspace",
        Route::Activity(ActivityTab::Operations) => "Operations",
        Route::Activity(ActivityTab::Storage) => "Storage",
        Route::Diff(_) => "Diff",
    }
}

fn compact_pane_name(app: &App) -> &'static str {
    match app.route {
        Route::Projects => ["List 1/2", "Detail 2/2"][app.compact_pane % 2],
        Route::Project(_) | Route::Branch(_, _) if app.explorer.mode != ExplorerMode::Topology => {
            ["Paths 1/3", "Content 2/3", "Inspector 3/3"][app.compact_pane % 3]
        }
        Route::Project(_) => ["Layers 1/3", "Graph 2/3", "Inspector 3/3"][app.compact_pane % 3],
        Route::Branch(_, _) => ["Tree 1/2", "Inspector 2/2"][app.compact_pane % 2],
        Route::Workspaces(_) => ["List 1/2", "Detail 2/2"][app.compact_pane % 2],
        Route::Workspace(_) => {
            ["Navigator 1/3", "Content 2/3", "Inspector 3/3"][app.compact_pane % 3]
        }
        Route::Activity(ActivityTab::Operations) => {
            ["Operations 1/2", "Receipt 2/2"][app.compact_pane % 2]
        }
        Route::Activity(ActivityTab::Storage) => {
            ["Inventory 1/2", "Dedup 2/2"][app.compact_pane % 2]
        }
        Route::Diff(_) => ["Paths 1/3", "Content 2/3", "Details 3/3"][app.compact_pane % 3],
    }
}

fn branch_breadcrumb(app: &App) -> String {
    let Some(snapshot) = app.project.as_ref() else {
        return "Branch".into();
    };
    let Some(target) = app.selected_graph.as_ref() else {
        return format!("{} > Branch", snapshot.project.name);
    };
    let (branch_id, selected_commit) = match target {
        GraphTarget::Branch(branch) => (branch, None),
        GraphTarget::Commit(branch, commit) => (branch, Some(commit)),
    };
    let mut parts = vec![snapshot.project.name.to_string()];
    append_branch_path(snapshot, branch_id, selected_commit, &mut parts);
    parts.join(" > ")
}

fn append_branch_path(
    snapshot: &layerfs_cli::ProjectSnapshot,
    branch_id: &BranchId,
    selected_commit: Option<&layerfs_cli::CommitId>,
    parts: &mut Vec<String>,
) {
    let Some(branch) = snapshot
        .branches
        .items
        .iter()
        .find(|branch| &branch.id == branch_id)
    else {
        return;
    };
    match &branch.origin {
        layerfs_cli::BranchOrigin::Layer(layer_id) => {
            if let Some(layer) = snapshot
                .layers
                .items
                .iter()
                .find(|layer| &layer.id == layer_id)
            {
                parts.push(format!("L{}", layer.number));
            }
        }
        layerfs_cli::BranchOrigin::Commit(parent, commit) => {
            append_branch_path(snapshot, parent, Some(commit), parts);
        }
    }
    let at = selected_commit.map_or_else(String::new, |id| {
        branch
            .commits
            .iter()
            .find(|commit| &commit.id == id)
            .map(|commit| format!("@C{}", commit.number))
            .unwrap_or_else(|| format!("@{}", display_id(id)))
    });
    parts.push(format!("{}{at}", branch.name));
}

fn optional_id<T: ToString>(value: Option<&T>) -> String {
    value.map(display_id).unwrap_or_else(|| "—".into())
}

#[cfg(test)]
mod tests;

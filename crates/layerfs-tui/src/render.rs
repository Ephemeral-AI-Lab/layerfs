use crate::{
    app::{
        Action, App, Focus, HistoryCategory, HistoryGroup, HistoryRow, LineageBase, LineageChild,
        RelationKind, StoreRow, TopologyEntry,
    },
    theme::{self, Palette},
};
use layerfs_cli::Fact;
use ratatui::{
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Padding, Paragraph},
    Frame,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ScreenLayout {
    pub(crate) stores: Rect,
    pub(crate) histories: Rect,
    pub(crate) lineage: Rect,
    pub(crate) details: Rect,
    pub(crate) command: Rect,
    pub(crate) footer: Rect,
}

pub(crate) fn layout(area: Rect) -> ScreenLayout {
    let rows = Layout::vertical([
        Constraint::Length(4),
        Constraint::Min(7),
        Constraint::Length(4),
        Constraint::Length(2),
    ])
    .split(area);
    let columns =
        Layout::horizontal([Constraint::Percentage(30), Constraint::Percentage(70)]).split(rows[1]);
    let left = Layout::vertical([Constraint::Percentage(34), Constraint::Percentage(66)])
        .split(columns[0]);
    let middle = Layout::vertical([Constraint::Percentage(34), Constraint::Percentage(66)])
        .split(columns[1]);
    ScreenLayout {
        stores: left[0],
        histories: left[1],
        details: middle[0],
        lineage: middle[1],
        command: rows[2],
        footer: rows[3],
    }
}

pub(crate) fn draw(frame: &mut Frame, app: &App) {
    let theme = theme::palette();
    frame.render_widget(
        Block::default().style(Style::default().bg(theme.canvas).fg(theme.text)),
        frame.area(),
    );
    if frame.area().width < 60 || frame.area().height < 16 {
        too_small(frame, &theme);
        return;
    }
    let areas = layout(frame.area());
    header(frame, frame.area(), app, &theme);
    stores(frame, areas.stores, app, &theme);
    histories(frame, areas.histories, app, &theme);
    lineage(frame, areas.lineage, app, &theme);
    details(frame, areas.details, app, &theme);
    command_input(frame, areas.command, app, &theme);
    completion_popup(frame, areas.command, app, &theme);
    footer(frame, areas.footer, app, &theme);
    if app.help_visible() {
        help_overlay(frame, &theme);
    }
}

fn too_small(frame: &mut Frame, theme: &Palette) {
    let message = format!(
        "LayerFS needs at least 60x16 cells for the TUI.\nCurrent terminal: {}x{}\n\nThe standalone CLI remains available:\n  layerfs status",
        frame.area().width,
        frame.area().height
    );
    frame.render_widget(
        Paragraph::new(message)
            .style(Style::default().fg(theme.text))
            .block(Block::default().padding(Padding::uniform(1))),
        frame.area(),
    );
}

fn header(frame: &mut Frame, area: Rect, app: &App, theme: &Palette) {
    let (name, location) = app
        .selected_store()
        .map(|store| (store.name.as_str(), store.location.as_str()))
        .unwrap_or(("not connected", ""));
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                " /\\ ",
                Style::default()
                    .fg(theme.layer)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                "LayerFS",
                Style::default().fg(theme.text).add_modifier(Modifier::BOLD),
            ),
            Span::styled("  ·  navigator  ", Style::default().fg(theme.muted)),
            Span::styled(
                name,
                Style::default().fg(theme.text).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                if location.is_empty() { "" } else { "  /  " },
                Style::default().fg(theme.muted),
            ),
            Span::raw(location),
        ]))
        .block(
            Block::default()
                .borders(Borders::BOTTOM)
                .border_style(Style::default().fg(theme.border))
                .padding(Padding::horizontal(1)),
        ),
        Rect::new(area.x, area.y, area.width, 4),
    );
}

fn stores(frame: &mut Frame, area: Rect, app: &App, theme: &Palette) {
    let rows = app.store_rows();
    let selected = app.selected_store_index();
    let mut lines = Vec::new();
    if rows.is_empty() {
        lines.push(Line::styled(
            "No connected LayerStore",
            Style::default().fg(theme.muted),
        ));
        lines.push(Line::from(""));
        lines.push(Line::styled(
            "Press / to connect or create one",
            Style::default().fg(theme.muted),
        ));
    } else {
        for (position, row) in rows.iter().enumerate() {
            let entry = &app.topology()[row.topology_index];
            let selected_row = selected == Some(row.topology_index);
            let connector = store_connector(&rows, position);
            let status = sync_summary(entry);
            let line = Line::from(vec![
                Span::styled(
                    if selected_row { "▸ " } else { "  " },
                    Style::default().fg(theme.focus),
                ),
                Span::styled(connector, Style::default().fg(theme.muted)),
                Span::styled("● ", Style::default().fg(theme.secondary)),
                Span::styled(
                    format!("{} · {}", role_label(&entry.role), entry.name),
                    row_style(selected_row, app.focus() == Focus::Stores, theme),
                ),
                Span::styled(status, Style::default().fg(theme.muted)),
            ]);
            lines.push(line);
        }
    }
    let block = panel_block(
        format!(" STORES · {} ", rows.len()),
        app.focus() == Focus::Stores,
        theme,
    );
    frame.render_widget(
        Paragraph::new(lines)
            .scroll((app.store_scroll(), 0))
            .block(block),
        area,
    );
}

fn histories(frame: &mut Frame, area: Rect, app: &App, theme: &Palette) {
    let title = app
        .selected_store()
        .map(|store| {
            let marker = if app
                .selected_store_history()
                .is_some_and(|group| group.has_more)
            {
                " …"
            } else {
                ""
            };
            format!(
                " HISTORIES · {} {}{} ",
                role_label(&store.role),
                store.name,
                marker
            )
        })
        .unwrap_or_else(|| " HISTORIES ".to_owned());
    let selected = app.selected_history();
    let rows = app.history_rows();
    let mut lines = Vec::new();
    if app.selected_store().is_none() {
        lines.push(Line::styled(
            "Connect a LayerStore to begin",
            Style::default().fg(theme.muted),
        ));
    } else {
        for (position, row) in rows.iter().enumerate() {
            let group_selected = selected == Some(*row);
            match row.fact {
                None => {
                    let expanded = app.history_category_expanded(row.category);
                    let count = app
                        .selected_store_history()
                        .map(|group| {
                            group
                                .facts
                                .iter()
                                .filter(|fact| history_container_matches(row.category, **fact))
                                .count()
                        })
                        .unwrap_or(0);
                    lines.push(Line::from(vec![
                        Span::styled(
                            if group_selected { "▸ " } else { "  " },
                            Style::default().fg(theme.focus),
                        ),
                        Span::styled(
                            if expanded { "▾ " } else { "▸ " },
                            Style::default().fg(theme.muted),
                        ),
                        Span::styled(
                            format!("{} ({count})", row.category.label()),
                            row_style(group_selected, app.focus() == Focus::Histories, theme),
                        ),
                    ]));
                }
                Some(index) => {
                    let Some(group) = app.selected_store_history() else {
                        continue;
                    };
                    let Some(fact) = group.facts.get(index).copied() else {
                        continue;
                    };
                    let line = Line::from(vec![
                        Span::raw(fact_indent(*row, fact)),
                        Span::styled(
                            fact_symbol(fact),
                            Style::default().fg(if group_selected {
                                theme.focus
                            } else {
                                theme.secondary
                            }),
                        ),
                        Span::styled(
                            format!(
                                " {}{}{}",
                                fact_label(fact),
                                if row.number > 0 {
                                    format!(" ({})", row.number)
                                } else {
                                    String::new()
                                },
                                if row.head { " · head" } else { "" }
                            ),
                            row_style(group_selected, app.focus() == Focus::Histories, theme),
                        ),
                    ]);
                    lines.push(line);
                }
            }
            let _ = position;
        }
        if lines.is_empty() {
            lines.push(Line::styled(
                "No local histories",
                Style::default().fg(theme.muted),
            ));
        }
    }
    frame.render_widget(
        Paragraph::new(lines)
            .scroll((app.history_scroll(), 0))
            .block(panel_block(title, app.focus() == Focus::Histories, theme)),
        area,
    );
}

fn lineage(frame: &mut Frame, area: Rect, app: &App, theme: &Palette) {
    let block = panel_block(
        " SELECTED LINEAGE ".to_owned(),
        app.focus() == Focus::Lineage,
        theme,
    );
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let Some(group) = app.selected_store_history() else {
        frame.render_widget(
            Paragraph::new("Select a layer, stack, branch, or commit")
                .style(Style::default().fg(theme.muted)),
            inner,
        );
        return;
    };
    let mut lines = vec![lineage_lane_line(
        group,
        &app.lineage_visual_rows(),
        app.selected_history().and_then(|row| row.fact),
        app.lineage_base(),
        app,
        theme,
    )];
    let child_lanes = app.lineage_child_lanes();
    if !child_lanes.is_empty() {
        for (index, child) in child_lanes.iter().copied().enumerate() {
            if index + 1 == child_lanes.len() {
                lines.push(lineage_edge_line(group, child, app, theme));
            } else {
                lines.push(Line::styled("  │", Style::default().fg(theme.muted)));
            }
            lines.push(lineage_child_header(child, theme));
            let rows = app.lineage_visual_rows_for_child(child);
            lines.push(lineage_lane_line(
                group,
                &rows,
                Some(child.selected),
                app.lineage_base_for_child(child),
                app,
                theme,
            ));
            if index + 1 == child_lanes.len() {
                lines.extend(lineage_relation_lines(group, app, theme));
            }
        }
    } else {
        lines.extend(lineage_relation_lines(group, app, theme));
    }
    frame.render_widget(
        Paragraph::new(lines).scroll((0, app.lineage_scroll())),
        inner,
    );
}

fn lineage_lane_line(
    group: &HistoryGroup,
    rows: &[crate::app::LineageRow],
    selected_index: Option<usize>,
    base: Option<LineageBase>,
    app: &App,
    theme: &Palette,
) -> Line<'static> {
    let mut line = Line::default();
    if let Some(base) = base.and_then(|base| {
        group
            .facts
            .get(base.fact_index)
            .copied()
            .and_then(|fact| lineage_node_name(fact, base.number))
    }) {
        line.spans
            .push(Span::styled(base, Style::default().fg(theme.secondary)));
        line.spans.push(Span::styled(
            " ──base──▶ ",
            Style::default().fg(theme.focus),
        ));
    }
    let mut position = 0;
    for row in rows.iter().filter(|row| {
        group
            .facts
            .get(row.fact_index)
            .is_some_and(|fact| lineage_node_name(*fact, row.number).is_some())
    }) {
        let Some(fact) = group.facts.get(row.fact_index).copied() else {
            continue;
        };
        let Some(node) = lineage_node_name(fact, row.number) else {
            continue;
        };
        if position > 0 {
            line.spans
                .push(Span::styled("  →  ", Style::default().fg(theme.muted)));
        }
        line.spans.push(Span::styled(
            node,
            row_style(
                selected_index == Some(row.fact_index),
                app.focus() == Focus::Lineage,
                theme,
            ),
        ));
        position += 1;
    }
    if line.spans.is_empty() {
        line.spans.push(Span::styled(
            "(no linked records)",
            Style::default().fg(theme.muted),
        ));
    }
    line
}

fn lineage_edge_line(
    group: &HistoryGroup,
    child: LineageChild,
    _app: &App,
    theme: &Palette,
) -> Line<'static> {
    let source = group
        .facts
        .get(child.source)
        .map(|fact| relation_fact_label(*fact))
        .unwrap_or_else(|| "source".to_owned());
    let target = group
        .facts
        .get(child.container)
        .map(|fact| relation_fact_label(*fact))
        .unwrap_or_else(|| "target".to_owned());
    Line::from(vec![
        Span::styled("  ╰─ ", Style::default().fg(theme.muted)),
        Span::styled(source, Style::default().fg(theme.secondary)),
        Span::styled(" ──▶ ", Style::default().fg(theme.focus)),
        Span::styled(target, Style::default().fg(theme.text)),
    ])
}

fn lineage_child_header(child: LineageChild, theme: &Palette) -> Line<'static> {
    let title = match child.category {
        HistoryCategory::Stacks => "STACK HISTORY",
        HistoryCategory::Branches => "BRANCH HISTORY",
        HistoryCategory::Layers => "LAYER HISTORY",
    };
    Line::styled(format!("  {title}"), Style::default().fg(theme.muted))
}

fn lineage_relation_lines(group: &HistoryGroup, app: &App, theme: &Palette) -> Vec<Line<'static>> {
    let relations = app.lineage_relations();
    if relations.is_empty() {
        return vec![
            Line::styled("  RELATIONSHIP SEARCH", Style::default().fg(theme.muted)),
            Line::styled(
                "  (no cross-type relationships)",
                Style::default().fg(theme.muted),
            ),
        ];
    }
    let mut lines = vec![Line::styled(
        "  RELATIONSHIP SEARCH",
        Style::default().fg(theme.muted),
    )];
    lines.extend(relations.iter().enumerate().map(|(index, relation)| {
        let source = group
            .facts
            .get(relation.source)
            .map(|fact| relation_fact_label(*fact))
            .unwrap_or_else(|| "source".to_owned());
        let target = group
            .facts
            .get(relation.target)
            .map(|fact| relation_fact_label(*fact))
            .unwrap_or_else(|| "target".to_owned());
        let (left, right, arrow) = match relation.kind {
            RelationKind::CreatedBy => (source, target, "◀"),
            RelationKind::Base => (target, source, "▶"),
            RelationKind::Produces | RelationKind::Instantiates => (source, target, "▶"),
        };
        Line::from(vec![
            Span::styled(
                if app.lineage_relation_focus() && index == app.lineage_relation_offset() {
                    "  ▸ "
                } else {
                    "    "
                },
                Style::default().fg(theme.focus),
            ),
            Span::styled("╰─ ", Style::default().fg(theme.muted)),
            Span::styled(left, Style::default().fg(theme.secondary)),
            Span::styled(
                format!(" ──{}──{arrow} ", relation.kind.label()),
                Style::default().fg(theme.focus),
            ),
            Span::styled(
                right,
                row_style(
                    app.lineage_relation_focus() && index == app.lineage_relation_offset(),
                    app.focus() == Focus::Lineage,
                    theme,
                ),
            ),
        ])
    }));
    lines
}

fn relation_fact_label(fact: Fact) -> String {
    let kind = match fact {
        Fact::Layer(_) => "layer",
        Fact::Stack(_) => "stack",
        Fact::LayerHistory(_) => "layer history",
        Fact::StackHistory(_) => "stack history",
        Fact::Branch(_) => "branch",
        Fact::Commit(_) => "commit",
        Fact::AddResult(_) => "result",
    };
    format!("{kind} {}", short_id(&fact.id()))
}

fn lineage_node_name(fact: Fact, number: usize) -> Option<String> {
    let node = match fact {
        Fact::Layer(_) => "layer",
        Fact::Stack(_) => "stack",
        Fact::Commit(_) => "commit",
        _ => return None,
    };
    Some(format!("{node}({number})"))
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum LineageHit {
    Node(usize),
    Relation(usize),
}

pub(crate) fn lineage_hit_at(area: Rect, column: u16, row: u16, app: &App) -> Option<LineageHit> {
    let inner = panel_block("".to_owned(), false, &theme::palette()).inner(area);
    if column < inner.x || row < inner.y {
        return None;
    }
    let group = app.selected_store_history()?;
    let local_row = usize::from(row - inner.y);
    let child_lanes = app.lineage_child_lanes();
    let (rows, relation_start, node_kind, prefix_width) = if local_row == 0 {
        (
            app.lineage_visual_rows(),
            0,
            0,
            app.lineage_base_prefix_width(),
        )
    } else if !child_lanes.is_empty() && local_row <= child_lanes.len() * 3 {
        if local_row % 3 == 0 {
            let child = child_lanes[local_row / 3 - 1];
            (
                app.lineage_visual_rows_for_child(child),
                0,
                1,
                app.lineage_base_prefix_width_for_child(child),
            )
        } else {
            return None;
        }
    } else if !child_lanes.is_empty() && local_row >= child_lanes.len() * 3 + 2 {
        (Vec::new(), child_lanes.len() * 3 + 2, 2, 0)
    } else if child_lanes.is_empty() && local_row >= 2 {
        (Vec::new(), 2, 2, 0)
    } else {
        return None;
    };
    if node_kind == 2 {
        let relation = local_row.saturating_sub(relation_start);
        return (relation < app.lineage_relations().len())
            .then_some(LineageHit::Relation(relation));
    }
    let x = usize::from(column - inner.x) + usize::from(app.lineage_scroll());
    let mut offset: usize = prefix_width;
    for lineage_row in rows {
        let fact = group.facts.get(lineage_row.fact_index).copied()?;
        let Some(node) = lineage_node_name(fact, lineage_row.number) else {
            continue;
        };
        let width = Line::raw(node.as_str()).width();
        if (offset..offset.saturating_add(width)).contains(&x) {
            return Some(LineageHit::Node(lineage_row.fact_index));
        }
        offset = offset.saturating_add(width + 5);
    }
    None
}

#[allow(dead_code)]
pub(crate) fn lineage_node_at(area: Rect, column: u16, row: u16, app: &App) -> Option<usize> {
    match lineage_hit_at(area, column, row, app) {
        Some(LineageHit::Node(index)) => Some(index),
        _ => None,
    }
}

#[allow(dead_code)]
fn hover_details(
    fact: Fact,
    group: &HistoryGroup,
    app: &App,
    theme: &Palette,
) -> Vec<Line<'static>> {
    match fact {
        Fact::Layer(layer) => layer_hover_details(Fact::Layer(layer), app, theme),
        Fact::Stack(stack) => stack_hover_details(Fact::Stack(stack), app, theme),
        Fact::Branch(branch) => branch_hover_details(Fact::Branch(branch), group, theme),
        Fact::Commit(commit) => commit_hover_details(Fact::Commit(commit), theme),
        _ => Vec::new(),
    }
}

#[allow(dead_code)]
fn layer_hover_details(fact: Fact, app: &App, theme: &Palette) -> Vec<Line<'static>> {
    let Fact::Layer(layer) = fact else {
        return Vec::new();
    };
    let mut lines = vec![Line::styled(
        "LAYER RELATIONSHIPS",
        Style::default().add_modifier(Modifier::BOLD),
    )];
    let parent = layer
        .parent_id
        .map(|id| format!("layer {}", short_id(id.to_bytes().as_slice())))
        .unwrap_or_else(|| "(genesis)".to_owned());
    lines.push(relation_line("created from", parent, theme));
    let created_by = app
        .history_groups()
        .iter()
        .flat_map(|group| group.facts.iter())
        .filter_map(|fact| match fact {
            Fact::AddResult(result)
                if result.result_id.as_slice() == layer.id.to_bytes().as_slice() =>
            {
                let kind = match result.source_id.as_slice().first() {
                    Some(0x11) => "branch",
                    Some(0x22) => "stack",
                    _ => "source",
                };
                Some(format!("{kind} {}", short_id(result.source_id.as_slice())))
            }
            _ => None,
        })
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    lines.push(relation_line(
        "created by",
        values_or_none(created_by),
        theme,
    ));
    let mut children = Vec::new();
    for group in app.history_groups() {
        for fact in &group.facts {
            match *fact {
                Fact::StackHistory(history) if history.base_layer_id == layer.id => {
                    children.push(format!(
                        "stack {}",
                        short_id(history.head_stack_id.to_bytes().as_slice())
                    ));
                }
                Fact::Branch(branch)
                    if branch.base_id.as_slice() == layer.id.to_bytes().as_slice() =>
                {
                    children.push(format!(
                        "branch {}",
                        short_id(branch.id.to_bytes().as_slice())
                    ));
                }
                _ => {}
            }
        }
    }
    children.sort();
    children.dedup();
    lines.push(relation_line(
        "instantiated",
        values_or_none(children),
        theme,
    ));
    lines
}

#[allow(dead_code)]
fn stack_hover_details(fact: Fact, app: &App, theme: &Palette) -> Vec<Line<'static>> {
    let Fact::Stack(stack) = fact else {
        return Vec::new();
    };
    let mut lines = vec![Line::styled(
        "STACK RELATIONSHIPS",
        Style::default().add_modifier(Modifier::BOLD),
    )];
    let history = app.history_groups().iter().find_map(|group| {
        group.facts.iter().find_map(|fact| match *fact {
            Fact::StackHistory(history) if history.id == stack.history_id => Some(history),
            _ => None,
        })
    });
    let created_from = history
        .map(|history| {
            format!(
                "layer {}",
                short_id(history.base_layer_id.to_bytes().as_slice())
            )
        })
        .unwrap_or_else(|| "(unknown layer)".to_owned());
    lines.push(relation_line("created from", created_from, theme));
    lines.push(relation_line(
        "parent stack",
        stack
            .parent_id
            .map(|id| format!("stack {}", short_id(id.to_bytes().as_slice())))
            .unwrap_or_else(|| "(genesis)".to_owned()),
        theme,
    ));
    let branches = app
        .history_groups()
        .iter()
        .flat_map(|group| group.facts.iter())
        .filter_map(|fact| match *fact {
            Fact::Branch(branch) if branch.base_id.as_slice() == stack.id.to_bytes().as_slice() => {
                Some(format!(
                    "branch {}",
                    short_id(branch.id.to_bytes().as_slice())
                ))
            }
            _ => None,
        })
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    lines.push(relation_line(
        "instantiated",
        values_or_none(branches),
        theme,
    ));
    lines
}

#[allow(dead_code)]
fn branch_hover_details(fact: Fact, group: &HistoryGroup, theme: &Palette) -> Vec<Line<'static>> {
    let Fact::Branch(branch) = fact else {
        return Vec::new();
    };
    let mut lines = vec![Line::styled(
        "BRANCH RELATIONSHIPS",
        Style::default().add_modifier(Modifier::BOLD),
    )];
    let commit_ids = commits_for_branch(group, Fact::Branch(branch));
    let commit_count = commit_ids.len();
    lines.push(relation_line(
        "created from",
        match branch.base_id.as_slice().first() {
            Some(0x32) => format!("layer {}", short_id(branch.base_id.as_slice())),
            Some(0x22) => format!("stack {}", short_id(branch.base_id.as_slice())),
            _ => "(unknown base)".to_owned(),
        },
        theme,
    ));
    lines.push(Line::styled(
        "COMMITS",
        Style::default().add_modifier(Modifier::BOLD),
    ));
    if commit_ids.is_empty() {
        lines.push(Line::styled("  (none)", Style::default().fg(theme.muted)));
    } else {
        for (position, id) in commit_ids.iter().enumerate() {
            lines.push(Line::styled(
                format!(
                    "  commit({}) {}",
                    commit_count.saturating_sub(position),
                    short_id(id.as_slice())
                ),
                Style::default().fg(theme.text),
            ));
        }
    }
    let selected_ids = commits_for_branch(group, Fact::Branch(branch));
    let subbranches = group
        .facts
        .iter()
        .filter_map(|fact| match *fact {
            Fact::Branch(candidate) if candidate.id != branch.id => {
                let candidate_ids = commits_for_branch(group, Fact::Branch(candidate));
                let shared = candidate_ids
                    .iter()
                    .find(|id| selected_ids.iter().any(|selected| selected == *id))?;
                Some(format!(
                    "branch {} via commit {}",
                    short_id(candidate.id.to_bytes().as_slice()),
                    short_id(shared)
                ))
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    lines.push(relation_line(
        "possible subbranches",
        if subbranches.is_empty() {
            "(none inferred)".to_owned()
        } else {
            format!("{} (inferred)", subbranches.join(", "))
        },
        theme,
    ));
    lines.push(Line::styled(
        "  exact parent branch is not stored in BranchRecord",
        Style::default().fg(theme.muted),
    ));
    lines
}

#[allow(dead_code)]
fn commit_hover_details(fact: Fact, theme: &Palette) -> Vec<Line<'static>> {
    let Fact::Commit(commit) = fact else {
        return Vec::new();
    };
    vec![
        Line::styled(
            "COMMIT RELATIONSHIPS",
            Style::default().add_modifier(Modifier::BOLD),
        ),
        relation_line(
            "parent",
            commit
                .parent_id
                .map(|id| format!("commit {}", short_id(id.to_bytes().as_slice())))
                .unwrap_or_else(|| "(root)".to_owned()),
            theme,
        ),
        relation_line(
            "merge parent",
            commit
                .merge_parent_id
                .map(|id| format!("commit {}", short_id(id.to_bytes().as_slice())))
                .unwrap_or_else(|| "(none)".to_owned()),
            theme,
        ),
    ]
}

#[allow(dead_code)]
fn commits_for_branch(group: &HistoryGroup, fact: Fact) -> Vec<Vec<u8>> {
    let Fact::Branch(branch) = fact else {
        return Vec::new();
    };
    let commits = group
        .facts
        .iter()
        .filter_map(|fact| match *fact {
            Fact::Commit(commit) => Some((commit.id.to_bytes().to_vec(), commit.parent_id)),
            _ => None,
        })
        .collect::<std::collections::BTreeMap<_, _>>();
    let mut ids = Vec::new();
    let mut current = Some(branch.head_commit_id.to_bytes().to_vec());
    let mut seen = std::collections::BTreeSet::new();
    while let Some(id) = current {
        if !seen.insert(id.clone()) {
            break;
        }
        let Some(parent) = commits.get(&id).copied() else {
            break;
        };
        ids.push(id);
        current = parent.map(|id| id.to_bytes().to_vec());
    }
    ids
}

#[allow(dead_code)]
fn relation_line(label: &str, value: String, theme: &Palette) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("{label:<22}"), Style::default().fg(theme.muted)),
        Span::styled(value, Style::default().fg(theme.text)),
    ])
}

#[allow(dead_code)]
fn values_or_none(values: Vec<String>) -> String {
    if values.is_empty() {
        "(none)".to_owned()
    } else {
        values.join(", ")
    }
}

fn short_id(bytes: &[u8]) -> String {
    let id = hex(bytes);
    if id.len() > 14 {
        format!("{}…{}", &id[..6], &id[id.len() - 4..])
    } else {
        id
    }
}

fn details(frame: &mut Frame, area: Rect, app: &App, theme: &Palette) {
    let Some(store) = app.selected_store() else {
        frame.render_widget(
            Paragraph::new("No store selected")
                .style(Style::default().fg(theme.muted))
                .block(panel_block(" DETAILS ".to_owned(), false, theme)),
            area,
        );
        return;
    };
    let parent = store
        .parent
        .as_deref()
        .and_then(|location| {
            app.topology()
                .iter()
                .find(|entry| entry.location == location)
        })
        .map(|entry| format!("{} · {}", role_label(&entry.role), entry.name))
        .unwrap_or_else(|| "-".to_owned());
    let selected = app.active_lineage_fact();
    let mut lines = vec![Line::styled(
        format!("{} · {}", role_label(&store.role), store.name),
        Style::default().add_modifier(Modifier::BOLD),
    )];
    lines.extend([
        Line::styled(
            if store.role == "layerstore" {
                "AUTHORITY"
            } else {
                "CONNECTED"
            },
            Style::default().fg(theme.secondary),
        ),
        Line::from(vec![
            Span::styled("Parent ", Style::default().fg(theme.muted)),
            Span::raw(parent.clone()),
        ]),
        Line::from(vec![
            Span::styled("Route  ", Style::default().fg(theme.muted)),
            Span::raw(route_label(app, store)),
        ]),
        Line::from(vec![
            Span::styled("State  ", Style::default().fg(theme.muted)),
            Span::raw(if store.active { "active" } else { "connected" }),
        ]),
        Line::from(""),
    ]);
    if let Some(fact) = selected {
        lines.push(Line::styled(
            fact_title(fact),
            Style::default().add_modifier(Modifier::BOLD),
        ));
        lines.push(Line::styled(
            format!("id     {}", hex(&fact.id())),
            Style::default().fg(theme.muted),
        ));
    } else {
        lines.push(Line::styled(
            "Select a concrete history record",
            Style::default().fg(theme.muted),
        ));
    }
    lines.push(Line::from(""));
    lines.push(Line::styled("OPERATIONS", Style::default().fg(theme.muted)));
    let actions = app.actions();
    if actions.is_empty() {
        lines.push(Line::styled(
            "  none for this selection",
            Style::default().fg(theme.muted),
        ));
    } else {
        for (index, action) in actions.iter().copied().enumerate() {
            let selected_action = index == app.selected_action_index();
            lines.push(Line::styled(
                format!(
                    "  [{}] {} {}",
                    action.key(),
                    action.label(),
                    action_target(app, action)
                ),
                row_style(selected_action, app.focus() == Focus::Details, theme),
            ));
        }
        lines.push(Line::styled(
            "  ↑↓ choose · Enter prepare command",
            Style::default().fg(theme.muted),
        ));
    }
    frame.render_widget(
        Paragraph::new(lines)
            .scroll((app.details_scroll(), 0))
            .block(panel_block(
                " DETAILS ".to_owned(),
                app.focus() == Focus::Details,
                theme,
            )),
        area,
    );
}

fn action_target(app: &App, action: Action) -> String {
    let Some(store) = app.selected_store() else {
        return String::new();
    };
    let target = if store.role == "branchstore" {
        store
            .parent
            .as_deref()
            .and_then(|location| {
                app.topology()
                    .iter()
                    .find(|entry| entry.location == location)
            })
            .map(|entry| entry.name.as_str())
            .unwrap_or("parent")
    } else {
        app.topology()
            .iter()
            .find(|entry| entry.role == "layerstore")
            .map(|entry| entry.name.as_str())
            .unwrap_or("LayerStore")
    };
    let arrow = match action {
        Action::PullLayer | Action::PullStack | Action::PullBranch | Action::PullCommits => "←",
        Action::PushStack | Action::PushBranch => "→",
    };
    format!("{arrow} {target}")
}

fn action_offset(app: &App) -> usize {
    if app.active_lineage_fact().is_some() {
        10
    } else {
        9
    }
}

pub(crate) fn action_row_at(area: Rect, row: u16, app: &App) -> Option<usize> {
    let content = area.y.saturating_add(2);
    let line = usize::from(row.saturating_sub(content)) + usize::from(app.details_scroll());
    let index = line.checked_sub(action_offset(app))?;
    (index < app.actions().len()).then_some(index)
}

pub(crate) fn store_row_at(area: Rect, row: u16, app: &App) -> Option<usize> {
    let line =
        usize::from(row.saturating_sub(area.y.saturating_add(2))) + usize::from(app.store_scroll());
    let rows = app.store_rows();
    rows.get(line).map(|_| line)
}

pub(crate) fn history_row_at(area: Rect, row: u16, app: &App) -> Option<usize> {
    let line = usize::from(row.saturating_sub(area.y.saturating_add(2)))
        + usize::from(app.history_scroll());
    (line < app.history_rows().len()).then_some(line)
}

fn panel_block<'a>(title: String, focused: bool, theme: &Palette) -> Block<'a> {
    Block::bordered()
        .title(title)
        .border_style(if focused {
            Style::default().fg(theme.focus)
        } else {
            Style::default().fg(theme.border)
        })
        .padding(Padding::uniform(1))
}

fn row_style(selected: bool, focused: bool, theme: &Palette) -> Style {
    if selected && focused {
        Style::default()
            .fg(theme.focus)
            .add_modifier(Modifier::BOLD | Modifier::REVERSED)
    } else if selected {
        Style::default()
            .fg(theme.focus)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme.text)
    }
}

fn store_connector(rows: &[StoreRow], position: usize) -> String {
    let row = rows[position];
    if row.depth == 0 {
        return String::new();
    }
    let mut prefix = String::new();
    for depth in 1..=row.depth {
        let ancestor = (0..position)
            .rev()
            .find(|index| rows[*index].depth == depth - 1);
        if depth == row.depth {
            prefix.push_str(if row.last { "└─ " } else { "├─ " });
        } else {
            prefix.push_str(if ancestor.is_some_and(|index| !rows[index].last) {
                "│  "
            } else {
                "   "
            });
        }
    }
    prefix
}

fn role_label(role: &str) -> &str {
    match role {
        "layerstore" => "LayerStore",
        "stackstore" => "StackStore",
        "branchstore" => "BranchStore",
        _ => "Store",
    }
}

fn sync_summary(entry: &TopologyEntry) -> String {
    if entry.role == "layerstore" {
        return "  AUTHORITY".to_owned();
    }
    if entry.active {
        "  ACTIVE".to_owned()
    } else {
        "  CONNECTED".to_owned()
    }
}

fn route_label(app: &App, store: &TopologyEntry) -> String {
    if store.role == "layerstore" {
        return "authority".to_owned();
    }
    let mut names = vec![store.name.clone()];
    let mut parent = store.parent.as_deref();
    while let Some(location) = parent {
        let Some(entry) = app
            .topology()
            .iter()
            .find(|entry| entry.location == location)
        else {
            break;
        };
        names.push(entry.name.clone());
        parent = entry.parent.as_deref();
    }
    names.reverse();
    names.join(" → ")
}

fn history_container_matches(category: HistoryCategory, fact: Fact) -> bool {
    match category {
        HistoryCategory::Layers => matches!(fact, Fact::LayerHistory(_)),
        HistoryCategory::Stacks => matches!(fact, Fact::StackHistory(_)),
        HistoryCategory::Branches => matches!(fact, Fact::Branch(_)),
    }
}

fn fact_indent(row: HistoryRow, fact: Fact) -> String {
    if matches!(
        fact,
        Fact::LayerHistory(_) | Fact::StackHistory(_) | Fact::Branch(_)
    ) {
        return "  ".to_owned();
    }
    format!(
        "  {} ",
        if row.tail {
            "└─"
        } else if row.depth == 2 {
            "├─"
        } else {
            "│ "
        }
    )
}

fn fact_symbol(fact: Fact) -> &'static str {
    match fact {
        Fact::LayerHistory(_) | Fact::Layer(_) | Fact::StackHistory(_) | Fact::Stack(_) => "◆",
        Fact::Branch(_) | Fact::Commit(_) | Fact::AddResult(_) => "◇",
    }
}

fn fact_label(fact: Fact) -> String {
    let id = hex(&fact.id());
    let short = if id.len() > 14 {
        format!("{}…{}", &id[..6], &id[id.len() - 4..])
    } else {
        id
    };
    match fact {
        Fact::LayerHistory(_) => format!("LayerHistory {short}"),
        Fact::Layer(_) => format!("Layer {short}"),
        Fact::StackHistory(_) => format!("StackHistory {short}"),
        Fact::Stack(_) => format!("Stack {short}"),
        Fact::Branch(_) => format!("Branch {short}"),
        Fact::Commit(_) => format!("Commit {short}"),
        Fact::AddResult(_) => format!("Result {short}"),
    }
}

fn fact_title(fact: Fact) -> &'static str {
    match fact {
        Fact::LayerHistory(_) => "LAYER HISTORY",
        Fact::Layer(_) => "LAYER",
        Fact::StackHistory(_) => "STACK HISTORY",
        Fact::Stack(_) => "STACK",
        Fact::Branch(_) => "BRANCH",
        Fact::Commit(_) => "COMMIT",
        Fact::AddResult(_) => "RESULT",
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn command_input(frame: &mut Frame, area: Rect, app: &App, theme: &Palette) {
    let border = if app.command_focused() || app.command_running() {
        theme.focus
    } else {
        theme.border
    };
    let title = if app.command_running() {
        " COMMAND · RUNNING "
    } else {
        " COMMAND "
    };
    let block = Block::bordered()
        .title(title)
        .title_style(Style::default().fg(border).add_modifier(Modifier::BOLD))
        .border_style(Style::default().fg(border))
        .style(Style::default().bg(theme.surface))
        .padding(Padding::horizontal(1));
    let inner = block.inner(area);
    let cursor_width = Line::raw(&app.command()[..app.command_cursor()]).width();
    let offset = cursor_width.saturating_sub(inner.width.saturating_sub(1) as usize);
    let command = if app.command().is_empty() {
        Line::styled("/db …", Style::default().fg(theme.muted))
    } else {
        Line::styled(app.command(), Style::default().fg(theme.text))
    };
    let message = app.message().map_or_else(
        || Line::raw(""),
        |message| {
            Line::styled(
                message.text(),
                Style::default().fg(if message.is_error() {
                    theme.error
                } else {
                    theme.secondary
                }),
            )
        },
    );
    frame.render_widget(block, area);
    frame.render_widget(
        Paragraph::new(command).scroll((0, offset.min(u16::MAX as usize) as u16)),
        Rect::new(inner.x, inner.y, inner.width, 1),
    );
    frame.render_widget(
        Paragraph::new(message),
        Rect::new(inner.x, inner.y + 1, inner.width, 1),
    );
    if app.command_focused() {
        frame.set_cursor_position((
            inner.x
                + cursor_width
                    .saturating_sub(offset)
                    .min(inner.width as usize) as u16,
            inner.y,
        ));
    }
}

fn completion_popup(frame: &mut Frame, command_area: Rect, app: &App, theme: &Palette) {
    if !app.command_focused() || app.completions().is_empty() {
        return;
    }
    let shown = app.completions().len().min(5);
    let width = frame.area().width.saturating_sub(8).min(72);
    let area = Rect::new(
        frame.area().x + frame.area().width.saturating_sub(width) / 2,
        command_area.y.saturating_sub(shown as u16 + 2),
        width,
        shown as u16 + 2,
    );
    let lines = app
        .completions()
        .iter()
        .take(shown)
        .enumerate()
        .map(|(index, completion)| {
            let selected = index == app.selected_completion();
            Line::from(vec![
                Span::styled(
                    if selected { "> " } else { "  " },
                    Style::default().fg(theme.focus),
                ),
                Span::styled(
                    &completion.value,
                    Style::default()
                        .fg(if selected { theme.focus } else { theme.text })
                        .add_modifier(if selected {
                            Modifier::BOLD
                        } else {
                            Modifier::empty()
                        }),
                ),
                Span::styled(
                    format!("  {}", completion.description),
                    Style::default().fg(theme.muted),
                ),
            ])
        })
        .collect::<Vec<_>>();
    frame.render_widget(Clear, area);
    frame.render_widget(
        Paragraph::new(lines).block(
            Block::bordered()
                .title(" COMPLETION ")
                .border_style(Style::default().fg(theme.focus))
                .style(Style::default().bg(theme.canvas)),
        ),
        area,
    );
}

fn help_overlay(frame: &mut Frame, theme: &Palette) {
    let width = frame.area().width.saturating_sub(4).min(72);
    let height = frame.area().height.saturating_sub(4).min(16);
    let area = Rect::new(
        frame.area().x + frame.area().width.saturating_sub(width) / 2,
        frame.area().y + frame.area().height.saturating_sub(height) / 2,
        width,
        height,
    );
    let lines = vec![
        Line::styled("NAVIGATION", Style::default().add_modifier(Modifier::BOLD)),
        Line::raw("j/k           move within the focused list"),
        Line::raw("Space         fold or unfold the selected group"),
        Line::raw("Arrow keys    move; at a list edge, cross to the adjacent panel"),
        Line::raw("Enter         open a relationship or prepare an operation"),
        Line::raw("Down          open relationship search in lineage"),
        Line::raw("Tab           next panel   Shift-Tab previous"),
        Line::raw("Esc           back or close this help"),
        Line::raw("/             command input   ? toggle help"),
        Line::raw("p / P         push / pull selected history"),
        Line::raw("q             quit"),
        Line::raw(""),
        Line::styled("MOUSE", Style::default().add_modifier(Modifier::BOLD)),
        Line::raw("Click a row or action; wheel scrolls its panel."),
        Line::raw("Resize is handled by the terminal event loop."),
    ];
    frame.render_widget(Clear, area);
    frame.render_widget(
        Paragraph::new(lines)
            .block(
                Block::bordered()
                    .title(" HELP ")
                    .border_style(Style::default().fg(theme.focus))
                    .style(Style::default().bg(theme.canvas))
                    .padding(Padding::uniform(1)),
            )
            .style(Style::default().fg(theme.text)),
        area,
    );
}

fn footer(frame: &mut Frame, area: Rect, app: &App, theme: &Palette) {
    let hints: &[(&str, &str)] = if app.command_focused() {
        &[
            (" Tab ", "complete"),
            (" ↑↓ ", "select"),
            (" Enter ", "run"),
            (" Esc ", "back"),
        ]
    } else {
        match app.focus() {
            Focus::Stores => &[
                (" j/k ", "store"),
                (" Space ", "fold"),
                (" arrows ", "move / edge jump"),
                (" Enter ", "histories"),
                (" q ", "quit"),
            ],
            Focus::Histories => &[
                (" j/k ", "history"),
                (" Space ", "fold"),
                (" arrows ", "move / edge jump"),
                (" Enter ", "lineage"),
                (" q ", "quit"),
            ],
            Focus::Details => &[
                (" j/k ", "operation"),
                (" arrows ", "move / edge jump"),
                (" Enter ", "prepare"),
                (" p/P ", "push/pull"),
                (" Esc ", "back"),
                (" q ", "quit"),
            ],
            Focus::Lineage => &[
                (" j/k ", "next node / relation"),
                (" PgUp/Dn ", "jump nodes"),
                (" ←→ ", "next / relation"),
                (" ↓ ", "relationships"),
                (" Enter ", "open"),
                (" Tab ", "focus"),
                (" Esc ", "details"),
                (" q ", "quit"),
            ],
            Focus::Command => &[(" / ", "commands"), (" q ", "quit")],
        }
    };
    let mut spans = Vec::with_capacity(hints.len() * 3);
    for (index, (key, label)) in hints.iter().enumerate() {
        if index > 0 {
            spans.push(Span::raw("   "));
        }
        spans.push(Span::styled(
            *key,
            Style::default()
                .fg(theme.focus)
                .add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::styled(*label, Style::default().fg(theme.secondary)));
    }
    frame.render_widget(
        Paragraph::new(Line::from(spans))
            .alignment(Alignment::Center)
            .style(Style::default().bg(theme.canvas))
            .block(
                Block::default()
                    .borders(Borders::TOP)
                    .border_style(Style::default().fg(theme.border)),
            ),
        area,
    );
}

#[cfg(test)]
mod tests {
    use ratatui::{backend::TestBackend, layout::Rect, Terminal};

    use crate::app::{App, HistoryGroup, TopologyEntry};

    #[test]
    fn renders_empty_and_bound_store_views() {
        let mut terminal = Terminal::new(TestBackend::new(160, 30)).unwrap();
        let mut app = App::default();
        terminal.draw(|frame| super::draw(frame, &app)).unwrap();
        let empty = screen(&terminal);
        assert!(empty.contains("No connected LayerStore"));
        assert!(empty.contains("Select a layer, stack, branch, or commit"));
        assert!(empty.contains("COMMAND"));

        app.replace_topology(vec![
            entry("layerstore", "main", "/tmp/layer.db", None),
            entry(
                "stackstore",
                "release",
                "/tmp/stack.db",
                Some("/tmp/layer.db"),
            ),
            entry(
                "branchstore",
                "feature",
                "/tmp/branch.db",
                Some("/tmp/stack.db"),
            ),
        ]);
        app.set_histories(vec![
            HistoryGroup {
                name: "main".into(),
                role: "layerstore".into(),
                location: "/tmp/layer.db".into(),
                parent: None,
                facts: Vec::new(),
                has_more: false,
            },
            HistoryGroup {
                name: "release".into(),
                role: "stackstore".into(),
                location: "/tmp/stack.db".into(),
                parent: Some("/tmp/layer.db".into()),
                facts: Vec::new(),
                has_more: false,
            },
            HistoryGroup {
                name: "feature".into(),
                role: "branchstore".into(),
                location: "/tmp/branch.db".into(),
                parent: Some("/tmp/stack.db".into()),
                facts: Vec::new(),
                has_more: false,
            },
        ]);
        terminal.draw(|frame| super::draw(frame, &app)).unwrap();
        let bound = screen(&terminal);
        assert!(bound.contains("STORES"));
        assert!(bound.contains("LayerStore · main"));
        assert!(bound.contains("StackStore · release"));
        assert!(bound.contains("BranchStore · feature"));
        assert!(bound.contains("LAYER HISTORIES"));
        assert!(bound.contains("BRANCH HISTORIES"));
    }

    #[test]
    fn stacks_store_and_history_panels_on_narrow_terminals() {
        let narrow = super::layout(Rect::new(0, 0, 100, 40));
        assert!(narrow.stores.y < narrow.histories.y);
        assert_eq!(narrow.stores.x, narrow.histories.x);
        assert_eq!(narrow.stores.width, narrow.histories.width);
        assert!(narrow.histories.x + narrow.histories.width <= narrow.details.x);
        assert!(narrow.details.y < narrow.lineage.y);
        assert_eq!(narrow.details.x, narrow.lineage.x);
        assert_eq!(narrow.details.width, narrow.lineage.width);
        assert_eq!(narrow.stores.height, narrow.details.height);
        assert_eq!(narrow.histories.height, narrow.lineage.height);

        let wide = super::layout(Rect::new(0, 0, 160, 40));
        assert!(wide.stores.y < wide.histories.y);
        assert_eq!(wide.stores.x, wide.histories.x);
        assert!(wide.histories.x + wide.histories.width <= wide.details.x);
        assert!(wide.details.y < wide.lineage.y);
        assert_eq!(wide.details.x, wide.lineage.x);
        assert_eq!(wide.details.width, wide.lineage.width);
        assert_eq!(wide.stores.height, wide.details.height);
        assert_eq!(wide.histories.height, wide.lineage.height);
    }

    fn screen(terminal: &Terminal<TestBackend>) -> String {
        terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect()
    }

    fn entry(role: &str, name: &str, location: &str, parent: Option<&str>) -> TopologyEntry {
        TopologyEntry {
            role: role.to_owned(),
            name: name.to_owned(),
            location: location.to_owned(),
            parent: parent.map(str::to_owned),
            active: false,
        }
    }
}

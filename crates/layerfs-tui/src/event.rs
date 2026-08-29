use std::{io, time::Duration};

use crossterm::{
    event::{
        self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEvent, KeyEventKind,
        KeyModifiers, MouseButton, MouseEventKind,
    },
    execute,
};
use layerfs_cli::{
    default_context_location, CliEvent, CliResult, CliSession, Command, CommandResult, DbCommand,
    OperationHandle, StoreQuery, StoreScope, StoreSnapshot, ViewQuery, ViewSnapshot,
};
use ratatui::DefaultTerminal;

use crate::{
    app::{Action, App, Direction, Focus, HistoryGroup, TopologyEntry},
    render,
};

pub fn run(terminal: &mut DefaultTerminal) -> io::Result<()> {
    execute!(io::stdout(), EnableMouseCapture)?;
    let result = run_loop(terminal);
    let mouse_result = execute!(io::stdout(), DisableMouseCapture);
    result.and(mouse_result)
}

fn run_loop(terminal: &mut DefaultTerminal) -> io::Result<()> {
    let session = CliSession::open(default_context_location()).map_err(io::Error::other)?;
    let mut app = restore_app(&session).map_err(io::Error::other)?;
    let mut operation = None;

    while app.is_running() {
        if poll_operation(&mut app, &session, operation.as_ref())? {
            operation = None;
        }
        terminal.draw(|frame| render::draw(frame, &app))?;
        if event::poll(Duration::from_millis(250))? {
            let size = terminal.size()?;
            let area = ratatui::layout::Rect::new(0, 0, size.width, size.height);
            if let Some(started) =
                handle(event::read()?, &mut app, &session, area, operation.as_ref())
            {
                operation = Some(started);
            }
        }
    }

    Ok(())
}

fn restore_app(session: &CliSession) -> CliResult<App> {
    let mut app = App::default();
    app.replace_topology(topology(session)?);
    app.set_histories(histories(session)?);
    if app.topology().is_empty() {
        app.focus_command();
        refresh_completions(&mut app, session);
    }
    Ok(app)
}

fn poll_operation(
    app: &mut App,
    session: &CliSession,
    operation: Option<&OperationHandle>,
) -> io::Result<bool> {
    let Some(operation) = operation else {
        return Ok(false);
    };
    while let Some(event) = operation.try_next_event().map_err(io::Error::other)? {
        if let CliEvent::Finished { result, .. } = event {
            match result {
                Ok(result) => match (topology(session), histories(session)) {
                    (Ok(topology), Ok(histories)) => {
                        app.replace_topology(topology);
                        app.set_histories(histories);
                        app.command_succeeded(command_result(result));
                    }
                    (Err(error), _) | (_, Err(error)) => app.command_failed(error.to_string()),
                },
                Err(error) => app.command_failed(error.to_string()),
            }
            return Ok(true);
        }
    }
    Ok(false)
}

fn command_result(result: CommandResult) -> String {
    match result {
        CommandResult::Database { role, location } => format!("READY {role} {location}"),
        _ => "OK".to_owned(),
    }
}

fn handle(
    event: Event,
    app: &mut App,
    session: &CliSession,
    area: ratatui::layout::Rect,
    operation: Option<&OperationHandle>,
) -> Option<OperationHandle> {
    match event {
        Event::Key(key) => {
            if key.kind != KeyEventKind::Press {
                return None;
            }
            if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
                if let Some(operation) = operation {
                    let _ = operation.interrupt();
                }
                app.quit();
                return None;
            }
            main_key(key, app, session)
        }
        Event::Mouse(mouse) => mouse_event(mouse.kind, mouse.column, mouse.row, app, session, area),
        Event::Resize(_, _) => {
            app.on_resize();
            None
        }
        Event::Paste(text) => {
            if app.command_focused() && !app.command_running() {
                for character in text.chars() {
                    if character != '\n' && character != '\r' {
                        app.type_command(character);
                    }
                }
                refresh_completions(app, session);
            }
            None
        }
        Event::FocusGained | Event::FocusLost => None,
    }
}

fn main_key(key: KeyEvent, app: &mut App, session: &CliSession) -> Option<OperationHandle> {
    if app.help_visible() {
        if matches!(key.code, KeyCode::Esc | KeyCode::Char('?')) {
            app.toggle_help();
        }
        return None;
    }
    if app.command_running() {
        return None;
    }
    if app.command_focused() {
        return command_key(key, app, session);
    }
    if key.code == KeyCode::Char('?') {
        app.toggle_help();
        return None;
    }
    if key.code == KeyCode::Char('q') {
        app.quit();
        return None;
    }
    match app.focus() {
        Focus::Stores => store_key(key, app, session),
        Focus::Histories => history_key(key, app, session),
        Focus::Details => details_key(key, app, session),
        Focus::Lineage => lineage_key(key, app, session),
        Focus::Command => None,
    }
}

fn directional_key(key: KeyCode) -> Option<Direction> {
    Some(match key {
        KeyCode::Up => Direction::Up,
        KeyCode::Down => Direction::Down,
        KeyCode::Left => Direction::Left,
        KeyCode::Right => Direction::Right,
        _ => return None,
    })
}

fn move_or_focus(app: &mut App, direction: Direction) {
    let moved = match (app.focus(), direction) {
        (Focus::Stores, Direction::Up) => app.select_store_previous_if_possible(),
        (Focus::Stores, Direction::Down) => app.select_store_next_if_possible(),
        (Focus::Histories, Direction::Up) => app.select_history_previous_if_possible(),
        (Focus::Histories, Direction::Down) => app.select_history_next_if_possible(),
        (Focus::Details, Direction::Up) => app.select_action_previous_if_possible(),
        (Focus::Details, Direction::Down) => app.select_action_next_if_possible(),
        (Focus::Lineage, Direction::Left) => app.select_lineage_previous_if_possible(),
        (Focus::Lineage, Direction::Right) => app.select_lineage_next_if_possible(),
        _ => false,
    };
    if !moved {
        app.focus_direction(direction);
    }
}

fn store_key(key: KeyEvent, app: &mut App, session: &CliSession) -> Option<OperationHandle> {
    if let Some(direction) = directional_key(key.code) {
        move_or_focus(app, direction);
        return None;
    }
    match key.code {
        KeyCode::Char('/') => {
            app.focus_command();
            refresh_completions(app, session);
        }
        KeyCode::Tab => {
            if key.modifiers.contains(KeyModifiers::SHIFT) {
                app.focus_previous();
            } else {
                app.focus_next();
            }
        }
        KeyCode::BackTab => app.focus_previous(),
        KeyCode::Char('k') => app.select_store_previous(),
        KeyCode::Char('j') => app.select_store_next(),
        KeyCode::Home | KeyCode::Char('g') => app.select_store_first(),
        KeyCode::End | KeyCode::Char('G') => app.select_store_last(),
        KeyCode::Char(' ') => app.toggle_store(),
        KeyCode::Enter => {
            if let Some(operation) = activate_store(app, session) {
                return Some(operation);
            }
            app.focus_histories();
        }
        KeyCode::PageUp => app.scroll_stores(false),
        KeyCode::PageDown => app.scroll_stores(true),
        _ => {}
    }
    None
}

fn history_key(key: KeyEvent, app: &mut App, session: &CliSession) -> Option<OperationHandle> {
    if let Some(direction) = directional_key(key.code) {
        move_or_focus(app, direction);
        return None;
    }
    match key.code {
        KeyCode::Char('/') => {
            app.focus_command();
            refresh_completions(app, session);
        }
        KeyCode::Tab => {
            if key.modifiers.contains(KeyModifiers::SHIFT) {
                app.focus_previous();
            } else {
                app.focus_next();
            }
        }
        KeyCode::BackTab => app.focus_previous(),
        KeyCode::Esc => app.focus_stores(),
        KeyCode::Char('k') => app.select_history_previous(),
        KeyCode::Char('j') => app.select_history_next(),
        KeyCode::Home | KeyCode::Char('g') => app.select_history_first(),
        KeyCode::End | KeyCode::Char('G') => app.select_history_last(),
        KeyCode::Char(' ') => app.toggle_history_category(),
        KeyCode::Enter => app.focus_lineage(),
        KeyCode::PageUp => app.scroll_histories(false),
        KeyCode::PageDown => app.scroll_histories(true),
        _ => {}
    }
    None
}

fn details_key(key: KeyEvent, app: &mut App, session: &CliSession) -> Option<OperationHandle> {
    if let Some(direction) = directional_key(key.code) {
        move_or_focus(app, direction);
        return None;
    }
    match key.code {
        KeyCode::Char('/') => {
            app.focus_command();
            refresh_completions(app, session);
        }
        KeyCode::Tab => {
            if key.modifiers.contains(KeyModifiers::SHIFT) {
                app.focus_previous();
            } else {
                app.focus_next();
            }
        }
        KeyCode::BackTab => app.focus_previous(),
        KeyCode::Esc => app.focus_histories(),
        KeyCode::Char('k') => app.select_action_previous(),
        KeyCode::Char('j') => app.select_action_next(),
        KeyCode::Home | KeyCode::Char('g') => app.select_action_first(),
        KeyCode::End | KeyCode::Char('G') => app.select_action_last(),
        KeyCode::Enter => prepare_action(app),
        KeyCode::Char('p') => prepare_keyed_action(app, 'p'),
        KeyCode::Char('P') => prepare_keyed_action(app, 'P'),
        KeyCode::PageUp => app.scroll_details(false),
        KeyCode::PageDown => app.scroll_details(true),
        _ => {}
    }
    None
}

fn lineage_key(key: KeyEvent, app: &mut App, session: &CliSession) -> Option<OperationHandle> {
    match key.code {
        KeyCode::Left => {
            if app.lineage_relation_focus() {
                app.select_lineage_relation_previous();
            } else {
                app.select_lineage_previous_if_possible();
            }
        }
        KeyCode::Right => {
            if app.lineage_relation_focus() {
                app.select_lineage_relation_next();
            } else {
                app.select_lineage_next_if_possible();
            }
        }
        KeyCode::Up => {
            if app.lineage_relation_focus() {
                app.focus_lineage_nodes();
            } else if !app.close_lineage_child() {
                app.focus_direction(Direction::Up);
            }
        }
        KeyCode::Down => {
            if !app.lineage_relation_focus() && !app.focus_lineage_relations() {
                app.focus_direction(Direction::Down);
            }
        }
        KeyCode::Char('/') => {
            app.focus_command();
            refresh_completions(app, session);
        }
        KeyCode::Tab => {
            if key.modifiers.contains(KeyModifiers::SHIFT) {
                app.focus_previous();
            } else {
                app.focus_next();
            }
        }
        KeyCode::BackTab => app.focus_previous(),
        KeyCode::Esc => {
            if !app.close_lineage_child() {
                app.focus_details();
            }
        }
        KeyCode::Char('j') => {
            if app.lineage_relation_focus() {
                app.select_lineage_relation_next();
            } else {
                app.select_lineage_next_if_possible();
            }
        }
        KeyCode::Char('k') => {
            if app.lineage_relation_focus() {
                app.select_lineage_relation_previous();
            } else {
                app.select_lineage_previous_if_possible();
            }
        }
        KeyCode::Home | KeyCode::Char('g') => {
            if app.lineage_relation_focus() {
                app.select_lineage_relation_first();
            } else {
                app.select_lineage_first();
            }
        }
        KeyCode::End | KeyCode::Char('G') => {
            if app.lineage_relation_focus() {
                app.select_lineage_relation_last();
            } else {
                app.select_lineage_last();
            }
        }
        KeyCode::PageUp => {
            if app.lineage_relation_focus() {
                app.select_lineage_relation_first();
            } else {
                app.select_lineage_page(false);
            }
        }
        KeyCode::PageDown => {
            if app.lineage_relation_focus() {
                app.select_lineage_relation_last();
            } else {
                app.select_lineage_page(true);
            }
        }
        KeyCode::Enter => {
            if app.lineage_relation_focus() {
                app.open_lineage_relation();
            } else {
                app.focus_lineage_relations();
            }
        }
        _ => {}
    }
    None
}

fn prepare_action(app: &mut App) {
    let Some(action) = app.selected_action() else {
        app.info("select a history record with an operation");
        return;
    };
    if let Some(command) = action_command(app, action) {
        app.prepare_command(command);
    } else {
        app.info("operation needs a concrete history record");
    }
}

fn prepare_keyed_action(app: &mut App, key: char) {
    let Some(action) = app.actions().into_iter().find(|action| action.key() == key) else {
        app.info("no matching operation for this history");
        return;
    };
    if let Some(command) = action_command(app, action) {
        app.prepare_command(command);
    }
}

fn action_command(app: &App, action: Action) -> Option<String> {
    let fact = app.active_lineage_fact()?;
    let id = hex(&fact.id());
    Some(match action {
        Action::PullLayer => format!("/layer pull {id}"),
        Action::PullStack => format!("/stack pull {id}"),
        Action::PushStack => format!("/stack push {id}"),
        Action::PullBranch => format!("/branch pull {id}"),
        Action::PullCommits => format!("/branch pull-commits {id}"),
        Action::PushBranch => format!("/branch push {id}"),
    })
}

fn mouse_event(
    kind: MouseEventKind,
    column: u16,
    row: u16,
    app: &mut App,
    session: &CliSession,
    area: ratatui::layout::Rect,
) -> Option<OperationHandle> {
    if app.help_visible() {
        if matches!(kind, MouseEventKind::Down(MouseButton::Left)) {
            app.toggle_help();
        }
        return None;
    }
    let layout = render::layout(area);
    match kind {
        MouseEventKind::ScrollUp | MouseEventKind::ScrollDown => {
            let down = matches!(kind, MouseEventKind::ScrollDown);
            if layout.stores.contains((column, row).into()) {
                app.scroll_stores(down);
                app.focus_stores();
            } else if layout.histories.contains((column, row).into()) {
                app.scroll_histories(down);
                app.focus_histories();
            } else if layout.details.contains((column, row).into()) {
                app.scroll_details(down);
                app.focus_details();
            } else if layout.lineage.contains((column, row).into()) {
                if down {
                    if app.lineage_relation_focus() {
                        app.select_lineage_relation_next();
                    } else {
                        app.select_lineage_next_if_possible();
                    }
                } else {
                    if app.lineage_relation_focus() {
                        app.select_lineage_relation_previous();
                    } else {
                        app.select_lineage_previous_if_possible();
                    }
                }
                app.focus_lineage();
            }
        }
        MouseEventKind::Down(MouseButton::Left) => {
            if layout.stores.contains((column, row).into()) {
                if let Some(index) = render::store_row_at(layout.stores, row, app) {
                    app.select_store_row(index);
                    app.focus_stores();
                }
            } else if layout.histories.contains((column, row).into()) {
                if let Some(index) = render::history_row_at(layout.histories, row, app) {
                    app.select_history_row(index);
                    app.focus_histories();
                }
            } else if layout.details.contains((column, row).into()) {
                if let Some(index) = render::action_row_at(layout.details, row, app) {
                    app.select_action_row(index);
                    app.focus_details();
                    prepare_action(app);
                } else {
                    app.focus_details();
                }
            } else if layout.lineage.contains((column, row).into()) {
                match render::lineage_hit_at(layout.lineage, column, row, app) {
                    Some(render::LineageHit::Node(index)) => {
                        app.select_lineage_fact(index);
                    }
                    Some(render::LineageHit::Relation(index)) => {
                        app.select_lineage_relation_first();
                        for _ in 0..index {
                            app.select_lineage_relation_next();
                        }
                        app.focus_lineage_relations();
                    }
                    None => {}
                }
                app.focus_lineage();
            } else if layout.command.contains((column, row).into()) {
                app.focus_command();
                refresh_completions(app, session);
            }
        }
        _ => {}
    }
    None
}

fn command_key(key: KeyEvent, app: &mut App, session: &CliSession) -> Option<OperationHandle> {
    match key.code {
        KeyCode::Esc => app.blur_command(),
        KeyCode::Enter => return execute_command(app, session),
        KeyCode::Backspace => app.erase_command(),
        KeyCode::Delete => app.delete_command(),
        KeyCode::Left => app.move_command_left(),
        KeyCode::Right => app.move_command_right(),
        KeyCode::Home => app.move_command_home(),
        KeyCode::End => app.move_command_end(),
        KeyCode::Up => {
            app.previous_completion();
            return None;
        }
        KeyCode::Down => {
            app.next_completion();
            return None;
        }
        KeyCode::Tab => app.apply_completion(),
        KeyCode::Char(character)
            if !key
                .modifiers
                .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
        {
            app.type_command(character);
        }
        _ => return None,
    }
    refresh_completions(app, session);
    None
}

fn execute_command(app: &mut App, session: &CliSession) -> Option<OperationHandle> {
    let input = app.command().trim();
    match input {
        "/help" => {
            app.command_succeeded(
                "/db create|connect <role> <name> <path> [--parent <name>]; /db disconnect <role> [name]",
            );
            return None;
        }
        "/quit" => {
            app.quit();
            return None;
        }
        _ => {}
    }
    let Some(input) = input.strip_prefix('/') else {
        app.command_failed("slash command required");
        return None;
    };
    let command = match CliSession::parse_line(input) {
        Ok(command @ Command::Db { .. })
        | Ok(command @ Command::Layer { .. })
        | Ok(command @ Command::Stack { .. })
        | Ok(command @ Command::Branch { .. }) => command,
        Ok(_) => {
            app.command_failed("this view only supports store and sync commands");
            return None;
        }
        Err(error) => {
            app.command_failed(error.to_string());
            return None;
        }
    };
    match session.execute(command) {
        Ok(operation) => {
            app.focus_after_command(Focus::Histories);
            app.command_started();
            Some(operation)
        }
        Err(error) => {
            app.command_failed(error.to_string());
            None
        }
    }
}

fn refresh_completions(app: &mut App, session: &CliSession) {
    let input = app.command().strip_prefix('/').unwrap_or(app.command());
    let cursor = app
        .command_cursor()
        .saturating_sub(usize::from(app.command().starts_with('/')));
    let completions = session.complete(input, cursor).unwrap_or_default();
    app.set_completions(completions);
}

fn activate_store(app: &mut App, session: &CliSession) -> Option<OperationHandle> {
    let store = app.selected_store()?.clone();
    if !matches!(store.role.as_str(), "stackstore" | "branchstore") {
        return None;
    }
    let command = Command::Db {
        command: DbCommand::Use {
            location: store.location.into(),
        },
    };
    match session.execute(command) {
        Ok(operation) => {
            app.command_started();
            Some(operation)
        }
        Err(error) => {
            app.command_failed(error.to_string());
            None
        }
    }
}

fn topology(session: &CliSession) -> CliResult<Vec<TopologyEntry>> {
    match session.snapshot(ViewQuery::Topology)? {
        ViewSnapshot::Topology(entries) => Ok(entries
            .into_iter()
            .map(|entry| TopologyEntry {
                name: std::path::Path::new(&entry.location)
                    .file_stem()
                    .and_then(|value| value.to_str())
                    .unwrap_or(&entry.role)
                    .to_owned(),
                role: match entry.role.as_str() {
                    "layer" => "layerstore".to_owned(),
                    "stack" => "stackstore".to_owned(),
                    "branch" => "branchstore".to_owned(),
                    _ => entry.role.clone(),
                },
                location: entry.location,
                parent: entry.parent,
                active: entry.active,
            })
            .collect()),
        _ => Err(layerfs_cli::CliError::Integrity),
    }
}

fn histories(session: &CliSession) -> CliResult<Vec<HistoryGroup>> {
    let stores = topology(session)?;
    let mut groups = Vec::with_capacity(stores.len());
    for store in stores {
        let scope = match store.role.as_str() {
            "layerstore" => StoreScope::Layer,
            "stackstore" => StoreScope::Stack(store.location.clone().into()),
            "branchstore" => StoreScope::Branch(store.location.clone().into()),
            _ => continue,
        };
        let mut facts = Vec::new();
        let mut has_more = false;
        let kinds = match store.role.as_str() {
            "branchstore" => vec![layerfs_cli::FactKind::Branch, layerfs_cli::FactKind::Commit],
            _ => vec![
                layerfs_cli::FactKind::LayerHistory,
                layerfs_cli::FactKind::Layer,
                layerfs_cli::FactKind::StackHistory,
                layerfs_cli::FactKind::Stack,
                layerfs_cli::FactKind::Branch,
                layerfs_cli::FactKind::Commit,
                layerfs_cli::FactKind::AddResult,
            ],
        };
        for kind in kinds {
            let snapshot = session.snapshot(ViewQuery::Store(StoreQuery::Page {
                scope: scope.clone(),
                kind,
                after: None,
                limit: 128,
            }))?;
            let ViewSnapshot::Store(StoreSnapshot::Page { facts: page, next }) = snapshot else {
                return Err(layerfs_cli::CliError::Integrity);
            };
            has_more |= next.is_some();
            facts.extend(
                page.iter()
                    .map(layerfs_cli::StoreFact::fact)
                    .collect::<CliResult<Vec<_>>>()?,
            );
        }
        groups.push(HistoryGroup {
            name: store.name,
            role: store.role,
            location: store.location,
            parent: store.parent,
            facts,
            has_more,
        });
    }
    let snapshots = groups.clone();
    for group in &mut groups {
        if group.role != "branchstore" {
            continue;
        }
        let Some(parent) = group.parent.as_deref() else {
            continue;
        };
        let Some(parent_group) = snapshots
            .iter()
            .find(|candidate| candidate.location == parent)
        else {
            continue;
        };
        let mut known = group
            .facts
            .iter()
            .map(|fact| fact.id())
            .collect::<std::collections::BTreeSet<_>>();
        group.facts.extend(
            parent_group
                .facts
                .iter()
                .copied()
                .filter(|fact| known.insert(fact.id())),
        );
    }
    Ok(groups)
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::{hex, move_or_focus};
    use crate::app::{App, Direction, Focus, TopologyEntry};

    #[test]
    fn encodes_operation_ids_without_truncation() {
        assert_eq!(hex(&[0, 1, 0xfe, 0xff]), "0001feff");
    }

    #[test]
    fn arrows_move_inside_a_panel_before_crossing_its_edge() {
        let mut app = App::default();
        app.replace_topology(vec![
            TopologyEntry {
                role: "layerstore".into(),
                name: "main".into(),
                location: "/tmp/layer.db".into(),
                parent: None,
                active: true,
            },
            TopologyEntry {
                role: "stackstore".into(),
                name: "release".into(),
                location: "/tmp/stack.db".into(),
                parent: Some("/tmp/layer.db".into()),
                active: false,
            },
        ]);
        move_or_focus(&mut app, Direction::Down);
        assert_eq!(app.focus(), Focus::Stores);
        assert_eq!(app.selected_store_index(), Some(1));
        move_or_focus(&mut app, Direction::Down);
        assert_eq!(app.focus(), Focus::Histories);
    }
}

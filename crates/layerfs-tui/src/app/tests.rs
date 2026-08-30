use super::{App, GraphTarget, Overlay, Route};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use layerfs_cli::{BranchId, CommitId};

#[test]
fn navigates_by_stable_ids() {
    let mut app = App::demo();
    let selected = app.selected_project.clone();
    app.handle_key(KeyEvent::new(KeyCode::Char('r'), KeyModifiers::NONE));
    assert_eq!(app.selected_project, selected);
    app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    assert!(matches!(app.route, Route::Project(_)));
}

#[test]
fn depth_four_graph_is_derived_from_fixture() {
    let mut app = App::demo();
    app.selected_layer = Some("L-A-15".into());
    let rows = app.graph_rows();
    assert!(rows.iter().any(|row| {
        row.target == GraphTarget::Branch(BranchId::from("B-search-a1x-i")) && row.depth >= 8
    }));
    assert!(rows.iter().any(|row| {
        row.target == GraphTarget::Commit(BranchId::from("B-main"), CommitId::from("C-B-main-35"))
    }));
}

#[test]
fn interaction_guards_and_stable_navigation_work() {
    let mut app = App::demo();
    app.handle_key(KeyEvent::new(KeyCode::BackTab, KeyModifiers::NONE));
    assert_eq!(app.focus, 1);
    app.search = "web-client".into();
    app.apply_search();
    assert_eq!(app.selected_project.as_ref().unwrap().as_str(), "SW-20");
    app.refresh_project("SA-91".into());
    app.selected_layer = Some("L-A-15".into());
    app.set_demo_route("topology");
    app.selected_graph = Some(GraphTarget::Branch(BranchId::from("B-main")));
    let expanded_count = app.graph_rows().len();
    app.set_expanded(false);
    assert!(app.graph_rows().len() < expanded_count);
    assert!(!app.graph_rows().iter().any(|row| {
        row.target == GraphTarget::Commit(BranchId::from("B-main"), CommitId::from("C-B-main-35"))
    }));
    app.set_expanded(true);
    assert_eq!(app.graph_rows().len(), expanded_count);
}

#[test]
fn workspace_shortcut_requires_local_head() {
    let mut app = App::demo();
    app.set_demo_route("topology");
    app.selected_graph = Some(GraphTarget::Commit(
        BranchId::from("B-main"),
        CommitId::from("C-B-main-35"),
    ));
    app.prefill_workspace();
    assert_eq!(app.overlay, Overlay::None);
    assert!(app.error.is_some());
    app.selected_graph = Some(GraphTarget::Commit(
        BranchId::from("B-search-a2"),
        CommitId::from("C-B-search-a2-02"),
    ));
    app.prefill_workspace();
    assert_eq!(app.overlay, Overlay::Command);
    assert!(app.command.contains("--commit C-B-search-a2-02"));
}

#[test]
fn active_operation_cannot_be_replaced() {
    let mut app = App::demo();
    let first =
        layerfs_cli::CliSession::parse_line("layerstack pull --through L-A-19 --replica").unwrap();
    app.start_operation(first);
    let id = app.active_operation.as_ref().unwrap().id().clone();
    let second = layerfs_cli::CliSession::parse_line("branch push B-search-a").unwrap();
    app.start_operation(second);
    assert_eq!(app.active_operation.as_ref().unwrap().id(), &id);
    assert!(app.error.as_deref().unwrap().contains("already running"));
}

#[test]
fn operation_events_progress_and_refresh_activity() {
    let mut app = App::demo();
    let command = layerfs_cli::CliSession::parse_line("branch push B-search-a").unwrap();
    app.start_operation(command);
    for _ in 0..16 {
        app.tick();
    }
    assert!(app.active_operation.is_none());
    assert!(app
        .activity
        .operations
        .items
        .iter()
        .any(|operation| operation.id.as_str() == "OP-100"));
    assert!(app
        .event_log
        .iter()
        .any(|event| event.contains("Finished Succeeded")));
}

#[test]
fn command_cursor_edits_in_place() {
    let mut app = App::demo();
    app.overlay = Overlay::Command;
    app.command = "branch psh".into();
    app.command_cursor = 8;
    app.handle_key(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::NONE));
    assert_eq!(app.command, "branch push");
    app.handle_key(KeyEvent::new(KeyCode::Left, KeyModifiers::NONE));
    app.handle_key(KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE));
    assert_eq!(app.command, "branch ush");
}

#[test]
fn diff_consumes_continuation_without_losing_selection() {
    let mut app = App::demo();
    app.set_demo_route("diff");
    let first = app.diff.as_ref().unwrap();
    assert_eq!(first.entries.items.len(), 128);
    assert!(first.entries.next.is_some());
    app.selected_diff_path = first.entries.items.last().map(|entry| entry.path.clone());
    app.move_selection(1);
    let diff = app.diff.as_ref().unwrap();
    assert_eq!(diff.entries.items.len(), 136);
    assert!(diff.entries.next.is_none());
    assert_eq!(
        app.selected_diff_path.as_ref(),
        diff.entries.items.get(128).map(|entry| &entry.path)
    );
}

#[test]
fn initial_layer_workspace_returns_to_exact_branch_anchor() {
    let mut app = App::demo();
    app.route = Route::Workspaces(Some("SA-91".into()));
    app.selected_workspace = Some("W50".into());
    app.open_selected();
    assert!(matches!(
        app.route,
        Route::Branch(_, ref branch) if branch.as_str() == "B-empty"
    ));
    assert_eq!(
        app.selected_graph,
        Some(GraphTarget::Branch(BranchId::from("B-empty")))
    );
    assert_eq!(
        app.selected_layer.as_ref().map(|id| id.as_str()),
        Some("L-A-18")
    );
}

#[test]
fn command_result_routes_to_diff_page() {
    let mut app = App::demo();
    let command = layerfs_cli::CliSession::parse_line(
        "branch diff --branch B-search-a --from C-B-search-a-05 --to C-B-search-a-08",
    )
    .unwrap();
    app.start_operation(command);
    for _ in 0..16 {
        app.tick();
    }
    assert!(matches!(app.route, Route::Diff(_)));
    assert!(app.diff.is_some());
}

#[test]
fn quit_interrupts_and_records_terminal_event() {
    let mut app = App::demo();
    let command =
        layerfs_cli::CliSession::parse_line("layerstack pull --through L-A-19 --replica").unwrap();
    app.start_operation(command);
    app.quit_cleanly();
    assert!(app.should_quit);
    assert!(app.active_operation.is_none());
    assert!(app.activity.operations.items.iter().any(|operation| {
        operation.id.as_str() == "OP-100"
            && operation.state == layerfs_cli::OperationState::Interrupted
    }));
}

#[test]
fn stale_add_routes_to_reconciliation_workspace() {
    let mut app = App::demo();
    let command = layerfs_cli::CliSession::parse_line("layerstack add B-local-sync").unwrap();
    app.start_operation(command);
    for _ in 0..16 {
        app.tick();
    }
    assert!(matches!(app.route, Route::Workspaces(_)));
    assert_eq!(
        app.selected_workspace.as_ref().map(|id| id.as_str()),
        Some("W90")
    );
    let workspace = app
        .workspaces
        .workspaces
        .items
        .iter()
        .find(|workspace| workspace.id.as_str() == "W90")
        .unwrap();
    assert_eq!(workspace.anchor_layer.as_ref().unwrap().as_str(), "L-A-19");
    assert_eq!(workspace.conflicts.len(), 2);
}

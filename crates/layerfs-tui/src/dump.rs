use crate::{
    app::{App, Overlay, Route},
    render,
    theme::Theme,
    WorkspaceTab,
};
use ratatui::{backend::TestBackend, Terminal};
use std::io;

pub fn render_to_string(page: &str, width: u16, height: u16, no_color: bool) -> io::Result<String> {
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend)?;
    let (route, state) = page.split_once(':').unwrap_or((page, ""));
    let mut app = if route == "empty" {
        App::empty_demo()
    } else {
        App::demo()
    };
    app.set_demo_route(if route == "empty" { "projects" } else { route });
    app.workspace_tab = match state {
        "files" => WorkspaceTab::Files,
        "changes" => WorkspaceTab::Changes,
        "runs" => WorkspaceTab::Runs,
        "workspace-storage" => WorkspaceTab::Storage,
        _ => app.workspace_tab,
    };
    app.sync_workspace_item();
    match state {
        "detail" | "content" | "graph" | "receipt" | "dedup" => {
            app.focus = 1;
            app.compact_pane = 1;
        }
        "inspector" => {
            let pane = if matches!(app.route, Route::Branch(_, _)) {
                1
            } else {
                2
            };
            app.focus = pane;
            app.compact_pane = pane;
        }
        "details" => {
            app.focus = 2;
            app.compact_pane = 2;
        }
        "help" => app.overlay = Overlay::Help,
        "command" => {
            app.overlay = Overlay::Command;
            app.command = "branch pu".into();
            app.command_cursor = app.command.len();
            app.completions = app
                .session
                .complete(&app.command, app.command_cursor)
                .unwrap_or_default();
        }
        "plan" => {
            let command = layerfs_cli::CliSession::parse_line("branch push B-search-a").unwrap();
            let plan = app.session.plan(&command).unwrap();
            app.plan = Some((command, plan));
            app.overlay = Overlay::Plan;
        }
        "operation" => {
            app.operation_drawer = true;
            app.operation_title = "Pull api-server through L19".into();
            app.operation_progress = Some((17, 21, "verifying closure".into()));
            app.event_log.push_back("Progress roots 17/21".into());
        }
        "error" => app.error = Some("mock integrity failure".into()),
        _ => {}
    }
    terminal.draw(|frame| render::draw(frame, &app, Theme::with_color(!no_color)))?;
    let buffer = terminal.backend().buffer();
    let mut output = String::new();
    for y in 0..height {
        let mut line = String::new();
        for x in 0..width {
            if let Some(cell) = buffer.cell((x, y)) {
                line.push_str(cell.symbol());
            }
        }
        output.push_str(line.trim_end());
        output.push('\n');
    }
    Ok(output)
}

use super::{ActivityTab, App, Overlay, Route};
use crate::{ExplorerMode, WorkspaceTab};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

impl App {
    pub(super) fn normal_key(&mut self, key: KeyEvent) {
        if key
            .modifiers
            .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
        {
            return;
        }
        match key.code {
            KeyCode::Char('1') => self.go(Route::Projects),
            KeyCode::Char('2') => self.show_topology(),
            KeyCode::Char('3') => self.go(Route::Workspaces(self.selected_project.clone())),
            KeyCode::Char('4') => self.go(Route::Activity(ActivityTab::Operations)),
            KeyCode::Char('q') if self.active_operation.is_some() => {
                self.error = Some(
                    "Operation is running; Esc dismisses this message, then x interrupts".into(),
                )
            }
            KeyCode::Char('q') => self.quit_cleanly(),
            KeyCode::Esc | KeyCode::Backspace => self.navigate_back(),
            KeyCode::Tab if key.modifiers.contains(KeyModifiers::SHIFT) => self.shift_focus(-1),
            KeyCode::Tab => self.shift_focus(1),
            KeyCode::BackTab => self.shift_focus(-1),
            KeyCode::Down | KeyCode::Char('j') => self.move_selection(1),
            KeyCode::Up | KeyCode::Char('k') => self.move_selection(-1),
            KeyCode::PageDown => self.move_selection(10),
            KeyCode::PageUp => self.move_selection(-10),
            KeyCode::Left | KeyCode::Char('h') => self.tree_horizontal(false),
            KeyCode::Right | KeyCode::Char('l') => self.tree_horizontal(true),
            KeyCode::Char(' ') => self.toggle_visible_tree(),
            KeyCode::Enter => self.open_selected(),
            KeyCode::Char(':') => {
                self.overlay = Overlay::Command;
                self.command.clear();
                self.command_cursor = 0;
                self.update_completions();
            }
            KeyCode::Char('/') => {
                self.overlay = Overlay::Search;
                self.search.clear();
            }
            KeyCode::Char('?') => self.overlay = Overlay::Help,
            KeyCode::Char('o') => self.operation_drawer = !self.operation_drawer,
            KeyCode::Char('r') => self.refresh_all(),
            KeyCode::Char('x') if self.active_operation.is_some() => {
                self.interrupt_active();
            }
            KeyCode::Char('f') if self.visible_topology() => self.prefill_fork(),
            KeyCode::Char('w') if self.visible_topology() => self.prefill_workspace(),
            KeyCode::Char('d') if matches!(self.route, Route::Workspace(_)) => {
                self.workspace_tab = WorkspaceTab::Changes;
                self.sync_workspace_item();
            }
            KeyCode::Char('d') if matches!(self.route, Route::Project(_) | Route::Branch(_, _)) => {
                self.set_explorer_mode(ExplorerMode::Changes)
            }
            KeyCode::Char('[') if matches!(self.route, Route::Workspace(_)) => {
                self.workspace_tab = self.workspace_tab.next(-1);
                self.sync_workspace_item();
            }
            KeyCode::Char(']') if matches!(self.route, Route::Workspace(_)) => {
                self.workspace_tab = self.workspace_tab.next(1);
                self.sync_workspace_item();
            }
            KeyCode::Char('[') if matches!(self.route, Route::Project(_) | Route::Branch(_, _)) => {
                self.cycle_explorer_mode(-1)
            }
            KeyCode::Char(']') if matches!(self.route, Route::Project(_) | Route::Branch(_, _)) => {
                self.cycle_explorer_mode(1)
            }
            KeyCode::Char('[' | ']') if matches!(self.route, Route::Activity(_)) => {
                self.route = match self.route {
                    Route::Activity(ActivityTab::Operations) => {
                        Route::Activity(ActivityTab::Storage)
                    }
                    _ => Route::Activity(ActivityTab::Operations),
                };
                self.focus = 0;
                self.compact_pane = 0;
            }
            KeyCode::Char('x') if matches!(self.route, Route::Workspace(_)) => {
                self.prefill_workspace_bash()
            }
            KeyCode::Char('c') if matches!(self.route, Route::Workspace(_)) => {
                self.plan_workspace_command("commit", false)
            }
            KeyCode::Char('e') if matches!(self.route, Route::Workspace(_)) => {
                self.plan_workspace_command("end", false)
            }
            KeyCode::Char('D') if matches!(self.route, Route::Workspace(_)) => {
                self.plan_workspace_command("end", true)
            }
            _ => {}
        }
    }

    fn show_topology(&mut self) {
        if let Some(id) = self.selected_project.clone() {
            self.go(Route::Project(id));
            self.set_explorer_mode(ExplorerMode::Topology);
        }
    }

    fn navigate_back(&mut self) {
        if matches!(self.route, Route::Project(_) | Route::Branch(_, _))
            && self.explorer.mode != ExplorerMode::Topology
        {
            self.set_explorer_mode(ExplorerMode::Topology);
        } else {
            self.back();
        }
    }

    fn shift_focus(&mut self, delta: isize) {
        let count = self.focus_count();
        self.focus = (self.focus as isize + delta).rem_euclid(count as isize) as usize;
        self.compact_pane = self.focus;
        if self.visible_topology() {
            match (&self.route, self.focus) {
                (Route::Project(_), 0) => self.activate_selected_layer(),
                (Route::Project(_), 1) | (Route::Branch(_, _), 0) => self.activate_selected_graph(),
                _ => {}
            }
        }
    }

    fn visible_topology(&self) -> bool {
        matches!(self.route, Route::Project(_) | Route::Branch(_, _))
            && self.explorer.mode == ExplorerMode::Topology
    }

    fn tree_horizontal(&mut self, expanded: bool) {
        match (&self.route, self.explorer.mode, self.focus) {
            (Route::Project(_) | Route::Branch(_, _), ExplorerMode::Files, 0) => {
                self.toggle_explorer_directory(expanded)
            }
            (Route::Project(_), ExplorerMode::Topology, 1)
            | (Route::Branch(_, _), ExplorerMode::Topology, 0) => self.set_expanded(expanded),
            _ => {}
        }
    }

    fn toggle_visible_tree(&mut self) {
        match (&self.route, self.explorer.mode, self.focus) {
            (Route::Project(_) | Route::Branch(_, _), ExplorerMode::Files, 0) => {
                let expanded = self
                    .explorer
                    .selected_path
                    .as_ref()
                    .is_some_and(|path| self.explorer.expanded_dirs.contains(path));
                self.toggle_explorer_directory(!expanded);
            }
            (Route::Project(_), ExplorerMode::Topology, 1)
            | (Route::Branch(_, _), ExplorerMode::Topology, 0) => self.toggle_expanded(),
            _ => {}
        }
    }
}

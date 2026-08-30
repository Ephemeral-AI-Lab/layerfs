use crate::{
    app::{ActivityTab, App, Route},
    ExplorerMode,
};

impl App {
    pub(crate) fn apply_search(&mut self) {
        let needle = self.search.to_ascii_lowercase();
        if needle.is_empty() {
            return;
        }
        match self.route.clone() {
            Route::Projects => {
                self.selected_project = self
                    .projects
                    .iter()
                    .find(|project| project.name.as_str().to_ascii_lowercase().contains(&needle))
                    .map(|project| project.id.clone());
            }
            Route::Project(_) | Route::Branch(_, _)
                if self.explorer.mode != ExplorerMode::Topology =>
            {
                self.explorer.selected_path = self
                    .explorer_paths()
                    .into_iter()
                    .find(|path| path.to_ascii_lowercase().contains(&needle));
            }
            Route::Project(_) if self.focus == 0 => {
                self.selected_layer = self
                    .project
                    .as_ref()
                    .and_then(|project| {
                        project.layers.items.iter().find(|layer| {
                            format!("L{} {}", layer.number, layer.id)
                                .to_ascii_lowercase()
                                .contains(&needle)
                        })
                    })
                    .map(|layer| layer.id.clone());
                self.activate_selected_layer();
            }
            Route::Project(_) => {
                self.selected_graph = self
                    .graph_rows()
                    .into_iter()
                    .find(|row| {
                        format!("{} {}", row.label, row.detail)
                            .to_ascii_lowercase()
                            .contains(&needle)
                    })
                    .map(|row| row.target);
                self.activate_selected_graph();
            }
            Route::Branch(_, branch) => {
                self.selected_graph = self
                    .focused_branch_rows(&branch)
                    .into_iter()
                    .find(|row| {
                        format!("{} {}", row.label, row.detail)
                            .to_ascii_lowercase()
                            .contains(&needle)
                    })
                    .map(|row| row.target);
                self.activate_selected_graph();
            }
            Route::Workspaces(_) => {
                self.selected_workspace = self
                    .workspaces
                    .workspaces
                    .items
                    .iter()
                    .find(|workspace| {
                        format!("{} {}", workspace.id, workspace.branch_name)
                            .to_ascii_lowercase()
                            .contains(&needle)
                    })
                    .map(|workspace| workspace.id.clone());
            }
            Route::Workspace(_) => {
                self.selected_workspace_path = self
                    .workspace_items()
                    .into_iter()
                    .find(|item| item.to_ascii_lowercase().contains(&needle));
            }
            Route::Activity(ActivityTab::Operations) => {
                self.selected_operation = self
                    .activity
                    .operations
                    .items
                    .iter()
                    .find(|operation| operation.title.to_ascii_lowercase().contains(&needle))
                    .map(|operation| operation.id.clone());
            }
            Route::Activity(ActivityTab::Storage) => {}
            Route::Diff(_) => {
                self.selected_diff_path = self.diff.as_ref().and_then(|diff| {
                    diff.entries
                        .items
                        .iter()
                        .find(|entry| entry.path.to_ascii_lowercase().contains(&needle))
                        .map(|entry| entry.path.clone())
                });
            }
        }
    }
}

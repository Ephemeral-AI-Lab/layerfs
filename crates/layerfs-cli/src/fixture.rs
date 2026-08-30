use crate::model::{
    ActivitySnapshot, BranchOrigin, BranchRelation, BranchView, CommitView, ConflictView,
    DiffChange, DiffEntryView, DiffSnapshot, LayerCoverage, LayerView, OperationReceipt,
    OperationState, OperationView, Page, PageRequest, ProjectRelation, ProjectSnapshot,
    ProjectSummary, RemotePlacement, SemanticAction, StorageSnapshot, WorkspaceSnapshot,
    WorkspaceState, WorkspaceView,
};
use crate::workspace::{
    canonical_tree, seed_tree, CanonicalObject, Tree, TreeEntry, WorkspaceRecord,
};
use crate::{
    BranchId, CliError, CliResult, CommitId, ConflictId, EntityName, ExecutionId, LayerId,
    LayerStackId, ObjectId, OperationId, WorkspaceId,
};
use std::collections::{HashMap, HashSet};

#[derive(Clone)]
pub(crate) struct ProjectRecord {
    pub id: LayerStackId,
    pub name: EntityName,
    pub authority_layers: u16,
    pub work_layers: Option<u16>,
    pub mode: Option<RemotePlacement>,
    pub complete_roots: u16,
    pub authority_available: bool,
    pub observed: String,
}

#[derive(Clone)]
pub(crate) struct LayerRecord {
    pub id: LayerId,
    pub project_id: LayerStackId,
    pub number: u16,
    pub source: Option<(BranchId, CommitId)>,
    pub root: ObjectId,
    pub tree: Tree,
    pub objects: Vec<CanonicalObject>,
}

#[derive(Clone)]
pub(crate) struct BranchRecord {
    pub id: BranchId,
    pub project_id: LayerStackId,
    pub name: EntityName,
    pub origin: BranchOrigin,
    pub authority_head: Option<u16>,
    pub work_head: Option<u16>,
    pub remote_complete_through: Option<u16>,
    pub boundary_commit: Option<CommitId>,
    pub effective_base: Option<LayerId>,
    pub relation: BranchRelation,
    pub commits: Vec<CommitRecord>,
}

#[derive(Clone)]
pub(crate) struct CommitRecord {
    pub id: CommitId,
    pub number: u16,
    pub authority: bool,
    pub work: bool,
    pub inherited: bool,
    pub owned: bool,
    pub accepted_layer: Option<LayerId>,
    pub root: ObjectId,
    pub tree: Tree,
    pub objects: Vec<CanonicalObject>,
}

impl BranchRecord {
    pub(crate) fn commit(&self, number: u16) -> Option<&CommitRecord> {
        self.commits.iter().find(|commit| commit.number == number)
    }

    pub(crate) fn commit_id(&self, number: u16) -> Option<CommitId> {
        self.commit(number).map(|commit| commit.id.clone())
    }
}

#[derive(Clone)]
pub(crate) struct MockState {
    pub projects: Vec<ProjectRecord>,
    pub layers: Vec<LayerRecord>,
    pub branches: Vec<BranchRecord>,
    pub workspaces: Vec<WorkspaceRecord>,
    pub operations: Vec<OperationView>,
    pub storage: StorageSnapshot,
    pub next_branch: u16,
    pub next_workspace: u16,
    pub last_operation_elapsed_micros: Option<u64>,
}

impl MockState {
    pub fn empty() -> Self {
        let mut state = Self::demo();
        state.projects.clear();
        state.layers.clear();
        state.branches.clear();
        state.workspaces.clear();
        state.operations.clear();
        state.storage.projects = 0;
        state.storage.layers = 0;
        state.storage.authority_branches = 0;
        state.storage.remote_branches = 0;
        state.storage.local_branches = 0;
        state
    }

    pub fn demo() -> Self {
        let api = LayerStackId::from("SA-91");
        let web = LayerStackId::from("SW-20");
        let eval = LayerStackId::from("SE-44");
        let mut state = Self {
            projects: vec![
                ProjectRecord {
                    id: api.clone(),
                    name: name("api-server"),
                    authority_layers: 19,
                    work_layers: Some(18),
                    mode: Some(RemotePlacement::Replica),
                    complete_roots: 18,
                    authority_available: true,
                    observed: "2s ago".into(),
                },
                ProjectRecord {
                    id: web.clone(),
                    name: name("web-client"),
                    authority_layers: 11,
                    work_layers: Some(11),
                    mode: Some(RemotePlacement::Reference),
                    complete_roots: 9,
                    authority_available: true,
                    observed: "4s ago".into(),
                },
                ProjectRecord {
                    id: eval.clone(),
                    name: name("evaluation"),
                    authority_layers: 7,
                    work_layers: None,
                    mode: None,
                    complete_roots: 0,
                    authority_available: false,
                    observed: "last seen 18s ago".into(),
                },
            ],
            layers: Vec::new(),
            branches: Vec::new(),
            workspaces: Vec::new(),
            operations: operations(),
            storage: StorageSnapshot {
                layerstack_store_id: "LS-91".into(),
                branch_store_id: "BS-31".into(),
                projects: 3,
                layers: 37,
                authority_branches: 10,
                remote_branches: 4,
                local_branches: 12,
                shared_objects: 184_217,
                authority_bytes: 5_121_843_200,
                branch_bytes: 1_902_362_624,
                unique_bytes: 5_512_822_784,
                replica_roots: 32,
                reference_scopes: 4,
                analysis_available: true,
            },
            next_branch: 90,
            next_workspace: 90,
            last_operation_elapsed_micros: None,
        };

        state.add_branch(branch(
            "B-seed",
            &api,
            "seed",
            BranchOrigin::Layer(layer_id("A", 1)),
            Some(14),
            None,
            BranchRelation::AuthorityOnly,
            14,
        ));
        state.add_branch(branch(
            "B-main",
            &api,
            "main",
            BranchOrigin::Layer(layer_id("A", 15)),
            Some(42),
            Some(37),
            BranchRelation::RemotePullBehind {
                mode: RemotePlacement::Replica,
                commits: 5,
            },
            42,
        ));
        state.add_branch(branch(
            "B-search-a",
            &api,
            "search-a",
            BranchOrigin::Commit(BranchId::from("B-main"), commit_id("B-main", 35)),
            Some(5),
            Some(8),
            BranchRelation::LocalPushAhead { commits: 3 },
            8,
        ));
        state.add_branch(branch(
            "B-search-a1",
            &api,
            "search-a1",
            BranchOrigin::Commit(BranchId::from("B-search-a"), commit_id("B-search-a", 3)),
            None,
            Some(2),
            BranchRelation::LocalOnly,
            2,
        ));
        state.add_branch(branch(
            "B-search-a1x",
            &api,
            "search-a1x",
            BranchOrigin::Commit(BranchId::from("B-search-a1"), commit_id("B-search-a1", 2)),
            None,
            Some(1),
            BranchRelation::LocalOnly,
            1,
        ));
        state.add_branch(branch(
            "B-search-a1x-i",
            &api,
            "search-a1x-i",
            BranchOrigin::Commit(BranchId::from("B-search-a1x"), commit_id("B-search-a1x", 1)),
            None,
            Some(1),
            BranchRelation::LocalOnly,
            1,
        ));
        state.add_branch(branch(
            "B-search-a2",
            &api,
            "search-a2",
            BranchOrigin::Commit(BranchId::from("B-search-a"), commit_id("B-search-a", 4)),
            None,
            Some(2),
            BranchRelation::LocalOnly,
            2,
        ));
        state.add_branch(branch(
            "B-search-b",
            &api,
            "search-b",
            BranchOrigin::Commit(BranchId::from("B-main"), commit_id("B-main", 37)),
            Some(2),
            Some(3),
            BranchRelation::LocalPushAhead { commits: 1 },
            3,
        ));
        state.add_branch(branch(
            "B-rollout-4",
            &api,
            "rollout-4",
            BranchOrigin::Layer(layer_id("A", 15)),
            Some(8),
            Some(8),
            BranchRelation::LocalCurrent,
            8,
        ));
        state.add_branch(branch(
            "B-search-final",
            &api,
            "search-final",
            BranchOrigin::Commit(BranchId::from("B-main"), commit_id("B-main", 36)),
            Some(8),
            None,
            BranchRelation::AuthorityOnly,
            8,
        ));
        state.add_branch(branch(
            "B-rollout-7",
            &api,
            "rollout-7",
            BranchOrigin::Layer(layer_id("A", 18)),
            Some(9),
            None,
            BranchRelation::AuthorityOnly,
            9,
        ));
        state.add_branch(branch(
            "B-baseline",
            &api,
            "baseline",
            BranchOrigin::Layer(layer_id("A", 15)),
            Some(12),
            Some(12),
            BranchRelation::RemoteCurrent {
                mode: RemotePlacement::Reference,
            },
            12,
        ));
        state.add_branch(branch(
            "B-rollout-x",
            &api,
            "rollout-x",
            BranchOrigin::Layer(layer_id("A", 15)),
            None,
            Some(6),
            BranchRelation::LocalOnly,
            6,
        ));
        state.add_branch(branch(
            "B-local-sync",
            &api,
            "local-sync",
            BranchOrigin::Layer(layer_id("A", 15)),
            Some(4),
            Some(4),
            BranchRelation::LocalCurrent,
            4,
        ));
        state.add_branch(branch(
            "B-hijacked",
            &api,
            "hijacked",
            BranchOrigin::Layer(layer_id("A", 15)),
            Some(10),
            Some(8),
            BranchRelation::AuthorityAhead { commits: 2 },
            10,
        ));
        state.add_branch(branch(
            "B-remote-new",
            &api,
            "remote-new",
            BranchOrigin::Commit(BranchId::from("B-main"), commit_id("B-main", 40)),
            Some(3),
            None,
            BranchRelation::AuthorityOnly,
            3,
        ));
        state.add_branch(branch(
            "B-empty",
            &api,
            "empty-rollout",
            BranchOrigin::Layer(layer_id("A", 18)),
            None,
            None,
            BranchRelation::LocalOnly,
            0,
        ));

        state.add_branch(branch(
            "BW-seed",
            &web,
            "seed",
            BranchOrigin::Layer(layer_id("SW", 1)),
            Some(10),
            None,
            BranchRelation::AuthorityOnly,
            10,
        ));
        state.add_branch(branch(
            "BW-main",
            &web,
            "main",
            BranchOrigin::Layer(layer_id("SW", 11)),
            Some(9),
            Some(9),
            BranchRelation::RemoteCurrent {
                mode: RemotePlacement::Reference,
            },
            9,
        ));
        state.add_branch(branch(
            "BW-redesign",
            &web,
            "redesign",
            BranchOrigin::Commit(BranchId::from("BW-main"), commit_id("BW-main", 3)),
            None,
            Some(3),
            BranchRelation::LocalOnly,
            3,
        ));
        state.add_branch(branch(
            "BE-seed",
            &eval,
            "seed",
            BranchOrigin::Layer(layer_id("SE", 1)),
            Some(6),
            None,
            BranchRelation::AuthorityOnly,
            6,
        ));
        state.add_branch(branch(
            "BE-release",
            &eval,
            "release",
            BranchOrigin::Layer(layer_id("SE", 7)),
            Some(5),
            None,
            BranchRelation::AuthorityOnly,
            5,
        ));

        state.layers = api_layers(&state.branches)
            .into_iter()
            .chain(simple_layers(&web, "SW", 11, "BW-seed"))
            .chain(simple_layers(&eval, "SE", 7, "BE-seed"))
            .collect();
        state.mark_accepted_edges();
        state.workspaces = workspaces(&state);
        state
    }

    fn add_branch(&mut self, branch: BranchRecord) {
        self.branches.push(branch);
    }

    fn mark_accepted_edges(&mut self) {
        let accepted = self
            .layers
            .iter()
            .filter_map(|layer| {
                layer
                    .source
                    .clone()
                    .map(|source| (source, layer.id.clone()))
            })
            .collect::<HashMap<_, _>>();
        for branch in &mut self.branches {
            for commit in &mut branch.commits {
                commit.accepted_layer = accepted
                    .get(&(branch.id.clone(), commit.id.clone()))
                    .cloned();
            }
        }
    }

    pub fn project_summaries(&self, page: &PageRequest) -> Page<ProjectSummary> {
        paginate(
            self.projects
                .iter()
                .map(|project| self.project_summary(project))
                .collect(),
            page,
            "projects",
        )
    }

    pub fn project_snapshot(
        &self,
        id: &LayerStackId,
        page: &PageRequest,
    ) -> Option<ProjectSnapshot> {
        let project = self.projects.iter().find(|project| &project.id == id)?;
        let layers = self
            .layers
            .iter()
            .filter(|layer| &layer.project_id == id)
            .map(|layer| self.layer_view(project, layer))
            .collect();
        let branches = self
            .branches
            .iter()
            .filter(|branch| &branch.project_id == id)
            .map(|branch| self.branch_view(branch, page))
            .collect();
        Some(ProjectSnapshot {
            project: self.project_summary(project),
            layers: paginate(layers, page, "layers"),
            branches: paginate(branches, page, "branches"),
        })
    }

    pub fn workspace_snapshot(
        &self,
        project: Option<&LayerStackId>,
        page: &PageRequest,
    ) -> WorkspaceSnapshot {
        WorkspaceSnapshot {
            workspaces: paginate(
                self.workspaces
                    .iter()
                    .filter(|workspace| project.is_none_or(|id| &workspace.project_id == id))
                    .map(|workspace| workspace.view.clone())
                    .map(|mut workspace| {
                        if let Some(branch) = self.branch(&workspace.branch_id) {
                            workspace.branch_relation = branch.relation.clone();
                        }
                        workspace
                    })
                    .collect(),
                page,
                "workspaces",
            ),
        }
    }

    pub fn activity_snapshot(&self, page: &PageRequest) -> ActivitySnapshot {
        let mut storage = self.storage.clone();
        storage.projects = self.projects.len() as u16;
        storage.layers = self.layers.len() as u16;
        storage.authority_branches = self
            .branches
            .iter()
            .filter(|branch| branch.authority_head.is_some())
            .count() as u16;
        storage.remote_branches = self
            .branches
            .iter()
            .filter(|branch| {
                matches!(
                    branch.relation,
                    BranchRelation::RemoteCurrent { .. } | BranchRelation::RemotePullBehind { .. }
                )
            })
            .count() as u16;
        storage.local_branches = self
            .branches
            .iter()
            .filter(|branch| {
                matches!(
                    branch.relation,
                    BranchRelation::LocalOnly
                        | BranchRelation::LocalCurrent
                        | BranchRelation::LocalPushAhead { .. }
                        | BranchRelation::AuthorityAhead { .. }
                        | BranchRelation::Diverged
                )
            })
            .count() as u16;
        ActivitySnapshot {
            operations: paginate(self.operations.clone(), page, "operations"),
            storage,
        }
    }

    pub fn diff_snapshot(
        &self,
        title: &str,
        from: &str,
        to: &str,
        page: &PageRequest,
    ) -> DiffSnapshot {
        let mut entries = vec![
            DiffEntryView {
                path: "src/storage.rs".into(),
                change: DiffChange::Modify,
                aspects: vec!["content".into(), "mode".into()],
                before: Some("old single-point transfer".into()),
                after: Some("through-history transfer".into()),
            },
            DiffEntryView {
                path: "src/pull/history.rs".into(),
                change: DiffChange::Add,
                aspects: vec!["content".into()],
                before: None,
                after: Some("bounded ancestry traversal".into()),
            },
            DiffEntryView {
                path: "src/legacy.rs".into(),
                change: DiffChange::Remove,
                aspects: vec!["content".into()],
                before: Some("compatibility facade".into()),
                after: None,
            },
            DiffEntryView {
                path: "tests/hard-links.rs".into(),
                change: DiffChange::Modify,
                aspects: vec!["metadata".into(), "hard-links".into()],
                before: Some("links 2".into()),
                after: Some("links 3".into()),
            },
        ];
        for index in 0..132 {
            entries.push(DiffEntryView {
                path: format!("fixtures/generated/file-{index:03}.txt"),
                change: DiffChange::Modify,
                aspects: vec!["content".into()],
                before: Some(format!("old-{index}")),
                after: Some(format!("new-{index}")),
            });
        }
        DiffSnapshot {
            title: title.into(),
            from: from.into(),
            to: to.into(),
            entries: paginate(entries, page, "diff"),
        }
    }

    pub fn branch(&self, id: &BranchId) -> Option<&BranchRecord> {
        self.branches.iter().find(|branch| &branch.id == id)
    }

    pub fn branch_mut(&mut self, id: &BranchId) -> Option<&mut BranchRecord> {
        self.branches.iter_mut().find(|branch| &branch.id == id)
    }

    pub fn find_branch(&self, value: &str) -> Option<&BranchRecord> {
        self.branches
            .iter()
            .find(|branch| branch.id.as_str() == value)
    }

    pub fn find_layer(&self, value: &str) -> Option<&LayerRecord> {
        self.layers.iter().find(|layer| layer.id.as_str() == value)
    }

    pub(crate) fn layer_id(&self, project: &LayerStackId, number: u16) -> Option<LayerId> {
        self.layers
            .iter()
            .find(|layer| &layer.project_id == project && layer.number == number)
            .map(|layer| layer.id.clone())
    }

    fn commit_number(&self, id: &CommitId) -> Option<u16> {
        self.branches
            .iter()
            .flat_map(|branch| &branch.commits)
            .find(|commit| &commit.id == id)
            .map(|commit| commit.number)
    }

    pub(crate) fn resolve_commit(
        &self,
        branch: &BranchRecord,
        value: &str,
    ) -> CliResult<CommitRecord> {
        if let Some(commit) = branch
            .commits
            .iter()
            .find(|commit| commit.id.as_str() == value || format!("C{}", commit.number) == value)
        {
            return Ok(commit.clone());
        }
        let boundary = branch
            .boundary_commit
            .as_ref()
            .filter(|commit| commit.as_str() == value)
            .ok_or_else(|| CliError::NotFound(format!("Commit {value} in {}", branch.name)))?;
        self.branches
            .iter()
            .flat_map(|record| &record.commits)
            .find(|commit| &commit.id == boundary)
            .cloned()
            .ok_or_else(|| CliError::NotFound(boundary.to_string()))
    }

    pub fn relation_name_exists(&self, project: &LayerStackId, name: &EntityName) -> bool {
        self.branches
            .iter()
            .any(|branch| &branch.project_id == project && &branch.name == name)
    }

    fn project_summary(&self, project: &ProjectRecord) -> ProjectSummary {
        let branches = self
            .branches
            .iter()
            .filter(|branch| branch.project_id == project.id)
            .collect::<Vec<_>>();
        let relation = if !project.authority_available {
            ProjectRelation::AuthorityUnknown
        } else {
            match (project.work_layers, project.mode) {
                (None, _) => ProjectRelation::NotPulled,
                (Some(work), Some(mode)) if work == project.authority_layers => {
                    ProjectRelation::Current { mode }
                }
                (Some(work), Some(mode)) => ProjectRelation::PullBehind {
                    mode,
                    layers: project.authority_layers.saturating_sub(work),
                },
                _ => ProjectRelation::Integrity,
            }
        };
        let workspaces = self
            .workspaces
            .iter()
            .filter(|workspace| workspace.project_id == project.id)
            .collect::<Vec<_>>();
        ProjectSummary {
            id: project.id.clone(),
            name: project.name.clone(),
            authority_head: self
                .layer_id(&project.id, project.authority_layers)
                .expect("project authority Layer"),
            authority_number: project.authority_layers,
            work_boundary: project
                .work_layers
                .and_then(|number| self.layer_id(&project.id, number)),
            work_number: project.work_layers,
            complete_roots: project.complete_roots,
            relation,
            remote_branches: branches
                .iter()
                .filter(|branch| {
                    matches!(
                        branch.relation,
                        BranchRelation::RemoteCurrent { .. }
                            | BranchRelation::RemotePullBehind { .. }
                    )
                })
                .count() as u16,
            local_branches: branches
                .iter()
                .filter(|branch| {
                    matches!(
                        branch.relation,
                        BranchRelation::LocalOnly
                            | BranchRelation::LocalCurrent
                            | BranchRelation::LocalPushAhead { .. }
                            | BranchRelation::AuthorityAhead { .. }
                            | BranchRelation::Diverged
                    )
                })
                .count() as u16,
            workspaces: workspaces.len() as u16,
            dirty_workspaces: workspaces
                .iter()
                .filter(|workspace| workspace.state == WorkspaceState::Dirty)
                .count() as u16,
            running_workspaces: workspaces
                .iter()
                .filter(|workspace| workspace.state == WorkspaceState::Running)
                .count() as u16,
            busy_workspaces: workspaces
                .iter()
                .filter(|workspace| workspace.state == WorkspaceState::Busy)
                .count() as u16,
            retained_workspaces: workspaces
                .iter()
                .filter(|workspace| workspace.state == WorkspaceState::HeadMoved)
                .count() as u16,
            observed: project.observed.clone(),
        }
    }

    fn layer_view(&self, project: &ProjectRecord, layer: &LayerRecord) -> LayerView {
        let work = project
            .work_layers
            .is_some_and(|number| layer.number <= number);
        let coverage = if !work {
            LayerCoverage::NotPulled
        } else if layer.number <= project.complete_roots {
            LayerCoverage::Complete
        } else {
            LayerCoverage::ParentBacked
        };
        LayerView {
            id: layer.id.clone(),
            number: layer.number,
            parent: layer
                .number
                .checked_sub(1)
                .filter(|number| *number > 0)
                .and_then(|number| self.layer_id(&project.id, number)),
            root: layer.root.clone(),
            source: layer.source.clone(),
            authority: true,
            work,
            coverage,
            direct_branches: self
                .branches
                .iter()
                .filter(|branch| branch.origin == BranchOrigin::Layer(layer.id.clone()))
                .count() as u16,
            authority_head: layer.number == project.authority_layers,
            work_boundary: project.work_layers == Some(layer.number),
        }
    }

    fn branch_view(&self, branch: &BranchRecord, page: &PageRequest) -> BranchView {
        let child_counts = self.child_counts();
        let workspaces = self
            .workspaces
            .iter()
            .filter(|workspace| workspace.branch_id == branch.id)
            .collect::<Vec<_>>();
        let boundary_present = !matches!(
            branch.relation,
            BranchRelation::AuthorityOnly | BranchRelation::Integrity
        );
        let mut commits = self.inherited_views(branch, boundary_present);
        commits.extend(branch.commits.iter().map(|commit| {
            CommitView {
                id: commit.id.clone(),
                number: commit.number,
                parent: if commit.number > 1 {
                    branch.commit_id(commit.number - 1)
                } else {
                    branch.boundary_commit.clone()
                },
                root: commit.root.clone(),
                base_layer: self.branch_base_layer(branch),
                authority: commit.authority,
                work: commit.work,
                inherited: commit.inherited,
                owned: commit.owned,
                authority_head: branch.authority_head == Some(commit.number),
                work_head: branch.work_head == Some(commit.number),
                child_branches: *child_counts
                    .get(&(branch.id.clone(), commit.id.clone()))
                    .unwrap_or(&0),
                workspaces: workspaces
                    .iter()
                    .filter(|workspace| workspace.anchor_commit.as_ref() == Some(&commit.id))
                    .map(|workspace| workspace.id.clone())
                    .collect(),
                accepted_layer: commit.accepted_layer.clone(),
                actions: Vec::new(),
            }
        }));
        if branch.work_head.is_none() && boundary_present {
            if let Some(boundary) = branch.boundary_commit.as_ref() {
                if let Some(commit) = commits.iter_mut().find(|commit| &commit.id == boundary) {
                    commit.work_head = true;
                }
            }
        }
        let effective_head = branch
            .work_head
            .and_then(|number| branch.commit_id(number))
            .or_else(|| {
                boundary_present
                    .then(|| branch.boundary_commit.clone())
                    .flatten()
            });
        let writable = matches!(
            branch.relation,
            BranchRelation::LocalOnly
                | BranchRelation::LocalCurrent
                | BranchRelation::LocalPushAhead { .. }
        );
        for commit in &mut commits {
            commit.workspaces = workspaces
                .iter()
                .filter(|workspace| workspace.anchor_commit.as_ref() == Some(&commit.id))
                .map(|workspace| workspace.id.clone())
                .collect();
            if commit.work {
                commit.actions.extend([
                    SemanticAction::Fork,
                    SemanticAction::Diff,
                    SemanticAction::Materialize,
                ]);
            }
            if writable
                && effective_head.as_ref() == Some(&commit.id)
                && commit.workspaces.is_empty()
            {
                commit.actions.push(SemanticAction::Workspace);
            }
        }
        let commits = paginate(commits, page, &format!("commits:{}", branch.id));
        BranchView {
            id: branch.id.clone(),
            project_id: branch.project_id.clone(),
            name: branch.name.clone(),
            origin: branch.origin.clone(),
            authority_head: branch
                .authority_head
                .and_then(|number| branch.commit_id(number)),
            authority_number: branch.authority_head,
            work_head: branch
                .work_head
                .and_then(|number| branch.commit_id(number))
                .or_else(|| boundary_present.then(|| branch.boundary_commit.clone()).flatten()),
            work_number: branch.work_head.or_else(|| {
                boundary_present
                    .then(|| {
                        branch
                            .boundary_commit
                            .as_ref()
                            .and_then(|id| self.commit_number(id))
                    })
                    .flatten()
            }),
            remote_complete_through: branch
                .remote_complete_through
                .and_then(|number| branch.commit_id(number)),
            visible_roots_complete: match &branch.relation {
                BranchRelation::RemoteCurrent { .. }
                | BranchRelation::RemotePullBehind { .. } => branch.work_head.is_some_and(|head| {
                    branch.remote_complete_through.is_some_and(|complete| complete >= head)
                }),
                BranchRelation::AuthorityOnly | BranchRelation::Integrity => false,
                _ => true,
            },
            relation: branch.relation.clone(),
            commits: commits.items,
            commits_next: commits.next,
            direct_children: self
                .branches
                .iter()
                .filter(|child| matches!(&child.origin, BranchOrigin::Commit(id, _) if id == &branch.id))
                .count() as u16,
            descendant_count: self.descendants(&branch.id, &mut HashSet::new()),
            workspace_count: workspaces.len() as u16,
            actions: self.branch_actions(branch),
        }
    }

    fn inherited_views(&self, branch: &BranchRecord, work: bool) -> Vec<CommitView> {
        let BranchOrigin::Commit(parent_id, boundary) = &branch.origin else {
            return Vec::new();
        };
        let Some(parent) = self.branch(parent_id) else {
            return Vec::new();
        };
        let mut views = self.inherited_views(parent, work);
        if parent.boundary_commit.as_ref() == Some(boundary) {
            return views;
        }
        let boundary_number = parent
            .commits
            .iter()
            .find(|commit| &commit.id == boundary)
            .map_or(0, |commit| commit.number);
        views.extend(
            parent
                .commits
                .iter()
                .filter(|commit| commit.number <= boundary_number)
                .map(|commit| CommitView {
                    id: commit.id.clone(),
                    number: commit.number,
                    parent: if commit.number > 1 {
                        parent.commit_id(commit.number - 1)
                    } else {
                        parent.boundary_commit.clone()
                    },
                    root: commit.root.clone(),
                    base_layer: self.branch_base_layer(parent),
                    authority: commit.authority,
                    work,
                    inherited: true,
                    owned: false,
                    authority_head: false,
                    work_head: false,
                    child_branches: 0,
                    workspaces: Vec::new(),
                    accepted_layer: commit.accepted_layer.clone(),
                    actions: Vec::new(),
                }),
        );
        views
    }

    fn branch_actions(&self, branch: &BranchRecord) -> Vec<SemanticAction> {
        let mut actions = Vec::new();
        match &branch.relation {
            BranchRelation::AuthorityOnly
            | BranchRelation::RemoteCurrent { .. }
            | BranchRelation::RemotePullBehind { .. } => actions.push(SemanticAction::Pull),
            BranchRelation::LocalOnly
            | BranchRelation::LocalCurrent
            | BranchRelation::LocalPushAhead { .. } => actions.push(SemanticAction::Push),
            BranchRelation::AuthorityAhead { .. }
            | BranchRelation::Diverged
            | BranchRelation::Integrity => {}
        }
        if !matches!(
            branch.relation,
            BranchRelation::AuthorityOnly | BranchRelation::Integrity
        ) {
            actions.extend([SemanticAction::Fork, SemanticAction::Diff]);
        }
        let head = branch
            .work_head
            .and_then(|number| branch.commit_id(number))
            .map(|commit| (branch.id.clone(), commit));
        let accepted = head.as_ref().is_some_and(|head| {
            self.layers
                .iter()
                .any(|layer| layer.source.as_ref() == Some(head))
        });
        if branch.relation == BranchRelation::LocalCurrent && !accepted {
            actions.push(SemanticAction::Add);
        }
        if branch.relation == BranchRelation::LocalOnly
            && branch.work_head.is_none()
            && branch.boundary_commit.is_none()
            && !self
                .workspaces
                .iter()
                .any(|workspace| workspace.branch_id == branch.id)
        {
            actions.push(SemanticAction::Workspace);
        }
        actions
    }

    pub(crate) fn branch_base_layer(&self, branch: &BranchRecord) -> LayerId {
        if let Some(base) = &branch.effective_base {
            return base.clone();
        }
        match &branch.origin {
            BranchOrigin::Layer(layer) => layer.clone(),
            BranchOrigin::Commit(parent, _) => self
                .branch(parent)
                .map(|record| self.branch_base_layer(record))
                .unwrap_or_else(|| LayerId::from("L-unknown")),
        }
    }

    fn child_counts(&self) -> HashMap<(BranchId, CommitId), u16> {
        let mut counts = HashMap::new();
        for branch in &self.branches {
            if let BranchOrigin::Commit(parent, commit) = &branch.origin {
                *counts.entry((parent.clone(), commit.clone())).or_default() += 1;
            }
        }
        counts
    }

    fn descendants(&self, id: &BranchId, visited: &mut HashSet<BranchId>) -> u16 {
        if !visited.insert(id.clone()) {
            return 0;
        }
        self.branches
            .iter()
            .filter(
                |branch| matches!(&branch.origin, BranchOrigin::Commit(parent, _) if parent == id),
            )
            .map(|branch| 1 + self.descendants(&branch.id, visited))
            .sum()
    }
}

fn name(value: &str) -> EntityName {
    EntityName::parse(value).expect("fixture name")
}

fn layer_id(prefix: &str, number: u16) -> LayerId {
    LayerId::new(format!("L-{prefix}-{number:02}"))
}

pub(crate) fn layer_for(project: &ProjectRecord, number: u16) -> LayerId {
    let prefix = match project.id.as_str() {
        "SA-91" => "A",
        "SW-20" => "SW",
        "SE-44" => "SE",
        value => value,
    };
    layer_id(prefix, number)
}

pub(crate) fn commit_id(branch: &str, number: u16) -> CommitId {
    CommitId::new(format!("C-{branch}-{number:02}"))
}

#[allow(clippy::too_many_arguments)]
fn branch(
    id: &str,
    project: &LayerStackId,
    branch_name: &str,
    origin: BranchOrigin,
    authority_head: Option<u16>,
    work_head: Option<u16>,
    relation: BranchRelation,
    commit_count: u16,
) -> BranchRecord {
    let boundary_commit = match &origin {
        BranchOrigin::Commit(_, commit) => Some(commit.clone()),
        BranchOrigin::Layer(_) => None,
    };
    let effective_base = match &origin {
        BranchOrigin::Layer(layer) => Some(layer.clone()),
        BranchOrigin::Commit(_, _) => None,
    };
    BranchRecord {
        id: BranchId::from(id),
        project_id: project.clone(),
        name: name(branch_name),
        origin,
        authority_head,
        work_head,
        remote_complete_through: match &relation {
            BranchRelation::RemoteCurrent {
                mode: RemotePlacement::Replica,
            }
            | BranchRelation::RemotePullBehind {
                mode: RemotePlacement::Replica,
                ..
            } => work_head,
            _ => None,
        },
        boundary_commit,
        effective_base,
        relation,
        commits: (1..=commit_count)
            .map(|number| {
                let tree = seed_tree(&format!("{id}-C{number}"));
                let (root, objects) = canonical_tree(&tree);
                CommitRecord {
                    id: commit_id(id, number),
                    number,
                    authority: authority_head.is_some_and(|head| number <= head),
                    work: work_head.is_some_and(|head| number <= head),
                    inherited: false,
                    owned: true,
                    accepted_layer: None,
                    root,
                    tree,
                    objects,
                }
            })
            .collect(),
    }
}

pub(crate) fn trailing_number(value: &str) -> Option<u16> {
    value.rsplit('-').next()?.parse().ok()
}

fn api_layers(branches: &[BranchRecord]) -> Vec<LayerRecord> {
    let project = LayerStackId::from("SA-91");
    let mut layers = Vec::new();
    for number in 1u16..=19 {
        let source = match number {
            1 => None,
            16 => Some((BranchId::from("B-rollout-4"), commit_id("B-rollout-4", 8))),
            17 => Some((BranchId::from("B-main"), commit_id("B-main", 38))),
            18 => Some((
                BranchId::from("B-search-final"),
                commit_id("B-search-final", 8),
            )),
            19 => Some((BranchId::from("B-rollout-7"), commit_id("B-rollout-7", 9))),
            value => Some((
                BranchId::from("B-seed"),
                commit_id("B-seed", value.saturating_sub(1)),
            )),
        };
        if let Some((branch, commit)) = &source {
            assert!(branches.iter().any(|record| {
                record.id == *branch && record.commits.iter().any(|item| item.id == *commit)
            }));
        }
        let id = layer_id("A", number);
        let tree = source
            .as_ref()
            .and_then(|(branch, commit)| {
                branches
                    .iter()
                    .find(|record| &record.id == branch)
                    .and_then(|record| record.commits.iter().find(|item| &item.id == commit))
                    .map(|commit| commit.tree.clone())
            })
            .unwrap_or_else(|| seed_tree(id.as_str()));
        let (root, objects) = canonical_tree(&tree);
        layers.push(LayerRecord {
            id,
            project_id: project.clone(),
            number,
            source,
            root,
            tree,
            objects,
        });
    }
    layers
}

fn simple_layers(
    project: &LayerStackId,
    prefix: &str,
    count: u16,
    source_branch: &str,
) -> impl Iterator<Item = LayerRecord> {
    let project = project.clone();
    let prefix = prefix.to_string();
    let source_branch = source_branch.to_string();
    (1..=count).map(move |number| {
        let id = layer_id(&prefix, number);
        let tree = seed_tree(id.as_str());
        let (root, objects) = canonical_tree(&tree);
        LayerRecord {
            id,
            project_id: project.clone(),
            number,
            source: (number > 1).then(|| {
                (
                    BranchId::new(source_branch.clone()),
                    commit_id(&source_branch, number - 1),
                )
            }),
            root,
            tree,
            objects,
        }
    })
}

fn workspaces(state: &MockState) -> Vec<WorkspaceRecord> {
    vec![
        workspace(
            state,
            "W9",
            "B-search-a",
            Some(commit_id("B-search-a", 8)),
            None,
            WorkspaceState::Dirty,
            "FUSE",
            "host",
            "/workspaces/search-a",
        ),
        workspace(
            state,
            "W14",
            "B-search-a1",
            Some(commit_id("B-search-a1", 2)),
            None,
            WorkspaceState::Running,
            "FUSE",
            "container",
            "/workspace/search-a1",
        ),
        workspace(
            state,
            "W22",
            "BW-redesign",
            Some(commit_id("BW-redesign", 3)),
            None,
            WorkspaceState::Clean,
            "materialize",
            "host",
            "/tmp/redesign",
        ),
        workspace(
            state,
            "W31",
            "B-hijacked",
            Some(commit_id("B-hijacked", 8)),
            None,
            WorkspaceState::HeadMoved,
            "FUSE",
            "host",
            "/workspaces/hijacked",
        ),
        workspace(
            state,
            "W44",
            "B-rollout-x",
            Some(commit_id("B-rollout-x", 6)),
            None,
            WorkspaceState::Busy,
            "FUSE",
            "container",
            "/workspace/rollout-x",
        ),
        workspace(
            state,
            "W50",
            "B-empty",
            None,
            Some(layer_id("A", 18)),
            WorkspaceState::Clean,
            "materialize",
            "host",
            "/tmp/empty-rollout",
        ),
    ]
}

#[allow(clippy::too_many_arguments)]
fn workspace(
    state: &MockState,
    id: &str,
    branch_id: &str,
    anchor_commit: Option<CommitId>,
    anchor_layer: Option<LayerId>,
    status: WorkspaceState,
    projection: &str,
    placement: &str,
    mount: &str,
) -> WorkspaceRecord {
    let branch = state
        .branch(&BranchId::from(branch_id))
        .expect("fixture branch");
    let project = state
        .projects
        .iter()
        .find(|project| project.id == branch.project_id)
        .expect("fixture project");
    let anchor = anchor_commit
        .as_ref()
        .and_then(|id| {
            state
                .branches
                .iter()
                .flat_map(|branch| &branch.commits)
                .find(|commit| &commit.id == id)
                .map(|commit| commit.tree.clone())
        })
        .or_else(|| {
            anchor_layer.as_ref().and_then(|id| {
                state
                    .layers
                    .iter()
                    .find(|layer| &layer.id == id)
                    .map(|layer| layer.tree.clone())
            })
        })
        .unwrap_or_else(|| seed_tree(id));
    let (anchor_root, _) = canonical_tree(&anchor);
    let view = WorkspaceView {
        id: WorkspaceId::from(id),
        project_id: project.id.clone(),
        project_name: project.name.clone(),
        branch_id: branch.id.clone(),
        branch_name: branch.name.clone(),
        branch_relation: branch.relation.clone(),
        anchor_commit,
        anchor_layer,
        anchor_root,
        expected_branch_head: branch
            .work_head
            .and_then(|number| branch.commit_id(number))
            .or_else(|| branch.boundary_commit.clone()),
        published_commit: None,
        published_root: None,
        state: status,
        generation: 0,
        projection: projection.into(),
        placement: placement.into(),
        mount: mount.into(),
        changed_paths: 0,
        output_bytes: 412 * 1024,
        execution: matches!(status, WorkspaceState::Running | WorkspaceState::Busy)
            .then(|| ExecutionId::new(format!("E-{id}"))),
        output: vec![
            "checking workspace filesystem".into(),
            "tests passed: 42".into(),
            "output retained by host".into(),
        ],
        conflicts: if status == WorkspaceState::HeadMoved {
            vec![ConflictView {
                id: ConflictId::from("conflict-head-moved"),
                path: "src/model.rs".into(),
                kind: "Content".into(),
            }]
        } else {
            Vec::new()
        },
        files: Vec::new(),
        changes: Vec::new(),
        runs: Vec::new(),
        storage: Default::default(),
        timing: Default::default(),
        commit_receipt: None,
    };
    let changed = matches!(
        status,
        WorkspaceState::Dirty
            | WorkspaceState::Running
            | WorkspaceState::Busy
            | WorkspaceState::HeadMoved
    );
    let mut current = anchor.clone();
    if changed {
        current.insert(
            "src/workspace-change.js".into(),
            TreeEntry::File(format!("export const workspace = {id:?};\n").into_bytes()),
        );
    }
    let mut record = if changed {
        WorkspaceRecord::fixture_dirty(view, anchor, current)
    } else {
        WorkspaceRecord::fixture(view, anchor)
    }
    .expect("fixture Workspace");
    record.view.state = status;
    record
}

fn operations() -> Vec<OperationView> {
    vec![
        operation(
            "OP-1",
            "Pull api-server through L19",
            "api-server",
            14,
            19,
            OperationState::Running,
        ),
        operation(
            "OP-2",
            "Push search-a",
            "api-server",
            3,
            5,
            OperationState::Running,
        ),
        operation(
            "OP-3",
            "Commit W9",
            "api-server",
            1,
            1,
            OperationState::Succeeded,
        ),
        operation(
            "OP-4",
            "Diff L18 to L19",
            "api-server",
            128,
            136,
            OperationState::Succeeded,
        ),
        operation(
            "OP-5",
            "Pull hijacked",
            "api-server",
            2,
            4,
            OperationState::Failed,
        ),
        operation(
            "OP-6",
            "Materialize W50",
            "api-server",
            4,
            4,
            OperationState::Succeeded,
        ),
    ]
}

fn operation(
    id: &str,
    title: &str,
    project: &str,
    completed: u64,
    total: u64,
    state: OperationState,
) -> OperationView {
    OperationView {
        id: OperationId::from(id),
        title: title.into(),
        project: Some(name(project)),
        phase: if state == OperationState::Running {
            "verifying roots".into()
        } else {
            "finished".into()
        },
        completed,
        total,
        state,
        receipt: OperationReceipt {
            facts_announced: 19,
            facts_missing: 1,
            facts_inserted: 1,
            objects_announced: 184_217,
            objects_missing: 12_402,
            objects_sent: 12_402,
            objects_inserted: 12_391,
            objects_raced: 11,
            elapsed_ms: 1_056,
            elapsed_micros: 1_056_000,
        },
        events: vec![
            "Started".into(),
            format!("Progress {completed}/{total}"),
            "Snapshot project".into(),
        ],
    }
}

fn paginate<T>(items: Vec<T>, request: &PageRequest, prefix: &str) -> Page<T> {
    let start = request
        .after
        .as_deref()
        .and_then(|cursor| cursor.rsplit(':').next())
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(0)
        .min(items.len());
    let limit = usize::from(request.limit.clamp(1, 128));
    let end = (start + limit).min(items.len());
    let next = (end < items.len()).then(|| format!("{prefix}:{end}"));
    Page {
        items: items.into_iter().skip(start).take(limit).collect(),
        next,
    }
}

#[cfg(test)]
mod tests;

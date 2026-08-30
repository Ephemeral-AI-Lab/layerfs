use super::MockState;
use crate::{BranchId, BranchOrigin, BranchRelation, LayerStackId, PageRequest, ProjectRelation};

#[test]
fn fixture_is_relationally_consistent() {
    let state = MockState::demo();
    let project = state
        .project_snapshot(&LayerStackId::from("SA-91"), &PageRequest::first(64))
        .unwrap();
    assert!(matches!(
        project.project.relation,
        ProjectRelation::PullBehind { layers: 1, .. }
    ));
    let main = project
        .branches
        .items
        .iter()
        .find(|branch| branch.id == BranchId::from("B-main"))
        .unwrap();
    assert!(matches!(
        main.relation,
        BranchRelation::RemotePullBehind { commits: 5, .. }
    ));
    assert_eq!(main.authority_number, Some(42));
    assert_eq!(main.work_number, Some(37));
    for branch in &project.branches.items {
        match &branch.origin {
            BranchOrigin::Layer(layer) => {
                assert!(project.layers.items.iter().any(|item| &item.id == layer));
            }
            BranchOrigin::Commit(parent, commit) => {
                let parent = state.branch(parent).expect("origin Branch");
                assert!(parent.commits.iter().any(|item| &item.id == commit));
            }
        }
    }
    for layer in project.layers.items.iter().skip(1) {
        let (branch, commit) = layer.source.as_ref().expect("non-genesis source");
        let source = state.branch(branch).expect("source Branch");
        let source_commit = source
            .commits
            .iter()
            .find(|item| &item.id == commit)
            .expect("source Commit");
        assert_eq!(source_commit.accepted_layer.as_ref(), Some(&layer.id));
    }
    for project in state.project_summaries(&PageRequest::first(64)).items {
        let snapshot = state
            .project_snapshot(&project.id, &PageRequest::first(64))
            .unwrap();
        for layer in snapshot.layers.items.iter().skip(1) {
            let (branch, _) = layer.source.as_ref().expect("non-genesis source");
            let source = state.branch(branch).expect("source Branch");
            let base = state.branch_base_layer(source);
            let base = snapshot
                .layers
                .items
                .iter()
                .find(|candidate| candidate.id == base)
                .expect("source base Layer");
            assert!(
                base.number < layer.number,
                "{} -> {}",
                base.number,
                layer.number
            );
        }
    }
}

#[test]
fn fixture_has_depth_four_rollout() {
    let state = MockState::demo();
    let deep = state.branch(&BranchId::from("B-search-a1x-i")).unwrap();
    assert_eq!(deep.name.as_str(), "search-a1x-i");
    let root = state.branch(&BranchId::from("B-main")).unwrap();
    assert!(state.descendants(&root.id, &mut Default::default()) >= 4);
}

#[test]
fn dirty_fixture_reports_its_actual_final_delta() {
    let state = MockState::demo();
    let workspace = state
        .workspaces
        .iter()
        .find(|workspace| workspace.id.as_str() == "W9")
        .unwrap();
    assert_eq!(workspace.changed_paths as usize, workspace.changes.len());
    assert_eq!(workspace.changed_paths, 1);
    assert!(workspace
        .changes
        .iter()
        .any(|change| change.path == "src/workspace-change.js"));
}

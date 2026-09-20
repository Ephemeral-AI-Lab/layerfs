//! Exact stages, conditional Branch advance and conditional stack advance.
mod support;

use layerfs_content::ObjectId;
use layerfs_history::*;
use support::*;

struct Fixture {
    _temp: Temp,
    catalog: sqlite::SqliteCatalog,
    stack: LayerStackId,
    branch: BranchId,
    base: LayerId,
    root: ObjectId,
}

fn fixture(label: &str) -> Fixture {
    let temp = Temp::new(label);
    let catalog = create(&temp.join("catalog.sqlite"));
    let stack = stack(0x11);
    let base = genesis(stack, root(0x40));
    catalog
        .initialize_layerstack(&StackInitialization {
            stack,
            name: name("main"),
            scope: root(0x41),
            profile: root(0x42),
            genesis_root: root(0x40),
        })
        .unwrap();
    let branch = branch_id(0x12);
    catalog
        .fork(&ForkRequest {
            stack,
            branch,
            name: name("work"),
            source: ForkSource::Layer(base),
        })
        .unwrap();
    Fixture {
        _temp: temp,
        catalog,
        stack,
        branch,
        base,
        root: root(0x40),
    }
}

impl Fixture {
    fn stage(&self, body: u8, candidate: ObjectId) -> StageRecord {
        self.catalog
            .stage_changes(&StageRequest {
                workspace: workspace(body),
                branch: self.branch,
                expected_head: None,
                expected_base: self.base,
                expected_root: self.root,
                construction_base_root: self.root,
                intended_commit_base: self.base,
                candidate_root: candidate,
                profile: root(0x42),
                scope: root(0x41),
                generation: 1,
            })
            .unwrap()
    }
}

#[test]
fn a_stale_loser_keeps_its_exact_stage() {
    let fixture = fixture("conditional-stale");
    let winner = fixture.stage(0x21, root(0x50));
    let loser = fixture.stage(0x22, root(0x51));
    let committed = fixture
        .catalog
        .commit_staged(&CommitStagedRequest {
            workspace: winner.workspace,
            token: winner.token,
        })
        .unwrap();
    let head = match committed {
        CommitStagedOutcome::Committed(record) => record.id,
        other => panic!("expected a Commit, got {other:?}"),
    };
    let moved = fixture
        .catalog
        .commit_staged(&CommitStagedRequest {
            workspace: loser.workspace,
            token: loser.token,
        })
        .unwrap_err();
    match moved.cause() {
        HistoryError::HeadMoved(state) => {
            assert_eq!(state.expected_head, None);
            assert_eq!(state.actual_head, Some(head));
            assert_eq!(state.expected_base, fixture.base);
            assert_eq!(state.actual_base, fixture.base);
        }
        other => panic!("expected HeadMoved, got {other:?}"),
    }
    assert_eq!(
        fixture.catalog.stage(loser.workspace).unwrap().unwrap(),
        loser
    );
}

#[test]
fn an_exact_token_is_the_only_stage_a_discard_removes() {
    let fixture = fixture("conditional-token");
    let original = fixture.stage(0x31, root(0x60));
    assert_eq!(
        fixture
            .catalog
            .discard_stage(&DiscardRequest {
                workspace: original.workspace,
                token: original.token,
            })
            .unwrap(),
        DiscardOutcome::Removed
    );
    assert_eq!(
        fixture
            .catalog
            .discard_stage(&DiscardRequest {
                workspace: original.workspace,
                token: original.token,
            })
            .unwrap(),
        DiscardOutcome::Absent
    );
    let replacement = fixture.stage(0x31, root(0x61));
    assert_ne!(replacement.token, original.token);
    let delayed = fixture
        .catalog
        .discard_stage(&DiscardRequest {
            workspace: original.workspace,
            token: original.token,
        })
        .unwrap_err();
    assert_eq!(
        delayed.cause(),
        &HistoryError::StageChanged {
            expected: original.token,
            actual: Some(replacement.token),
        }
    );
    assert_eq!(
        fixture
            .catalog
            .stage(replacement.workspace)
            .unwrap()
            .unwrap(),
        replacement
    );
    let wrong = fixture
        .catalog
        .commit_staged(&CommitStagedRequest {
            workspace: replacement.workspace,
            token: original.token,
        })
        .unwrap_err();
    assert!(matches!(wrong.cause(), HistoryError::StageChanged { .. }));
    assert!(fixture
        .catalog
        .stage(replacement.workspace)
        .unwrap()
        .is_some());
}

#[test]
fn stage_insertion_enforces_the_frozen_context() {
    let fixture = fixture("conditional-context");
    let mut request = StageRequest {
        workspace: workspace(0x41),
        branch: fixture.branch,
        expected_head: None,
        expected_base: fixture.base,
        expected_root: fixture.root,
        construction_base_root: root(0x70),
        intended_commit_base: fixture.base,
        candidate_root: root(0x71),
        profile: root(0x42),
        scope: root(0x41),
        generation: 1,
    };
    assert_eq!(
        fixture.catalog.stage_changes(&request).unwrap_err(),
        HistoryError::InvalidInput("construction base root")
    );
    request.construction_base_root = fixture.root;
    request.intended_commit_base = genesis(fixture.stack, root(0x99));
    assert_eq!(
        fixture.catalog.stage_changes(&request).unwrap_err(),
        HistoryError::InvalidInput("intended Commit base")
    );
    request.intended_commit_base = fixture.base;
    request.profile = root(0x98);
    assert_eq!(
        fixture.catalog.stage_changes(&request).unwrap_err(),
        HistoryError::InvalidInput("stage profile")
    );
    request.profile = root(0x42);
    request.scope = root(0x97);
    assert_eq!(
        fixture.catalog.stage_changes(&request).unwrap_err(),
        HistoryError::InvalidInput("stage scope")
    );
    request.scope = root(0x41);
    let inserted = fixture.catalog.stage_changes(&request).unwrap();
    // One stage per incarnation: a second insertion is refused and the first
    // stage is unchanged.
    request.candidate_root = root(0x72);
    assert_eq!(
        fixture.catalog.stage_changes(&request).unwrap_err(),
        HistoryError::InvalidInput("stage already present")
    );
    assert_eq!(
        fixture.catalog.stage(inserted.workspace).unwrap().unwrap(),
        inserted
    );
}

#[test]
fn an_exact_earlier_publication_is_up_to_date_before_a_stale_head() {
    let fixture = fixture("conditional-publish");
    let staged = fixture.stage(0x51, root(0x80));
    let commit = match fixture
        .catalog
        .commit_staged(&CommitStagedRequest {
            workspace: staged.workspace,
            token: staged.token,
        })
        .unwrap()
    {
        CommitStagedOutcome::Committed(record) => record,
        other => panic!("expected a Commit, got {other:?}"),
    };
    let added = fixture
        .catalog
        .add_layer(&AddLayerRequest {
            stack: fixture.stack,
            branch: fixture.branch,
            commit: commit.id,
            expected_stack_head: fixture.base,
            expected_branch_base: fixture.base,
        })
        .unwrap();
    let layer = match added {
        AddLayerOutcome::Added(layer) => layer,
        other => panic!("expected a Layer, got {other:?}"),
    };
    // The same question with the stale expected head is answered from the
    // publication, not refused as a moved head.
    assert_eq!(
        fixture
            .catalog
            .add_layer(&AddLayerRequest {
                stack: fixture.stack,
                branch: fixture.branch,
                commit: commit.id,
                expected_stack_head: fixture.base,
                expected_branch_base: fixture.base,
            })
            .unwrap(),
        AddLayerOutcome::UpToDate { layer: layer.id }
    );
    // A second Commit on the same Branch is captured against the new head.
    let second = fixture
        .catalog
        .stage_changes(&StageRequest {
            workspace: workspace(0x52),
            branch: fixture.branch,
            expected_head: Some(commit.id),
            expected_base: fixture.base,
            expected_root: commit.root,
            construction_base_root: commit.root,
            intended_commit_base: fixture.base,
            candidate_root: root(0x81),
            profile: root(0x42),
            scope: root(0x41),
            generation: 2,
        })
        .unwrap();
    let second = match fixture
        .catalog
        .commit_staged(&CommitStagedRequest {
            workspace: second.workspace,
            token: second.token,
        })
        .unwrap()
    {
        CommitStagedOutcome::Committed(record) => record,
        other => panic!("expected a Commit, got {other:?}"),
    };
    // The Branch base still points at the genesis Layer, so publishing the new
    // Commit would need a second child of that parent: the stack has moved.
    match fixture
        .catalog
        .add_layer(&AddLayerRequest {
            stack: fixture.stack,
            branch: fixture.branch,
            commit: second.id,
            expected_stack_head: fixture.base,
            expected_branch_base: fixture.base,
        })
        .unwrap_err()
    {
        HistoryError::StackMoved { expected, actual } => {
            assert_eq!(expected, fixture.base);
            assert_eq!(actual, layer.id);
        }
        other => panic!("expected StackMoved, got {other:?}"),
    }
}

#[test]
fn a_commit_whose_root_equals_its_base_is_no_changes() {
    let fixture = fixture("conditional-nochanges");
    // A Branch with no head whose candidate equals its base is up to date.
    let staged = fixture.stage(0x61, fixture.root);
    match fixture
        .catalog
        .commit_staged(&CommitStagedRequest {
            workspace: staged.workspace,
            token: staged.token,
        })
        .unwrap()
    {
        CommitStagedOutcome::UpToDate { head, root } => {
            assert_eq!(head, None);
            assert_eq!(root, fixture.root);
        }
        other => panic!("expected UpToDate, got {other:?}"),
    }
    // One real Commit, then a second whose root returns to the base root.
    let first = fixture.stage(0x62, root(0x90));
    let first = match fixture
        .catalog
        .commit_staged(&CommitStagedRequest {
            workspace: first.workspace,
            token: first.token,
        })
        .unwrap()
    {
        CommitStagedOutcome::Committed(record) => record,
        other => panic!("expected a Commit, got {other:?}"),
    };
    let second = fixture
        .catalog
        .stage_changes(&StageRequest {
            workspace: workspace(0x63),
            branch: fixture.branch,
            expected_head: Some(first.id),
            expected_base: fixture.base,
            expected_root: first.root,
            construction_base_root: first.root,
            intended_commit_base: fixture.base,
            candidate_root: fixture.root,
            profile: root(0x42),
            scope: root(0x41),
            generation: 2,
        })
        .unwrap();
    let second = match fixture
        .catalog
        .commit_staged(&CommitStagedRequest {
            workspace: second.workspace,
            token: second.token,
        })
        .unwrap()
    {
        CommitStagedOutcome::Committed(record) => record,
        other => panic!("expected a Commit, got {other:?}"),
    };
    assert_eq!(second.parent, Some(first.id));
    assert_eq!(second.root, fixture.root);
    // Publishing a Commit that already equals the base root is a no-op.
    assert_eq!(
        fixture
            .catalog
            .add_layer(&AddLayerRequest {
                stack: fixture.stack,
                branch: fixture.branch,
                commit: second.id,
                expected_stack_head: fixture.base,
                expected_branch_base: fixture.base,
            })
            .unwrap(),
        AddLayerOutcome::NoChanges { head: fixture.base }
    );
}

#[test]
fn two_branches_commit_independently_and_one_wins_the_stack_head() {
    let fixture = fixture("conditional-branches");
    let sibling = branch_id(0x13);
    fixture
        .catalog
        .fork(&ForkRequest {
            stack: fixture.stack,
            branch: sibling,
            name: name("sibling"),
            source: ForkSource::Layer(fixture.base),
        })
        .unwrap();
    let mut heads = Vec::new();
    for (body, candidate) in [(0x71u8, root(0xa0)), (0x72, root(0xa1))] {
        let identity = if body == 0x71 {
            fixture.branch
        } else {
            sibling
        };
        let staged = fixture
            .catalog
            .stage_changes(&StageRequest {
                workspace: workspace(body),
                branch: identity,
                expected_head: None,
                expected_base: fixture.base,
                expected_root: fixture.root,
                construction_base_root: fixture.root,
                intended_commit_base: fixture.base,
                candidate_root: candidate,
                profile: root(0x42),
                scope: root(0x41),
                generation: 1,
            })
            .unwrap();
        heads.push(
            match fixture
                .catalog
                .commit_staged(&CommitStagedRequest {
                    workspace: staged.workspace,
                    token: staged.token,
                })
                .unwrap()
            {
                CommitStagedOutcome::Committed(record) => record,
                other => panic!("expected a Commit, got {other:?}"),
            },
        );
    }
    // Both Branch Commits succeeded and neither head moved the other's.
    assert_eq!(
        fixture
            .catalog
            .branch(fixture.branch)
            .unwrap()
            .unwrap()
            .head_commit,
        Some(heads[0].id)
    );
    assert_eq!(
        fixture
            .catalog
            .branch(sibling)
            .unwrap()
            .unwrap()
            .head_commit,
        Some(heads[1].id)
    );
    // The shared stack head is the first publication's to win.
    let winner = fixture
        .catalog
        .add_layer(&AddLayerRequest {
            stack: fixture.stack,
            branch: fixture.branch,
            commit: heads[0].id,
            expected_stack_head: fixture.base,
            expected_branch_base: fixture.base,
        })
        .unwrap();
    let layer = match winner {
        AddLayerOutcome::Added(layer) => layer,
        other => panic!("expected a Layer, got {other:?}"),
    };
    match fixture
        .catalog
        .add_layer(&AddLayerRequest {
            stack: fixture.stack,
            branch: sibling,
            commit: heads[1].id,
            expected_stack_head: fixture.base,
            expected_branch_base: fixture.base,
        })
        .unwrap_err()
    {
        HistoryError::StackMoved { expected, actual } => {
            assert_eq!(expected, fixture.base);
            assert_eq!(actual, layer.id);
        }
        other => panic!("expected StackMoved, got {other:?}"),
    }
    // The loser published nothing and its Branch is unchanged.
    assert_eq!(
        fixture
            .catalog
            .layer_stack(fixture.stack)
            .unwrap()
            .unwrap()
            .head_layer,
        layer.id
    );
    assert_eq!(
        fixture
            .catalog
            .branch(sibling)
            .unwrap()
            .unwrap()
            .head_commit,
        Some(heads[1].id)
    );
}

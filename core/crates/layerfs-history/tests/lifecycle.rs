//! Genesis creation, zero-copy forks and the Commit/Layer transition.
mod support;

use layerfs_history::*;
use support::*;

#[test]
fn genesis_is_atomic_and_unique() {
    let temp = Temp::new("genesis");
    let catalog = create(&temp.join("catalog.sqlite"));
    let identity = stack(0x01);
    let record = catalog
        .initialize_layerstack(&StackInitialization {
            stack: identity,
            name: name("main"),
            scope: root(0x10),
            profile: root(0x11),
            genesis_root: root(0x12),
        })
        .expect("genesis");
    assert_eq!(record.head_layer, genesis(identity, root(0x12)));
    assert_eq!(
        catalog.layer(record.head_layer).unwrap().unwrap().parent,
        None
    );
    // A second genesis with the same identity is an integrity failure and the
    // first stack is untouched.
    let again = catalog.initialize_layerstack(&StackInitialization {
        stack: identity,
        name: name("other"),
        scope: root(0x13),
        profile: root(0x11),
        genesis_root: root(0x14),
    });
    assert!(matches!(again, Err(HistoryError::Integrity(_))));
    assert_eq!(catalog.layer_stack(identity).unwrap().unwrap(), record);
    // A different identity may not reuse the authority-local name.
    let duplicate = catalog.initialize_layerstack(&StackInitialization {
        stack: stack(0x02),
        name: name("main"),
        scope: root(0x15),
        profile: root(0x11),
        genesis_root: root(0x16),
    });
    assert!(matches!(duplicate, Err(HistoryError::Integrity(_))));
}

#[test]
fn fork_shares_content_and_uses_the_selected_commit_base() {
    let temp = Temp::new("fork");
    let catalog = create(&temp.join("catalog.sqlite"));
    let stack = stack(0x03);
    let base = genesis(stack, root(0x20));
    catalog
        .initialize_layerstack(&StackInitialization {
            stack,
            name: name("main"),
            scope: root(0x21),
            profile: root(0x22),
            genesis_root: root(0x20),
        })
        .unwrap();
    let from_layer = catalog
        .fork(&ForkRequest {
            stack,
            branch: branch_id(0x04),
            name: name("work"),
            source: ForkSource::Layer(base),
        })
        .unwrap();
    assert_eq!(from_layer.branch.base_layer, base);
    assert_eq!(from_layer.branch.head_commit, None);
    assert_eq!(from_layer.effective_root, root(0x20));
    assert_eq!(from_layer.base_root, root(0x20));
    assert_eq!(from_layer.scope, root(0x21));

    // One Commit on the first Branch, then a fork from that Commit.
    let staged = catalog
        .stage_changes(&StageRequest {
            workspace: workspace(0x05),
            branch: branch_id(0x04),
            expected_head: None,
            expected_base: base,
            expected_root: root(0x20),
            construction_base_root: root(0x20),
            intended_commit_base: base,
            candidate_root: root(0x23),
            profile: root(0x22),
            scope: root(0x21),
            generation: 1,
        })
        .unwrap();
    let commit = match catalog
        .commit_staged(&CommitStagedRequest {
            workspace: workspace(0x05),
            token: staged.token,
        })
        .unwrap()
    {
        CommitStagedOutcome::Committed(record) => record,
        other => panic!("expected a Commit, got {other:?}"),
    };
    assert_eq!(commit.id, CommitId::derive(root(0x23), None, base));

    // The stack head moves to the published Layer; the Branch base does not.
    let added = catalog
        .add_layer(&AddLayerRequest {
            stack,
            branch: branch_id(0x04),
            commit: commit.id,
            expected_stack_head: base,
            expected_branch_base: base,
        })
        .unwrap();
    let layer = match added {
        AddLayerOutcome::Added(layer) => layer,
        other => panic!("expected a Layer, got {other:?}"),
    };
    let work = catalog.branch_snapshot(branch_id(0x04)).unwrap().unwrap();
    assert_eq!(work.branch.head_commit, Some(commit.id));
    assert_eq!(work.branch.base_layer, base);

    // A fork from the historical Commit keeps the Commit's own base.
    let historical = catalog
        .fork(&ForkRequest {
            stack,
            branch: branch_id(0x06),
            name: name("historical"),
            source: ForkSource::Commit {
                branch: branch_id(0x04),
                commit: commit.id,
            },
        })
        .unwrap();
    assert_eq!(historical.branch.base_layer, commit.base_layer);
    assert_eq!(historical.branch.head_commit, Some(commit.id));
    assert_eq!(historical.effective_root, commit.root);
    assert_eq!(historical.head_root, Some(commit.root));
    assert_eq!(layer.root, commit.root);
    assert_eq!(layer.parent, Some(base));
    assert_eq!(layer.source_branch, Some(branch_id(0x04)));
    assert_eq!(layer.source_commit, Some(commit.id));
}

#[test]
fn history_reads_return_the_recorded_ancestry_and_chain() {
    let temp = Temp::new("history");
    let catalog = create(&temp.join("catalog.sqlite"));
    let identity = stack(0x07);
    let base = genesis(identity, root(0x30));
    catalog
        .initialize_layerstack(&StackInitialization {
            stack: identity,
            name: name("main"),
            scope: root(0x31),
            profile: root(0x32),
            genesis_root: root(0x30),
        })
        .unwrap();
    // The publication chain is linear: a Branch publishes once, and the next
    // publication needs a Branch forked from the Layer it produced.
    let mut head_layer = base;
    let mut head_root = root(0x30);
    let mut branch_body = 0x08u8;
    let mut roots = Vec::new();
    for step in 0..3u8 {
        let candidate = root(0x33 + step);
        let branch_identity = branch_id(branch_body);
        catalog
            .fork(&ForkRequest {
                stack: identity,
                branch: branch_identity,
                name: name(&format!("step{}", step)),
                source: ForkSource::Layer(head_layer),
            })
            .unwrap();
        let staged = catalog
            .stage_changes(&StageRequest {
                workspace: workspace(0x40 + step),
                branch: branch_identity,
                expected_head: None,
                expected_base: head_layer,
                expected_root: head_root,
                construction_base_root: head_root,
                intended_commit_base: head_layer,
                candidate_root: candidate,
                profile: root(0x32),
                scope: root(0x31),
                generation: u64::from(step) + 1,
            })
            .unwrap();
        let commit = match catalog
            .commit_staged(&CommitStagedRequest {
                workspace: workspace(0x40 + step),
                token: staged.token,
            })
            .unwrap()
        {
            CommitStagedOutcome::Committed(record) => record,
            other => panic!("expected a Commit, got {other:?}"),
        };
        let added = catalog
            .add_layer(&AddLayerRequest {
                stack: identity,
                branch: branch_identity,
                commit: commit.id,
                expected_stack_head: head_layer,
                expected_branch_base: head_layer,
            })
            .unwrap();
        let layer = match added {
            AddLayerOutcome::Added(layer) => layer,
            other => panic!("expected a Layer, got {other:?}"),
        };
        assert_eq!(layer.root, candidate);
        assert_eq!(layer.parent, Some(head_layer));
        roots.push((commit.id, layer.id));
        head_layer = layer.id;
        head_root = layer.root;
        branch_body += 1;
    }
    let layers = catalog
        .layer_history(&LayerHistoryRequest {
            stack: identity,
            start: None,
            cursor: None,
            limit: 8,
        })
        .unwrap();
    assert_eq!(layers.records.len(), 4);
    assert_eq!(layers.records[0].id, roots[2].1);
    assert_eq!(layers.records[2].id, roots[0].1);
    assert_eq!(layers.records[3].id, base);
    assert!(layers.records[3].parent.is_none());
    assert!(layers.records[3].source_branch.is_none());
    assert_eq!(
        catalog.layer_stack(identity).unwrap().unwrap().head_layer,
        roots[2].1
    );
    let commits = catalog
        .commit_history(&CommitHistoryRequest {
            branch: branch_id(0x0A),
            start: None,
            cursor: None,
            limit: 8,
        })
        .unwrap();
    assert_eq!(commits.records.len(), 1);
    assert_eq!(commits.records[0].id, roots[2].0);
    // An explicit immutable start outside the Branch's ancestry is refused.
    let outside = catalog
        .commit_history(&CommitHistoryRequest {
            branch: branch_id(0x0A),
            start: Some(roots[0].0),
            cursor: None,
            limit: 8,
        })
        .unwrap_err();
    assert_eq!(outside, HistoryError::NotInHistory("Commit"));
}

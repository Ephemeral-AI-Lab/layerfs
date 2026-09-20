//! Bounded pages, continuation binding and lineage work ceilings.
mod support;

use layerfs_history::error::Missing;
use layerfs_history::*;
use support::*;

fn stack_names(count: u8) -> Vec<HistoryName> {
    (0..count)
        .map(|index| name(&format!("stack{}", index)))
        .collect()
}

#[test]
fn every_bounded_list_paginates_without_gaps_or_repeats() {
    let temp = Temp::new("pages");
    let catalog = create(&temp.join("catalog.sqlite"));
    let scope = root(0xe0);
    let profile = root(0xe1);
    for (index, label) in stack_names(5).into_iter().enumerate() {
        catalog
            .initialize_layerstack(&StackInitialization {
                stack: stack(0x10 + index as u8),
                name: label,
                scope,
                profile,
                genesis_root: root(0xe2 + index as u8),
            })
            .unwrap();
    }
    let mut seen = Vec::new();
    let mut cursor = None;
    loop {
        let page = catalog
            .layer_stacks(&Page {
                cursor: cursor.clone(),
                limit: 2,
            })
            .unwrap();
        assert!(page.records.len() <= 2);
        seen.extend(page.records.iter().map(|record| record.name.clone()));
        match page.continuation {
            Some(next) => cursor = Some(next),
            None => break,
        }
    }
    assert_eq!(seen, stack_names(5));

    // One Branch's stages paginate in token order.
    let branch = branch_id(0x30);
    let base = genesis(stack(0x10), root(0xe2));
    catalog
        .fork(&ForkRequest {
            stack: stack(0x10),
            branch,
            name: name("work"),
            source: ForkSource::Layer(base),
        })
        .unwrap();
    let mut tokens = Vec::new();
    for step in 0..3u8 {
        let record = catalog
            .stage_changes(&StageRequest {
                workspace: workspace(0x40 + step),
                branch,
                expected_head: None,
                expected_base: base,
                expected_root: root(0xe2),
                construction_base_root: root(0xe2),
                intended_commit_base: base,
                candidate_root: root(0xe3 + step),
                profile,
                scope,
                generation: 1,
            })
            .unwrap();
        tokens.push(record.token);
    }
    let first = catalog
        .stages(
            branch,
            &Page {
                cursor: None,
                limit: 1,
            },
        )
        .unwrap();
    assert_eq!(first.records.len(), 1);
    assert_eq!(first.records[0].token, tokens[0]);
    let continuation = first.continuation.expect("more stages");
    let second = catalog
        .stages(
            branch,
            &Page {
                cursor: Some(continuation),
                limit: 2,
            },
        )
        .unwrap();
    assert_eq!(
        second
            .records
            .iter()
            .map(|record| record.token)
            .collect::<Vec<_>>(),
        vec![tokens[1], tokens[2]]
    );
    assert!(second.continuation.is_none());
}

#[test]
fn a_cursor_is_bound_to_its_range_subject_and_content() {
    let temp = Temp::new("pages-cursor");
    let catalog = create(&temp.join("catalog.sqlite"));
    let scope = root(0xf0);
    let profile = root(0xf1);
    for index in 0..3u8 {
        catalog
            .initialize_layerstack(&StackInitialization {
                stack: stack(0x20 + index),
                name: name(&format!("stack{}", index)),
                scope,
                profile,
                genesis_root: root(0xf2 + index),
            })
            .unwrap();
    }
    let page = catalog
        .layer_stacks(&Page {
            cursor: None,
            limit: 1,
        })
        .unwrap();
    let cursor = page.continuation.expect("more stacks");
    // Another range refuses the cursor instead of reinterpreting it.
    assert_eq!(
        catalog
            .branches(
                stack(0x20),
                &Page {
                    cursor: Some(cursor.clone()),
                    limit: 1,
                },
            )
            .unwrap_err(),
        HistoryError::InvalidInput("page cursor range")
    );
    // A tampered body fails its digest.
    let mut tampered = cursor.clone();
    tampered[100] ^= 0xff;
    assert_eq!(
        catalog
            .layer_stacks(&Page {
                cursor: Some(tampered),
                limit: 1,
            })
            .unwrap_err(),
        HistoryError::Integrity("page cursor digest")
    );
    // A truncated or over-long cursor is not a cursor.
    assert_eq!(
        catalog
            .layer_stacks(&Page {
                cursor: Some(cursor[..10].to_vec()),
                limit: 1,
            })
            .unwrap_err(),
        HistoryError::InvalidInput("page cursor width")
    );
    // The declared bounds are checked before any row is read.
    assert_eq!(
        catalog
            .layer_stacks(&Page {
                cursor: None,
                limit: 0,
            })
            .unwrap_err(),
        HistoryError::InvalidInput("page limit")
    );
    assert_eq!(
        catalog
            .layer_stacks(&Page {
                cursor: None,
                limit: MAXIMUM_PAGE_RECORDS + 1,
            })
            .unwrap_err(),
        HistoryError::InvalidInput("page limit")
    );
}

#[test]
fn an_immutable_anchor_keeps_a_cursor_valid_as_the_branch_advances() {
    let temp = Temp::new("pages-anchor");
    let catalog = create(&temp.join("catalog.sqlite"));
    let identity = stack(0x50);
    let base = genesis(identity, root(0x60));
    let scope = root(0x61);
    let profile = root(0x62);
    catalog
        .initialize_layerstack(&StackInitialization {
            stack: identity,
            name: name("main"),
            scope,
            profile,
            genesis_root: root(0x60),
        })
        .unwrap();
    let branch = branch_id(0x51);
    catalog
        .fork(&ForkRequest {
            stack: identity,
            branch,
            name: name("work"),
            source: ForkSource::Layer(base),
        })
        .unwrap();
    let mut head = None;
    let mut head_root = root(0x60);
    for step in 0..3u8 {
        let record = catalog
            .stage_changes(&StageRequest {
                workspace: workspace(0x60 + step),
                branch,
                expected_head: head,
                expected_base: base,
                expected_root: head_root,
                construction_base_root: head_root,
                intended_commit_base: base,
                candidate_root: root(0x63 + step),
                profile,
                scope,
                generation: u64::from(step) + 1,
            })
            .unwrap();
        let commit = match catalog
            .commit_staged(&CommitStagedRequest {
                workspace: record.workspace,
                token: record.token,
            })
            .unwrap()
        {
            CommitStagedOutcome::Committed(record) => record,
            other => panic!("expected a Commit, got {other:?}"),
        };
        head = Some(commit.id);
        head_root = commit.root;
    }
    let first = catalog
        .commit_history(&CommitHistoryRequest {
            branch,
            start: None,
            cursor: None,
            limit: 1,
        })
        .unwrap();
    assert_eq!(first.records.len(), 1);
    assert_eq!(first.records[0].id, head.unwrap());
    let cursor = first.continuation.expect("more Commits");
    // A Commit that does not exist is a typed absence, not a false ancestry.
    assert_eq!(
        catalog
            .commit_history(&CommitHistoryRequest {
                branch,
                start: Some(CommitId::derive(root(0x99), None, base)),
                cursor: None,
                limit: 1,
            })
            .unwrap_err(),
        HistoryError::Missing(Missing::Commit)
    );
    // A real Commit from a sibling Branch is in the same stack but not in this
    // Branch's ancestry, and that is a different answer again.
    let sibling = branch_id(0x52);
    catalog
        .fork(&ForkRequest {
            stack: identity,
            branch: sibling,
            name: name("sibling"),
            source: ForkSource::Layer(base),
        })
        .unwrap();
    let sibling_stage = catalog
        .stage_changes(&StageRequest {
            workspace: workspace(0x70),
            branch: sibling,
            expected_head: None,
            expected_base: base,
            expected_root: root(0x60),
            construction_base_root: root(0x60),
            intended_commit_base: base,
            candidate_root: root(0x70),
            profile,
            scope,
            generation: 1,
        })
        .unwrap();
    let sibling_commit = match catalog
        .commit_staged(&CommitStagedRequest {
            workspace: sibling_stage.workspace,
            token: sibling_stage.token,
        })
        .unwrap()
    {
        CommitStagedOutcome::Committed(record) => record,
        other => panic!("expected a Commit, got {other:?}"),
    };
    assert_eq!(
        catalog
            .commit_history(&CommitHistoryRequest {
                branch,
                start: Some(sibling_commit.id),
                cursor: None,
                limit: 1,
            })
            .unwrap_err(),
        HistoryError::NotInHistory("Commit")
    );
    // Continuing the same anchored walk still returns the original ancestry.
    let rest = catalog
        .commit_history(&CommitHistoryRequest {
            branch,
            start: None,
            cursor: Some(cursor),
            limit: 4,
        })
        .unwrap();
    assert_eq!(rest.records.len(), 2);
    assert_eq!(rest.records[0].root, root(0x64));
    assert_eq!(rest.records[1].root, root(0x63));
    assert!(rest.continuation.is_none());
}

//! Read-only reopen, binding validation and the writable-continuity envelope.
mod support;

use layerfs_history::*;
use support::*;

fn seeded(temp: &Temp) -> (sqlite::SqliteCatalog, LayerStackId, BranchId) {
    let catalog = create(&temp.join("catalog.sqlite"));
    let identity = stack(0xd0);
    let base = genesis(identity, root(0xd1));
    catalog
        .initialize_layerstack(&StackInitialization {
            stack: identity,
            name: name("main"),
            scope: root(0xd2),
            profile: root(0xd3),
            genesis_root: root(0xd1),
        })
        .unwrap();
    let branch = branch_id(0xd4);
    catalog
        .fork(&ForkRequest {
            stack: identity,
            branch,
            name: name("work"),
            source: ForkSource::Layer(base),
        })
        .unwrap();
    catalog
        .stage_changes(&StageRequest {
            workspace: workspace(0xd5),
            branch,
            expected_head: None,
            expected_base: base,
            expected_root: root(0xd1),
            construction_base_root: root(0xd1),
            intended_commit_base: base,
            candidate_root: root(0xd6),
            profile: root(0xd3),
            scope: root(0xd2),
            generation: 3,
        })
        .unwrap();
    (catalog, identity, branch)
}

#[test]
fn a_read_only_reopen_reads_every_record_and_mutates_nothing() {
    let temp = Temp::new("reopen");
    let (writer, identity, branch) = seeded(&temp);
    let reader = reopen(&temp.join("catalog.sqlite"));
    assert_eq!(reader.catalog_id(), writer.catalog_id());
    assert_eq!(reader.incarnation(), writer.incarnation());
    assert_eq!(
        reader.layer_stack(identity).unwrap(),
        writer.layer_stack(identity).unwrap()
    );
    assert_eq!(
        reader.branch_snapshot(branch).unwrap(),
        writer.branch_snapshot(branch).unwrap()
    );
    assert_eq!(
        reader.stage(workspace(0xd5)).unwrap(),
        writer.stage(workspace(0xd5)).unwrap()
    );
    assert_eq!(
        reader
            .layer_history(&LayerHistoryRequest {
                stack: identity,
                start: None,
                cursor: None,
                limit: 4,
            })
            .unwrap(),
        writer
            .layer_history(&LayerHistoryRequest {
                stack: identity,
                start: None,
                cursor: None,
                limit: 4,
            })
            .unwrap()
    );
    // Every mutation and every new allocation is refused before it is attempted.
    let scope = root(0xd2);
    assert_eq!(
        reader
            .reserve_inodes(&ReserveRequest { scope, count: 1 })
            .unwrap_err(),
        HistoryError::ContinuityUnavailable
    );
    assert_eq!(
        reader
            .fork(&ForkRequest {
                stack: identity,
                branch: branch_id(0xd7),
                name: name("second"),
                source: ForkSource::Layer(genesis(identity, root(0xd1))),
            })
            .unwrap_err(),
        HistoryError::ContinuityUnavailable
    );
    assert_eq!(
        reader
            .discard_stage(&DiscardRequest {
                workspace: workspace(0xd5),
                token: token(1),
            })
            .unwrap_err(),
        HistoryError::ContinuityUnavailable
    );
    assert_eq!(
        reader
            .stage_changes(&StageRequest {
                workspace: workspace(0xd8),
                branch,
                expected_head: None,
                expected_base: genesis(identity, root(0xd1)),
                expected_root: root(0xd1),
                construction_base_root: root(0xd1),
                intended_commit_base: genesis(identity, root(0xd1)),
                candidate_root: root(0xd9),
                profile: root(0xd3),
                scope,
                generation: 1,
            })
            .unwrap_err(),
        HistoryError::ContinuityUnavailable
    );
    assert_eq!(
        reader
            .add_layer(&AddLayerRequest {
                stack: identity,
                branch,
                commit: CommitId::derive(root(0xd6), None, genesis(identity, root(0xd1))),
                expected_stack_head: genesis(identity, root(0xd1)),
                expected_branch_base: genesis(identity, root(0xd1)),
            })
            .unwrap_err(),
        HistoryError::ContinuityUnavailable
    );
    // Nothing changed on disk: the writable handle still sees the same state.
    assert_eq!(
        writer.stage(workspace(0xd5)).unwrap().unwrap().generation,
        3
    );
    assert!(writer.branch(branch_id(0xd7)).unwrap().is_none());
}

#[test]
fn an_incompatible_or_foreign_catalog_is_refused() {
    let temp = Temp::new("reopen-refusals");
    let (writer, _, _) = seeded(&temp);
    let path = temp.join("catalog.sqlite");
    // A different binding key is a different authority.
    match sqlite::open_read_only(&path, b"another-binding") {
        Err(error) => assert_eq!(error, HistoryError::Integrity("catalog binding")),
        Ok(_) => panic!("a foreign binding key must not open the catalog"),
    }
    // Creation never adopts an existing catalog.
    match sqlite::create(
        &path,
        &HistoryCatalogConfig {
            binding_key: BINDING.to_vec(),
            incarnation: 9,
        },
    ) {
        Err(error) => assert_eq!(error, HistoryError::InvalidInput("catalog already exists")),
        Ok(_) => panic!("creation must not adopt an existing catalog"),
    }
    assert!(writer.layer_stack(stack(0xd0)).unwrap().is_some());
    // A file that is not a catalog at all never opens.
    let other = temp.join("not-a-catalog.sqlite");
    std::fs::write(&other, b"not a catalog").unwrap();
    assert!(sqlite::open_read_only(&other, BINDING).is_err());
}

#[test]
fn a_stage_written_by_one_handle_is_visible_to_a_read_only_handle() {
    let temp = Temp::new("reopen-shared");
    let (writer, identity, branch) = seeded(&temp);
    let base = genesis(identity, root(0xd1));
    let reader = reopen(&temp.join("catalog.sqlite"));
    assert_eq!(
        reader.stage(workspace(0xd5)).unwrap().unwrap().token,
        token(1)
    );
    // The writer consumes it; the reader sees the removal on its next read.
    assert_eq!(
        writer
            .discard_stage(&DiscardRequest {
                workspace: workspace(0xd5),
                token: token(1),
            })
            .unwrap(),
        DiscardOutcome::Removed
    );
    assert!(reader.stage(workspace(0xd5)).unwrap().is_none());
    // A new stage for the same incarnation gets the next token.
    let again = writer
        .stage_changes(&StageRequest {
            workspace: workspace(0xd5),
            branch,
            expected_head: None,
            expected_base: base,
            expected_root: root(0xd1),
            construction_base_root: root(0xd1),
            intended_commit_base: base,
            candidate_root: root(0xda),
            profile: root(0xd3),
            scope: root(0xd2),
            generation: 4,
        })
        .unwrap();
    assert_eq!(again.token, token(2));
    assert_eq!(reader.stage(workspace(0xd5)).unwrap().unwrap(), again);
}

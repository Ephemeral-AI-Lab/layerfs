//! A publication made over one head only (`CommitRequest::required_head`,
//! the Workspace Commit's `--expected`): checked inside `commit_branch`'s own
//! transaction, forced or not, and a forced publication decides on the head
//! that transaction reads. The Branch advance replaces only the head the
//! transaction read (`commit_advance_branch`'s head predicate, shared by
//! both backends). Each case runs on SQLite, and on PostgreSQL when
//! `LAYERFS_TEST_POSTGRES_URL` names a server (else that half is NOT_RUN).
use crate::support;

use layerfs_history::error::HistoryError;
use layerfs_history::records::{
    CommitMeta, CommitOutcome, CommitRecord, CommitRequest, CommitVia, ForkRequest, ForkSource,
    StackInitialization,
};
use layerfs_history::{BranchId, HistoryCatalog, HistoryCatalogConfig, LayerId, NodeId};
use support::{branch_id, name, root, stack, Temp};

/// Runs `case` on a fresh SQLite Store, then on a fresh PostgreSQL schema
/// when a server is named.
fn on_both(tag: &str, case: impl Fn(&dyn HistoryCatalog)) {
    let temp = Temp::new(tag);
    let sqlite = support::sqlite::create(
        &temp.join("store.sqlite"),
        &HistoryCatalogConfig {
            cursor_key: [61; 32],
            binding_key: b"required-head".to_vec(),
            incarnation: 1,
        },
    )
    .expect("catalog");
    case(&sqlite);
    if let Some(url) = support::pg::url() {
        let schema = support::pg::Schema::new(url, &tag.replace('-', "_"));
        let handles = schema.create(b"required-head", [61; 32]);
        case(&handles.history);
    }
}

/// Project `proj` with one Branch `work` at its genesis Layer.
fn project(history: &dyn HistoryCatalog) -> (BranchId, LayerId) {
    let record = history
        .initialize_layerstack(&StackInitialization {
            logical: 1,
            stack: stack(1),
            name: name("proj"),
            scope: root(200),
            profile: root(201),
            genesis_root: root(1),
        })
        .unwrap();
    let branch = branch_id(1);
    history
        .fork(&ForkRequest {
            stack: stack(1),
            branch,
            name: name("work"),
            source: ForkSource::Layer(record.head_layer),
        })
        .unwrap();
    (branch, record.head_layer)
}

/// A Commit of `tag` made over the Branch as it is now.
fn over_now(history: &dyn HistoryCatalog, branch: BranchId, tag: u8) -> CommitRequest {
    let record = history.branch(branch).unwrap().unwrap();
    let expected_root = match record.head_commit {
        Some(head) => history.commit(head).unwrap().unwrap().root,
        None => history.layer(record.base_layer).unwrap().unwrap().root,
    };
    CommitRequest {
        branch,
        expected_head: record.head_commit,
        expected_base: record.base_layer,
        expected_root,
        candidate_root: root(tag),
        profile: root(201),
        scope: root(200),
        second_parent: None,
        meta: CommitMeta::new(CommitVia::Files),
        force: false,
        required_head: None,
    }
}

fn committed(outcome: CommitOutcome) -> (CommitRecord, Option<layerfs_history::CommitId>) {
    match outcome {
        CommitOutcome::Committed { record, displaced } => (*record, displaced),
        other => panic!("not committed: {other:?}"),
    }
}

#[test]
fn another_head_is_refused_forced_or_not() {
    on_both("required-head-refused", another_head_is_refused);
}
fn another_head_is_refused(history: &dyn HistoryCatalog) {
    let (branch, genesis) = project(history);
    let stale = over_now(history, branch, 10);
    let (first, _) = committed(
        history
            .commit_branch(&over_now(history, branch, 11))
            .unwrap(),
    );
    for force in [false, true] {
        let request = CommitRequest {
            force,
            required_head: Some(NodeId::Layer(genesis)),
            ..stale.clone()
        };
        let refused = history.commit_branch(&request).expect_err("head moved");
        assert!(
            matches!(&refused, HistoryError::HeadMoved(state)
                if state.actual_head == Some(first.id)),
            "force {force}: {refused:?}"
        );
    }
    let now = history.branch(branch).unwrap().unwrap();
    assert_eq!(now.head_commit, Some(first.id), "nothing published");
}

#[test]
fn forced_over_the_required_head_displaces_only_it() {
    on_both("required-head-forced", forced_over_the_required_head);
}
fn forced_over_the_required_head(history: &dyn HistoryCatalog) {
    let (branch, _) = project(history);
    let stale = over_now(history, branch, 20);
    let (first, _) = committed(
        history
            .commit_branch(&over_now(history, branch, 21))
            .unwrap(),
    );
    let (forced, displaced) = committed(
        history
            .commit_branch(&CommitRequest {
                force: true,
                required_head: Some(NodeId::Commit(first.id)),
                ..stale
            })
            .unwrap(),
    );
    assert_eq!(displaced, Some(first.id));
    assert_eq!(forced.parent, None, "keeps its captured parent");
    let now = history.branch(branch).unwrap().unwrap();
    assert_eq!(now.head_commit, Some(forced.id));
}

/// A forced request built before the head moved twice displaces the head
/// its own transaction reads, the newest; the one between stays on that
/// head's first-parent chain.
#[test]
fn force_decides_on_the_head_its_transaction_reads() {
    on_both("required-head-force-reads", force_reads_in_its_transaction);
}
fn force_reads_in_its_transaction(history: &dyn HistoryCatalog) {
    let (branch, _) = project(history);
    let stale = CommitRequest {
        force: true,
        ..over_now(history, branch, 30)
    };
    let (first, _) = committed(
        history
            .commit_branch(&over_now(history, branch, 31))
            .unwrap(),
    );
    let (second, _) = committed(
        history
            .commit_branch(&over_now(history, branch, 32))
            .unwrap(),
    );
    let (_, displaced) = committed(history.commit_branch(&stale).unwrap());
    assert_eq!(displaced, Some(second.id));
    assert_eq!(second.parent, Some(first.id));
}

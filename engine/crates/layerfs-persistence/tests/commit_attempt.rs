//! A Commit's attempt (#776 R1): part of its identity, unique in the Store,
//! and the key a publisher whose reply was lost looks its Commit up by
//! (`HistoryCatalog::commit_by_attempt`). Each case runs on SQLite, and on
//! PostgreSQL when `LAYERFS_TEST_POSTGRES_URL` names a server (else that half
//! is NOT_RUN).
mod support;

use layerfs_history::records::{
    CommitMeta, CommitOutcome, CommitRecord, CommitRequest, CommitVia, ForkRequest, ForkSource,
    StackInitialization,
};
use layerfs_history::{BranchId, CommitAttempt, CommitId, HistoryCatalog, HistoryCatalogConfig};
use support::{branch_id, name, root, stack, Temp};

/// Runs `case` on a fresh SQLite Store, then on a fresh PostgreSQL schema
/// when a server is named.
fn on_both(tag: &str, case: impl Fn(&dyn HistoryCatalog)) {
    let temp = Temp::new(tag);
    let sqlite = support::sqlite::create(
        &temp.join("store.sqlite"),
        &HistoryCatalogConfig {
            cursor_key: [62; 32],
            binding_key: b"commit-attempt".to_vec(),
            incarnation: 1,
        },
    )
    .expect("catalog");
    case(&sqlite);
    if let Some(url) = support::pg::url() {
        let schema = support::pg::Schema::new(url, &tag.replace('-', "_"));
        let handles = schema.create(b"commit-attempt", [62; 32]);
        case(&handles.history);
    }
}

/// Project `proj` with one Branch `work` at its genesis Layer.
fn project(history: &dyn HistoryCatalog) -> BranchId {
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
    branch
}

/// A Commit of root `tag` under `attempt`, made over the Branch as it is now.
fn over_now(
    history: &dyn HistoryCatalog,
    branch: BranchId,
    tag: u8,
    attempt: Option<CommitAttempt>,
) -> CommitRequest {
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
        meta: CommitMeta {
            attempt,
            ..CommitMeta::new(CommitVia::Files)
        },
        force: false,
        required_head: None,
    }
}

fn committed(outcome: CommitOutcome) -> CommitRecord {
    match outcome {
        CommitOutcome::Committed { record, .. } => *record,
        other => panic!("not committed: {other:?}"),
    }
}

fn attempt(tag: u8) -> CommitAttempt {
    CommitAttempt::from_bytes([tag; 16])
}

#[test]
fn a_commit_is_found_by_its_attempt_and_only_by_it() {
    on_both("attempt-found", found_by_attempt);
}
fn found_by_attempt(history: &dyn HistoryCatalog) {
    let branch = project(history);
    let first = committed(
        history
            .commit_branch(&over_now(history, branch, 10, Some(attempt(1))))
            .unwrap(),
    );
    let second = committed(
        history
            .commit_branch(&over_now(history, branch, 11, Some(attempt(2))))
            .unwrap(),
    );
    // A Commit made without an attempt is stored as before and found by none.
    let plain = committed(
        history
            .commit_branch(&over_now(history, branch, 12, None))
            .unwrap(),
    );
    assert_eq!(first.meta.attempt, Some(attempt(1)));
    assert_eq!(plain.meta.attempt, None);
    for (tag, record) in [(1, &first), (2, &second)] {
        let found = history.commit_by_attempt(attempt(tag)).unwrap();
        assert_eq!(found.as_ref(), Some(record), "attempt {tag}");
        // The stored row decodes to the same record by identity too.
        assert_eq!(history.commit(record.id).unwrap().as_ref(), Some(record));
    }
    assert_eq!(history.commit_by_attempt(attempt(3)).unwrap(), None);
}

/// Two attempts of the same content are two Commits: the attempt is part of
/// the identity, so an identical earlier Commit can never absorb a later
/// publication (the insert's `ON CONFLICT (commit_id) DO NOTHING`) and make
/// its attempt unfindable.
#[test]
fn the_attempt_is_part_of_the_identity() {
    on_both("attempt-identity", attempt_in_identity);
}
fn attempt_in_identity(history: &dyn HistoryCatalog) {
    let branch = project(history);
    let record = committed(
        history
            .commit_branch(&over_now(history, branch, 20, Some(attempt(4))))
            .unwrap(),
    );
    let derive = |attempt| {
        CommitId::derive(
            record.root,
            record.parent,
            record.base_layer,
            record.second_parent,
            &CommitMeta {
                attempt,
                ..record.meta.clone()
            },
        )
    };
    assert_eq!(derive(Some(attempt(4))), record.id);
    let other = derive(Some(attempt(5)));
    let none = derive(None);
    assert!(other != record.id && none != record.id && other != none);
}

/// The attempt is unique among the Store's Commits: a second publication
/// under an attempt already used is refused and publishes nothing.
#[test]
fn an_attempt_names_at_most_one_commit() {
    on_both("attempt-unique", attempt_unique);
}
fn attempt_unique(history: &dyn HistoryCatalog) {
    let branch = project(history);
    let first = committed(
        history
            .commit_branch(&over_now(history, branch, 30, Some(attempt(6))))
            .unwrap(),
    );
    let refused = history.commit_branch(&over_now(history, branch, 31, Some(attempt(6))));
    assert!(refused.is_err(), "{refused:?}");
    let now = history.branch(branch).unwrap().unwrap();
    assert_eq!(now.head_commit, Some(first.id), "nothing published");
    assert_eq!(history.commit_by_attempt(attempt(6)).unwrap(), Some(first));
}

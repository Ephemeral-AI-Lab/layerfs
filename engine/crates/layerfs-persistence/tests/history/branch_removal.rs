//! #41 T3: `branch.remove` markers, visibility, typed `NameTaken` and the
//! project epoch on SQLite. The catalog fixture uses only the public
//! `HistoryCatalog` methods.
#[path = "../support/mod.rs"]
mod support;

use layerfs_history::error::{HolderKind, HolderState, Missing, NameSource};
use layerfs_history::records::{
    CommitHistoryRequest, CommitMeta, CommitOutcome, CommitRequest, CommitVia, ForkRequest,
    ForkSource, Page, StackInitialization,
};
use layerfs_history::{
    BranchId, HistoryCatalog, HistoryCatalogConfig, HistoryError, NamedSnapshot,
};
use layerfs_persistence::{PersistenceConfig, SqliteHandles, SqlitePersistenceProfile};
use support::{branch_id, name, root, stack, Temp};

const BINDING: &[u8] = b"branch-removal";
const CURSOR: [u8; 32] = [37; 32];

fn config(path: &std::path::Path) -> PersistenceConfig {
    PersistenceConfig::sqlite(path).with_sqlite_profile(SqlitePersistenceProfile::Normal)
}

fn catalog(temp: &Temp) -> layerfs_persistence::HistoryProvider {
    support::sqlite::create(
        &temp.join("store.sqlite"),
        &HistoryCatalogConfig {
            cursor_key: CURSOR,
            binding_key: BINDING.to_vec(),
            incarnation: 3,
        },
    )
    .expect("catalog")
}

fn writable(path: &std::path::Path) -> layerfs_persistence::HistoryProvider {
    SqliteHandles::open_writable(config(path), BINDING, CURSOR)
        .expect("writable catalog")
        .history
}

/// Project `text` with one Branch `work` and no Commit.
fn project(history: &dyn HistoryCatalog, text: &str) -> (BranchId, layerfs_history::LayerId) {
    let record = history
        .initialize_layerstack(&StackInitialization {
            logical: 1,
            stack: stack(1),
            name: name(text),
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

#[test]
fn a_removed_branch_is_hidden_and_the_epoch_bumps_once() {
    let temp = Temp::new("branch-removal");
    let path = temp.join("store.sqlite");
    let history = catalog(&temp);
    let (branch, genesis) = project(&history, "proj");
    // One retained Commit, so visibility cannot be confused with collection.
    let current = history.branch(branch).unwrap().unwrap().base_layer;
    let current = history.layer(current).unwrap().unwrap().root;
    let record = history.branch(branch).unwrap().unwrap();
    let outcome = history
        .commit_branch(&CommitRequest {
            branch,
            expected_head: record.head_commit,
            expected_base: record.base_layer,
            expected_root: current,
            candidate_root: root(9),
            profile: root(201),
            scope: root(200),
            second_parent: None,
            meta: CommitMeta::new(CommitVia::Files),
            force: false,
            required_head: None,
        })
        .unwrap();
    let CommitOutcome::Committed { record, .. } = outcome else {
        panic!("{outcome:?}");
    };
    let commit = record.id;

    history.mark_branch_removed(branch, 1_000).unwrap();

    // Every Branch resolution and listing hides it.
    assert!(history.branch(branch).unwrap().is_none());
    assert!(history
        .branch_named(stack(1), &name("work"))
        .unwrap()
        .is_none());
    assert!(history.branch_snapshot(branch).unwrap().is_none());
    assert!(matches!(
        history
            .branch_snapshot_named(&name("proj"), &name("work"))
            .unwrap(),
        NamedSnapshot::NoBranch
    ));
    let page = history
        .branches(
            stack(1),
            &Page {
                cursor: None,
                limit: 16,
            },
        )
        .unwrap();
    assert!(page.records.is_empty(), "{:?}", page.records);

    // The retained Commit still reads by identity; the retired Branch's own
    // history is hidden with the Branch.
    assert!(history.commit(commit).unwrap().is_some());
    assert!(matches!(
        history.commit_history(&CommitHistoryRequest {
            branch,
            start: None,
            cursor: None,
            limit: 16,
        }),
        Err(HistoryError::Missing(Missing::Branch))
    ));

    // A repeat is `NoSuchBranch`, not a second marker.
    assert!(matches!(
        history.mark_branch_removed(branch, 1_001),
        Err(HistoryError::Missing(Missing::Branch))
    ));

    // A Branch the marker retired no longer starts a fork, even from the
    // Commit it retained: the re-read inside the transaction is the
    // marker-filtered lookup, so the removal that landed first cannot be
    // outlived by a fork it never admitted.
    assert!(matches!(
        history.fork(&ForkRequest {
            stack: stack(1),
            branch: branch_id(3),
            name: name("from-commit"),
            source: ForkSource::Commit { branch, commit },
        }),
        Err(HistoryError::Missing(Missing::Branch))
    ));
    assert!(history.branch(branch_id(3)).unwrap().is_none());

    // The name is still owned by the retired row, with a typed holder.
    let refusal = history
        .fork(&ForkRequest {
            stack: stack(1),
            branch: branch_id(2),
            name: name("work"),
            source: ForkSource::Layer(genesis),
        })
        .unwrap_err();
    let HistoryError::NameTaken(state) = refusal else {
        panic!("{refusal:?}");
    };
    assert_eq!(state.holder.kind, HolderKind::Branch);
    assert_eq!(state.holder.name, "work");
    assert_eq!(state.holder.state, HolderState::Retired);
    assert_eq!(state.source, NameSource::Store);

    // The marker transaction bumped the epoch exactly once.
    drop(history);
    let connection = rusqlite::Connection::open(&path).unwrap();
    let epoch: i64 = connection
        .query_row(
            "SELECT epoch FROM project_epoch WHERE layer_stack_id = ?1",
            [stack(1).as_slice()],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(epoch, 1);
}

#[test]
fn a_live_holder_is_reported_for_branch_and_project_names() {
    let temp = Temp::new("branch-name-taken");
    let history = catalog(&temp);
    let (_, genesis) = project(&history, "proj");

    let refusal = history
        .fork(&ForkRequest {
            stack: stack(1),
            branch: branch_id(2),
            name: name("work"),
            source: ForkSource::Layer(genesis),
        })
        .unwrap_err();
    let HistoryError::NameTaken(state) = refusal else {
        panic!("{refusal:?}");
    };
    assert_eq!(state.holder.kind, HolderKind::Branch);
    assert_eq!(state.holder.state, HolderState::Live);
    assert_eq!(state.source, NameSource::Store);

    let refusal = history
        .initialize_layerstack(&StackInitialization {
            logical: 1,
            stack: stack(2),
            name: name("proj"),
            scope: root(200),
            profile: root(201),
            genesis_root: root(2),
        })
        .unwrap_err();
    let HistoryError::NameTaken(state) = refusal else {
        panic!("{refusal:?}");
    };
    assert_eq!(state.holder.kind, HolderKind::Project);
    assert_eq!(state.holder.name, "proj");
    assert_eq!(state.holder.state, HolderState::Live);
    assert_eq!(state.source, NameSource::Store);
}

#[test]
fn concurrent_removes_have_one_winner() {
    let temp = Temp::new("branch-removal-race");
    let path = temp.join("store.sqlite");
    let history = catalog(&temp);
    let (branch, _) = project(&history, "proj");
    drop(history);

    let left = writable(&path);
    let right = writable(&path);
    let (left, right) = std::thread::scope(|scope| {
        let left = scope.spawn(move || left.mark_branch_removed(branch, 2_000));
        let right = scope.spawn(move || right.mark_branch_removed(branch, 2_001));
        (left.join().unwrap(), right.join().unwrap())
    });
    let wins = [&left, &right]
        .iter()
        .filter(|result| result.is_ok())
        .count();
    assert_eq!(wins, 1, "{left:?} / {right:?}");
    for error in [left.err(), right.err()].into_iter().flatten() {
        assert!(
            matches!(
                error,
                HistoryError::Missing(Missing::Branch) | HistoryError::Busy
            ),
            "{error:?}"
        );
    }
}

/// A live Branch's retained Commit still starts a fork through the
/// marker-filtered re-read: only the removal marker hides a source.
#[test]
fn a_live_commit_source_still_forks() {
    let temp = Temp::new("branch-removal-fork-live");
    let history = catalog(&temp);
    let (branch, _genesis) = project(&history, "proj");
    let live = history.branch(branch).unwrap().unwrap();
    let base = history.layer(live.base_layer).unwrap().unwrap().root;
    let outcome = history
        .commit_branch(&CommitRequest {
            branch,
            expected_head: live.head_commit,
            expected_base: live.base_layer,
            expected_root: base,
            candidate_root: root(9),
            profile: root(201),
            scope: root(200),
            second_parent: None,
            meta: CommitMeta::new(CommitVia::Files),
            force: false,
            required_head: None,
        })
        .unwrap();
    let CommitOutcome::Committed { record, .. } = outcome else {
        panic!("{outcome:?}");
    };
    let forked = history
        .fork(&ForkRequest {
            stack: stack(1),
            branch: branch_id(2),
            name: name("from-commit"),
            source: ForkSource::Commit {
                branch,
                commit: record.id,
            },
        })
        .unwrap();
    assert_eq!(forked.branch.name.as_str(), "from-commit");
    assert_eq!(forked.branch.head_commit, Some(record.id));
}

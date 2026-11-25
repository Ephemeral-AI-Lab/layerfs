//! #768 over #807: a fused Workspace Commit on a Store `store upgrade`
//! brought to the current format. An upgraded Store has `commit.attempt`
//! where `ADD COLUMN` appended it (PostgreSQL after `history_serial`;
//! SQLite's d-appended variant at the end), not where a new Store has it,
//! so every text the fused path sends must name its columns. Two fused
//! Commits per Store (the second with a parent line), each found by its
//! attempt with its attempt stored, and the Branch's snapshot read back.
//!
//! PostgreSQL needs `LAYERFS_TEST_POSTGRES_URL` (NOT_RUN without it); the
//! SQLite cases always run.
mod support;

use layerfs_history::records::{CommitMeta, CommitOutcome, CommitRequest, CommitVia};
use layerfs_history::{BranchSnapshot, CommitAttempt, HistoryName};
use layerfs_history::{HistoryCatalog, NamedSnapshot};
use layerfs_persistence::upgrade::{upgrade_postgresql, upgrade_sqlite, UpgradeRequest};
use layerfs_persistence::{
    HistoryProvider, PersistenceConfig, PostgresHandles, PostgresSelection, SqliteHandles,
    SqlitePersistenceProfile,
};
use std::sync::atomic::AtomicBool;
use support::pg_fixture::{FixtureDb, SCHEMA};
use support::upgrade::{sqlite_fixture_copy, SQLITE_V2};
use support::Temp;

fn lib_dev(history: &HistoryProvider) -> BranchSnapshot {
    let NamedSnapshot::Found(snapshot) = history
        .branch_snapshot_named(
            &HistoryName::new("lib").unwrap(),
            &HistoryName::new("dev").unwrap(),
        )
        .unwrap()
    else {
        panic!("lib/dev");
    };
    *snapshot
}

/// Two fused Commits on `lib/dev`, each checked by its attempt.
fn two_fused(label: &str, history: &HistoryProvider) {
    for seed in [41u8, 42] {
        let snapshot = lib_dev(history);
        let attempt = CommitAttempt::from_bytes([seed; 16]);
        let request = CommitRequest {
            branch: snapshot.branch.id,
            expected_head: snapshot.branch.head_commit,
            expected_base: snapshot.branch.base_layer,
            expected_root: snapshot.effective_root,
            candidate_root: support::root(seed),
            profile: snapshot.profile,
            scope: snapshot.scope,
            second_parent: None,
            meta: CommitMeta {
                attempt: Some(attempt),
                ..CommitMeta::new(CommitVia::Workspace)
            },
            force: false,
            required_head: None,
        };
        let outcome = history
            .committer()
            .commit(None, snapshot.branch.stack, &request, &[])
            .unwrap_or_else(|e| panic!("{label}: fused Commit {seed}: {e:?}"));
        let CommitOutcome::Committed { record, .. } = outcome else {
            panic!("{label}: not committed: {outcome:?}");
        };
        assert_eq!(record.parent, snapshot.branch.head_commit, "{label}");
        let found = history.commit_by_attempt(attempt).unwrap().unwrap();
        assert_eq!(found.id, record.id, "{label}: found by its attempt");
        assert_eq!(found.meta.attempt, Some(attempt), "{label}: attempt stored");
        let now = lib_dev(history);
        assert_eq!(now.branch.head_commit, Some(record.id), "{label}: head");
        assert_eq!(now.effective_root, support::root(seed), "{label}: root");
    }
}

#[test]
fn postgresql_fused_commits_on_an_upgraded_store() {
    let Some(admin) = support::pg::url() else {
        return;
    };
    let db = FixtureDb::restore(&admin, "fused768");
    let request = UpgradeRequest {
        cancel: &AtomicBool::new(false),
        failpoint: None,
    };
    upgrade_postgresql(&db.url, SCHEMA, &request, &mut |_| {}).unwrap();
    let binding: Vec<u8> = db
        .client()
        .query_one("SELECT binding_key FROM history_meta", &[])
        .unwrap()
        .get(0);
    let handles =
        PostgresHandles::open_writable(&PostgresSelection::new(&db.url, SCHEMA), &binding, [1; 32])
            .unwrap();
    two_fused("postgresql", &handles.history);
}

#[test]
fn sqlite_fused_commits_on_upgraded_stores() {
    for (variant, folder) in SQLITE_V2 {
        let temp = Temp::new("fused-upgraded");
        let path = sqlite_fixture_copy(temp.path(), folder);
        let request = UpgradeRequest {
            cancel: &AtomicBool::new(false),
            failpoint: None,
        };
        upgrade_sqlite(&path, &request, &mut |_| {}).unwrap();
        let binding: Vec<u8> = rusqlite::Connection::open(&path)
            .unwrap()
            .query_row("SELECT binding_key FROM history_meta", [], |r| r.get(0))
            .unwrap();
        let config = PersistenceConfig::sqlite(path.clone())
            .with_sqlite_profile(SqlitePersistenceProfile::Normal);
        let handles = SqliteHandles::open_writable(config, &binding, [1; 32]).unwrap();
        two_fused(&format!("sqlite {variant}"), &handles.history);
    }
}

//! #776 P2: a GC session advisory key is taken and dropped outside the
//! generation fence, so a Store that moved to a later generation refuses the
//! registration with `StaleStore` and leaves no key held by this session.
//!
//! One test in its own binary, so no sibling test holds advisory keys while
//! it counts them. Needs `LAYERFS_TEST_POSTGRES_URL`; NOT_RUN without it.
#[path = "../support/mod.rs"]
mod support;

use layerfs_history::error::HistoryError;
use layerfs_history::records::{GcIntent, GcIntentKind, GcScopeKind};
use layerfs_history::HistoryCatalog;
use support::pg::{project, url, Schema};

/// Granted advisory locks every other backend holds.
fn advisory_keys_held(admin: &mut postgres::Client) -> i64 {
    admin
        .query_one(
            "SELECT count(*) FROM pg_locks WHERE locktype='advisory' AND granted \
               AND pid<>pg_backend_pid()",
            &[],
        )
        .unwrap()
        .get(0)
}

#[test]
fn a_stale_registration_leaves_no_session_key() {
    let Some(url) = url() else { return };
    let schema = Schema::new(url, "session_keys_stale");
    let binding = format!("postgresql-session-keys-{}", std::process::id()).into_bytes();
    let handles = schema.create(&binding, [91; 32]);
    let (stack, branch, _genesis) = project(&handles.history, 1);
    let mut admin = schema.admin();
    let before = advisory_keys_held(&mut admin);

    // Another process's compaction swap moved the Store to the next generation.
    admin
        .batch_execute(
            "UPDATE history_meta SET catalog_incarnation=catalog_incarnation+1 WHERE id=1",
        )
        .unwrap();

    let mount = GcIntent::shared(
        [7; 16],
        GcIntentKind::Mount,
        GcScopeKind::Branch,
        branch.as_slice(),
    )
    .in_project(stack.as_slice())
    .named(Some("proj"), Some("work"))
    .owned(Some("runtime-a"), Some("boot-a"));
    let refusal = handles.history.gc_register(&mount).unwrap_err();
    assert!(matches!(refusal, HistoryError::StaleStore), "{refusal:?}");
    assert_eq!(
        advisory_keys_held(&mut admin),
        before,
        "the refused registration must not keep its Store, project or Branch key"
    );
}

//! T12.3c: §12 AC-34, the mount bind-failure path over the SQLite Store
//! service. A committed mount intent whose bind fails after the admission is
//! deleted in the error path, so no `MountsActive` remains. The failed-delete
//! arm is not inducible on a SQLite Store: the storage schema guard rejects a
//! trigger on `gc_intent`, and an outside lock held across the bind would
//! refuse the register too, so the row names that clause as uncovered.
#[path = "../support/installed_store.rs"]
#[allow(dead_code)]
mod installed;
#[path = "../support/sdk.rs"]
#[allow(dead_code)]
mod support;

use layerfs_history::{BranchId, HistoryName, LayerStackId, WorkspaceId};
use layerfs_protocol::sdk::SdkValue;
use layerfs_store_service::store::{BindError, BindPhase, BindRequest};
use support::Fixture;

/// The installed fixture's project and a Branch `broken` whose stack profile
/// no longer matches its root: the bind's root check must fail after the
/// admission intent commits. The Commit itself is service-written, so every
/// history identity stays valid; only the stack row's profile changes.
fn break_root(fixture: &Fixture) -> BranchId {
    let stack = LayerStackId::from_authority([23; 16]);
    let branch = BranchId::named(stack, &HistoryName::new("broken").unwrap());
    fixture.fork("broken");
    fixture.apply("broken", vec![support::write("/b", "b\n")]);
    let connection = rusqlite::Connection::open(&fixture.host.config.path).unwrap();
    let changed = connection
        .execute(
            "UPDATE layer_stack SET profile_id = ?1 WHERE layer_stack_id = ?2",
            rusqlite::params![[0x42u8; 32].to_vec(), stack.as_slice().to_vec()],
        )
        .unwrap();
    assert_eq!(changed, 1, "the fixture has one LayerStack");
    branch
}

fn bind(
    fixture: &Fixture,
    branch: BranchId,
    id: u8,
) -> Result<(), layerfs_store_service::store::BindRefusal> {
    fixture
        .service
        .store
        .bind(
            fixture.owner.client(),
            BindRequest {
                workspace: WorkspaceId::from_authority([id; 32]).unwrap(),
                branch,
            },
        )
        .map(|_| ())
}

/// The committed admission intents the Store holds.
fn intents(fixture: &Fixture) -> Vec<Vec<u8>> {
    let connection = rusqlite::Connection::open(&fixture.host.config.path).unwrap();
    let mut statement = connection.prepare("SELECT op_id FROM gc_intent").unwrap();
    statement
        .query_map([], |row| row.get(0))
        .unwrap()
        .map(|row| row.unwrap())
        .collect()
}

#[test]
fn a_bind_failure_cleans_its_intent_and_leaves_no_mounts_active() {
    let fixture = Fixture::new("t123c-bind", |source| {
        std::fs::write(source.join("a"), "a\n").unwrap();
        1
    });
    let branch = break_root(&fixture);

    // The admission intent commits, the root check then fails: the refusal
    // names the root phase and the error path deleted the intent.
    let refusal = bind(&fixture, branch, 71).expect_err("the root is not a filesystem root");
    assert_eq!(refusal.phase, BindPhase::Root, "{refusal:?}");
    assert!(
        matches!(refusal.error, BindError::Content { .. }),
        "{refusal:?}"
    );
    assert!(
        intents(&fixture).is_empty(),
        "a failed bind leaves no committed intent"
    );
    // No `MountsActive` remains: the removal is admitted.
    fixture.call("project.remove_file", vec![("path", SdkValue::text("/a"))]);

    fixture.cleanup();
}

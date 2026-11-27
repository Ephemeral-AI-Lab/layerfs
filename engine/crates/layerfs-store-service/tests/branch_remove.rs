//! #41 T3: `branch.remove` through the Store service and the typed
//! `NameTaken`/`NoSuchBranch` fork syntax on an installed Store.
#[path = "support/installed_store.rs"]
#[allow(dead_code)]
mod installed;
#[path = "support/sdk.rs"]
#[allow(dead_code)]
mod support;

use layerfs_protocol::provision::install::{SdkRefusal, SdkValue};
use layerfs_protocol::sdk::hex;
use layerfs_store_service::history::{admission, lookup, removal};
use std::fs;
use support::Fixture;

fn text(value: &str) -> SdkValue {
    SdkValue::text(value)
}

fn refusal(fixture: &Fixture, method: &str, fields: Vec<(&str, SdkValue)>) -> SdkRefusal {
    fixture
        .try_call(method, fields)
        .expect_err("the call is refused")
}

fn project(fixture: &Fixture) -> layerfs_history::LayerStackRecord {
    lookup::project(&fixture.service.store, "installed").expect("installed project")
}

#[test]
fn a_valid_absent_fork_source_is_no_such_branch() {
    let fixture = Fixture::new("branch-remove-fork", |source| {
        fs::write(source.join("a.txt"), b"a\n").unwrap();
        1
    });
    let refused = refusal(
        &fixture,
        "branch.fork",
        vec![("name", text("work")), ("from", text("ghost"))],
    );
    assert_eq!(refused.code, "NoSuchBranch");
    assert_eq!(refused.message, "Branch ghost does not exist");
}

#[test]
fn a_live_name_is_a_typed_name_taken() {
    let fixture = Fixture::new("branch-remove-live", |source| {
        fs::write(source.join("a.txt"), b"a\n").unwrap();
        1
    });
    fixture.fork("feat");
    let refused = refusal(&fixture, "branch.fork", vec![("name", text("feat"))]);
    assert_eq!(refused.code, "NameTaken", "{refused:?}");
    assert_eq!(refused.detail.get_text("source"), Some("store"));
    let holder = refused.detail.get("holder").expect("holder detail");
    assert_eq!(holder.get_text("kind"), Some("branch"));
    assert_eq!(holder.get_text("name"), Some("feat"));
    assert_eq!(holder.get_text("state"), Some("live"));
}

#[test]
fn a_removed_branch_is_hidden_and_holds_its_name() {
    let fixture = Fixture::new("branch-remove-service", |source| {
        fs::write(source.join("a.txt"), b"a\n").unwrap();
        1
    });
    fixture.fork("gone");
    let stack = project(&fixture);
    let branch = lookup::branch(&fixture.service.store, &stack, "gone").expect("Branch gone");
    assert_eq!(branch.name.as_str(), "gone");

    let removed = removal::remove(&fixture.service.store, &stack, "gone").expect("remove");
    assert_eq!(removed.get_text("kind"), Some("Removed"));
    assert_eq!(removed.get_text("project"), Some("installed"));
    assert_eq!(removed.get_text("branch"), Some("gone"));
    assert!(removed.get_int("removed_ms").is_some_and(|ms| ms >= 0));

    // A second removal and every name resolution see no Branch.
    let refused = removal::remove(&fixture.service.store, &stack, "gone")
        .expect_err("a repeat is NoSuchBranch");
    assert_eq!(refused.code, "NoSuchBranch", "{refused:?}");
    let refused = refusal(
        &fixture,
        "branch.list_commits",
        vec![("branch", text("gone"))],
    );
    assert_eq!(refused.code, "NoSuchBranch");
    let refused = refusal(
        &fixture,
        "fs.read",
        vec![("branch", text("gone")), ("path", text("/a.txt"))],
    );
    assert_eq!(refused.code, "NoSuchBranch");

    // The Branch is gone from the listing.
    let listed = fixture.call("project.list_branches", Vec::new());
    let SdkValue::List(items) = listed.get("items").expect("items") else {
        panic!("{listed:?}");
    };
    assert!(
        !items
            .iter()
            .any(|item| item.get_text("name") == Some("gone")),
        "{listed:?}"
    );

    // A removed Branch still owns its name; the detail names the holder.
    let refused = refusal(&fixture, "branch.fork", vec![("name", text("gone"))]);
    assert_eq!(refused.code, "NameTaken", "{refused:?}");
    let holder = refused.detail.get("holder").expect("holder detail");
    assert_eq!(holder.get_text("kind"), Some("branch"));
    assert_eq!(holder.get_text("name"), Some("gone"));
    assert_eq!(holder.get_text("state"), Some("retired"));

    // Forking from its old name resolves no Branch.
    let refused = refusal(
        &fixture,
        "branch.fork",
        vec![("name", text("from-gone")), ("from", text("gone"))],
    );
    assert_eq!(refused.code, "NoSuchBranch", "{refused:?}");

    // `main` is reserved.
    let refused =
        removal::remove(&fixture.service.store, &stack, "main").expect_err("main is reserved");
    assert_eq!(refused.code, "ReservedName");
    // Spec gaps G1: a code with no spec'd detail carries an empty detail.
    assert!(
        matches!(&refused.detail, SdkValue::Map(fields) if fields.is_empty()),
        "{refused:?}"
    );
    // The other `ReservedName` sites use the same empty detail.
    fixture.fork("other");
    for (method, fields) in [
        ("branch.fork", vec![("name", text("main"))]),
        (
            "branch.merge",
            vec![
                ("branch", text("main")),
                ("with", text("other")),
                ("name", text("main")),
            ],
        ),
    ] {
        let refused = fixture
            .try_call(method, fields)
            .expect_err("main is reserved");
        assert_eq!(refused.code, "ReservedName", "{method}: {refused:?}");
        assert!(
            matches!(&refused.detail, SdkValue::Map(fields) if fields.is_empty()),
            "{method}: {refused:?}"
        );
    }
}

#[test]
fn a_concurrent_remove_reports_busy_with_the_retire_phase() {
    let fixture = Fixture::new("branch-remove-busy", |source| {
        fs::write(source.join("a.txt"), b"a\n").unwrap();
        1
    });
    fixture.fork("gone");
    let stack = project(&fixture);
    let branch = lookup::branch(&fixture.service.store, &stack, "gone").expect("Branch gone");
    // The row `branch.remove` takes before it writes its marker: its own
    // retire-phase intent, registered in a separate write transaction so a
    // concurrent remove can observe it. Pinning the same builder's row makes
    // this test deterministic.
    let holder = admission::branch_removal(&stack, &branch);
    let catalog = fixture.service.store.history();
    catalog
        .gc_register(&holder)
        .expect("register the concurrent removal");
    let refused = removal::remove(&fixture.service.store, &stack, "gone")
        .expect_err("the concurrent removal holds the Branch");
    assert_eq!(refused.code, "Busy", "{refused:?}");
    let entity = refused.detail.get("entity").expect("entity detail");
    assert_eq!(entity.get_text("kind"), Some("branch"), "{entity:?}");
    assert_eq!(entity.get_text("name"), Some("gone"), "{entity:?}");
    assert_eq!(refused.detail.get_text("phase"), Some("retire"));
    assert_eq!(
        refused.detail.get_text("op"),
        Some(hex(&holder.op_id).as_str())
    );
    // The refused call registered nothing of its own: the concurrent row is
    // the only intent, still certain and live.
    let listed = catalog
        .gc_intents(
            layerfs_history::records::GcScopeKind::Branch,
            branch.id.as_slice(),
        )
        .expect("intents");
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].op_id, holder.op_id);
    assert!(!listed[0].uncertain);

    catalog.gc_release(&holder.op_id).expect("release");
    // A successful remove registers its own retire intent and releases it, so
    // it leaves no admission row behind.
    removal::remove(&fixture.service.store, &stack, "gone").expect("remove after release");
    assert!(catalog
        .gc_intents(
            layerfs_history::records::GcScopeKind::Branch,
            branch.id.as_slice(),
        )
        .expect("intents")
        .is_empty());
    fixture.cleanup();
}

/// The registration is exposed before the marker: a trigger on
/// `branch_retired` records the live removal intent inside the marker
/// transaction. On the parent commit no such row exists at that point, so the
/// concurrent remove that F43-2 describes can never be `Busy`.
#[test]
fn a_remove_registers_its_retire_intent_before_the_marker() {
    let fixture = Fixture::new("branch-remove-register", |source| {
        fs::write(source.join("a.txt"), b"a\n").unwrap();
        1
    });
    fixture.fork("gone");
    let stack = project(&fixture);
    let branch = lookup::branch(&fixture.service.store, &stack, "gone").expect("Branch gone");
    let catalog = fixture.service.store.history();

    let connection = rusqlite::Connection::open(&fixture.host.config.path).unwrap();
    connection
        .execute_batch(
            "CREATE TABLE test_retire_seen (op_id BLOB NOT NULL, phase TEXT);
             CREATE TRIGGER test_retire_seen_marker AFTER INSERT ON branch_retired
             BEGIN
                 INSERT INTO test_retire_seen (op_id, phase)
                 SELECT op_id, phase FROM gc_intent
                 WHERE kind = 'removal' AND scope_kind = 'branch'
                   AND scope_id = NEW.branch_id;
             END;",
        )
        .unwrap();
    drop(connection);

    removal::remove(&fixture.service.store, &stack, "gone").expect("remove");

    let connection = rusqlite::Connection::open(&fixture.host.config.path).unwrap();
    let (op_id, phase): (Vec<u8>, String) = connection
        .query_row("SELECT op_id, phase FROM test_retire_seen", [], |row| {
            Ok((row.get(0)?, row.get(1)?))
        })
        .expect("the marker transaction saw the retire intent");
    assert_eq!(op_id.len(), 16);
    assert_eq!(phase, "retire");
    // The call released the intent after the marker.
    assert!(catalog
        .gc_intents(
            layerfs_history::records::GcScopeKind::Branch,
            branch.id.as_slice(),
        )
        .expect("intents")
        .is_empty());
    fixture.cleanup();
}

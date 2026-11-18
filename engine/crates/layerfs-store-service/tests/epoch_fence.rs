//! #41 T6.5: the writer-side `project_epoch` fence on the Workspace Commit
//! path. `fs.write` (and the SDK `commit` method) go through
//! `files::commit` -> `BoundWorkspace::commit` -> `HistoryCatalog::commit_branch`,
//! so a fence refusal must arrive as the spec's `Conflict`, not as a generic
//! failure, and an ordinary write must be unaffected.
#[path = "support/installed_store.rs"]
#[allow(dead_code)]
mod installed;
#[path = "support/sdk.rs"]
#[allow(dead_code)]
mod support;

use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_store_service::history::lookup;
use std::fs;
use support::Fixture;

fn text(value: &str) -> SdkValue {
    SdkValue::text(value)
}

fn write(fixture: &Fixture, content: &str) -> Result<SdkValue, SdkRefusal> {
    fixture.try_call(
        "fs.write",
        vec![
            ("branch", text("work")),
            ("path", text("/a.txt")),
            ("content", text(content)),
        ],
    )
}

fn commits(fixture: &Fixture) -> usize {
    support::items(&fixture.call("branch.list_commits", vec![("branch", text("work"))])).len()
}

#[test]
fn a_missing_epoch_row_is_conflict_on_the_workspace_commit_path() {
    let fixture = Fixture::new("epoch-fence-service", |source| {
        fs::write(source.join("a.txt"), b"a\n").unwrap();
        1
    });
    fixture.fork("work");

    // The ordinary path still commits and leaves the epoch row alone.
    write(&fixture, "b\n").expect("an ordinary write commits");
    let epoch_row = |path: &std::path::Path, stack: &[u8]| -> i64 {
        rusqlite::Connection::open(path)
            .unwrap()
            .query_row(
                "SELECT epoch FROM project_epoch WHERE layer_stack_id = ?1",
                [stack],
                |row| row.get(0),
            )
            .unwrap()
    };
    let stack = lookup::project(&fixture.service.store, "installed").expect("project");
    assert_eq!(epoch_row(&fixture.host.config.path, stack.id.as_slice()), 0);

    // Remove the fence row behind the service's back: the next Commit's
    // start-of-transaction read finds no epoch to honour.
    rusqlite::Connection::open(&fixture.host.config.path)
        .unwrap()
        .execute(
            "DELETE FROM project_epoch WHERE layer_stack_id = ?1",
            [stack.id.as_slice()],
        )
        .unwrap();

    let before = commits(&fixture);
    let refused = write(&fixture, "c\n").expect_err("the fence refuses the commit");
    assert_eq!(refused.code, "Conflict", "{refused:?}");
    assert!(refused.message.contains("project epoch"), "{refused:?}");
    assert_eq!(commits(&fixture), before, "nothing was published");

    fixture.cleanup();
}

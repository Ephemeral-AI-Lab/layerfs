//! `workspace.changes` over a bound Workspace, without a kernel mount: the
//! path-awareness of its base walk. Moved out of `operator/workspace.rs`:
//! product source holds no tests (the product-boundary guard).

#[path = "support/installed_store.rs"]
mod installed;

use layerfs_bridge::control::SdkValue;
use layerfs_daemon::application::internals::changes_of;
use layerfs_daemon::control::Service;
use layerfs_daemon::{Owner, OwnerConfig};
use layerfs_history::{PathRemovalKind, PathRemovalRequest, WorkspaceId};
use layerfs_storage::ReservationBlocks;
use layerfs_store_service::{
    bootstrap::open_store_observed, store::BindRequest, write::mutation::Mutation,
};
use std::fs;

fn listed(answer: &SdkValue) -> Vec<(String, String)> {
    let Some(SdkValue::List(items)) = answer.get("items") else {
        panic!("no items: {answer:?}");
    };
    items
        .iter()
        .map(|item| {
            (
                item.get_text("path").expect("path").to_owned(),
                item.get_text("kind").expect("kind").to_owned(),
            )
        })
        .collect()
}

/// A file and a directory removed by `project.remove_file` markers at the
/// bound node are absent from `changes`, subtree and all; a visible
/// change is still listed, and a post-bound re-creation is an addition.
#[test]
fn a_removed_file_and_a_hidden_directory_are_absent_from_changes() {
    let fixture = installed::Fixture::built("changes-visibility", |source| {
        fs::write(source.join("visible.txt"), b"visible\n").unwrap();
        fs::write(source.join("hidden.txt"), b"hidden\n").unwrap();
        fs::create_dir(source.join("hidden-dir")).unwrap();
        fs::write(source.join("hidden-dir/inner.txt"), b"inner\n").unwrap();
        4
    });
    let opened = open_store_observed(
        fixture.config.clone(),
        installed::BINDING,
        installed::CURSOR,
        64,
        8 * 1024 * 1024,
        ReservationBlocks::default(),
    )
    .unwrap();
    let stack = opened
        .store
        .history()
        .branch_snapshot(fixture.branch)
        .unwrap()
        .unwrap()
        .branch
        .stack;
    for path in ["hidden.txt", "hidden-dir"] {
        let outcome = opened
            .store
            .history()
            .remove_path(&PathRemovalRequest {
                stack,
                path: path.as_bytes().to_vec(),
                removed_ms: 1,
            })
            .unwrap();
        assert_eq!(outcome.kind, PathRemovalKind::Removed, "{path}");
    }
    let owner = Owner::start(
        &fixture.directory.join("overlay.sqlite"),
        layerfs_overlay::ProfileConfig::default(),
        OwnerConfig::default(),
    )
    .unwrap();
    let bound = opened
        .store
        .bind(
            owner.client(),
            BindRequest {
                branch: fixture.branch,
                workspace: WorkspaceId::from_authority([57; 32]).unwrap(),
            },
        )
        .unwrap()
        .workspace;
    // Bind wrote the whiteouts; a visible change and one re-creation follow.
    let mut mutation = Mutation::new(&bound).unwrap();
    mutation.write("/visible.txt", b"after\n", None).unwrap();
    mutation.finish().unwrap();
    let mut mutation = Mutation::new(&bound).unwrap();
    mutation.write("/hidden.txt", b"again\n", None).unwrap();
    mutation.finish().unwrap();

    let service = Service::new(opened.store.clone(), &owner);
    let answer = changes_of(
        &service,
        &bound,
        &SdkValue::map([("pageSize", SdkValue::Int(50))]),
    )
    .unwrap();
    assert_eq!(
        listed(&answer),
        [
            ("/hidden.txt".to_owned(), "Added".to_owned()),
            ("/visible.txt".to_owned(), "Modified".to_owned()),
        ]
    );
    assert_eq!(answer.get("hasMore"), Some(&SdkValue::Bool(false)));

    drop(service);
    bound.close().unwrap();
    owner.stop().unwrap();
    drop(opened);
    fixture.cleanup();
}

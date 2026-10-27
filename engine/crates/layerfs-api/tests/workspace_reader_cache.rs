//! A Workspace reader keeps its storage caches across demands (#354 A1): a
//! repeated object or length demand is answered from the pack bodies and
//! decoded groups an earlier demand read, with no statement and identical
//! bytes. The Store's control reader, which serves SDK calls, keeps none, and
//! a reader closed and mounted again starts empty.
#[path = "support/installed_store.rs"]
#[allow(dead_code)]
mod installed;
#[path = "support/sdk.rs"]
#[allow(dead_code)]
mod support;

use layerfs_bridge::control::SdkValue;
use layerfs_content::{object::inode_leaf::InodeKind, AuthenticatedObjects};
use layerfs_history::WorkspaceId;
use layerfs_store_service::read::window;
use support::Fixture;

const FILES: usize = 120;

#[test]
fn a_workspace_reader_keeps_its_caches_until_it_is_closed() {
    let fixture = Fixture::new("workspace-reader-cache", |source| {
        for f in 0..FILES {
            let dir = source.join(format!("d{}", f % 4));
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(
                dir.join(format!("f{f:03}.txt")),
                format!("file {f}\n").repeat(1 + f % 37),
            )
            .unwrap();
        }
        FILES + 4
    });
    let store = &fixture.service.store;
    let args = SdkValue::map([
        ("project", SdkValue::text("installed")),
        ("branch", SdkValue::text("main")),
    ]);
    let (stack, control, snap) =
        layerfs_store_service::operator::args::snapshot(store, &args).unwrap();
    let mut roots = Vec::new();
    window::walk(&control, &snap, "/", |_, entry| {
        if entry.value.kind == InodeKind::RegularFile {
            roots.push(entry.value.content_root);
        }
        Ok(true)
    })
    .unwrap();
    assert_eq!(roots.len(), FILES);
    let expected = control.read_canonical_batch(&roots).unwrap();
    let lengths = control.file_lengths(&roots).unwrap();
    let statements = || fixture.work().statements;

    // The control reader keeps nothing: the same demand reads packs again.
    let before = statements();
    assert_eq!(control.read_canonical_batch(&roots).unwrap(), expected);
    assert!(statements() > before, "the control reader read again");

    let workspace = WorkspaceId::from_authority([7; 32]).unwrap();
    store.mount_reader(workspace).unwrap();
    let ports = store.ports_for(stack.scope, workspace);
    assert_eq!(ports.read_canonical_batch(&roots).unwrap(), expected);
    // Its later demands come from what the first one kept.
    let before = statements();
    assert_eq!(ports.read_canonical_batch(&roots).unwrap(), expected);
    assert_eq!(ports.file_lengths(&roots).unwrap(), lengths);
    assert_eq!(statements(), before, "kept caches answered both demands");

    // Closed and mounted again, the reader starts empty.
    store.close_reader(workspace).unwrap();
    store.mount_reader(workspace).unwrap();
    let ports = store.ports_for(stack.scope, workspace);
    let before = statements();
    assert_eq!(ports.read_canonical_batch(&roots).unwrap(), expected);
    assert!(statements() > before, "a new reader reads again");
    store.close_reader(workspace).unwrap();
    drop((ports, control));
    fixture.cleanup();
}

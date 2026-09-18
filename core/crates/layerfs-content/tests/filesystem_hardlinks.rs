//! Aliases, derived counts and cross-directory moves.
//!
//! A binding points at an inode identity, so an alias added in one directory and
//! another in a second directory must reach the same count without a journal
//! replay, and a move must not transiently drop an inode to zero.

mod support;

use layerfs_content::filesystem::references::ReferenceReducer;
use layerfs_content::filesystem::{DirectoryUpdate, InodeUpdate, LogicalPath, PathName};
use layerfs_content::object::inode_leaf::{InodeKind, InodeValue};
use layerfs_content::ContentError;
use support::filesystem::{synthetic, value, Session};

fn name(value: &str) -> PathName {
    PathName::new(value).expect("name")
}

fn ordered(mut inodes: Vec<InodeUpdate>) -> Vec<InodeUpdate> {
    inodes.sort_by_key(|update| update.serial);
    inodes
}

fn ordered_serials(mut serials: Vec<u64>) -> Vec<u64> {
    serials.sort_unstable();
    serials
}

fn regular(content: &str) -> InodeValue {
    value(
        InodeKind::RegularFile,
        synthetic(content),
        synthetic("hardlink/meta"),
    )
}

fn dir_value() -> InodeValue {
    value(
        InodeKind::Directory,
        synthetic("hardlink/unused"),
        synthetic("hardlink/dir-meta"),
    )
}

/// Builds `/a/x`, `/b/y` and the two directories as a fresh tree.
fn tree() -> (Session, u64, u64, u64) {
    let mut session = Session::new(1).expect("empty");
    let file = session.allocate();
    let a = session.allocate();
    let b = session.allocate();
    let directories = [
        DirectoryUpdate {
            parent: 1,
            changes: vec![(name("a"), Some(a)), (name("b"), Some(b))],
        },
        DirectoryUpdate {
            parent: a,
            changes: vec![(name("x"), Some(file))],
        },
        DirectoryUpdate {
            parent: b,
            changes: vec![(name("y"), Some(file))],
        },
    ];
    let inodes = ordered(vec![
        InodeUpdate {
            serial: a,
            value: dir_value(),
        },
        InodeUpdate {
            serial: b,
            value: dir_value(),
        },
        InodeUpdate {
            serial: file,
            value: regular("hardlink/content"),
        },
    ]);
    session
        .apply(&directories, &inodes, &ordered_serials(vec![a, b, file]))
        .expect("tree");
    (session, file, a, b)
}

#[test]
fn two_aliases_reach_one_derived_count() {
    let (mut session, file, a, b) = tree();
    let mut read = session.read().expect("reader");
    assert_eq!(
        read.stat(&LogicalPath::new("a/x").unwrap())
            .expect("stat")
            .namespace_ref_count,
        2,
        "the count is derived from both retained bindings, not from one directory"
    );
    assert_eq!(
        read.resolve(&LogicalPath::new("b/y").unwrap())
            .expect("resolve")
            .serial,
        file
    );

    // A third alias in a third directory raises it again, and the two untouched
    // aliases keep their own bindings.
    let c = session.allocate();
    session
        .apply(
            &[
                DirectoryUpdate {
                    parent: 1,
                    changes: vec![
                        (name("a"), Some(a)),
                        (name("b"), Some(b)),
                        (name("c"), Some(c)),
                    ],
                },
                DirectoryUpdate {
                    parent: c,
                    changes: vec![(name("z"), Some(file))],
                },
            ],
            &ordered(vec![
                InodeUpdate {
                    serial: c,
                    value: dir_value(),
                },
                InodeUpdate {
                    serial: file,
                    value: regular("hardlink/content"),
                },
            ]),
            &ordered_serials(vec![c]),
        )
        .expect("third alias");
    let mut read = session.read().expect("reader");
    assert_eq!(
        read.stat(&LogicalPath::new("c/z").unwrap())
            .expect("stat")
            .namespace_ref_count,
        3
    );
    assert_eq!(
        read.stat(&LogicalPath::new("a/x").unwrap())
            .expect("stat")
            .namespace_ref_count,
        3,
        "unseen aliases outside the changed paths keep their contribution"
    );
}

#[test]
fn a_move_across_directories_keeps_the_count_and_the_inode() {
    let (mut session, file, a, b) = tree();
    session
        .apply(
            &[
                DirectoryUpdate {
                    parent: a,
                    changes: vec![(name("x"), None)],
                },
                DirectoryUpdate {
                    parent: b,
                    changes: vec![(name("moved"), Some(file)), (name("y"), Some(file))],
                },
            ],
            &[InodeUpdate {
                serial: file,
                value: regular("hardlink/content"),
            }],
            &[],
        )
        .expect("move");
    let mut read = session.read().expect("reader");
    assert!(matches!(
        read.stat(&LogicalPath::new("a/x").unwrap()),
        Err(layerfs_content::ContentError::PathNotFound)
    ));
    let moved = read
        .resolve(&LogicalPath::new("b/moved").unwrap())
        .expect("moved file");
    assert_eq!(moved.serial, file);
    assert_eq!(
        moved.value.namespace_ref_count, 2,
        "the move nets to zero change"
    );
}

#[test]
fn a_child_moved_out_of_a_removed_subtree_survives() {
    let mut session = Session::new(1).expect("empty");
    let directory = session.allocate();
    let child = session.allocate();
    session
        .apply(
            &[
                DirectoryUpdate {
                    parent: 1,
                    changes: vec![(name("d"), Some(directory))],
                },
                DirectoryUpdate {
                    parent: directory,
                    changes: vec![(name("child"), Some(child))],
                },
            ],
            &ordered(vec![
                InodeUpdate {
                    serial: directory,
                    value: dir_value(),
                },
                InodeUpdate {
                    serial: child,
                    value: regular("hardlink/child"),
                },
            ]),
            &ordered_serials(vec![directory, child]),
        )
        .expect("tree");
    // Move the child up to the root and delete the directory in one operation.
    session
        .apply(
            &[DirectoryUpdate {
                parent: 1,
                changes: vec![(name("child"), Some(child)), (name("d"), None)],
            }],
            &[InodeUpdate {
                serial: child,
                value: regular("hardlink/child"),
            }],
            &[],
        )
        .expect("moved out");
    let mut read = session.read().expect("reader");
    let survivor = read
        .resolve(&LogicalPath::new("child").unwrap())
        .expect("the moved child survives its directory's removal");
    assert_eq!(survivor.serial, child);
    assert_eq!(survivor.value.namespace_ref_count, 1);
    assert_eq!(
        read.lookup_inodes(&[directory]).expect("lookup")[0],
        None,
        "the removed directory is gone from the new table"
    );
}

#[test]
fn removing_one_alias_leaves_the_other_and_removing_the_last_removes_it() {
    let (mut session, file, a, _b) = tree();
    session
        .apply(
            &[DirectoryUpdate {
                parent: a,
                changes: vec![(name("x"), None)],
            }],
            &[InodeUpdate {
                serial: file,
                value: regular("hardlink/content"),
            }],
            &[],
        )
        .expect("first alias removed");
    let mut read = session.read().expect("reader");
    assert_eq!(
        read.stat(&LogicalPath::new("b/y").unwrap())
            .expect("stat")
            .namespace_ref_count,
        1,
        "removing one alias leaves the other binding intact"
    );
    let before = session.root;
    let removed = session
        .apply(
            &[DirectoryUpdate {
                parent: 1,
                changes: vec![(name("a"), None), (name("b"), None)],
            }],
            &[],
            &[],
        )
        .expect("removes the tree");
    assert_ne!(removed.root.0, before);
    let mut read = session.read().expect("reader");
    assert_eq!(
        read.lookup_inodes(&[file]).expect("lookup")[0],
        None,
        "the last binding removal removes the inode from the new filesystem"
    );
}

#[test]
fn a_regular_file_keeps_at_least_one_binding() {
    let mut session = Session::new(1).expect("empty");
    let file = session.allocate();
    // A new regular file declared with no binding at all is a disconnected
    // record, and the operation refuses it rather than storing a zero-count file.
    let outcome = session.apply(
        &[],
        &[InodeUpdate {
            serial: file,
            value: regular("hardlink/orphan"),
        }],
        &[file],
    );
    assert!(matches!(
        outcome,
        Err(ContentError::InvalidRecord("new inode without binding"))
    ));
}

#[test]
fn a_declared_new_inode_cannot_lose_a_binding() {
    // A serial the caller declared new did not exist before the operation, so no
    // base page can hold a binding to remove. The reducer refuses the
    // contradiction instead of folding it into the signed effect of a stored
    // record, and the label is the sibling of the disconnected-record refusal.
    let mut reducer = ReferenceReducer::new(64, None, 4096, 1 << 20);
    let declared = 41;
    reducer
        .declare_new(declared)
        .expect("declares a new serial");
    assert!(
        matches!(
            reducer.note_removed_binding(declared),
            Err(ContentError::InvalidRecord("new inode loses a binding"))
        ),
        "a declared new inode cannot lose a base binding"
    );
    // Any other serial is an ordinary stored record and takes the signed effect.
    reducer
        .note_removed_binding(77)
        .expect("an undeclared serial takes the signed effect");
    assert_eq!(
        reducer.state(77).expect("state"),
        Some(
            layerfs_content::filesystem::references::PendingState::Existing {
                value: None,
                delta: -1,
            }
        )
    );
}

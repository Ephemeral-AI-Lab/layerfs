//! Native updates: create, replace, remove and content/metadata-only changes.
//!
//! These exercise the operation through its own inputs, not through the sealed
//! reference cases: what changes, what must not change, and what the rooted result
//! reads back as.

mod support;

use layerfs_content::filesystem::{DirectoryUpdate, InodeUpdate, LogicalPath, PathName};
use layerfs_content::object::inode_leaf::{InodeKind, InodeValue};
use layerfs_content::ObjectId;
use support::filesystem::{synthetic, value, Session};

fn name(value: &str) -> PathName {
    PathName::new(value).expect("name")
}

fn inode(kind: InodeKind, content: &str, metadata: &str) -> InodeValue {
    value(kind, synthetic(content), synthetic(metadata))
}

#[test]
fn a_tiny_tree_is_created_and_read_back_exactly() {
    let mut session = Session::new(1).expect("empty filesystem");
    assert!(
        !session.store.is_empty(),
        "an empty root still emits its pages"
    );
    let directory = session.allocate();
    let file = session.allocate();
    let symlink = session.allocate();
    let updates = [DirectoryUpdate {
        parent: 1,
        changes: vec![
            (name("d"), Some(directory)),
            (name("f"), Some(file)),
            (name("s"), Some(symlink)),
        ],
    }];
    let inodes = [
        InodeUpdate {
            serial: directory,
            value: inode(InodeKind::Directory, "unused", "meta/d"),
        },
        InodeUpdate {
            serial: file,
            value: inode(InodeKind::RegularFile, "content/f", "meta/f"),
        },
        InodeUpdate {
            serial: symlink,
            value: inode(InodeKind::Symlink, "content/s", "meta/s"),
        },
    ];
    let empty_directory = DirectoryUpdate {
        parent: directory,
        changes: Vec::new(),
    };
    let directories = [updates[0].clone(), empty_directory];
    session
        .apply(&directories, &inodes, &[directory, file, symlink])
        .expect("creates the tree");

    let mut read = session.read().expect("reader");
    let root = read.stat(&LogicalPath::root()).expect("stat root");
    assert_eq!(root.kind, InodeKind::Directory);
    assert_eq!(root.namespace_ref_count, 0);
    let listing = read
        .list(&LogicalPath::root(), None, 16, 4096)
        .expect("list");
    assert_eq!(
        listing
            .entries
            .iter()
            .map(|(name, serial)| (name.as_str().to_owned(), *serial))
            .collect::<Vec<_>>(),
        vec![
            ("d".to_owned(), directory),
            ("f".to_owned(), file),
            ("s".to_owned(), symlink)
        ]
    );
    let stat = read
        .stat(&LogicalPath::new("f").unwrap())
        .expect("stat file");
    assert_eq!(stat.kind, InodeKind::RegularFile);
    assert_eq!(stat.namespace_ref_count, 1);
    assert_eq!(stat.content_root, synthetic("content/f"));
    let directory_stat = read
        .stat(&LogicalPath::new("d").unwrap())
        .expect("stat dir");
    assert_eq!(directory_stat.namespace_ref_count, 1);
}

#[test]
fn a_content_only_change_leaves_every_directory_page_untouched() {
    let mut session = Session::new(1).expect("empty filesystem");
    let file = session.allocate();
    let updates = [DirectoryUpdate {
        parent: 1,
        changes: vec![(name("f"), Some(file))],
    }];
    let inodes = [InodeUpdate {
        serial: file,
        value: inode(InodeKind::RegularFile, "content/one", "meta/f"),
    }];
    session
        .apply(&updates, &inodes, &[file])
        .expect("creates the file");
    let before = session.value.inode_table();
    let directories_before = session
        .store
        .order()
        .iter()
        .filter(|(_, role)| {
            matches!(
                role,
                layerfs_content::ObjectRole::DirectoryLeaf
                    | layerfs_content::ObjectRole::DirectoryBranch
            )
        })
        .count();

    // Rewrite only the file's content root: no binding changes at all.
    let inodes = [InodeUpdate {
        serial: file,
        value: inode(InodeKind::RegularFile, "content/two", "meta/f"),
    }];
    let result = session.apply(&[], &inodes, &[]).expect("content update");
    assert_ne!(
        result.value.inode_table(),
        before,
        "the inode table changes when a value does"
    );
    let directories_after = session
        .store
        .order()
        .iter()
        .filter(|(_, role)| {
            matches!(
                role,
                layerfs_content::ObjectRole::DirectoryLeaf
                    | layerfs_content::ObjectRole::DirectoryBranch
            )
        })
        .count();
    assert_eq!(
        directories_after, directories_before,
        "a value-only update must not rewrite or re-emit a directory page"
    );
    let mut read = session.read().expect("reader");
    assert_eq!(
        read.stat(&LogicalPath::new("f").unwrap())
            .expect("stat")
            .content_root,
        synthetic("content/two")
    );
}

#[test]
fn a_same_name_no_op_keeps_the_root_identity() {
    let mut session = Session::new(1).expect("empty filesystem");
    let file = session.allocate();
    let updates = [DirectoryUpdate {
        parent: 1,
        changes: vec![(name("f"), Some(file))],
    }];
    let inodes = [InodeUpdate {
        serial: file,
        value: inode(InodeKind::RegularFile, "content/f", "meta/f"),
    }];
    session.apply(&updates, &inodes, &[file]).expect("creates");
    let root = session.root;
    let table = session.value.inode_table();
    if std::env::var("LAYERFS_DEBUG").is_ok() {
        let mut read = session.read().expect("reader");
        eprintln!(
            "after create rows {:?}",
            read.lookup_inodes(&[1, file]).unwrap()
        );
        eprintln!(
            "after create list {:?}",
            read.list(&LogicalPath::root(), None, 8, 4096).unwrap()
        );
    }
    // The same final bindings and the same final values: nothing may change.
    let result = session.apply(&updates, &[], &[]).expect("no-op update");
    if std::env::var("LAYERFS_DEBUG").is_ok() {
        let mut read = session.read().expect("reader");
        eprintln!("root table {:?}", session.value.inode_table());
        eprintln!("rows {:?}", read.lookup_inodes(&[1, file]).unwrap());
        let base_table = session
            .store
            .canonical(session.root)
            .map(|bytes| layerfs_content::filesystem::FilesystemRoot::decode(bytes).unwrap());
        eprintln!("base {base_table:?}");
    }
    assert_eq!(
        result.root.0, root,
        "a rebuild of identical bindings must reuse the same root object"
    );
    assert_eq!(result.value.inode_table(), table);
}

#[test]
fn a_type_replacement_replaces_the_binding_and_the_record() {
    let mut session = Session::new(1).expect("empty filesystem");
    let file = session.allocate();
    session
        .apply(
            &[DirectoryUpdate {
                parent: 1,
                changes: vec![(name("x"), Some(file))],
            }],
            &[InodeUpdate {
                serial: file,
                value: inode(InodeKind::RegularFile, "content/x", "meta/x"),
            }],
            &[file],
        )
        .expect("file");
    let link = session.allocate();
    let result = session
        .apply(
            &[DirectoryUpdate {
                parent: 1,
                changes: vec![(name("x"), Some(link))],
            }],
            &[
                InodeUpdate {
                    serial: file,
                    value: inode(InodeKind::RegularFile, "content/x", "meta/x"),
                },
                InodeUpdate {
                    serial: link,
                    value: inode(InodeKind::Symlink, "content/link", "meta/link"),
                },
            ],
            &[link],
        )
        .expect("replacement");
    let mut read = session.read().expect("reader");
    let resolved = read
        .resolve(&LogicalPath::new("x").unwrap())
        .expect("resolve");
    assert_eq!(resolved.serial, link);
    assert_eq!(resolved.value.kind, InodeKind::Symlink);
    let _ = result;
}

#[test]
fn removing_the_last_binding_removes_the_inode_and_keeps_the_root_readable() {
    let mut session = Session::new(1).expect("empty filesystem");
    let file = session.allocate();
    session
        .apply(
            &[DirectoryUpdate {
                parent: 1,
                changes: vec![(name("gone"), Some(file))],
            }],
            &[InodeUpdate {
                serial: file,
                value: inode(InodeKind::RegularFile, "content/gone", "meta/gone"),
            }],
            &[file],
        )
        .expect("creates");
    let before = session.root;
    let result = session
        .apply(
            &[DirectoryUpdate {
                parent: 1,
                changes: vec![(name("gone"), None)],
            }],
            &[],
            &[],
        )
        .expect("removes");
    assert_ne!(result.root.0, before);
    let mut read = session.read().expect("reader");
    assert!(matches!(
        read.stat(&LogicalPath::new("gone").unwrap()),
        Err(layerfs_content::ContentError::MissingObject)
    ));
    assert_eq!(
        read.stat(&LogicalPath::root()).expect("root").kind,
        InodeKind::Directory
    );
    assert_eq!(
        read.lookup_inodes(&[file]).expect("lookup")[0],
        None,
        "the released inode is gone from the new table"
    );
}

/// Number of root objects this session has published so far.
fn root_objects(session: &Session) -> usize {
    session
        .store
        .order()
        .iter()
        .filter(|(_, role)| *role == layerfs_content::ObjectRole::FilesystemRoot)
        .count()
}

#[test]
fn an_unsorted_or_duplicate_change_list_is_refused_before_any_write() {
    let mut session = Session::new(1).expect("empty filesystem");
    let file = session.allocate();
    let updates = [DirectoryUpdate {
        parent: 1,
        changes: vec![(name("b"), Some(file)), (name("a"), Some(file))],
    }];
    let inodes = [InodeUpdate {
        serial: file,
        value: inode(InodeKind::RegularFile, "content/f", "meta/f"),
    }];
    let before = session.root;
    let roots_before = root_objects(&session);
    let outcome = session.apply(&updates, &inodes, &[file]);
    assert!(matches!(
        outcome,
        Err(layerfs_content::ContentError::NonCanonicalOrdering)
    ));
    assert_eq!(
        session.root, before,
        "a refused input leaves the root alone"
    );
    assert_eq!(
        root_objects(&session),
        roots_before,
        "a refused input publishes no new root object"
    );
    let duplicate = [DirectoryUpdate {
        parent: 1,
        changes: vec![(name("a"), Some(file)), (name("a"), None)],
    }];
    let outcome = session.apply(&duplicate, &inodes, &[file]);
    assert!(matches!(
        outcome,
        Err(layerfs_content::ContentError::NonCanonicalOrdering)
    ));
    let _ = ObjectId::for_bytes(b"unused");
}

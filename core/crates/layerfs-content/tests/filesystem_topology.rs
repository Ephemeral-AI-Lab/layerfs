//! Topology and identity validation: what the operation must refuse.
//!
//! Every check here runs before a result is promised: root invariants, identity
//! reuse, multiple parents, effective-tree cycles, disconnected new records and
//! caller-asserted counts that disagree with the bindings the merge retained.

mod support;

use layerfs_content::filesystem::{
    build_filesystem, DirectoryUpdate, FilesystemInput, FilesystemRoot, InodeUpdate, LogicalPath,
    PathName,
};
use layerfs_content::object::inode_leaf::{InodeKind, InodeValue};
use layerfs_content::{ContentError, ObjectId};
use support::filesystem::{resources, synthetic, value, with_objects, Session, TreeStore};

fn name(value: &str) -> PathName {
    PathName::new(value).expect("name")
}

fn directory(content: &str) -> InodeValue {
    value(
        InodeKind::Directory,
        synthetic(content),
        synthetic("topology/meta"),
    )
}

fn regular(content: &str) -> InodeValue {
    value(
        InodeKind::RegularFile,
        synthetic(content),
        synthetic("topology/meta"),
    )
}

/// Builds `/d/e` as a fresh tree and returns the session with both serials.
fn nested() -> (Session, u64, u64, u64) {
    let mut session = Session::new(1).expect("empty");
    let d = session.allocate();
    let e = session.allocate();
    let f = session.allocate();
    session
        .apply(
            &[
                DirectoryUpdate {
                    parent: 1,
                    changes: vec![(name("d"), Some(d))],
                },
                DirectoryUpdate {
                    parent: d,
                    changes: vec![(name("e"), Some(e)), (name("f"), Some(f))],
                },
                DirectoryUpdate {
                    parent: e,
                    changes: Vec::new(),
                },
            ],
            &[
                InodeUpdate {
                    serial: d,
                    value: directory("unused"),
                },
                InodeUpdate {
                    serial: e,
                    value: directory("unused"),
                },
                InodeUpdate {
                    serial: f,
                    value: regular("content/f"),
                },
            ],
            &[d, e, f],
        )
        .expect("nested tree");
    (session, d, e, f)
}

#[test]
fn the_root_stays_a_zero_count_directory() {
    let mut session = Session::new(1).expect("empty");
    let d = session.allocate();
    // Binding the root under another directory would give it a binding.
    let outcome = session.apply(
        &[DirectoryUpdate {
            parent: d,
            changes: vec![(name("root"), Some(1))],
        }],
        &[
            InodeUpdate {
                serial: 1,
                value: directory("unused"),
            },
            InodeUpdate {
                serial: d,
                value: directory("unused"),
            },
        ],
        &[d],
    );
    assert!(matches!(
        outcome,
        Err(ContentError::InvalidRecord("root directory binding"))
    ));

    // A root inode that is not a directory cannot be built at all.
    let mut store = TreeStore::new();
    let input = FilesystemInput {
        base: None,
        scope: layerfs_content::filesystem::scope_for_seed([0x22; 32]),
        root_serial: 1,
        directories: &[DirectoryUpdate {
            parent: 1,
            changes: Vec::new(),
        }],
        inodes: &[InodeUpdate {
            serial: 1,
            value: regular("content/root"),
        }],
        new_inodes: &[1],
        resources: resources(),
    };
    let outcome = with_objects(&mut store, |objects| {
        build_filesystem(objects, &input, None)
    });
    assert!(matches!(
        outcome,
        Err(ContentError::InvalidRecord("root inode kind"))
    ));
}

#[test]
fn a_reused_identity_is_refused() {
    let (mut session, d, _e, _f) = nested();
    let outcome = session.apply(
        &[DirectoryUpdate {
            parent: d,
            changes: vec![(name("again"), Some(d))],
        }],
        &[],
        &[d],
    );
    if std::env::var("LAYERFS_DEBUG").is_ok() {
        eprintln!("reused identity outcome: {outcome:?}");
    }
    assert!(matches!(
        outcome,
        Err(ContentError::InvalidRecord("reused inode serial"))
    ));
}

#[test]
fn a_directory_or_symlink_cannot_have_two_parents() {
    let (mut session, d, e, _f) = nested();
    let outcome = session.apply(
        &[
            DirectoryUpdate {
                parent: 1,
                changes: vec![(name("second"), Some(e))],
            },
            DirectoryUpdate {
                parent: d,
                changes: vec![(name("e"), Some(e))],
            },
        ],
        &[InodeUpdate {
            serial: e,
            value: directory("unused"),
        }],
        &[],
    );
    assert!(matches!(
        outcome,
        Err(ContentError::InvalidRecord("multiple parents"))
    ));
}

#[test]
fn a_cycle_formed_by_moving_a_directory_below_itself_is_refused() {
    let (mut session, d, e, _f) = nested();
    // `e` moves under `d/e`: a two-part cycle the old tree cannot show.
    let outcome = session.apply(
        &[
            DirectoryUpdate {
                parent: 1,
                changes: vec![(name("d"), Some(d))],
            },
            DirectoryUpdate {
                parent: d,
                changes: vec![(name("e"), Some(e))],
            },
            DirectoryUpdate {
                parent: e,
                changes: vec![(name("d"), Some(d))],
            },
        ],
        &[
            InodeUpdate {
                serial: d,
                value: directory("unused"),
            },
            InodeUpdate {
                serial: e,
                value: directory("unused"),
            },
        ],
        &[],
    );
    assert!(matches!(
        outcome,
        Err(ContentError::InvalidRecord("multiple parents"))
            | Err(ContentError::InvalidRecord("effective tree cycle"))
    ));
}

#[test]
fn a_self_binding_is_refused() {
    let (mut session, d, _e, _f) = nested();
    let outcome = session.apply(
        &[DirectoryUpdate {
            parent: d,
            changes: vec![(name("self"), Some(d))],
        }],
        &[],
        &[],
    );
    assert!(matches!(
        outcome,
        Err(ContentError::InvalidRecord("multiple parents"))
            | Err(ContentError::InvalidRecord("effective tree cycle"))
    ));
}

#[test]
fn a_disconnected_new_record_is_refused() {
    let mut session = Session::new(1).expect("empty");
    let orphan = session.allocate();
    let outcome = session.apply(
        &[],
        &[InodeUpdate {
            serial: orphan,
            value: regular("content/orphan"),
        }],
        &[orphan],
    );
    assert!(matches!(
        outcome,
        Err(ContentError::InvalidRecord("new inode without binding"))
    ));
}

#[test]
fn a_caller_asserted_count_is_never_trusted() {
    let mut session = Session::new(1).expect("empty");
    let file = session.allocate();
    let mut forged = regular("content/f");
    forged.namespace_ref_count = 99;
    session
        .apply(
            &[DirectoryUpdate {
                parent: 1,
                changes: vec![(name("f"), Some(file))],
            }],
            &[InodeUpdate {
                serial: file,
                value: forged,
            }],
            &[file],
        )
        .expect("creates");
    let mut read = session.read().expect("reader");
    assert_eq!(
        read.stat(&LogicalPath::new("f").unwrap())
            .expect("stat")
            .namespace_ref_count,
        1,
        "the derived count comes from the retained binding, not from the caller"
    );
}

#[test]
fn a_foreign_scope_or_profile_is_refused() {
    let (session, _d, _e, _f) = nested();
    let canonical = session.store.canonical(session.root).expect("root bytes");
    let root = FilesystemRoot::decode(canonical).expect("decode");
    let foreign = FilesystemRoot::new(
        ObjectId::for_bytes(b"layerfs/namespace-profile/not-this-one"),
        root.scope(),
        1,
        root.inode_table(),
    );
    assert!(matches!(
        foreign,
        Err(ContentError::UnsupportedProfile { .. })
    ));
    let mut bytes = canonical.to_vec();
    bytes[13 + 12] ^= 0x40;
    assert!(matches!(
        FilesystemRoot::decode(&bytes),
        Err(ContentError::UnsupportedProfile { .. })
    ));
    // A root whose scope is not the one the caller declares is refused.
    let mut session = session;
    let input = FilesystemInput {
        base: Some(layerfs_content::filesystem::FilesystemRootId(session.root)),
        scope: layerfs_content::filesystem::scope_for_seed([0x99; 32]),
        root_serial: 1,
        directories: &[],
        inodes: &[],
        new_inodes: &[],
        resources: resources(),
    };
    let outcome = with_objects(&mut session.store, |objects| {
        layerfs_content::filesystem::update_filesystem(objects, &input, None)
    });
    assert!(matches!(outcome, Err(ContentError::ScopeMismatch { .. })));
}

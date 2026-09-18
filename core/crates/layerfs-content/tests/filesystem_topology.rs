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

#[test]
fn a_base_resident_directory_cannot_gain_a_second_parent() {
    // `d/e` already has one parent. Binding the same directory inode under a
    // second name is exactly the case a same-batch duplicate is already refused
    // for, and it must not be reachable through the base either: the stored
    // record already owns the one binding a directory may have.
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
        &[],
        &[],
    );
    assert!(matches!(
        outcome,
        Err(ContentError::InvalidRecord("multiple parents"))
    ));
}

#[test]
fn a_base_resident_directory_can_be_renamed_by_unbinding_first() {
    // The legal form of the same edit: the old name is dropped in the same
    // final-state batch, so the directory keeps exactly one parent.
    let (mut session, d, e, _f) = nested();
    session
        .apply(
            &[DirectoryUpdate {
                parent: d,
                changes: vec![(name("e"), None), (name("moved"), Some(e))],
            }],
            &[],
            &[],
        )
        .expect("rename within one parent");
    let mut read = session.read().expect("reader");
    assert_eq!(
        read.resolve(&LogicalPath::new("d/moved").unwrap())
            .expect("resolve")
            .serial,
        e
    );
}

#[test]
fn a_base_resident_symlink_cannot_gain_a_second_parent() {
    let mut session = Session::new(1).expect("empty");
    let link = session.allocate();
    let holder = session.allocate();
    let mut link_value = value(
        InodeKind::Symlink,
        synthetic("topology/symlink-target"),
        synthetic("topology/meta"),
    );
    link_value.namespace_ref_count = 0;
    session
        .apply(
            &[
                DirectoryUpdate {
                    parent: 1,
                    changes: vec![(name("link"), Some(link))],
                },
                DirectoryUpdate {
                    parent: holder,
                    changes: Vec::new(),
                },
            ],
            &[
                InodeUpdate {
                    serial: link,
                    value: link_value,
                },
                InodeUpdate {
                    serial: holder,
                    value: directory("unused"),
                },
            ],
            &[link, holder],
        )
        .expect("symlink tree");
    let outcome = session.apply(
        &[
            DirectoryUpdate {
                parent: 1,
                changes: vec![(name("link"), Some(link)), (name("second"), Some(link))],
            },
            DirectoryUpdate {
                parent: holder,
                changes: vec![(name("third"), Some(link))],
            },
        ],
        &[],
        &[],
    );
    assert!(matches!(
        outcome,
        Err(ContentError::InvalidRecord("multiple parents"))
    ));
}

#[test]
fn a_build_with_a_disconnected_directory_cycle_is_refused() {
    // `a` holds `b` and `b` holds `a`: both are declared new, both are bound, so
    // the disconnected-record rule does not catch it and only an effective walk
    // can. The root binds nothing, which is what makes the cycle disconnected.
    let mut session = Session::new(1).expect("empty");
    let a = session.allocate();
    let b = session.allocate();
    let outcome = session.apply(
        &[
            DirectoryUpdate {
                parent: 1,
                changes: Vec::new(),
            },
            DirectoryUpdate {
                parent: a,
                changes: vec![(name("b"), Some(b))],
            },
            DirectoryUpdate {
                parent: b,
                changes: vec![(name("a"), Some(a))],
            },
        ],
        &[
            InodeUpdate {
                serial: a,
                value: directory("unused"),
            },
            InodeUpdate {
                serial: b,
                value: directory("unused"),
            },
        ],
        &[a, b],
    );
    assert!(matches!(
        outcome,
        Err(ContentError::InvalidRecord("effective tree cycle"))
            | Err(ContentError::InvalidRecord("new inode removal"))
            | Err(ContentError::InvalidRecord("new inode without binding"))
    ));
}

#[test]
fn a_build_whose_root_is_not_declared_new_is_refused_up_front() {
    // The root is allocated like any other inode. Without the declaration the
    // operation would reach an absent base with a placeholder identity and fail
    // later with a provider error instead of a precondition failure.
    let mut store = TreeStore::new();
    let input = FilesystemInput {
        base: None,
        scope: layerfs_content::filesystem::scope_for_seed([0x33; 32]),
        root_serial: 1,
        directories: &[DirectoryUpdate {
            parent: 1,
            changes: Vec::new(),
        }],
        inodes: &[InodeUpdate {
            serial: 1,
            value: directory("unused"),
        }],
        new_inodes: &[],
        resources: resources(),
    };
    let outcome = with_objects(&mut store, |objects| {
        build_filesystem(objects, &input, None)
    });
    assert!(
        matches!(
            outcome,
            Err(ContentError::InvalidRecord("root inode allocation"))
                | Err(ContentError::InvalidRecord("root inode value"))
        ),
        "an undeclared root must be refused before any object is read: {outcome:?}"
    );
}

#[test]
fn a_dropped_directory_emits_no_page_of_its_own() {
    // The root binds `dead` and drops it again in the same final-state batch,
    // and binds a surviving regular file beside it. The dropped directory ends
    // with no parent, so nothing can hold it: no page is built for it, and it
    // never enters the reduction at all.
    let mut session = Session::new(1).expect("empty");
    let dead = session.allocate();
    let file = session.allocate();
    let before = session.store.len();
    let result = session
        .apply(
            &[
                DirectoryUpdate {
                    parent: 1,
                    changes: vec![(name("dead"), None), (name("f"), Some(file))],
                },
                DirectoryUpdate {
                    parent: dead,
                    changes: Vec::new(),
                },
            ],
            &[
                InodeUpdate {
                    serial: dead,
                    value: directory("unused"),
                },
                InodeUpdate {
                    serial: file,
                    value: regular("content/f"),
                },
            ],
            &[dead, file],
        )
        .expect("a dropped empty directory is not an error");
    assert_eq!(
        result.counters.directory_updates, 1,
        "only the root's own update is merged"
    );
    assert_eq!(
        result.counters.references.rows_touched, 2,
        "two rows: the root directory and the file beside the dropped directory"
    );
    let emitted = session.store.len() - before;
    // Three objects: the rebuilt root directory, the inode table and the root.
    assert_eq!(
        emitted, 3,
        "a directory the batch drops contributes no page: {emitted} objects emitted"
    );
    let mut read = session.read().expect("reader");
    assert!(read.stat(&LogicalPath::new("f").unwrap()).is_ok());
    assert!(matches!(
        read.stat(&LogicalPath::new("dead").unwrap()),
        Err(ContentError::PathNotFound)
    ));
}

#[test]
fn a_cycle_formed_inside_a_build_is_refused() {
    // A build with no base can still state a cycle: `a` holds `b` and `b` holds
    // `a`, both declared new. Nothing is left unbound, so only an effective walk
    // over the operation's own bindings can see it.
    let mut session = Session::new(1).expect("empty");
    let a = session.allocate();
    let b = session.allocate();
    let outcome = session.apply(
        &[
            DirectoryUpdate {
                parent: 1,
                changes: Vec::new(),
            },
            DirectoryUpdate {
                parent: a,
                changes: vec![(name("b"), Some(b))],
            },
            DirectoryUpdate {
                parent: b,
                changes: vec![(name("a"), Some(a))],
            },
        ],
        &[
            InodeUpdate {
                serial: a,
                value: directory("unused"),
            },
            InodeUpdate {
                serial: b,
                value: directory("unused"),
            },
        ],
        &[a, b],
    );
    assert!(
        matches!(
            outcome,
            Err(ContentError::InvalidRecord("effective tree cycle"))
        ),
        "a cycle stated entirely inside one build must be refused: {outcome:?}"
    );
}

#[test]
fn an_update_cycling_two_declared_new_directories_is_refused() {
    // `a` is bound under an existing directory and then holds `b`, which holds
    // `a` again. Both are declared new, so no stored record exists for either
    // and only the effective walk over this operation's own bindings sees it.
    let (mut session, d, _e, _f) = nested();
    let a = session.allocate();
    let b = session.allocate();
    // `a` is bound under `d` first, then the third update makes `a` hold `b`
    // which holds `a`: the cycle is created entirely by this batch.
    let outcome = session.apply(
        &[
            DirectoryUpdate {
                parent: d,
                changes: vec![(name("a"), Some(a))],
            },
            DirectoryUpdate {
                parent: a,
                changes: vec![(name("b"), Some(b))],
            },
            DirectoryUpdate {
                parent: b,
                changes: vec![(name("a"), Some(a))],
            },
        ],
        &[
            InodeUpdate {
                serial: a,
                value: directory("unused"),
            },
            InodeUpdate {
                serial: b,
                value: directory("unused"),
            },
        ],
        &[a, b],
    );
    assert!(
        matches!(
            outcome,
            Err(ContentError::InvalidRecord("effective tree cycle"))
                | Err(ContentError::InvalidRecord("multiple parents"))
        ),
        "a cycle between two declared-new directories must be refused: {outcome:?}"
    );
}

#[test]
fn a_build_cycle_that_the_root_holds_is_refused() {
    // The root binds `a`, and `a` holds `b` and `b` holds `a`. Every declared
    // directory has a binding, so the disconnected-record rule cannot see it and
    // only reachability from the root can: the walk never reaches `a`'s second
    // edge into `b`, and `b` is bound once but not held by the tree the root
    // reaches.
    let mut session = Session::new(1).expect("empty");
    let a = session.allocate();
    let b = session.allocate();
    let outcome = session.apply(
        &[
            DirectoryUpdate {
                parent: 1,
                changes: vec![(name("a"), Some(a))],
            },
            DirectoryUpdate {
                parent: a,
                changes: vec![(name("b"), Some(b))],
            },
            DirectoryUpdate {
                parent: b,
                changes: vec![(name("a"), Some(a))],
            },
        ],
        &[
            InodeUpdate {
                serial: a,
                value: directory("unused"),
            },
            InodeUpdate {
                serial: b,
                value: directory("unused"),
            },
        ],
        &[a, b],
    );
    // The walk reaches `a` from the root, then `b`, then `a` again: a directory
    // held by two bindings is refused as a second parent, and a declared
    // directory the root never reaches is refused as a cycle.
    assert!(
        matches!(
            outcome,
            Err(ContentError::InvalidRecord("effective tree cycle"))
                | Err(ContentError::InvalidRecord("multiple parents"))
        ),
        "a cycle the root reaches through one edge must be refused: {outcome:?}"
    );
}

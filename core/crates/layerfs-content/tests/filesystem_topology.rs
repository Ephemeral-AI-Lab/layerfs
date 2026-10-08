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
    let before = session.store.len();
    // `d` is bound again under its own child `e`, with both base bindings
    // restated: the only placement is `d` under `e`, `e` lies in `d`'s
    // territory, and the upward walk from `d` returns to `d`.
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
    // The proof refuses before the derived count of two could: one label.
    assert_eq!(
        outcome.err(),
        Some(ContentError::InvalidRecord("effective tree cycle"))
    );
    assert_eq!(session.store.len(), before, "nothing is offered");
}

#[test]
fn a_self_binding_is_refused() {
    let (mut session, d, _e, _f) = nested();
    let before = session.store.len();
    // `d` keeps `/d` and is bound inside itself: its one placement names
    // itself, so the walk from `d` never leaves `d`.
    let outcome = session.apply(
        &[DirectoryUpdate {
            parent: d,
            changes: vec![(name("self"), Some(d))],
        }],
        &[],
        &[],
    );
    assert_eq!(
        outcome.err(),
        Some(ContentError::InvalidRecord("effective tree cycle"))
    );
    assert_eq!(session.store.len(), before, "nothing is offered");
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

/// Builds `/link` and a directory `holder`, which the root binds only when asked.
///
/// Left unbound, `holder` is declared new with a header and nothing holds it,
/// so the base drops it and no later update can name it.
fn symlink_tree(bind_holder: bool) -> (Session, u64, u64) {
    let mut session = Session::new(1).expect("empty");
    let link = session.allocate();
    let holder = session.allocate();
    let link_value = value(
        InodeKind::Symlink,
        synthetic("topology/symlink-target"),
        synthetic("topology/meta"),
    );
    let mut root = Vec::new();
    if bind_holder {
        root.push((name("holder"), Some(holder)));
    }
    root.push((name("link"), Some(link)));
    session
        .apply(
            &[
                DirectoryUpdate {
                    parent: 1,
                    changes: root,
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
    (session, link, holder)
}

/// Restates `/link` and binds the same symlink twice more.
fn bind_the_symlink_again(
    session: &mut Session,
    link: u64,
    holder: u64,
) -> layerfs_content::ContentResult<layerfs_content::filesystem::FilesystemResult> {
    session.apply(
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
    )
}

#[test]
fn a_base_resident_symlink_cannot_gain_a_second_parent() {
    // R4: the base binds `holder`, so the update names two stored directories
    // and the symlink's derived count of three is what refuses it. The fixture
    // used to leave `holder` unbound; that statement is pinned just below.
    let (mut session, link, holder) = symlink_tree(true);
    let outcome = bind_the_symlink_again(&mut session, link, holder);
    assert!(
        matches!(
            outcome,
            Err(ContentError::InvalidRecord("multiple parents"))
        ),
        "a stored symlink bound again must be refused: {outcome:?}"
    );
}

#[test]
fn a_header_for_a_directory_the_base_dropped_is_a_missing_inode() {
    // The fixture `a_base_resident_symlink_cannot_gain_a_second_parent` had
    // before R4: `holder` was never bound, so the base holds no such directory.
    // The symlink's second parent is now decided by its derived count, after
    // the directory merge, so the header naming an absent directory is refused
    // first, where the old same-batch check answered `multiple parents`.
    let (mut session, link, holder) = symlink_tree(false);
    let before = session.store.len();
    let outcome = bind_the_symlink_again(&mut session, link, holder);
    assert!(
        matches!(
            outcome,
            Err(ContentError::InvalidRecord("missing base inode"))
        ),
        "a header for a directory the base does not hold: {outcome:?}"
    );
    assert_eq!(session.store.len(), before, "nothing is offered");
}

#[test]
fn a_build_with_a_disconnected_directory_cycle_is_refused() {
    // `a` holds `b` and `b` holds `a`: both are declared new, both are bound, so
    // the disconnected-record rule does not catch it and only an effective walk
    // can. The root binds nothing, which is what makes the cycle disconnected.
    let mut session = Session::new(1).expect("empty");
    let a = session.allocate();
    let b = session.allocate();
    let before = session.store.len();
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
    // Both directories are bound, so neither new-inode rule applies: the proof
    // is the only check that answers, and it answers before anything is built.
    assert_eq!(
        outcome.err(),
        Some(ContentError::InvalidRecord("effective tree cycle"))
    );
    assert_eq!(session.store.len(), before, "nothing is offered");
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
    let before = session.store.len();
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
    // `a` is bound under `d` and under `b`: two placements of one directory
    // meet in classification, before the proof would walk the cycle.
    assert_eq!(
        outcome.err(),
        Some(ContentError::InvalidRecord("multiple parents"))
    );
    assert_eq!(session.store.len(), before, "nothing is offered");
}

#[test]
fn an_update_with_a_disconnected_fresh_directory_cycle_is_refused() {
    let (mut session, _d, _e, _f) = nested();
    let a = session.allocate();
    let b = session.allocate();
    let c = session.allocate();
    let outcome = session.apply(
        &[
            DirectoryUpdate {
                parent: a,
                changes: vec![(name("b"), Some(b))],
            },
            DirectoryUpdate {
                parent: b,
                changes: vec![(name("c"), Some(c))],
            },
            DirectoryUpdate {
                parent: c,
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
            InodeUpdate {
                serial: c,
                value: directory("unused"),
            },
        ],
        &[a, b, c],
    );
    assert!(
        matches!(
            outcome,
            Err(ContentError::InvalidRecord("effective tree cycle"))
        ),
        "a disconnected fresh cycle must be refused: {outcome:?}"
    );
}

#[test]
fn a_cycle_through_a_fresh_directory_inside_the_moved_directory_is_refused() {
    // `d` holds `e`. One batch allocates `p` inside `e`, moves `d` under `p` and
    // drops `d` from the root: d -> p -> e -> d, with nothing left under the
    // root. The stored directory is bound under a fresh one and the fresh one
    // under a stored one, so no single binding joins two stored directories,
    // and every derived count is one: only the effective tree shows the cycle.
    let (mut session, d, e, _f) = nested();
    let p = session.allocate();
    let before = session.store.len();
    let outcome = session.apply(
        &[
            DirectoryUpdate {
                parent: 1,
                changes: vec![(name("d"), None)],
            },
            DirectoryUpdate {
                parent: e,
                changes: vec![(name("p"), Some(p))],
            },
            DirectoryUpdate {
                parent: p,
                changes: vec![(name("d"), Some(d))],
            },
        ],
        &[InodeUpdate {
            serial: p,
            value: directory("unused"),
        }],
        &[p],
    );
    assert!(
        matches!(
            outcome,
            Err(ContentError::InvalidRecord("effective tree cycle"))
        ),
        "a moved directory below a fresh directory it already holds must be refused: {outcome:?}"
    );
    assert_eq!(
        session.store.len(),
        before,
        "the refusal arrives before any object is offered"
    );
}

#[test]
fn a_permutation_of_65_directory_bindings_is_one_linear_walk() {
    let mut session = Session::new(1).expect("empty");
    let serials: Vec<u64> = (0..65).map(|_| session.allocate()).collect();
    let mut directories = vec![DirectoryUpdate {
        parent: 1,
        changes: serials
            .iter()
            .enumerate()
            .map(|(index, serial)| (name(&format!("d{index:03}")), Some(*serial)))
            .collect(),
    }];
    directories.extend(serials.iter().map(|serial| DirectoryUpdate {
        parent: *serial,
        changes: Vec::new(),
    }));
    let values: Vec<InodeUpdate> = serials
        .iter()
        .map(|serial| InodeUpdate {
            serial: *serial,
            value: directory("unused"),
        })
        .collect();
    session
        .apply(&directories, &values, &serials)
        .expect("base");
    let rotated = DirectoryUpdate {
        parent: 1,
        changes: (0..65)
            .map(|index| {
                (
                    name(&format!("d{index:03}")),
                    Some(serials[(index + 1) % 65]),
                )
            })
            .collect(),
    };
    let result = session.apply(&[rotated], &[], &[]).expect("permutation");
    let work = result.counters.validation;
    // 65 stored directories each take a name another one had: 65 placements
    // under the root, one header and 65 names classified.
    assert_eq!((work.placements, work.peak_window_rows), (65, 66));
    // Every placement names the root, so each walk is one step, taken once to
    // prove it and once to mark it: twice the placements, not their square.
    assert_eq!(work.ancestry_steps, 130);
    // No placement lands under a stored directory other than the root, so no
    // parent is scanned and no base directory is listed.
    assert_eq!(
        (
            work.in_place_scans,
            work.in_place_rows,
            work.in_place_directories
        ),
        (0, 0, 0)
    );
    assert_eq!(
        (
            work.territory_directories,
            work.territory_entries,
            work.entries_examined
        ),
        (0, 0, 0)
    );
}

#[test]
fn a_build_cycle_that_the_root_holds_is_refused() {
    // The root binds `a`, and `a` holds `b` and `b` holds `a`. Every declared
    // directory has a binding, so the disconnected-record rule cannot see it;
    // `a` has two of them.
    let mut session = Session::new(1).expect("empty");
    let a = session.allocate();
    let b = session.allocate();
    let before = session.store.len();
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
    // `a` is bound under the root and under `b`: two placements of one
    // directory meet in classification, before the proof would walk the cycle.
    assert_eq!(
        outcome.err(),
        Some(ContentError::InvalidRecord("multiple parents"))
    );
    assert_eq!(session.store.len(), before, "nothing is offered");
}

#[test]
fn a_fresh_directory_bound_only_inside_a_dropped_directory_is_refused_by_an_update() {
    // `dead` is declared new, carries a header and nothing binds it, so it is
    // dropped. `inside` is bound only there: its upward walk ends at a directory
    // that reaches neither the root nor a base position.
    let (mut session, _d, _e, _f) = nested();
    let dead = session.allocate();
    let inside = session.allocate();
    let before = session.store.len();
    let outcome = session.apply(
        &[
            DirectoryUpdate {
                parent: dead,
                changes: vec![(name("inside"), Some(inside))],
            },
            DirectoryUpdate {
                parent: inside,
                changes: Vec::new(),
            },
        ],
        &[
            InodeUpdate {
                serial: dead,
                value: directory("unused"),
            },
            InodeUpdate {
                serial: inside,
                value: directory("unused"),
            },
        ],
        &[dead, inside],
    );
    assert!(
        matches!(
            outcome,
            Err(ContentError::InvalidRecord("effective tree cycle"))
        ),
        "a fresh directory held only by a dropped one must be refused: {outcome:?}"
    );
    assert_eq!(
        session.store.len(),
        before,
        "the refusal arrives before any object is offered"
    );
}

#[test]
fn a_fresh_directory_bound_only_inside_a_dropped_directory_is_refused_by_a_build() {
    // The same statement with no base: the root binds nothing, `2` is dropped
    // and `3` is bound only inside it. Both routes refuse with the same label.
    let mut store = TreeStore::new();
    let input = FilesystemInput {
        base: None,
        scope: layerfs_content::filesystem::scope_for_seed([0x44; 32]),
        root_serial: 1,
        directories: &[
            DirectoryUpdate {
                parent: 1,
                changes: Vec::new(),
            },
            DirectoryUpdate {
                parent: 2,
                changes: vec![(name("inside"), Some(3))],
            },
            DirectoryUpdate {
                parent: 3,
                changes: Vec::new(),
            },
        ],
        inodes: &[
            InodeUpdate {
                serial: 1,
                value: directory("unused"),
            },
            InodeUpdate {
                serial: 2,
                value: directory("unused"),
            },
            InodeUpdate {
                serial: 3,
                value: directory("unused"),
            },
        ],
        new_inodes: &[1, 2, 3],
        resources: resources(),
    };
    let outcome = with_objects(&mut store, |objects| {
        build_filesystem(objects, &input, None)
    });
    assert!(
        matches!(
            outcome,
            Err(ContentError::InvalidRecord("effective tree cycle"))
        ),
        "a fresh directory held only by a dropped one must be refused: {outcome:?}"
    );
    assert!(store.is_empty(), "nothing is offered before the refusal");
}

#[test]
fn a_stored_directory_bound_again_inside_a_dropped_directory_is_refused() {
    // `e` keeps `/d/e` and is bound once more inside a directory this batch
    // allocates and never binds. The placement's upward walk ends at that
    // dropped directory, which reaches neither the root nor a base position, so
    // the proof refuses it before anything is offered. The validator this
    // replaced emitted the directory with a count of two.
    let (mut session, _d, e, _f) = nested();
    let dead = session.allocate();
    let before = session.store.len();
    let outcome = session.apply(
        &[DirectoryUpdate {
            parent: dead,
            changes: vec![(name("again"), Some(e))],
        }],
        &[InodeUpdate {
            serial: dead,
            value: directory("unused"),
        }],
        &[dead],
    );
    assert!(
        matches!(
            outcome,
            Err(ContentError::InvalidRecord("effective tree cycle"))
        ),
        "a binding inside a dropped directory reaches nothing: {outcome:?}"
    );
    assert_eq!(session.store.len(), before, "nothing was offered");
}

#[test]
fn a_stored_directory_moved_only_into_a_dropped_directory_is_refused() {
    // `e` loses `/d/e` and is bound only inside a dropped directory. Its
    // derived count would be one, so only the proof can refuse it: accepted,
    // it would stay in the inode table with no reachable name.
    let (mut session, d, e, _f) = nested();
    let dead = session.allocate();
    let before = session.store.len();
    let outcome = session.apply(
        &[
            DirectoryUpdate {
                parent: d,
                changes: vec![(name("e"), None)],
            },
            DirectoryUpdate {
                parent: dead,
                changes: vec![(name("e"), Some(e))],
            },
        ],
        &[InodeUpdate {
            serial: dead,
            value: directory("unused"),
        }],
        &[dead],
    );
    assert!(
        matches!(
            outcome,
            Err(ContentError::InvalidRecord("effective tree cycle"))
        ),
        "a stored directory moved into a dropped one reaches nothing: {outcome:?}"
    );
    assert_eq!(session.store.len(), before, "nothing was offered");
}

#[test]
fn a_stored_symlink_bound_again_inside_a_dropped_directory_has_two_parents() {
    // A symlink is not topology evidence. A binding stated in a dropped
    // directory is still counted, so the symlink's derived count is two and the
    // reducer refuses it; the dropped directory builds no page.
    let (mut session, link, _holder) = symlink_tree(true);
    let dead = session.allocate();
    let before = session.store.len();
    let outcome = session.apply(
        &[DirectoryUpdate {
            parent: dead,
            changes: vec![(name("again"), Some(link))],
        }],
        &[InodeUpdate {
            serial: dead,
            value: directory("unused"),
        }],
        &[dead],
    );
    assert!(
        matches!(
            outcome,
            Err(ContentError::InvalidRecord("multiple parents"))
        ),
        "a second binding inside a dropped directory is a second parent: {outcome:?}"
    );
    assert_eq!(session.store.len(), before, "nothing was offered");
}

//! Independent final-parent and canonical-root oracles for shared cycle proofs.

mod support;

use std::collections::BTreeMap;

use layerfs_content::filesystem::validate::ValidationWork;
use layerfs_content::filesystem::{
    build_filesystem, check, DirectoryUpdate, FilesystemInput, FilesystemRead, FilesystemRootId,
    InodeUpdate, PathName,
};
use layerfs_content::object::inode_leaf::InodeKind;
use layerfs_content::ContentError;
use support::filesystem::{resources, synthetic, value, with_objects, Session, TreeStore};

fn name(serial: u64) -> PathName {
    PathName::new(&format!("d{serial}")).unwrap()
}

fn directory(serial: u64) -> InodeUpdate {
    InodeUpdate {
        serial,
        value: value(InodeKind::Directory, synthetic("unused"), synthetic("meta")),
    }
}

fn final_rows(parents: &[u64; 3], remove_base: bool) -> Vec<DirectoryUpdate> {
    let mut rows = BTreeMap::<u64, BTreeMap<PathName, Option<u64>>>::new();
    for parent in 1..=4 {
        rows.entry(parent).or_default();
    }
    if remove_base {
        rows.get_mut(&1).unwrap().insert(name(2), None);
        rows.get_mut(&1).unwrap().insert(name(3), None);
    }
    for (serial, parent) in (2..=4).zip(parents) {
        rows.get_mut(parent)
            .unwrap()
            .insert(name(serial), Some(serial));
    }
    rows.into_iter()
        .map(|(parent, changes)| DirectoryUpdate {
            parent,
            changes: changes.into_iter().collect(),
        })
        .collect()
}

/// This oracle follows independently supplied parent identities, never product
/// output, changed-row lookup or a copy of the descendant DFS.
fn rooted(parents: &[u64; 3]) -> bool {
    (2..=4).all(|serial| {
        let mut at = serial;
        for _ in 0..3 {
            at = parents[(at - 2) as usize];
            if at == 1 {
                return true;
            }
        }
        false
    })
}

#[test]
fn every_three_directory_final_parent_graph_matches_the_independent_oracle() {
    for a in 1..=4 {
        for b in 1..=4 {
            for c in 1..=4 {
                let parents = [a, b, c];
                let mut session = Session::new(1).unwrap();
                session
                    .apply(
                        &[
                            DirectoryUpdate {
                                parent: 1,
                                changes: vec![(name(2), Some(2)), (name(3), Some(3))],
                            },
                            DirectoryUpdate {
                                parent: 2,
                                changes: vec![],
                            },
                            DirectoryUpdate {
                                parent: 3,
                                changes: vec![],
                            },
                        ],
                        &[directory(2), directory(3)],
                        &[2, 3],
                    )
                    .unwrap();
                let old_root = session.root;
                let emitted_before = session.store.order().len();
                let result = session.apply(&final_rows(&parents, true), &[directory(4)], &[4]);
                assert_eq!(
                    result.is_ok(),
                    rooted(&parents),
                    "parents={parents:?}: {result:?}"
                );
                if !rooted(&parents) {
                    assert_eq!(session.root, old_root);
                    assert_eq!(
                        session.store.order().len(),
                        emitted_before,
                        "refusal publishes no objects"
                    );
                    continue;
                }
                // A separate base-less build of the expected final graph must
                // yield the same canonical root, including all derived counts.
                let mut expected_store = TreeStore::new();
                let root_value = FilesystemRead::new(&session.store, FilesystemRootId(old_root))
                    .unwrap()
                    .resolve_inode(1)
                    .unwrap()
                    .value;
                let final_inodes = [
                    InodeUpdate {
                        serial: 1,
                        value: root_value,
                    },
                    directory(2),
                    directory(3),
                    directory(4),
                ];
                let expected_rows = final_rows(&parents, false);
                let expected = FilesystemInput {
                    base: None,
                    scope: session.scope,
                    root_serial: 1,
                    directories: &expected_rows,
                    inodes: &final_inodes,
                    new_inodes: &[1, 2, 3, 4],
                    resources: resources(),
                };
                let expected_root = with_objects(&mut expected_store, |objects| {
                    build_filesystem(objects, &expected, None)
                })
                .unwrap()
                .root;
                assert_eq!(
                    FilesystemRootId(session.root),
                    expected_root,
                    "parents={parents:?}"
                );
                let mut old =
                    FilesystemRead::new(&session.store, FilesystemRootId(old_root)).unwrap();
                assert_eq!(old.resolve_child(1, &name(2)).unwrap().serial, 2);
                assert_eq!(old.resolve_child(1, &name(3)).unwrap().serial, 3);
                let mut new = session.read().unwrap();
                for (serial, parent) in (2..=4).zip(parents) {
                    assert_eq!(
                        new.resolve_child(parent, &name(serial)).unwrap().serial,
                        serial
                    );
                }
            }
        }
    }
}

#[test]
fn completed_subtree_reuse_still_refuses_a_second_final_owner() {
    let mut session = Session::new(1).unwrap();
    let rows = [
        DirectoryUpdate {
            parent: 1,
            changes: vec![(name(2), Some(2)), (name(3), Some(3))],
        },
        DirectoryUpdate {
            parent: 2,
            changes: vec![(name(4), Some(4))],
        },
        DirectoryUpdate {
            parent: 3,
            changes: vec![(name(4), Some(4))],
        },
    ];
    let root = session.root;
    let emitted = session.store.order().len();
    assert!(matches!(
        session.apply(
            &rows,
            &[directory(2), directory(3), directory(4)],
            &[2, 3, 4]
        ),
        Err(ContentError::InvalidRecord("multiple parents"))
    ));
    assert_eq!(session.root, root);
    assert_eq!(session.store.order().len(), emitted);
}

#[test]
fn shared_proof_state_and_effective_edges_keep_the_declared_work_refusal() {
    let session = Session::new(1).unwrap();
    let directories = [
        DirectoryUpdate {
            parent: 1,
            changes: vec![(name(2), Some(2)), (name(3), Some(3))],
        },
        DirectoryUpdate {
            parent: 2,
            changes: vec![],
        },
        DirectoryUpdate {
            parent: 3,
            changes: vec![],
        },
    ];
    let inodes = [directory(2), directory(3)];
    let input = FilesystemInput {
        base: Some(FilesystemRootId(session.root)),
        scope: session.scope,
        root_serial: 1,
        directories: &directories,
        inodes: &inodes,
        new_inodes: &[2, 3],
        resources: layerfs_content::filesystem::FilesystemResources {
            ordering_bytes: 1024,
            ..resources()
        },
    };
    assert!(matches!(
        check(
            &session.store,
            &input,
            &BTreeMap::new(),
            &mut ValidationWork::default()
        ),
        Err(ContentError::InvalidRecord("cycle check work limit"))
    ));
}

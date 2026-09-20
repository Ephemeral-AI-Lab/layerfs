//! Bounded parent batches share acquisitions and preserve canonical results.

mod support;

use layerfs_content::filesystem::{
    update_filesystem, DirectoryUpdate, FilesystemInput, FilesystemObjects, FilesystemRead,
    FilesystemResources, FilesystemResult, FilesystemRootId, InodeUpdate, LogicalPath,
};
use layerfs_content::object::inode_leaf::InodeKind;
use layerfs_content::{ContentError, ObjectRole};
use support::filesystem::{
    name_of, synthetic, value, CountingProvider, RecordingBacking, Session, TempDir, TreeStore,
};

fn parents() -> Session {
    let mut session = Session::new(1).expect("empty filesystem");
    let serials = (0..5).map(|_| session.allocate()).collect::<Vec<_>>();
    let mut directories = vec![DirectoryUpdate {
        parent: 1,
        changes: serials
            .iter()
            .map(|serial| (name_of(&format!("d{serial}")), Some(*serial)))
            .collect(),
    }];
    directories.extend(serials.iter().map(|serial| DirectoryUpdate {
        parent: *serial,
        changes: Vec::new(),
    }));
    let inodes = serials
        .iter()
        .map(|serial| InodeUpdate {
            serial: *serial,
            value: value(
                InodeKind::Directory,
                synthetic("unused"),
                synthetic(&format!("metadata/{serial}")),
            ),
        })
        .collect::<Vec<_>>();
    session
        .apply(&directories, &inodes, &serials)
        .expect("parents");
    assert_eq!(
        session.store.role(session.value.inode_table()),
        Some(ObjectRole::InodeLeaf)
    );
    session
}

fn apply(
    session: &Session,
    directories: &[DirectoryUpdate],
    inodes: &[InodeUpdate],
    new_inodes: &[u64],
    batch: usize,
) -> (FilesystemResult, TreeStore, usize) {
    let input = FilesystemInput {
        base: Some(FilesystemRootId(session.root)),
        scope: session.scope,
        root_serial: session.root_serial,
        directories,
        inodes,
        new_inodes,
        resources: FilesystemResources {
            base_read_batch: batch,
            ..FilesystemResources::default()
        },
    };
    let provider = CountingProvider::new(&session.store);
    let mut sink = TreeStore::new();
    let result = update_filesystem(
        &mut FilesystemObjects::new(&provider, &mut sink),
        &input,
        None,
    )
    .expect("parent update");
    (result, sink, provider.demands())
}

#[test]
fn unchanged_parents_share_reads_and_reuse_their_authenticated_values() {
    let session = parents();
    let directories = (2..=6)
        .map(|parent| DirectoryUpdate {
            parent,
            changes: Vec::new(),
        })
        .collect::<Vec<_>>();
    let values = session
        .read()
        .expect("base reader")
        .lookup_inodes(&[2, 3, 4, 5, 6])
        .expect("parent values");
    let explicit = (2..=6)
        .zip(values)
        .map(|(serial, value)| InodeUpdate {
            serial,
            value: value.expect("parent record"),
        })
        .collect::<Vec<_>>();
    let mut demands = Vec::new();
    let mut explicit_demands = Vec::new();
    for batch in [1, 3, 64] {
        let (result, _, reads) = apply(&session, &directories, &[], &[], batch);
        assert_eq!(
            result.root.0, session.root,
            "batch {batch}: canonical no-op"
        );
        demands.push(reads);
        let (explicit_result, _, explicit_reads) =
            apply(&session, &directories, &explicit, &[], batch);
        assert_eq!(
            explicit_result.root, result.root,
            "caller/base metadata equivalence"
        );
        explicit_demands.push(explicit_reads);
    }
    eprintln!("parent lookup demands for batches 1/3/64: {demands:?}");
    eprintln!("explicit parent value demands for batches 1/3/64: {explicit_demands:?}");
    // Only one bounded batch is retained across the effects/value boundary.
    // When every parent fits, metadata reuses that entire authenticated batch;
    // smaller windows may reread earlier batches to preserve reducer order.
    assert_eq!(demands[2], explicit_demands[2], "one-window metadata reuse");
    assert!(demands[0] > demands[1] && demands[1] > demands[2]);
    // Six object acquisitions suffice for this one-leaf fixture: root load,
    // validation, parent values, zero-count check, reduction and inode merge.
    // The pre-optimization path demands 15 because it looks each parent up twice.
    assert!(demands[2] <= 6, "repeated parent reads: {demands:?}");
}

#[test]
fn partial_parent_batches_preserve_values_new_directories_and_removals() {
    let session = parents();
    let directories = vec![
        DirectoryUpdate {
            parent: 1,
            changes: vec![(name_of("d6"), None)],
        },
        DirectoryUpdate {
            parent: 2,
            changes: vec![(name_of("nested"), Some(7))],
        },
        DirectoryUpdate {
            parent: 3,
            changes: Vec::new(),
        },
        DirectoryUpdate {
            parent: 4,
            changes: Vec::new(),
        },
        DirectoryUpdate {
            parent: 5,
            changes: Vec::new(),
        },
        DirectoryUpdate {
            parent: 7,
            changes: Vec::new(),
        },
        // Allocated but unbound: this directory must never enter the final tree.
        DirectoryUpdate {
            parent: 8,
            changes: Vec::new(),
        },
    ];
    let inodes = [3, 7, 8].map(|serial| InodeUpdate {
        serial,
        value: value(
            InodeKind::Directory,
            synthetic("unused"),
            synthetic("supplied-meta"),
        ),
    });
    let mut expected = None;
    for batch in [1, 3, 64] {
        let (result, sink, _) = apply(&session, &directories, &inodes, &[7, 8], batch);
        if let Some(root) = expected {
            assert_eq!(result.root, root, "batch {batch}: canonical identity");
        } else {
            expected = Some(result.root);
        }
        let mut store = session.store.clone();
        store.absorb(&sink);
        let mut read = FilesystemRead::new(&store, result.root).expect("result reader");
        for path in ["d2", "d4", "d5"] {
            let stat = read
                .stat(&LogicalPath::new(path).unwrap())
                .expect("retained parent");
            assert_eq!(
                stat.metadata_root,
                synthetic(&format!("metadata/{}", &path[1..]))
            );
        }
        for path in ["d3", "d2/nested"] {
            let stat = read
                .stat(&LogicalPath::new(path).unwrap())
                .expect("supplied parent");
            assert_eq!(stat.metadata_root, synthetic("supplied-meta"));
            assert_eq!(stat.namespace_ref_count, 1);
        }
        assert!(matches!(
            read.stat(&LogicalPath::new("d6").unwrap()),
            Err(ContentError::PathNotFound)
        ));
        assert_eq!(
            read.lookup_inodes(&[6, 8]).expect("absent records"),
            vec![None, None]
        );
    }
}

#[test]
fn parent_values_remain_correct_when_the_reference_reducer_spills() {
    let session = parents();
    let directories = (2..=6)
        .map(|parent| DirectoryUpdate {
            parent,
            changes: Vec::new(),
        })
        .collect::<Vec<_>>();
    let temp = TempDir::new("parent-lookup-spill");
    let mut backing = RecordingBacking::new(temp.path());
    let input = FilesystemInput {
        base: Some(FilesystemRootId(session.root)),
        scope: session.scope,
        root_serial: session.root_serial,
        directories: &directories,
        inodes: &[],
        new_inodes: &[],
        resources: FilesystemResources {
            maximum_pending_records: 2,
            base_read_batch: 3,
            ..FilesystemResources::default()
        },
    };
    let mut sink = TreeStore::new();
    let result = update_filesystem(
        &mut FilesystemObjects::new(&session.store, &mut sink),
        &input,
        Some(&mut backing),
    )
    .expect("spilled parent update");
    assert_eq!(result.root.0, session.root);
    assert!(
        backing.counters().creates > 0,
        "the reducer actually spilled"
    );
    assert!(!backing.owns_storage(), "completion released the spill");
}

#[test]
fn parent_value_ordering_quota_matrix_is_explicit_and_canonical() {
    let mut session = parents();
    let file = session.allocate();
    session
        .apply(
            &[DirectoryUpdate {
                parent: 1,
                changes: vec![(name_of("file"), Some(file))],
            }],
            &[InodeUpdate {
                serial: file,
                value: value(
                    InodeKind::RegularFile,
                    synthetic("shared-file"),
                    synthetic("file-metadata"),
                ),
            }],
            &[file],
        )
        .expect("shared file");
    // Binding effects on the same child interleave with five distinct parent
    // values if those values enter the reducer during directory processing.
    let directories = (2..=6)
        .map(|parent| DirectoryUpdate {
            parent,
            changes: vec![(name_of("link"), Some(file))],
        })
        .collect::<Vec<_>>();
    let expected = apply(&session, &directories, &[], &[], 64).0.root;
    let mut successes = 0;
    let mut refusals = 0;
    for pending in [1, 2] {
        for rows in [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 12, 16, 24, 32] {
            for batch in [1, 64] {
                let ordering_bytes = rows * 96;
                let temp = TempDir::new("parent-lookup-quota");
                let mut backing = RecordingBacking::new(temp.path());
                let input = FilesystemInput {
                    base: Some(FilesystemRootId(session.root)),
                    scope: session.scope,
                    root_serial: session.root_serial,
                    directories: &directories,
                    inodes: &[],
                    new_inodes: &[],
                    resources: FilesystemResources {
                        maximum_pending_records: pending,
                        base_read_batch: batch,
                        ordering_bytes,
                        ..FilesystemResources::default()
                    },
                };
                let mut sink = TreeStore::new();
                let outcome = update_filesystem(
                    &mut FilesystemObjects::new(&session.store, &mut sink),
                    &input,
                    Some(&mut backing),
                );
                match outcome {
                    Ok(result) => {
                        successes += 1;
                        assert_eq!(result.root, expected);
                        eprintln!("quota pending={pending} bytes={ordering_bytes} batch={batch} outcome=OK root={:?}", result.root);
                    }
                    Err(error) => {
                        refusals += 1;
                        eprintln!("quota pending={pending} bytes={ordering_bytes} batch={batch} outcome=ERR {error:?}");
                        // These eight cells succeeded in the retained baseline.
                        // Earlier parent-value insertion regressed their quotas.
                        assert!(
                            !matches!((pending, ordering_bytes), (1, 864 | 960) | (2, 960 | 1152)),
                            "baseline-supported ordering budget refused: {error:?}"
                        );
                        assert!(
                            matches!(
                                error,
                                ContentError::ObjectLimitExceeded { .. }
                                    | ContentError::ResourceUnavailable { .. }
                                    | ContentError::BoundedCapacityExceeded { .. }
                            ),
                            "unexpected refusal: {error:?}"
                        );
                        assert!(!sink
                            .order()
                            .iter()
                            .any(|(_, role)| *role == ObjectRole::FilesystemRoot));
                    }
                }
                assert!(
                    !backing.owns_storage(),
                    "quota completion must release resources"
                );
            }
        }
    }
    assert!(
        successes > 0 && refusals > 0,
        "matrix must straddle the resource boundary"
    );
}

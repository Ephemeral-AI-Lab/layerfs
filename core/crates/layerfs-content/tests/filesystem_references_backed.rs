//! Public backed reference/release operations. Providers here are explicit
//! external memory fixtures; this does not qualify the real Save owner wiring.
mod support;
use layerfs_content::filesystem::references::record::Row;
use layerfs_content::filesystem::{
    build_filesystem, scope_for_seed, update_filesystem, update_filesystem_streamed_backed,
    DirectoryUpdate, FilesystemInput, FilesystemObjects, FilesystemRead, FilesystemResources,
    FilesystemResult, InodeUpdate,
};
use layerfs_content::object::inode_leaf::{InodeKind, InodeValue};
use layerfs_content::{
    AuthenticatedObjects, ContentError, ContentResult, FinalizedConsumer, FinalizedObject,
    ObjectId, ObjectRole,
};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};
use support::filesystem::{name_of, synthetic, value, TreeStore};
use support::reference_records::{key, Records, FRAME, FRESH, NODE, ROW, TOUCH, WORK};
use support::reference_rows::{prepared, Rows};

fn typed(kind: InodeKind) -> InodeValue {
    value(
        kind,
        synthetic("references/content"),
        synthetic("references/metadata"),
    )
}
fn directory(parent: u64, entries: &[(&str, u64)]) -> DirectoryUpdate {
    let mut changes = entries
        .iter()
        .map(|(name, serial)| (name_of(name), Some(*serial)))
        .collect::<Vec<_>>();
    changes.sort_by(|left, right| left.0.cmp(&right.0));
    DirectoryUpdate { parent, changes }
}
fn base(
    directories: &[DirectoryUpdate],
    kinds: &[(u64, InodeKind)],
) -> (TreeStore, FilesystemResult) {
    let mut store = TreeStore::new();
    let new = kinds.iter().map(|(serial, _)| *serial).collect::<Vec<_>>();
    let inodes = kinds
        .iter()
        .map(|(serial, kind)| InodeUpdate {
            serial: *serial,
            value: typed(*kind),
        })
        .collect::<Vec<_>>();
    let input = FilesystemInput {
        base: None,
        scope: scope_for_seed([73; 32]),
        root_serial: 1,
        resources: FilesystemResources::default(),
        directories,
        inodes: &inodes,
        new_inodes: &new,
    };
    let result = build_filesystem(
        &mut FilesystemObjects::new(&TreeStore::new(), &mut store),
        &input,
        None,
    )
    .unwrap();
    (store, result)
}
fn update_input<'a>(
    base: &FilesystemResult,
    directories: &'a [DirectoryUpdate],
    inodes: &'a [InodeUpdate],
    new: &'a [u64],
    resources: FilesystemResources,
) -> FilesystemInput<'a> {
    FilesystemInput {
        base: Some(base.root),
        scope: base.value.scope(),
        root_serial: 1,
        resources,
        directories,
        inodes,
        new_inodes: new,
    }
}
struct Consumer(Rc<RefCell<TreeStore>>, Rc<Cell<bool>>, Rc<Cell<usize>>);
impl FinalizedConsumer for Consumer {
    fn accept(&mut self, object: FinalizedObject) -> ContentResult<()> {
        if self.1.get() {
            self.2.set(self.2.get() + 1);
        }
        self.0.borrow_mut().accept(object)
    }
}
struct Accepted<'a> {
    base: &'a TreeStore,
    emitted: Rc<RefCell<TreeStore>>,
    failure: Option<&'static str>,
    reads: Cell<usize>,
    fault: Rc<Cell<bool>>,
}
impl AuthenticatedObjects for Accepted<'_> {
    fn read_canonical_batch(&self, ids: &[ObjectId]) -> ContentResult<Vec<Vec<u8>>> {
        self.reads.set(self.reads.get() + 1);
        if let Some(what) = self.failure {
            self.fault.set(true);
            return Err(ContentError::ProviderFailure { what });
        }
        ids.iter()
            .map(|id| {
                // Exact presence selects the original accepted object. Neither an
                // unavailable object nor a failed authentication triggers fallback.
                if self.emitted.borrow().canonical(*id).is_some() {
                    self.emitted.borrow().read_canonical(*id)
                } else {
                    self.base.read_canonical(*id)
                }
            })
            .collect()
    }
}
struct InitialReader<'a> {
    store: &'a TreeStore,
    fault: Rc<Cell<bool>>,
    after_failure: Rc<Cell<usize>>,
}
impl AuthenticatedObjects for InitialReader<'_> {
    fn read_canonical_batch(&self, ids: &[ObjectId]) -> ContentResult<Vec<Vec<u8>>> {
        if self.fault.get() {
            self.after_failure.set(self.after_failure.get() + 1);
        }
        self.store.read_canonical_batch(ids)
    }
}
fn backed(
    store: &TreeStore,
    input: &FilesystemInput<'_>,
    records: &mut Records,
    accepted: bool,
    failure: Option<&'static str>,
) -> (ContentResult<FilesystemResult>, TreeStore, usize) {
    let emitted = Rc::new(RefCell::new(TreeStore::new()));
    let post_accepts = Rc::new(Cell::new(0));
    let post_reads = Rc::new(Cell::new(0));
    let mut consumer = Consumer(emitted.clone(), records.fault.clone(), post_accepts.clone());
    let initial = InitialReader {
        store,
        fault: records.fault.clone(),
        after_failure: post_reads.clone(),
    };
    let source = Accepted {
        base: store,
        emitted: emitted.clone(),
        failure,
        reads: Cell::new(0),
        fault: records.fault.clone(),
    };
    let mut objects = if accepted {
        FilesystemObjects::new_with_accepted(&initial, &mut consumer, &source)
    } else {
        FilesystemObjects::new(&initial, &mut consumer)
    };
    let rows = Rows(input);
    let result = update_filesystem_streamed_backed(&mut objects, &prepared(&rows), records, None);
    assert_eq!(
        post_accepts.get(),
        0,
        "no consumer emission after first deciding provider failure"
    );
    assert_eq!(
        post_reads.get(),
        0,
        "no initial demand after first deciding provider failure"
    );
    let retained = emitted.borrow().clone();
    (result, retained, source.reads.get())
}
fn no_final(store: &TreeStore) {
    assert!(!store
        .order()
        .iter()
        .any(|(_, role)| *role == ObjectRole::FilesystemRoot));
}
fn assert_retained_children(store: &TreeStore) {
    assert!(store
        .order()
        .iter()
        .any(|(_, role)| *role == ObjectRole::DirectoryLeaf));
    no_final(store);
}
fn folded(store: &TreeStore, emitted: &TreeStore) -> TreeStore {
    let mut all = store.clone();
    all.absorb(emitted);
    all
}
fn row(records: &Records, serial: u64) -> Row {
    Row::decode(
        records.values[&key(ROW, serial)]
            .as_slice()
            .try_into()
            .unwrap(),
    )
    .unwrap()
}

#[test]
fn wide_multiwindow_direct_rows_remove_the_reducer_quota_and_match_canonical_roots() {
    let count = 257u64;
    let directories = [DirectoryUpdate {
        parent: 1,
        changes: (2..count + 2)
            .map(|serial| (name_of(&format!("f{serial:05}")), Some(serial)))
            .collect(),
    }];
    let mut kinds = vec![(1, InodeKind::Directory)];
    kinds.extend((2..count + 2).map(|serial| (serial, InodeKind::RegularFile)));
    let (store, original) = base(&directories, &kinds);
    let edits = [DirectoryUpdate {
        parent: 1,
        changes: directories[0]
            .changes
            .iter()
            .map(|(name, _)| (name.clone(), None))
            .collect(),
    }];
    let input = update_input(
        &original,
        &edits,
        &[],
        &[],
        FilesystemResources {
            maximum_pending_records: 1,
            ordering_bytes: 8192,
            base_read_batch: 7,
            ..Default::default()
        },
    );
    let mut records = Records::default();
    let (actual, emitted, reads) = backed(&store, &input, &mut records, false, None);
    let actual = actual.unwrap();
    let expected_input = update_input(&original, &edits, &[], &[], FilesystemResources::default());
    let mut expected_store = TreeStore::new();
    let expected = update_filesystem(
        &mut FilesystemObjects::new(&store, &mut expected_store),
        &expected_input,
        None,
    )
    .unwrap();
    assert_eq!((actual.root, actual.value), (expected.root, expected.value));
    assert_eq!(actual.counters.references.final_removals, count);
    assert_eq!(actual.counters.references.serials_scanned, count + 1);
    assert_eq!(actual.counters.references.peak_pending, 0);
    assert_eq!(actual.counters.references.rows_spilled, 0);
    assert!(records.enumerations >= 10);
    assert_eq!(records.maximum_keys, 64);
    assert!(records.maximum_job <= 65_536);
    assert_eq!(reads, 0);
    assert!(!records
        .values
        .keys()
        .any(|key| key.kind == WORK || key.kind == FRAME));
    let all = folded(&store, &emitted);
    let mut read = FilesystemRead::new(&all, actual.root).unwrap();
    assert_eq!(read.resolve_inode(2), Err(ContentError::PathNotFound));
    assert_eq!(
        read.resolve_inode(count + 1),
        Err(ContentError::PathNotFound)
    );
    assert_eq!(records.after_failure, 0);
}

#[test]
fn release_creates_lower_serial_rows_repeated_counts_and_surviving_aliases_without_losing_final_rows(
) {
    let directories = [
        directory(1, &[("keep", 3), ("top", 100)]),
        directory(100, &[("a", 2), ("b", 2), ("shared", 3), ("sub", 101)]),
        directory(101, &[("leaf", 4)]),
    ];
    let kinds = [
        (1, InodeKind::Directory),
        (2, InodeKind::RegularFile),
        (3, InodeKind::RegularFile),
        (4, InodeKind::RegularFile),
        (100, InodeKind::Directory),
        (101, InodeKind::Directory),
    ];
    let (store, original) = base(&directories, &kinds);
    let edits = [DirectoryUpdate {
        parent: 1,
        changes: vec![(name_of("top"), None)],
    }];
    let input = update_input(
        &original,
        &edits,
        &[],
        &[],
        FilesystemResources {
            base_read_batch: 2,
            ..Default::default()
        },
    );
    let mut records = Records::default();
    let (result, emitted, _) = backed(&store, &input, &mut records, false, None);
    let result = result.unwrap();
    let all = folded(&store, &emitted);
    let mut read = FilesystemRead::new(&all, result.root).unwrap();
    for serial in [2, 4, 100, 101] {
        assert_eq!(read.resolve_inode(serial), Err(ContentError::PathNotFound));
    }
    assert_eq!(read.resolve_inode(3).unwrap().value.namespace_ref_count, 1);
    assert_eq!(read.resolve_child(1, &name_of("keep")).unwrap().serial, 3);
    assert!(matches!(row(&records, 2), Row::Effect { delta: -2, .. }));
    assert!(matches!(row(&records, 3), Row::Effect { delta: -1, .. }));
    assert!(records.values.contains_key(&key(TOUCH, 2)));
    assert!(records
        .values
        .keys()
        .filter(|key| key.kind == NODE)
        .all(|key| records.values[key] == [1, 2]));
    assert_eq!(result.counters.release.released, 5);
    assert_eq!(result.counters.release.peak_depth, 2);
    let mut sink = TreeStore::new();
    let expected =
        update_filesystem(&mut FilesystemObjects::new(&store, &mut sink), &input, None).unwrap();
    assert_eq!(result.root, expected.root);
}

#[test]
fn deep_release_persists_namespace_frames_and_finishes_all_fifo_windows() {
    let depth = 70u64;
    let mut directories = vec![directory(1, &[("top", 2)])];
    directories.extend((2..depth + 1).map(|serial| directory(serial, &[("next", serial + 1)])));
    directories.push(directory(depth + 1, &[]));
    let kinds = (1..depth + 2)
        .map(|serial| (serial, InodeKind::Directory))
        .collect::<Vec<_>>();
    let (store, original) = base(&directories, &kinds);
    let edits = [DirectoryUpdate {
        parent: 1,
        changes: vec![(name_of("top"), None)],
    }];
    let input = update_input(&original, &edits, &[], &[], FilesystemResources::default());
    let mut records = Records::default();
    let (result, emitted, _) = backed(&store, &input, &mut records, false, None);
    let result = result.unwrap();
    assert_eq!(result.counters.release.peak_depth, depth as usize);
    assert_eq!(result.counters.references.final_removals, depth);
    assert_eq!(
        records.values.keys().filter(|key| key.kind == NODE).count(),
        depth as usize
    );
    assert!(!records
        .values
        .keys()
        .any(|key| key.kind == WORK || key.kind == FRAME));
    let all = folded(&store, &emitted);
    let mut read = FilesystemRead::new(&all, result.root).unwrap();
    assert_eq!(
        read.resolve_inode(depth + 1),
        Err(ContentError::PathNotFound)
    );
    assert_eq!(records.after_failure, 0);
}

#[test]
fn complete_zero_seed_finishes_before_any_directory_release_even_across_windows() {
    let count = 70u64;
    let mut directories = vec![DirectoryUpdate {
        parent: 1,
        changes: (2..count + 2)
            .map(|serial| (name_of(&format!("d{serial:04}")), Some(serial)))
            .collect(),
    }];
    directories.extend((2..count + 2).map(|serial| directory(serial, &[])));
    let kinds = (1..count + 2)
        .map(|serial| (serial, InodeKind::Directory))
        .collect::<Vec<_>>();
    let (store, original) = base(&directories, &kinds);
    let edits = [DirectoryUpdate {
        parent: 1,
        changes: directories[0]
            .changes
            .iter()
            .map(|(name, _)| (name.clone(), None))
            .collect(),
    }];
    let input = update_input(
        &original,
        &edits,
        &[],
        &[],
        FilesystemResources {
            base_read_batch: 5,
            ..Default::default()
        },
    );
    let mut records = Records {
        refuse_kind: Some(FRAME),
        ..Default::default()
    };
    let (result, emitted, _) = backed(&store, &input, &mut records, false, None);
    assert_eq!(
        result,
        Err(ContentError::ProviderFailure {
            what: "filesystem backing precondition"
        })
    );
    assert_eq!(
        records.values.keys().filter(|key| key.kind == WORK).count(),
        count as usize
    );
    assert!(!records.values.keys().any(|key| key.kind == FRAME));
    assert_eq!(
        records.values.keys().filter(|key| key.kind == NODE).count(),
        count as usize
    );
    assert_eq!(records.after_failure, 0);
    assert_retained_children(&emitted);
    let original = records.original.as_ref().unwrap();
    assert!(original
        .iter()
        .any(|change| change.key.kind == WORK && change.value.is_none()));
    assert!(original.iter().any(|change| change.key.kind == FRAME));
    let mut control = Records::default();
    let (result, _, _) = backed(&store, &input, &mut control, false, None);
    let result = result.unwrap();
    // All independent seeds are transferred before page work, just as on the
    // resident route. This is cursor-frame occupancy, not tree/path depth.
    assert_eq!(result.counters.release.peak_depth, count as usize);
    assert_eq!(result.counters.references.final_removals, count);
    assert!(!control
        .values
        .keys()
        .any(|key| key.kind == WORK || key.kind == FRAME));
}

#[test]
fn required_row_touch_fresh_and_full_key_grammar_fail_without_final_output_or_later_calls() {
    let directories = [directory(1, &[("file", 2)])];
    let (store, original) = base(
        &directories,
        &[(1, InodeKind::Directory), (2, InodeKind::RegularFile)],
    );
    let edits = [DirectoryUpdate {
        parent: 1,
        changes: vec![(name_of("file"), None)],
    }];
    let input = update_input(&original, &edits, &[], &[], FilesystemResources::default());
    for (mut records, expected) in [
        (
            Records {
                missing_kind: Some(ROW),
                ..Default::default()
            },
            ContentError::InvalidRecord("filesystem reference row missing"),
        ),
        (
            Records {
                missing_kind: Some(TOUCH),
                ..Default::default()
            },
            ContentError::InvalidRecord("filesystem reference row missing"),
        ),
        (
            Records {
                corrupt_kind: Some(TOUCH),
                ..Default::default()
            },
            ContentError::InvalidRecord("filesystem reference membership"),
        ),
        (
            Records {
                corrupt_row_tag: true,
                ..Default::default()
            },
            ContentError::InvalidRecord("filesystem reference fresh tag"),
        ),
        (
            Records {
                malformed_key: true,
                ..Default::default()
            },
            ContentError::InvalidRecord("filesystem reference key"),
        ),
        (
            Records {
                duplicate_key: true,
                ..Default::default()
            },
            ContentError::NonCanonicalOrdering,
        ),
    ] {
        let (result, emitted, _) = backed(&store, &input, &mut records, false, None);
        assert_eq!(result, Err(expected));
        no_final(&emitted);
        assert_eq!(records.after_failure, 0);
    }
    let empty: &[DirectoryUpdate] = &[];
    let new_values = [InodeUpdate {
        serial: 3,
        value: typed(InodeKind::RegularFile),
    }];
    let input = update_input(
        &original,
        empty,
        &new_values,
        &[3],
        FilesystemResources::default(),
    );
    let mut records = Records {
        corrupt_kind: Some(FRESH),
        ..Default::default()
    };
    let (result, emitted, _) = backed(&store, &input, &mut records, false, None);
    assert_eq!(
        result,
        Err(ContentError::InvalidRecord(
            "filesystem reference membership"
        ))
    );
    no_final(&emitted);
    assert_eq!(records.after_failure, 0);
}

#[test]
fn row_plus_touch_mutation_and_child_progress_are_atomic_and_keep_original_refusal_custody() {
    let directories = [directory(1, &[("top", 10)]), directory(10, &[("file", 2)])];
    let (store, original) = base(
        &directories,
        &[
            (1, InodeKind::Directory),
            (2, InodeKind::RegularFile),
            (10, InodeKind::Directory),
        ],
    );
    let edits = [DirectoryUpdate {
        parent: 1,
        changes: vec![(name_of("top"), None)],
    }];
    let input = update_input(&original, &edits, &[], &[], FilesystemResources::default());
    let mut refused = Records {
        refuse_child_progress: true,
        ..Default::default()
    };
    let (result, emitted, _) = backed(&store, &input, &mut refused, false, None);
    assert_eq!(
        result,
        Err(ContentError::ProviderFailure {
            what: "filesystem backing precondition"
        })
    );
    assert!(!refused.values.contains_key(&key(ROW, 2)));
    assert!(!refused.values.contains_key(&key(TOUCH, 2)));
    let frame = &refused.values[&key(FRAME, 1)];
    assert_eq!(frame.len(), 45);
    assert_eq!(&frame[43..], &[0, 0]);
    let original = refused.original.as_ref().unwrap();
    assert!(original.iter().any(|change| change.key == key(ROW, 2)));
    assert!(original.iter().any(|change| change.key == key(TOUCH, 2)));
    assert!(original.iter().any(|change| change.key == key(FRAME, 1)));
    assert_eq!(refused.after_failure, 0);
    assert_retained_children(&emitted);
    let mut failed = Records {
        fail_kind: Some(WORK),
        ..Default::default()
    };
    let (result, emitted, _) = backed(&store, &input, &mut failed, false, None);
    assert_eq!(
        result,
        Err(ContentError::ProviderFailure {
            what: "original indexed reference failure"
        })
    );
    assert!(failed.original.is_some());
    assert_eq!(failed.after_failure, 0);
    assert_retained_children(&emitted);
}

#[test]
fn missing_work_window_is_not_false_completion_and_no_rows_are_dropped_for_final_scan() {
    let directories = [directory(1, &[("top", 10)]), directory(10, &[])];
    let (store, original) = base(
        &directories,
        &[(1, InodeKind::Directory), (10, InodeKind::Directory)],
    );
    let edits = [DirectoryUpdate {
        parent: 1,
        changes: vec![(name_of("top"), None)],
    }];
    let input = update_input(&original, &edits, &[], &[], FilesystemResources::default());
    let mut records = Records {
        missing_queued: true,
        ..Default::default()
    };
    let (result, emitted, _) = backed(&store, &input, &mut records, false, None);
    assert_eq!(
        result,
        Err(ContentError::InvalidRecord(
            "filesystem release work missing"
        ))
    );
    assert!(records.values.contains_key(&key(WORK, 1)));
    assert!(records.values.contains_key(&key(ROW, 10)));
    assert_eq!(records.after_failure, 0);
    assert_retained_children(&emitted);
}

#[test]
fn rebuilt_released_directory_requires_same_save_capability_and_keeps_original_accepted_reader_error(
) {
    let directories = [
        directory(1, &[("top", 10)]),
        directory(10, &[("a", 2), ("b", 3)]),
    ];
    let (store, original) = base(
        &directories,
        &[
            (1, InodeKind::Directory),
            (2, InodeKind::RegularFile),
            (3, InodeKind::RegularFile),
            (10, InodeKind::Directory),
        ],
    );
    // Directory10 was rebuilt and accepted before its last parent binding was
    // released. Reading its old root would incorrectly retain inode3.
    let edits = [
        DirectoryUpdate {
            parent: 1,
            changes: vec![(name_of("top"), None)],
        },
        DirectoryUpdate {
            parent: 10,
            changes: vec![(name_of("b"), None)],
        },
    ];
    let input = update_input(&original, &edits, &[], &[], FilesystemResources::default());
    let mut missing = Records::default();
    let (result, emitted, reads) = backed(&store, &input, &mut missing, false, None);
    assert_eq!(
        result,
        Err(ContentError::ProviderFailure {
            what: "filesystem accepted-object reader unavailable"
        })
    );
    assert_eq!(reads, 0);
    assert_retained_children(&emitted);
    assert!(missing.values.keys().any(|key| key.kind == WORK));
    let mut failed = Records::default();
    let (result, emitted, reads) = backed(
        &store,
        &input,
        &mut failed,
        true,
        Some("original same-Save accepted failure"),
    );
    assert_eq!(
        result,
        Err(ContentError::ProviderFailure {
            what: "original same-Save accepted failure"
        })
    );
    assert_eq!(reads, 1);
    assert_retained_children(&emitted);
    assert!(failed.values.keys().any(|key| key.kind == FRAME));
    let mut records = Records::default();
    let (result, emitted, reads) = backed(&store, &input, &mut records, true, None);
    let result = result.unwrap();
    assert!(reads > 0);
    let all = folded(&store, &emitted);
    let mut read = FilesystemRead::new(&all, result.root).unwrap();
    for serial in [2, 3, 10] {
        assert_eq!(read.resolve_inode(serial), Err(ContentError::PathNotFound));
    }
    assert!(matches!(row(&records, 2), Row::Effect { delta: -1, .. }));
    assert!(matches!(row(&records, 3), Row::Effect { delta: -1, .. }));
    assert_eq!(records.after_failure, 0);
}

#[test]
fn paged_release_keeps_actual_utf8_name_boundaries_and_all_repeated_parent_pages() {
    let count = 257u64;
    let inner = DirectoryUpdate {
        parent: 1000,
        changes: (2..count + 2)
            .map(|serial| {
                (
                    name_of(&format!(
                        "é-{serial:05}-{}",
                        "x".repeat((serial % 7) as usize)
                    )),
                    Some(serial),
                )
            })
            .collect(),
    };
    let directories = [directory(1, &[("top", 1000)]), inner];
    let mut kinds = vec![(1, InodeKind::Directory)];
    kinds.extend((2..count + 2).map(|serial| (serial, InodeKind::RegularFile)));
    kinds.push((1000, InodeKind::Directory));
    let (store, original) = base(&directories, &kinds);
    let edits = [DirectoryUpdate {
        parent: 1,
        changes: vec![(name_of("top"), None)],
    }];
    let input = update_input(
        &original,
        &edits,
        &[],
        &[],
        FilesystemResources {
            base_read_batch: 3,
            ..Default::default()
        },
    );
    let mut records = Records::default();
    let (result, emitted, _) = backed(&store, &input, &mut records, false, None);
    let result = result.unwrap();
    assert_eq!(result.counters.release.entries, count);
    assert_eq!(result.counters.release.released, count);
    assert_eq!(result.counters.release.pages, 5);
    assert_eq!(result.counters.references.final_removals, count + 1);
    assert!(!records
        .values
        .keys()
        .any(|key| key.kind == FRAME || key.kind == WORK));
    for serial in 2..count + 2 {
        assert!(matches!(
            row(&records, serial),
            Row::Effect { delta: -1, .. }
        ));
    }
    let all = folded(&store, &emitted);
    let mut read = FilesystemRead::new(&all, result.root).unwrap();
    assert_eq!(read.resolve_inode(258), Err(ContentError::PathNotFound));
    assert_eq!(records.after_failure, 0);
}

#[test]
fn missing_fresh_membership_and_stale_row_guard_cannot_change_rows_or_touch_witnesses() {
    let (store, original) = base(&[directory(1, &[])], &[(1, InodeKind::Directory)]);
    let edits = [directory(1, &[("new", 2)])];
    let values = [InodeUpdate {
        serial: 2,
        value: typed(InodeKind::RegularFile),
    }];
    let input = update_input(
        &original,
        &edits,
        &values,
        &[2],
        FilesystemResources::default(),
    );
    let mut missing = Records {
        missing_kind: Some(FRESH),
        ..Default::default()
    };
    let (result, emitted, _) = backed(&store, &input, &mut missing, false, None);
    assert_eq!(
        result,
        Err(ContentError::InvalidRecord("filesystem fresh membership"))
    );
    assert!(!missing
        .values
        .keys()
        .any(|key| key.kind == ROW || key.kind == TOUCH));
    assert_eq!(missing.after_failure, 0);
    no_final(&emitted);
    let mut stale = Records::default();
    let old = Row::Count {
        serial: 2,
        value: Some(typed(InodeKind::RegularFile)),
        count: 19,
    }
    .encode()
    .unwrap()
    .to_vec();
    stale.values.insert(key(ROW, 2), old.clone());
    stale.values.insert(key(TOUCH, 2), vec![1]);
    // Fresh declaration checks row/touch absence in the same all-guards-before-
    // effects batch, so an occupied row cannot borrow a new declaration.
    let (result, emitted, _) = backed(&store, &input, &mut stale, false, None);
    assert_eq!(
        result,
        Err(ContentError::ProviderFailure {
            what: "filesystem backing precondition"
        })
    );
    assert_eq!(stale.values.get(&key(ROW, 2)), Some(&old));
    assert!(!stale.values.contains_key(&key(FRESH, 2)));
    assert!(stale.original.is_some());
    assert_eq!(stale.after_failure, 0);
    no_final(&emitted);
}

#[test]
fn new_child_inside_released_rebuilt_parent_remains_an_explicit_refusal_with_known_children_retained(
) {
    let directories = [directory(1, &[("top", 10)]), directory(10, &[("old", 2)])];
    let (store, original) = base(
        &directories,
        &[
            (1, InodeKind::Directory),
            (2, InodeKind::RegularFile),
            (10, InodeKind::Directory),
        ],
    );
    let edits = [
        DirectoryUpdate {
            parent: 1,
            changes: vec![(name_of("top"), None)],
        },
        directory(10, &[("new", 3)]),
    ];
    let values = [InodeUpdate {
        serial: 3,
        value: typed(InodeKind::RegularFile),
    }];
    let input = update_input(
        &original,
        &edits,
        &values,
        &[3],
        FilesystemResources::default(),
    );
    let mut records = Records::default();
    let (result, emitted, reads) = backed(&store, &input, &mut records, true, None);
    assert_eq!(result, Err(ContentError::InvalidRecord("released child")));
    assert!(reads > 0);
    assert_retained_children(&emitted);
    assert!(matches!(row(&records, 3), Row::Count { count: 1, .. }));
    assert!(records.values.keys().any(|key| key.kind == FRAME));
    assert_eq!(records.after_failure, 0);
}

#[test]
fn authenticated_max_count_plus_one_is_length_overflow_before_release_or_final_root_on_both_routes()
{
    use layerfs_content::filesystem::inode::read::InodeTable;
    use layerfs_content::filesystem::references::ReferenceReducer;
    use layerfs_content::object::inode_leaf::{decode_inode_value, encode_inode_value, InodeLeaf};
    let (mut store, original) = base(
        &[directory(1, &[("file", 2)])],
        &[(1, InodeKind::Directory), (2, InodeKind::RegularFile)],
    );
    // A genuine new content identity encodes the supported u64 count boundary;
    // there is no identity mismatch or mutable-provider byte injection.
    let mut leaf =
        InodeLeaf::decode(store.canonical(original.value.inode_table()).unwrap()).unwrap();
    let file = leaf.rows.iter_mut().find(|row| row.serial == 2).unwrap();
    let mut maximum = decode_inode_value(&file.value).unwrap();
    maximum.namespace_ref_count = u64::MAX;
    file.value = encode_inode_value(maximum);
    let table = store.insert(ObjectRole::InodeLeaf, leaf.encode().unwrap());
    let value = original.value.with_inode_table(table);
    let root = store.insert(ObjectRole::FilesystemRoot, value.encode().unwrap());
    let maximum_base = FilesystemResult {
        root: layerfs_content::filesystem::FilesystemRootId(root),
        value,
        counters: original.counters,
    };
    let edits = [directory(1, &[("alias", 2)])];
    let input = update_input(
        &maximum_base,
        &edits,
        &[],
        &[],
        FilesystemResources::default(),
    );
    let mut records = Records::default();
    let (result, emitted, _) = backed(&store, &input, &mut records, false, None);
    assert_eq!(result, Err(ContentError::LengthOverflow));
    no_final(&emitted);
    assert!(!records
        .values
        .keys()
        .any(|key| key.kind == WORK || key.kind == NODE));
    assert!(!emitted
        .order()
        .iter()
        .any(|(_, role)| *role == ObjectRole::InodeLeaf));
    let mut resident = TreeStore::new();
    assert_eq!(
        update_filesystem(
            &mut FilesystemObjects::new(&store, &mut resident),
            &input,
            None
        ),
        Err(ContentError::LengthOverflow)
    );
    no_final(&resident);
    let mut reducer = ReferenceReducer::new(4, None, 96, 4096);
    reducer.note_retained_binding(2).unwrap();
    let mut rows = reducer
        .finish(
            &store,
            InodeTable {
                root: table,
                root_serial: 1,
            },
            1,
            1,
        )
        .unwrap();
    assert_eq!(rows.next_change(), Err(ContentError::LengthOverflow));
    // At the exact boundary, a zero effect still retains MAX. A supplied typed
    // count cannot replace the authenticated count in either final derivation.
    let values = [InodeUpdate {
        serial: 2,
        value: typed(InodeKind::RegularFile),
    }];
    let control = update_input(
        &maximum_base,
        &[],
        &values,
        &[],
        FilesystemResources::default(),
    );
    let mut records = Records::default();
    let (result, emitted, _) = backed(&store, &control, &mut records, false, None);
    let result = result.unwrap();
    let all = folded(&store, &emitted);
    let mut read = FilesystemRead::new(&all, result.root).unwrap();
    assert_eq!(
        read.resolve_inode(2).unwrap().value.namespace_ref_count,
        u64::MAX
    );
}

#[test]
fn late_final_row_failure_is_forwarded_as_error_without_eof_finalization_or_later_demand() {
    let count = 257u64;
    let directories = [DirectoryUpdate {
        parent: 1,
        changes: (2..count + 2)
            .map(|serial| (name_of(&format!("f{serial:05}")), Some(serial)))
            .collect(),
    }];
    let mut kinds = vec![(1, InodeKind::Directory)];
    kinds.extend((2..count + 2).map(|serial| (serial, InodeKind::RegularFile)));
    let (store, original) = base(&directories, &kinds);
    let values = (2..count + 2)
        .map(|serial| InodeUpdate {
            serial,
            value: InodeValue {
                metadata_root: synthetic("references/late-final-new"),
                ..typed(InodeKind::RegularFile)
            },
        })
        .collect::<Vec<_>>();
    let input = update_input(
        &original,
        &[],
        &values,
        &[],
        FilesystemResources {
            base_read_batch: 7,
            ..Default::default()
        },
    );
    let mut records = Records {
        fail_final_row_at: Some(251),
        ..Default::default()
    };
    let (result, emitted, _) = backed(&store, &input, &mut records, false, None);
    assert_eq!(
        result,
        Err(ContentError::ProviderFailure {
            what: "original indexed reference failure"
        })
    );
    assert!(records.scan_starts >= 2);
    assert_eq!(records.after_failure, 0);
    assert!(emitted
        .order()
        .iter()
        .any(|(_, role)| *role == ObjectRole::InodeLeaf));
    no_final(&emitted);
    // The common helper's fault latch independently asserts zero postfailure
    // initial-provider demand and zero consumer acceptance, including finalization.
}

//! Public serial-state construction through the existing neutral raw port.
mod support;
use layerfs_content::filesystem::rows::RowSource;
use layerfs_content::filesystem::{
    build_filesystem, build_filesystem_streamed_backed, scope_for_seed, update_filesystem,
    update_filesystem_streamed_backed, DirectoryChangeLookup, DirectoryChangeSource,
    DirectoryHeader, DirectoryHeaderSource, DirectoryUpdate, FilesystemInput, FilesystemObjects,
    FilesystemRead, FilesystemResources, FilesystemResult, FilesystemRootId, InodeRowSource,
    InodeUpdate, PathName, SerialRowSource, StreamedFilesystemInput, StreamedRowSource,
};
use layerfs_content::object::inode_leaf::{InodeKind, InodeValue};
use layerfs_content::{
    ConstructionRecordApply, ConstructionRecordChange, ConstructionRecordExpected,
    ConstructionRecordKey, ContentError, ContentResult, IndexedConstructionBacking, ObjectRole,
};
use std::collections::BTreeMap;
use support::filesystem::{name_of, synthetic, value, Session, TreeStore};

const CONTEXT: u32 = 0x4653_0000;
const PARENT: u32 = CONTEXT + 1;
const ROOT: u32 = CONTEXT + 2;
const COUNT: u32 = CONTEXT + 3;
#[derive(Default)]
struct Records {
    values: BTreeMap<ConstructionRecordKey, Vec<u8>>,
    calls: usize,
    after_failure: usize,
    terminal: bool,
    fail_apply: Option<u32>,
    refuse_apply: Option<u32>,
    corrupt_root: bool,
    missing_root: bool,
    missing_count: bool,
    disappear_replayed_root: bool,
    root_reads: usize,
    fail_count_read: Option<usize>,
    count_reads: usize,
    maximum_job: usize,
    original: Option<Vec<ConstructionRecordChange>>,
}
impl Records {
    fn enter(&mut self) -> ContentResult<()> {
        if self.terminal {
            self.after_failure += 1;
            return Err(ContentError::ProviderFailure {
                what: "fixture terminal",
            });
        }
        self.calls += 1;
        Ok(())
    }
    fn fail(&mut self) -> ContentError {
        self.terminal = true;
        ContentError::ProviderFailure {
            what: "original filesystem record refusal",
        }
    }
}
impl IndexedConstructionBacking for Records {
    fn contains(&mut self, key: ConstructionRecordKey) -> ContentResult<bool> {
        self.enter()?;
        Ok(self.values.contains_key(&key))
    }
    fn get(&mut self, key: ConstructionRecordKey) -> ContentResult<Option<Vec<u8>>> {
        self.enter()?;
        if self.disappear_replayed_root && key.kind == ROOT {
            self.root_reads += 1;
            if self.root_reads == 2 {
                self.values.remove(&key);
                self.terminal = true;
                return Ok(None);
            }
        }
        if key.kind == COUNT {
            self.count_reads += 1;
            if self.fail_count_read == Some(self.count_reads) {
                return Err(self.fail());
            }
            if self.missing_count {
                return Ok(None);
            }
        }
        if self.missing_root && key.kind == ROOT {
            return Ok(None);
        }
        if self.corrupt_root && key.kind == ROOT && self.values.contains_key(&key) {
            return Ok(Some(vec![255]));
        }
        Ok(self.values.get(&key).cloned())
    }
    fn apply(
        &mut self,
        changes: Vec<ConstructionRecordChange>,
    ) -> ContentResult<ConstructionRecordApply> {
        self.enter()?;
        assert!(changes.windows(2).all(|p| p[0].key < p[1].key));
        assert!(changes
            .iter()
            .all(|c| (CONTEXT..=CONTEXT + 9).contains(&c.key.kind)));
        let bytes = changes.capacity() * std::mem::size_of::<ConstructionRecordChange>()
            + changes
                .iter()
                .map(|change| {
                    let old = match &change.expected {
                        ConstructionRecordExpected::Missing => 0,
                        ConstructionRecordExpected::ExactBytes(bytes) => bytes.capacity(),
                    };
                    old + change.value.as_ref().map_or(0, Vec::capacity)
                })
                .sum::<usize>();
        assert!(bytes <= 65_536);
        self.maximum_job = self.maximum_job.max(bytes);
        if changes.iter().any(|c| Some(c.key.kind) == self.fail_apply) {
            self.original = Some(changes);
            return Err(self.fail());
        }
        for (index, change) in changes.iter().enumerate() {
            let actual = self.values.get(&change.key);
            let valid = match (&change.expected, actual) {
                (ConstructionRecordExpected::Missing, None) => true,
                (ConstructionRecordExpected::ExactBytes(expected), Some(actual)) => {
                    expected == actual
                }
                _ => false,
            };
            if !valid || self.refuse_apply == Some(change.key.kind) {
                let result = ConstructionRecordApply::NotApplied {
                    index,
                    key: change.key,
                    actual: actual.cloned(),
                };
                self.original = Some(changes);
                self.terminal = true;
                return Ok(result);
            }
        }
        for change in changes {
            if let Some(value) = change.value {
                self.values.insert(change.key, value);
            } else {
                self.values.remove(&change.key);
            }
        }
        Ok(ConstructionRecordApply::Applied)
    }
    fn first_keys(
        &mut self,
        kind: u32,
        excluded: Option<[u8; 32]>,
    ) -> ContentResult<Vec<[u8; 32]>> {
        self.enter()?;
        assert_eq!(
            kind,
            CONTEXT + 7,
            "only the destructive release FIFO uses first_keys"
        );
        Ok(self
            .values
            .keys()
            .filter(|key| key.kind == kind && excluded != Some(key.key))
            .take(64)
            .map(|key| key.key)
            .collect())
    }
    fn keys_after(&mut self, kind: u32, after: Option<[u8; 32]>) -> ContentResult<Vec<[u8; 32]>> {
        self.enter()?;
        assert_eq!(
            kind,
            CONTEXT + 6,
            "only sealed touched membership uses this cursor"
        );
        Ok(self
            .values
            .keys()
            .filter(|key| key.kind == kind && after.is_none_or(|after| key.key > after))
            .take(64)
            .map(|key| key.key)
            .collect())
    }
}
struct Headers<'a>(std::slice::Iter<'a, DirectoryUpdate>);
impl DirectoryHeaderSource for Headers<'_> {
    fn next_row(&mut self) -> ContentResult<Option<DirectoryHeader>> {
        Ok(self.0.next().map(|row| DirectoryHeader {
            parent: row.parent,
            change_rows: row.changes.len() as u64,
        }))
    }
}
struct Changes<'a>(std::slice::Iter<'a, (PathName, Option<u64>)>);
impl DirectoryChangeSource for Changes<'_> {
    fn next_row(&mut self) -> ContentResult<Option<(PathName, Option<u64>)>> {
        Ok(self.0.next().cloned())
    }
}
struct Rows<'a>(&'a FilesystemInput<'a>);
impl StreamedRowSource for Rows<'_> {
    fn directory_rows(&self) -> usize {
        self.0.directories.len()
    }
    fn inode_rows(&self) -> usize {
        self.0.inodes.len()
    }
    fn new_rows(&self) -> usize {
        self.0.new_inodes.len()
    }
    fn directory_headers(&self) -> ContentResult<Box<dyn DirectoryHeaderSource + '_>> {
        Ok(Box::new(Headers(self.0.directories.iter())))
    }
    fn directory_header(&self, parent: u64) -> ContentResult<Option<DirectoryHeader>> {
        Ok(self
            .0
            .directories
            .iter()
            .find(|row| row.parent == parent)
            .map(|row| DirectoryHeader {
                parent,
                change_rows: row.changes.len() as u64,
            }))
    }
    fn directory_changes(&self, parent: u64) -> ContentResult<Box<dyn DirectoryChangeSource + '_>> {
        let row = self
            .0
            .directories
            .iter()
            .find(|row| row.parent == parent)
            .expect("sealed header");
        Ok(Box::new(Changes(row.changes.iter())))
    }
    fn directory_change(
        &self,
        parent: u64,
        name: &PathName,
    ) -> ContentResult<DirectoryChangeLookup> {
        Ok(self
            .0
            .directories
            .iter()
            .find(|row| row.parent == parent)
            .and_then(|row| row.changes.iter().find(|(key, _)| key == name))
            .map_or(DirectoryChangeLookup::Unchanged, |(_, binding)| {
                binding.map_or(DirectoryChangeLookup::Removed, DirectoryChangeLookup::Bound)
            }))
    }
    fn inodes(&self) -> ContentResult<Box<dyn InodeRowSource + '_>> {
        self.0.inodes()
    }
    fn new_inodes(&self) -> ContentResult<Box<dyn SerialRowSource + '_>> {
        self.0.new_inodes()
    }
    fn value_for(&self, serial: u64) -> ContentResult<Option<InodeValue>> {
        self.0.value_for(serial)
    }
    fn new_position(&self, serial: u64) -> ContentResult<Option<usize>> {
        self.0.new_position(serial)
    }
}
fn prepared<'a>(rows: &'a Rows<'a>) -> StreamedFilesystemInput<'a> {
    StreamedFilesystemInput {
        base: rows.0.base,
        scope: rows.0.scope,
        root_serial: rows.0.root_serial,
        resources: rows.0.resources,
        rows,
    }
}
struct Fixture {
    directories: Vec<DirectoryUpdate>,
    inodes: Vec<InodeUpdate>,
    new: Vec<u64>,
}
impl Fixture {
    fn input(&self, resources: FilesystemResources) -> FilesystemInput<'_> {
        FilesystemInput {
            base: None,
            scope: scope_for_seed([53; 32]),
            root_serial: 1,
            directories: &self.directories,
            inodes: &self.inodes,
            new_inodes: &self.new,
            resources,
        }
    }
}
fn typed(kind: InodeKind) -> InodeValue {
    value(kind, synthetic("content"), synthetic("metadata"))
}
fn directory_fixture(count: usize, dropped: bool) -> Fixture {
    let root = DirectoryUpdate {
        parent: 1,
        changes: (0..count)
            .map(|index| (name_of(&format!("d{index:05}")), Some(index as u64 + 2)))
            .collect(),
    };
    let mut directories = vec![root];
    directories.extend((2..count as u64 + 2).map(|parent| DirectoryUpdate {
        parent,
        changes: Vec::new(),
    }));
    if dropped {
        directories.push(DirectoryUpdate {
            parent: count as u64 + 2,
            changes: Vec::new(),
        });
    }
    let last = count as u64 + 1 + u64::from(dropped);
    Fixture {
        directories,
        inodes: (1..=last)
            .map(|serial| InodeUpdate {
                serial,
                value: typed(InodeKind::Directory),
            })
            .collect(),
        new: (1..=last).collect(),
    }
}
fn file_fixture(count: usize, alias: bool) -> Fixture {
    let mut changes = (0..count)
        .map(|index| (name_of(&format!("f{index:05}")), Some(index as u64 + 2)))
        .collect::<Vec<_>>();
    if alias {
        changes.push((name_of("zz-alias"), Some(2)));
    }
    Fixture {
        directories: vec![DirectoryUpdate { parent: 1, changes }],
        inodes: (1..=count as u64 + 1)
            .map(|serial| InodeUpdate {
                serial,
                value: typed(if serial == 1 {
                    InodeKind::Directory
                } else {
                    InodeKind::RegularFile
                }),
            })
            .collect(),
        new: (1..=count as u64 + 1).collect(),
    }
}
fn build(
    fixture: &Fixture,
    resources: FilesystemResources,
    records: &mut Records,
    sink: &mut TreeStore,
) -> ContentResult<FilesystemResult> {
    let input = fixture.input(resources);
    let rows = Rows(&input);
    build_filesystem_streamed_backed(
        &mut FilesystemObjects::new(&TreeStore::new(), sink),
        &prepared(&rows),
        records,
        None,
    )
}

#[test]
fn backed_new_parent_membership_exceeds_only_the_old_map_allowance_and_drops_empty_parent() {
    let fixture = directory_fixture(33, true);
    let resources = FilesystemResources {
        ordering_bytes: 16 * 1024,
        ..Default::default()
    };
    let mut records = Records::default();
    let mut sink = TreeStore::new();
    let result = build(&fixture, resources, &mut records, &mut sink).unwrap();
    assert!(records.maximum_job <= 65_536);
    assert_eq!(result.counters.references.final_values, 34);
    let mut read = FilesystemRead::new(&sink, result.root).unwrap();
    assert_eq!(
        read.resolve_child(1, &name_of("d00032")).unwrap().serial,
        34
    );
    assert_eq!(read.resolve_inode(34).unwrap().value.namespace_ref_count, 1);
    assert_eq!(read.resolve_inode(35), Err(ContentError::PathNotFound));
    let mut resident = TreeStore::new();
    assert!(matches!(
        build_filesystem(
            &mut FilesystemObjects::new(&TreeStore::new(), &mut resident),
            &fixture.input(resources),
            None
        ),
        Err(ContentError::ObjectLimitExceeded {
            limit: 16,
            actual: 17
        })
    ));
    let expected = build_filesystem(
        &mut FilesystemObjects::new(&TreeStore::new(), &mut resident),
        &fixture.input(FilesystemResources::default()),
        None,
    )
    .unwrap();
    assert_eq!((result.root, result.value), (expected.root, expected.value));
    assert_eq!(records.after_failure, 0);
}

#[test]
fn backed_initial_counts_and_streamed_final_rows_preserve_real_alias_counts() {
    let fixture = file_fixture(257, true);
    let mut records = Records::default();
    let mut sink = TreeStore::new();
    let result = build(
        &fixture,
        FilesystemResources::default(),
        &mut records,
        &mut sink,
    )
    .unwrap();
    let mut read = FilesystemRead::new(&sink, result.root).unwrap();
    assert_eq!(
        read.resolve_child(1, &name_of("zz-alias")).unwrap().serial,
        2
    );
    assert_eq!(read.resolve_inode(2).unwrap().value.namespace_ref_count, 2);
    assert_eq!(
        read.resolve_inode(258).unwrap().value.namespace_ref_count,
        1
    );
    assert_eq!(read.resolve_inode(1).unwrap().value.namespace_ref_count, 0);
    let mut resident = TreeStore::new();
    let expected = build_filesystem(
        &mut FilesystemObjects::new(&TreeStore::new(), &mut resident),
        &fixture.input(FilesystemResources::default()),
        None,
    )
    .unwrap();
    assert_eq!((result.root, result.value), (expected.root, expected.value));
    assert_eq!(result.counters, expected.counters);
}

#[test]
fn exact_context_marker_refuses_reuse_before_new_effects_and_retains_original_batch() {
    let fixture = file_fixture(2, false);
    let mut records = Records::default();
    let mut first = TreeStore::new();
    build(
        &fixture,
        FilesystemResources::default(),
        &mut records,
        &mut first,
    )
    .unwrap();
    let marker = records
        .values
        .iter()
        .find(|(key, _)| key.kind == CONTEXT)
        .unwrap()
        .1;
    assert_eq!(marker.len(), 106);
    assert_eq!(&marker[..2], &[1, 0]);
    assert_eq!(&marker[2..34], &[0; 32]);
    assert_eq!(
        &marker[34..66],
        scope_for_seed([53; 32]).object().as_bytes()
    );
    assert_eq!(
        &marker[66..98],
        layerfs_content::filesystem::profile_id().as_bytes()
    );
    assert_eq!(&marker[98..], &1u64.to_be_bytes());
    let retained = records.values.clone();
    let mut second = TreeStore::new();
    assert_eq!(
        build(
            &fixture,
            FilesystemResources::default(),
            &mut records,
            &mut second
        ),
        Err(ContentError::ProviderFailure {
            what: "filesystem backing precondition"
        })
    );
    assert_eq!(records.values, retained);
    assert!(second.is_empty());
    assert_eq!(records.original.as_ref().unwrap().len(), 1);
    assert_eq!(records.original.as_ref().unwrap()[0].key.kind, CONTEXT);
    assert_eq!(records.after_failure, 0);
}

#[test]
fn original_parent_count_and_post_accept_root_failures_stop_without_another_job() {
    let fixture = directory_fixture(2, false);
    for kind in [PARENT, COUNT, ROOT] {
        let mut records = Records {
            fail_apply: Some(kind),
            ..Default::default()
        };
        let mut sink = TreeStore::new();
        assert_eq!(
            build(
                &fixture,
                FilesystemResources::default(),
                &mut records,
                &mut sink
            ),
            Err(ContentError::ProviderFailure {
                what: "original filesystem record refusal"
            })
        );
        assert_eq!(records.after_failure, 0);
        assert!(records
            .original
            .as_ref()
            .unwrap()
            .iter()
            .any(|c| c.key.kind == kind));
        assert!(!sink
            .order()
            .iter()
            .any(|(_, role)| *role == ObjectRole::FilesystemRoot));
        if kind == ROOT {
            assert!(
                !sink.is_empty(),
                "accepted directory stays in consumer custody"
            );
        } else {
            assert!(sink.is_empty());
        }
    }
}

#[test]
fn atomic_not_applied_and_malformed_root_are_refusals_not_fallbacks() {
    let fixture = directory_fixture(2, false);
    let mut refused = Records {
        refuse_apply: Some(COUNT),
        ..Default::default()
    };
    let mut sink = TreeStore::new();
    assert_eq!(
        build(
            &fixture,
            FilesystemResources::default(),
            &mut refused,
            &mut sink
        ),
        Err(ContentError::ProviderFailure {
            what: "filesystem backing precondition"
        })
    );
    assert!(!refused.values.keys().any(|key| key.kind == COUNT));
    assert_eq!(refused.after_failure, 0);
    let mut malformed = Records {
        corrupt_root: true,
        ..Default::default()
    };
    let mut sink = TreeStore::new();
    assert_eq!(
        build(
            &fixture,
            FilesystemResources::default(),
            &mut malformed,
            &mut sink
        ),
        Err(ContentError::InvalidRecord("filesystem rebuilt root state"))
    );
    assert!(!sink
        .order()
        .iter()
        .any(|(_, role)| *role == ObjectRole::FilesystemRoot));
}

#[test]
fn missing_declared_count_or_directory_root_never_uses_a_default_or_supplied_draft_root() {
    let fixture = directory_fixture(2, false);
    for (count, error) in [
        (
            true,
            ContentError::InvalidRecord("filesystem count missing"),
        ),
        (
            false,
            ContentError::InvalidRecord("filesystem rebuilt root missing"),
        ),
    ] {
        let mut records = Records {
            missing_count: count,
            missing_root: !count,
            ..Default::default()
        };
        let mut sink = TreeStore::new();
        assert_eq!(
            build(
                &fixture,
                FilesystemResources::default(),
                &mut records,
                &mut sink
            ),
            Err(error)
        );
        assert!(!sink
            .order()
            .iter()
            .any(|(_, role)| *role == ObjectRole::FilesystemRoot));
    }
}

#[test]
fn stale_serial_state_refuses_its_missing_guard_and_file_editor_kinds_remain_separate() {
    let fixture = file_fixture(2, false);
    let mut key = [0; 32];
    key[24..].copy_from_slice(&2u64.to_be_bytes());
    let mut records = Records::default();
    records
        .values
        .insert(ConstructionRecordKey { kind: 1, key }, vec![7]);
    let mut sink = TreeStore::new();
    build(
        &fixture,
        FilesystemResources::default(),
        &mut records,
        &mut sink,
    )
    .unwrap();
    assert_eq!(
        records.values.get(&ConstructionRecordKey { kind: 1, key }),
        Some(&vec![7])
    );
    let mut stale = Records::default();
    let mut value = vec![1];
    value.extend_from_slice(&999u64.to_be_bytes());
    stale
        .values
        .insert(ConstructionRecordKey { kind: COUNT, key }, value.clone());
    let mut sink = TreeStore::new();
    assert_eq!(
        build(
            &fixture,
            FilesystemResources::default(),
            &mut stale,
            &mut sink
        ),
        Err(ContentError::ProviderFailure {
            what: "filesystem backing precondition"
        })
    );
    assert_eq!(
        stale
            .values
            .get(&ConstructionRecordKey { kind: COUNT, key }),
        Some(&value)
    );
    assert!(sink.is_empty());
    assert_eq!(stale.after_failure, 0);
}

#[test]
fn late_initial_count_read_failure_keeps_accepted_inode_pages_and_no_final_root() {
    let fixture = file_fixture(257, false);
    let mut records = Records {
        // A second complete inode page makes the first one final/accepted;
        // the unfinished sibling remains private when this later read fails.
        fail_count_read: Some(257 + 230),
        ..Default::default()
    };
    let mut sink = TreeStore::new();
    assert_eq!(
        build(
            &fixture,
            FilesystemResources::default(),
            &mut records,
            &mut sink
        ),
        Err(ContentError::ProviderFailure {
            what: "original filesystem record refusal"
        })
    );
    assert_eq!(records.after_failure, 0);
    assert!(sink
        .order()
        .iter()
        .any(|(_, role)| *role == ObjectRole::InodeLeaf));
    assert!(!sink
        .order()
        .iter()
        .any(|(_, role)| *role == ObjectRole::FilesystemRoot));
}

#[test]
fn backed_update_consumes_rebuilt_roots_in_order_before_values_and_matches_move_alias_result() {
    let mut session = Session::new(1).unwrap();
    let dir = session.allocate();
    let file = session.allocate();
    session
        .apply(
            &[
                DirectoryUpdate {
                    parent: 1,
                    changes: vec![(name_of("dir"), Some(dir)), (name_of("file"), Some(file))],
                },
                DirectoryUpdate {
                    parent: dir,
                    changes: Vec::new(),
                },
            ],
            &[
                InodeUpdate {
                    serial: dir,
                    value: typed(InodeKind::Directory),
                },
                InodeUpdate {
                    serial: file,
                    value: typed(InodeKind::RegularFile),
                },
            ],
            &[dir, file],
        )
        .unwrap();
    let directories = [
        DirectoryUpdate {
            parent: 1,
            changes: vec![
                (name_of("dir"), None),
                (name_of("file"), None),
                (name_of("linked"), Some(file)),
                (name_of("moved"), Some(dir)),
            ],
        },
        DirectoryUpdate {
            parent: dir,
            changes: vec![(name_of("inner"), Some(file))],
        },
    ];
    let input = FilesystemInput {
        base: Some(FilesystemRootId(session.root)),
        scope: session.scope,
        root_serial: 1,
        resources: FilesystemResources {
            base_read_batch: 1,
            ..Default::default()
        },
        directories: &directories,
        inodes: &[],
        new_inodes: &[],
    };
    let rows = Rows(&input);
    let mut records = Records::default();
    let mut emitted = TreeStore::new();
    let actual = update_filesystem_streamed_backed(
        &mut FilesystemObjects::new(&session.store, &mut emitted),
        &prepared(&rows),
        &mut records,
        None,
    )
    .unwrap();
    let mut resident = TreeStore::new();
    let expected = update_filesystem(
        &mut FilesystemObjects::new(&session.store, &mut resident),
        &input,
        None,
    )
    .unwrap();
    assert_eq!((actual.root, actual.value), (expected.root, expected.value));
    session.store.absorb(&emitted);
    let mut read = FilesystemRead::new(&session.store, actual.root).unwrap();
    assert_eq!(
        read.resolve_child(1, &name_of("moved")).unwrap().serial,
        dir
    );
    assert_eq!(
        read.resolve_child(dir, &name_of("inner")).unwrap().serial,
        file
    );
    assert_eq!(
        read.resolve_inode(file).unwrap().value.namespace_ref_count,
        2
    );
    let marker = records
        .values
        .iter()
        .find(|(key, _)| key.kind == CONTEXT)
        .unwrap()
        .1;
    assert_eq!(&marker[..2], &[1, 1]);
    assert_eq!(&marker[2..34], session.root.as_bytes());
}

#[test]
fn disappearing_replayed_root_cannot_be_overwritten_by_a_supplied_directory_value() {
    let mut session = Session::new(1).unwrap();
    let file = session.allocate();
    session
        .apply(
            &[DirectoryUpdate {
                parent: 1,
                changes: vec![(name_of("file"), Some(file))],
            }],
            &[InodeUpdate {
                serial: file,
                value: typed(InodeKind::RegularFile),
            }],
            &[file],
        )
        .unwrap();
    let original = FilesystemRead::new(&session.store, FilesystemRootId(session.root))
        .unwrap()
        .resolve_inode(1)
        .unwrap()
        .value;
    let inodes = [InodeUpdate {
        serial: 1,
        value: original,
    }];
    let directories = [DirectoryUpdate {
        parent: 1,
        changes: vec![(name_of("linked"), Some(file))],
    }];
    let input = FilesystemInput {
        base: Some(FilesystemRootId(session.root)),
        scope: session.scope,
        root_serial: 1,
        resources: FilesystemResources::default(),
        directories: &directories,
        inodes: &inodes,
        new_inodes: &[],
    };
    let rows = Rows(&input);
    let mut records = Records {
        disappear_replayed_root: true,
        ..Default::default()
    };
    let mut sink = TreeStore::new();
    assert_eq!(
        update_filesystem_streamed_backed(
            &mut FilesystemObjects::new(&session.store, &mut sink),
            &prepared(&rows),
            &mut records,
            None,
        ),
        Err(ContentError::InvalidRecord(
            "filesystem rebuilt root missing"
        ))
    );
    assert_eq!(
        records.root_reads, 2,
        "original root was replayed before the missing lookup"
    );
    assert_eq!(records.after_failure, 0);
    assert!(
        sink.order()
            .iter()
            .any(|(_, role)| *role == ObjectRole::DirectoryLeaf),
        "accepted rebuilt directory remains in consumer custody"
    );
    assert!(!sink
        .order()
        .iter()
        .any(|(_, role)| *role == ObjectRole::FilesystemRoot));

    // No directory header means a legitimate supplied metadata/value update;
    // its existing authenticated content root is retained without a ROOT slot.
    let metadata = FilesystemInput {
        directories: &[],
        ..input
    };
    let rows = Rows(&metadata);
    let mut records = Records::default();
    let mut sink = TreeStore::new();
    let unchanged = update_filesystem_streamed_backed(
        &mut FilesystemObjects::new(&session.store, &mut sink),
        &prepared(&rows),
        &mut records,
        None,
    )
    .unwrap();
    assert_eq!(unchanged.root.0, session.root);
}

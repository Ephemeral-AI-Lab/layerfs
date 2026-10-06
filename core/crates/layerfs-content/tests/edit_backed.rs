//! Public indexed edit port tests; the fixture is external, not a product store.
mod support;
use layerfs_content::{
    apply_edits, apply_edits_backed, ConstructionPolicy, ContentError, ContentResult, Edit,
    EditRecordApply, EditRecordChange, EditRecordExpected, EditRecordKey, EditRequest,
    FinalizedConsumer, FinalizedObject, IndexedEditBacking, ObjectId, ObjectRole,
};
use std::{cell::Cell, collections::BTreeMap, rc::Rc};
use support::{
    build_file, disabled_scope,
    edits::{Edits, Parts},
    noise, read_back, MemoryStore,
};

#[derive(Default)]
struct Backing {
    values: BTreeMap<EditRecordKey, Vec<u8>>,
    calls: usize,
    calls_after_failure: usize,
    failed: bool,
    refuse_draft: bool,
    refuse_settle: bool,
    refuse_ack: bool,
    corrupt_draft: bool,
    max_batch_bytes: usize,
    max_value: usize,
    saw_none_exclusion: bool,
    refuse_file_ack: bool,
    external_failure: Option<Rc<Cell<bool>>>,
    corrupt_repeated_boundaries: bool,
    corrupted_parent: Option<EditRecordKey>,
    saw_three_repeated_page_children: bool,
}
impl Backing {
    fn enter(&mut self) -> ContentResult<()> {
        if self.failed
            || self
                .external_failure
                .as_ref()
                .is_some_and(|flag| flag.get())
        {
            self.calls_after_failure += 1;
            return Err(ContentError::ProviderFailure {
                what: "fixture already failed",
            });
        }
        self.calls += 1;
        Ok(())
    }
    fn refusal(&mut self) -> ContentError {
        self.failed = true;
        ContentError::ProviderFailure {
            what: "original external backing refusal",
        }
    }
    fn shift_repeated_boundary(value: &mut [u8]) -> bool {
        // Independent corruption of a version1 temporary branch. Leave its
        // first interval and final totals intact, shift an interior boundary
        // between three equal draft references, preserving strict ordering.
        if value.len() < 21 || value[..3] != [1, 1, 1] {
            return false;
        }
        let count = u16::from_be_bytes(value[19..21].try_into().unwrap()) as usize;
        if value.len() != 21 + count * 49 {
            return false;
        }
        for first in 0..count.saturating_sub(2) {
            let a = 21 + first * 49;
            let b = a + 49;
            let c = b + 49;
            if value[a + 16] != 0
                && value[a + 16..a + 49] == value[b + 16..b + 49]
                && value[b + 16..b + 49] == value[c + 16..c + 49]
            {
                let end = u64::from_be_bytes(value[b..b + 8].try_into().unwrap());
                let next = u64::from_be_bytes(value[c..c + 8].try_into().unwrap());
                if end + 1 < next {
                    value[b..b + 8].copy_from_slice(&(end + 1).to_be_bytes());
                    return true;
                }
            }
        }
        false
    }
}
impl IndexedEditBacking for Backing {
    fn contains(&mut self, key: EditRecordKey) -> ContentResult<bool> {
        self.enter()?;
        Ok(self.values.contains_key(&key))
    }
    fn get(&mut self, key: EditRecordKey) -> ContentResult<Option<Vec<u8>>> {
        self.enter()?;
        if self.corrupt_repeated_boundaries && self.corrupted_parent.is_none() && key.kind == 7 {
            let parent = EditRecordKey {
                kind: 1,
                key: key.key,
            };
            if self
                .values
                .get_mut(&parent)
                .is_some_and(|value| Self::shift_repeated_boundary(value))
            {
                self.corrupted_parent = Some(parent);
            }
        }
        if self.corrupt_draft && key.kind == 1 && self.values.contains_key(&key) {
            return Ok(Some(vec![255]));
        }
        Ok(self.values.get(&key).cloned())
    }
    fn apply(&mut self, changes: Vec<EditRecordChange>) -> ContentResult<EditRecordApply> {
        self.enter()?;
        assert!(changes.windows(2).all(|p| p[0].key < p[1].key));
        let bytes = changes.capacity() * std::mem::size_of::<EditRecordChange>()
            + changes
                .iter()
                .map(|c| {
                    let old = match &c.expected {
                        EditRecordExpected::Missing => 0,
                        EditRecordExpected::ExactBytes(v) => v.capacity(),
                    };
                    old + c.value.as_ref().map_or(0, Vec::capacity)
                })
                .sum::<usize>();
        assert!(bytes <= 65_536);
        self.max_batch_bytes = self.max_batch_bytes.max(bytes);
        if self.refuse_draft && changes.iter().any(|c| matches!(c.key.kind, 1 | 2)) {
            return Err(self.refusal());
        }
        if self.refuse_ack
            && changes
                .iter()
                .any(|c| c.key.kind == 9 && c.value.as_deref() == Some(&[1]))
        {
            return Err(self.refusal());
        }
        if self.refuse_file_ack
            && changes.iter().any(|c| {
                c.key.kind == 10
                    && c.value.as_deref() == Some(&[1])
                    && matches!(&c.expected, EditRecordExpected::ExactBytes(v) if v == &[2])
            })
        {
            return Err(self.refusal());
        }
        for (index, change) in changes.iter().enumerate() {
            let actual = self.values.get(&change.key);
            let valid = match (&change.expected, actual) {
                (EditRecordExpected::Missing, None) => true,
                (EditRecordExpected::ExactBytes(a), Some(b)) => a == b,
                _ => false,
            };
            if !valid {
                self.failed = true;
                return Ok(EditRecordApply::NotApplied {
                    index,
                    key: change.key,
                    actual: actual.cloned(),
                });
            }
        }
        for change in changes {
            if let Some(value) = change.value {
                if change.key.kind == 2 && value.len() >= 8 {
                    let length = u16::from_be_bytes(value[3..5].try_into().unwrap()) as usize;
                    if let Some(canonical) = value.get(8..8 + length) {
                        if let layerfs_content::file::mapping::ExtentNode::Branch {
                            children, ..
                        } = layerfs_content::file::mapping::decode_node(canonical).unwrap()
                        {
                            self.saw_three_repeated_page_children |= children.len() == 3
                                && children.iter().all(|child| {
                                    child.child_object_id == children[0].child_object_id
                                });
                        }
                    }
                }
                self.max_value = self.max_value.max(value.len());
                self.values.insert(change.key, value);
            } else {
                self.values.remove(&change.key);
            }
        }
        Ok(EditRecordApply::Applied)
    }
    fn first_keys(
        &mut self,
        kind: u32,
        excluded: Option<[u8; 32]>,
    ) -> ContentResult<Vec<[u8; 32]>> {
        self.enter()?;
        self.saw_none_exclusion |= excluded.is_none();
        if self.refuse_settle {
            return Err(self.refusal());
        }
        Ok(self
            .values
            .range(
                EditRecordKey { kind, key: [0; 32] }..=EditRecordKey {
                    kind,
                    key: [255; 32],
                },
            )
            .filter_map(|(k, _)| (Some(k.key) != excluded).then_some(k.key))
            .take(64)
            .collect())
    }
}
fn run(
    store: &MemoryStore,
    root: ObjectId,
    edits: &Edits,
    source: &Parts,
    consumer: &mut dyn FinalizedConsumer,
    backing: &mut Backing,
) -> ContentResult<layerfs_content::ConstructedFile> {
    let policy = ConstructionPolicy::frozen_default();
    disabled_scope(|scope| {
        apply_edits_backed(
            policy,
            &policy.capacities(),
            store,
            EditRequest {
                root,
                edits,
                source,
            },
            consumer,
            backing,
            scope.child("backed"),
        )
    })
}
fn case() -> (Vec<u8>, MemoryStore, ObjectId, Edits, Parts) {
    let base = noise(2 * 1024 * 1024 + 71);
    let (store, built) = build_file(&base);
    let edits = Edits::new(
        base.len() as u64,
        vec![
            Edit::insert(19_137, 60_003),
            Edit::overwrite(811_111, 820_111),
            Edit::delete(1_377_777, 1_387_777),
        ],
    )
    .unwrap();
    let mut source = Parts::new();
    source.push(noise(60_003));
    source.push(vec![0x31; 9000]);
    source.push(vec![]);
    (base, store, built.root, edits, source)
}

#[test]
fn backed_and_memory_routes_share_canonical_result_and_end_with_acknowledged_state() {
    let (base, store, root, edits, source) = case();
    let policy = ConstructionPolicy::frozen_default();
    let mut memory = MemoryStore::new();
    let original = disabled_scope(|scope| {
        apply_edits(
            policy,
            &policy.capacities(),
            &store,
            EditRequest {
                root,
                edits: &edits,
                source: &source,
            },
            &mut memory,
            scope.child("memory"),
        )
    })
    .unwrap();
    let mut backing = Backing::default();
    let mut indexed = MemoryStore::new();
    let result = run(&store, root, &edits, &source, &mut indexed, &mut backing).unwrap();
    assert_eq!(
        (result.root, result.logical_len, result.counters),
        (original.root, original.logical_len, original.counters)
    );
    let mut merged = store.merged_clone();
    merged.absorb(&indexed);
    let mut expected = base;
    expected.splice(19_137..19_137, noise(60_003));
    expected.splice(811_111..820_111, vec![0x31; 9000]);
    expected.drain(1_377_777..1_387_777);
    assert_eq!(read_back(&merged, result.root).unwrap(), expected);
    assert!(backing.saw_none_exclusion);
    assert!(backing.max_value < 16 * 1024 && backing.max_batch_bytes <= 65_536);
    assert!(backing.values.keys().all(|k| !matches!(k.kind, 1..=6)));
    assert!(backing
        .values
        .iter()
        .filter(|(k, _)| k.kind == 9)
        .all(|(_, value)| value == &vec![1]));
    assert_eq!(
        backing.values.get(&EditRecordKey {
            kind: 10,
            key: [0; 32]
        }),
        Some(&vec![1])
    );
    assert_eq!(backing.calls_after_failure, 0);
}

#[test]
fn original_backing_discard_settle_and_post_accept_failure_end_without_another_job() {
    let (_, store, root, edits, source) = case();
    for mode in 0..3 {
        let mut backing = Backing {
            refuse_draft: mode == 0,
            refuse_settle: mode == 1,
            refuse_ack: mode == 2,
            ..Default::default()
        };
        let mut consumer = MemoryStore::new();
        let error = run(&store, root, &edits, &source, &mut consumer, &mut backing).unwrap_err();
        assert_eq!(
            error,
            ContentError::ProviderFailure {
                what: "original external backing refusal"
            }
        );
        assert_eq!(backing.calls_after_failure, 0);
        if mode == 2 {
            assert!(consumer
                .roles()
                .iter()
                .any(|r| matches!(r, ObjectRole::ExtentLeaf | ObjectRole::ExtentBranch)));
            assert!(backing
                .values
                .iter()
                .any(|(k, value)| k.kind == 9 && value == &vec![0]));
            assert!(!consumer.roles().contains(&ObjectRole::FileState));
        }
    }
}

#[test]
fn malformed_mapping_record_and_reused_file_scope_never_fall_back_to_stored_objects() {
    let (_, store, root, edits, source) = case();
    let mut malformed = Backing {
        corrupt_draft: true,
        ..Default::default()
    };
    let mut emitted = MemoryStore::new();
    assert!(run(&store, root, &edits, &source, &mut emitted, &mut malformed).is_err());
    assert_eq!(malformed.calls_after_failure, 0);
    let mut backing = Backing::default();
    let mut emitted = MemoryStore::new();
    run(&store, root, &edits, &source, &mut emitted, &mut backing).unwrap();
    let mut second = MemoryStore::new();
    assert_eq!(
        run(&store, root, &edits, &source, &mut second, &mut backing).unwrap_err(),
        ContentError::ProviderFailure {
            what: "edit backing precondition"
        }
    );
    assert!(second.is_empty());
    assert_eq!(backing.calls_after_failure, 0);
}

#[test]
fn original_consumer_refusal_leaves_attempt_marker_and_no_file_state_or_acknowledgement() {
    struct Refuse {
        attempts: usize,
        chunks: MemoryStore,
    }
    impl FinalizedConsumer for Refuse {
        fn accept(&mut self, object: FinalizedObject) -> ContentResult<()> {
            if matches!(
                object.role(),
                ObjectRole::ExtentLeaf | ObjectRole::ExtentBranch
            ) {
                self.attempts += 1;
                return Err(ContentError::OutputRejected);
            }
            self.chunks.accept(object)
        }
    }
    let (_, store, root, edits, source) = case();
    let mut backing = Backing::default();
    let mut consumer = Refuse {
        attempts: 0,
        chunks: MemoryStore::new(),
    };
    assert_eq!(
        run(&store, root, &edits, &source, &mut consumer, &mut backing).unwrap_err(),
        ContentError::OutputRejected
    );
    assert_eq!(consumer.attempts, 1);
    assert_eq!(backing.calls_after_failure, 0);
    assert!(backing
        .values
        .iter()
        .any(|(k, value)| k.kind == 9 && value == &vec![0]));
    assert!(!consumer.chunks.roles().contains(&ObjectRole::FileState));
}

#[test]
fn repeated_page_dag_reuses_real_stored_identity_without_conflating_private_drafts() {
    use layerfs_content::file::mapping::{emit_file_state, ExtentBuilder};
    let policy = ConstructionPolicy::frozen_default();
    let mut base = MemoryStore::new();
    let mut builder = ExtentBuilder::new(&policy.capacities());
    builder
        .push_repeated_chunk(&[0; 32_768], 1024, &mut base)
        .unwrap();
    let mapping = builder.finish(&mut base).unwrap().root.unwrap();
    let root = emit_file_state(&mut base, mapping).unwrap();
    let len = 32 * 1024 * 1024u64;
    let insert = 16 * 1024 * 1024usize;
    let edits = Edits::new(
        len,
        vec![
            Edit::insert(128 * 32_768, insert as u64),
            Edit::overwrite(len + insert as u64 - 1, len + insert as u64),
        ],
    )
    .unwrap();
    let mut source = Parts::new();
    source.push(vec![0; insert]);
    source.push(vec![0x77]);
    let mut backing = Backing::default();
    let mut indexed = MemoryStore::new();
    let result = run(&base, root, &edits, &source, &mut indexed, &mut backing).unwrap();
    let mut memory = MemoryStore::new();
    let original = disabled_scope(|scope| {
        apply_edits(
            policy,
            &policy.capacities(),
            &base,
            EditRequest {
                root,
                edits: &edits,
                source: &source,
            },
            &mut memory,
            scope.child("memory"),
        )
    })
    .unwrap();
    assert_eq!(result.root, original.root);
    assert_eq!(result.logical_len, len + insert as u64);
    // A constructor-produced Page draft and an immutable stored page naturally
    // have the same full canonical identity. Their provenance remains distinct.
    assert!(backing.values.keys().filter(|k| k.kind == 8).any(|k| base
        .canonical(ObjectId::from_bytes(&k.key).unwrap())
        .is_some()));
    let mut merged = base.merged_clone();
    merged.absorb(&indexed);
    let mut tail = Vec::new();
    disabled_scope(|scope| {
        layerfs_content::read_range(
            &merged,
            result.root,
            result.logical_len - 2..result.logical_len,
            &mut tail,
            scope.child("tail"),
        )
    })
    .unwrap();
    assert_eq!(tail, vec![0, 0x77]);
    assert!(backing.values.keys().all(|k| !matches!(k.kind, 1..=6)));
}

#[test]
fn three_repeated_replacement_pages_survive_both_boundary_joins_and_a_later_split() {
    use layerfs_content::file::mapping::{emit_file_state, ExtentBuilder};
    let policy = ConstructionPolicy::frozen_default();
    let mut base = MemoryStore::new();
    let mut builder = ExtentBuilder::new(&policy.capacities());
    builder
        .push_repeated_chunk(&[0; 32_768], 1024, &mut base)
        .unwrap();
    let mapping = builder.finish(&mut base).unwrap().root.unwrap();
    let root = emit_file_state(&mut base, mapping).unwrap();
    let page_bytes = 128 * 32_768u64;
    let insert = 3 * page_bytes;
    let base_len = 32 * 1024 * 1024u64;
    let edits = Edits::new(
        base_len,
        vec![
            Edit::insert(page_bytes, insert),
            Edit::overwrite(page_bytes + insert + 1, page_bytes + insert + 2),
        ],
    )
    .unwrap();
    let mut source = Parts::new();
    source.push(vec![0; insert as usize]);
    source.push(vec![0x77]);
    let mut backing = Backing::default();
    let mut indexed = MemoryStore::new();
    let result = run(&base, root, &edits, &source, &mut indexed, &mut backing).unwrap();
    assert!(
        backing.saw_three_repeated_page_children,
        "the replacement builder must own a three-child branch with one repeated Page identity"
    );
    let mut memory = MemoryStore::new();
    let original = disabled_scope(|scope| {
        apply_edits(
            policy,
            &policy.capacities(),
            &base,
            EditRequest {
                root,
                edits: &edits,
                source: &source,
            },
            &mut memory,
            scope.child("memory-three-pages"),
        )
    })
    .unwrap();
    assert_eq!(result.root, original.root);
    assert_eq!(result.logical_len, base_len + insert);
    let mut merged = base.merged_clone();
    merged.absorb(&indexed);
    let mut boundary = Vec::new();
    disabled_scope(|scope| {
        layerfs_content::read_range(
            &merged,
            result.root,
            page_bytes + insert - 1..page_bytes + insert + 3,
            &mut boundary,
            scope.child("three-page-boundary"),
        )
    })
    .unwrap();
    assert_eq!(boundary, vec![0, 0, 0x77, 0]);
    assert!(backing.values.keys().all(|key| !matches!(key.kind, 1..=6)));
}

#[test]
fn no_op_and_whole_file_transition_keep_the_existing_dispatch_without_backing_effects() {
    let (base, store, root, _, _) = case();
    let edits = Edits::new(base.len() as u64, vec![Edit::overwrite(0, 5)]).unwrap();
    let mut source = Parts::new();
    source.push(base[..5].to_vec());
    let mut backing = Backing::default();
    let mut emitted = MemoryStore::new();
    assert_eq!(
        run(&store, root, &edits, &source, &mut emitted, &mut backing)
            .unwrap()
            .root,
        root
    );
    assert_eq!(backing.calls, 0);
    assert!(emitted.is_empty());
    let edits = Edits::new(base.len() as u64, vec![Edit::delete(31, base.len() as u64)]).unwrap();
    let mut source = Parts::new();
    source.push(vec![]);
    let result = run(&store, root, &edits, &source, &mut emitted, &mut backing).unwrap();
    assert_eq!(read_back(&emitted, result.root).unwrap(), base[..31]);
    assert_eq!(backing.calls, 0);
    assert_eq!(emitted.roles(), vec![ObjectRole::WholeFile]);
}

#[test]
fn resolved_repeated_child_cannot_hide_a_shifted_interior_mapping_boundary() {
    use layerfs_content::file::mapping::{emit_file_state, ExtentBuilder};
    use layerfs_content::{EditSequence, EditSource};
    struct Insert {
        base: u64,
        length: u64,
    }
    impl EditSequence for Insert {
        fn base_len(&self) -> u64 {
            self.base
        }
        fn final_len(&self) -> u64 {
            self.base + self.length
        }
        fn len(&self) -> usize {
            1
        }
        fn edit_at(&self, index: usize) -> ContentResult<Edit> {
            if index == 0 {
                Ok(Edit::insert(4 * 1024 * 1024, self.length))
            } else {
                Err(ContentError::InvalidEdit { what: "index" })
            }
        }
    }
    impl EditSource for Insert {
        fn replacement_len(&self, _: usize) -> u64 {
            self.length
        }
        fn read_at(&self, _: usize, offset: u64, output: &mut [u8]) -> ContentResult<usize> {
            let count = output
                .len()
                .min(self.length.saturating_sub(offset) as usize);
            output[..count].fill(0);
            Ok(count)
        }
    }
    let policy = ConstructionPolicy::frozen_default();
    let mut base = MemoryStore::new();
    let mut builder = ExtentBuilder::new(&policy.capacities());
    builder
        .push_repeated_chunk(&[0; 32_768], 1024, &mut base)
        .unwrap();
    let mapping = builder.finish(&mut base).unwrap().root.unwrap();
    let root = emit_file_state(&mut base, mapping).unwrap();
    let input = Insert {
        base: 32 * 1024 * 1024,
        length: 64 * 1024 * 1024,
    };
    let mut backing = Backing {
        corrupt_repeated_boundaries: true,
        ..Default::default()
    };
    let mut consumer = MemoryStore::new();
    let error = disabled_scope(|scope| {
        apply_edits_backed(
            policy,
            &policy.capacities(),
            &base,
            EditRequest {
                root,
                edits: &input,
                source: &input,
            },
            &mut consumer,
            &mut backing,
            scope.child("corrupted-repeat"),
        )
    })
    .unwrap_err();
    assert!(
        backing.corrupted_parent.is_some(),
        "fixture must shift an interior repeated draft boundary"
    );
    assert_eq!(
        error,
        ContentError::InvalidRecord("resolved mapping summary")
    );
    assert!(!consumer.roles().contains(&ObjectRole::ExtentBranch));
    assert!(!consumer.roles().contains(&ObjectRole::FileState));
    assert!(
        consumer.roles().contains(&ObjectRole::ExtentLeaf),
        "the first valid repeated interval resolves before the deciding mismatch"
    );
    assert_eq!(backing.calls_after_failure, 0);
}

#[test]
fn file_state_refusal_and_post_accept_ack_refusal_retain_one_pending_attempt() {
    struct FileConsumer {
        fail: bool,
        attempts: usize,
        terminal: Rc<Cell<bool>>,
        original_refused: Option<FinalizedObject>,
        accepted: MemoryStore,
    }
    impl FinalizedConsumer for FileConsumer {
        fn accept(&mut self, object: FinalizedObject) -> ContentResult<()> {
            if object.role() == ObjectRole::FileState {
                self.attempts += 1;
                if self.fail {
                    self.original_refused = Some(object);
                    self.terminal.set(true);
                    return Err(ContentError::OutputRejected);
                }
            }
            self.accepted.accept(object)
        }
    }
    let (_, store, root, edits, source) = case();
    for consumer_refuses in [true, false] {
        let terminal = Rc::new(Cell::new(false));
        let mut backing = Backing {
            refuse_file_ack: !consumer_refuses,
            external_failure: Some(terminal.clone()),
            ..Default::default()
        };
        let mut consumer = FileConsumer {
            fail: consumer_refuses,
            attempts: 0,
            terminal,
            original_refused: None,
            accepted: MemoryStore::new(),
        };
        let error = run(&store, root, &edits, &source, &mut consumer, &mut backing).unwrap_err();
        assert_eq!(
            error,
            if consumer_refuses {
                ContentError::OutputRejected
            } else {
                ContentError::ProviderFailure {
                    what: "original external backing refusal",
                }
            }
        );
        assert_eq!(consumer.attempts, 1);
        assert_eq!(
            backing.values.get(&EditRecordKey {
                kind: 10,
                key: [0; 32]
            }),
            Some(&vec![2])
        );
        assert_eq!(backing.calls_after_failure, 0);
        if consumer_refuses {
            assert_eq!(
                consumer.original_refused.as_ref().unwrap().role(),
                ObjectRole::FileState
            );
            assert!(!consumer.accepted.roles().contains(&ObjectRole::FileState));
        } else {
            assert_eq!(
                consumer
                    .accepted
                    .roles()
                    .iter()
                    .filter(|r| **r == ObjectRole::FileState)
                    .count(),
                1
            );
        }
    }
}

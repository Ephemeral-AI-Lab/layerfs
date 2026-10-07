//! Real public edits with stable sparse runs, authenticated evidence and errors.
mod support;
use layerfs_content::file::mapping::{
    emit_file_state, encode_chunk_object, encode_node, ChildDescriptor, ExtentBuilder, ExtentNode,
    ExtentSlice, NodeSummary,
};
use layerfs_content::{
    apply_edits, apply_edits_backed, AuthenticatedObjects, ConstructionPolicy, ContentError,
    ContentResult, Edit, EditRecordKey, EditRequest, EditSource, FileRun, FinalizedConsumer,
    FinalizedObject, ObjectId, ObjectRole, PredecessorProvenance,
};
use std::cell::Cell;
use support::edit_runs::{Backing, Counted, Piece, Sparse};
use support::{build_file, disabled_scope, edits::Edits, noise, read_back, MemoryStore};

fn run(
    reader: &dyn AuthenticatedObjects,
    root: ObjectId,
    edits: &Edits,
    source: &dyn EditSource,
    consumer: &mut dyn FinalizedConsumer,
    backing: Option<&mut Backing>,
) -> ContentResult<layerfs_content::ConstructedFile> {
    let policy = ConstructionPolicy::frozen_default();
    disabled_scope(|scope| {
        let request = EditRequest {
            root,
            edits,
            source,
        };
        match backing {
            Some(backing) => apply_edits_backed(
                policy,
                &policy.capacities(),
                reader,
                request,
                consumer,
                backing,
                scope.child("run-edit"),
            ),
            None => apply_edits(
                policy,
                &policy.capacities(),
                reader,
                request,
                consumer,
                scope.child("run-edit"),
            ),
        }
    })
}
fn zero_file(chunks: u64) -> (MemoryStore, ObjectId, u64) {
    let policy = ConstructionPolicy::frozen_default();
    let mut store = MemoryStore::new();
    let mut builder = ExtentBuilder::new(&policy.capacities());
    builder
        .push_repeated_chunk(&[0; 32_768], chunks, &mut store)
        .unwrap();
    let mapping = builder.finish(&mut store).unwrap().root.unwrap();
    let root = emit_file_state(&mut store, mapping).unwrap();
    (store, root, chunks * 32_768)
}
fn range(store: &MemoryStore, root: ObjectId, start: u64, end: u64) -> Vec<u8> {
    let mut output = Vec::new();
    disabled_scope(|scope| {
        layerfs_content::read_range(store, root, start..end, &mut output, scope.child("oracle"))
    })
    .unwrap();
    output
}

#[test]
fn mixed_runs_keep_the_byte_route_root_across_localized_and_representation_transitions() {
    for base_len in [31_337, 2 * 1024 * 1024 + 19] {
        let base = noise(base_len);
        let (store, built) = build_file(&base);
        let sparse = Sparse::one(vec![
            Piece::Data(noise(8_193)),
            Piece::Zero(211_117),
            Piece::Data(noise(13)),
        ]);
        let replacement_len = sparse.replacement_len(0);
        let edits = Edits::new(base_len as u64, vec![Edit::new(11, 29, replacement_len)]).unwrap();
        let dense = sparse.materialize();
        let mut bytes = MemoryStore::new();
        let original = run(&store, built.root, &edits, &dense, &mut bytes, None).unwrap();
        for backed in [false, true] {
            let mut output = MemoryStore::new();
            let mut backing = Backing::default();
            let result = run(
                &store,
                built.root,
                &edits,
                &sparse,
                &mut output,
                backed.then_some(&mut backing),
            )
            .unwrap();
            assert_eq!(result.root, original.root);
            let mut merged = store.merged_clone();
            merged.absorb(&output);
            let mut expected = base[..11].to_vec();
            expected.extend_from_slice(&noise(8_193));
            expected.resize(expected.len() + 211_117, 0);
            expected.extend_from_slice(&noise(13));
            expected.extend_from_slice(&base[29..]);
            assert_eq!(read_back(&merged, result.root).unwrap(), expected);
        }
    }
    // Chunked -> WholeFile takes only the final bounded allocation and the run
    // port. Zero-only WholeFile no-op also stays on the original base root.
    let base = noise(512 * 1024);
    let (store, built) = build_file(&base);
    let sparse = Sparse::one(vec![Piece::Zero(31)]);
    let edits = Edits::new(base.len() as u64, vec![Edit::new(0, base.len() as u64, 31)]).unwrap();
    let mut output = MemoryStore::new();
    let mut backing = Backing {
        forbidden: true,
        ..Backing::default()
    };
    let result = run(
        &store,
        built.root,
        &edits,
        &sparse,
        &mut output,
        Some(&mut backing),
    )
    .unwrap();
    assert_eq!(output.roles(), vec![ObjectRole::WholeFile]);
    assert_eq!(read_back(&output, result.root).unwrap(), vec![0; 31]);
    let same = Edits::new(31, vec![Edit::overwrite(0, 31)]).unwrap();
    let mut none = MemoryStore::new();
    assert_eq!(
        run(
            &output,
            result.root,
            &same,
            &sparse,
            &mut none,
            Some(&mut backing)
        )
        .unwrap()
        .root,
        result.root
    );
    assert!(none.is_empty());
    let empty = Edits::new(31, vec![Edit::delete(0, 31)]).unwrap();
    let empty_source = Sparse::one(Vec::new());
    let mut empty_output = MemoryStore::new();
    let empty_result = run(
        &output,
        result.root,
        &empty,
        &empty_source,
        &mut empty_output,
        Some(&mut backing),
    )
    .unwrap();
    assert_eq!(empty_result.logical_len, 0);
    assert!(read_back(&empty_output, empty_result.root)
        .unwrap()
        .is_empty());
    assert_eq!(empty_source.calls.get(), 0);
}

#[test]
fn a_zero_replacement_above_four_gib_keeps_bounded_calls_and_exact_boundary_bytes() {
    let base = noise(512 * 1024 + 11);
    let (store, built) = build_file(&base);
    let length = (1u64 << 32) + 17;
    let sparse = Sparse::one(vec![Piece::Zero(length)]);
    let edits = Edits::new(base.len() as u64, vec![Edit::insert(19, length)]).unwrap();
    let mut roots = Vec::new();
    for backed in [false, true] {
        let before = sparse.calls.get();
        let mut backing = Backing::default();
        let mut output = MemoryStore::new();
        let result = run(
            &store,
            built.root,
            &edits,
            &sparse,
            &mut output,
            backed.then_some(&mut backing),
        )
        .unwrap();
        roots.push(result.root);
        assert_eq!(result.logical_len, base.len() as u64 + length);
        assert_eq!(
            sparse.calls.get() - before,
            1,
            "one explicit logical zero run"
        );
        assert_eq!(sparse.data_bytes.get(), 0);
        assert!(
            output.len() <= 64,
            "real accepted distinct objects, not logical bytes"
        );
        assert!(
            output.order().len() <= 96,
            "actual acceptance attempts stay bounded for this repeated fixture"
        );
        assert!(backing.max_bytes <= 65_536);
        let mut merged = store.merged_clone();
        merged.absorb(&output);
        assert_eq!(
            range(&merged, result.root, 17, 22),
            [&base[17..19], &[0; 3]].concat()
        );
        assert_eq!(
            range(&merged, result.root, 19 + length / 2, 24 + length / 2),
            vec![0; 5]
        );
        assert_eq!(
            range(&merged, result.root, 19 + length - 2, 19 + length + 3),
            [&[0; 2][..], &base[19..22]].concat()
        );
    }
    assert_eq!(roots[0], roots[1]);
}

#[test]
fn a_large_zero_no_op_proves_real_repeated_pages_without_backing_or_byte_loops() {
    let (store, root, length) = zero_file((1u64 << 32) / 32_768 + 1);
    let provider = Counted::new(&store);
    let source = Sparse::one(vec![Piece::Zero(length)]);
    let edits = Edits::new(length, vec![Edit::overwrite(0, length)]).unwrap();
    let mut output = MemoryStore::new();
    let mut backing = Backing {
        forbidden: true,
        ..Backing::default()
    };
    let result = run(
        &provider,
        root,
        &edits,
        &source,
        &mut output,
        Some(&mut backing),
    )
    .unwrap();
    assert_eq!(result.root, root);
    assert_eq!(source.calls.get(), 1);
    assert_eq!(source.zero_bytes.get(), length);
    assert_eq!(source.data_bytes.get(), 0);
    assert!(output.is_empty());
    assert_eq!(backing.calls, 0);
    let requests = provider.requests.borrow();
    assert!(
        requests.len() <= 16,
        "actual authenticated demands: {}",
        requests.len()
    );
    assert!(requests.iter().any(|id| store
        .canonical(*id)
        .is_some_and(|canonical| layerfs_content::file::mapping::decode_node(canonical).is_ok())));
    assert!(requests
        .iter()
        .any(|id| store.canonical(*id).is_some_and(|canonical| {
            layerfs_content::object::decode_bytes_object(canonical)
                .ok()
                .and_then(|value| layerfs_content::file::mapping::decode_chunk_payload(value).ok())
                .is_some()
        })));
}

#[test]
fn a_partial_zero_payload_never_certifies_its_nonzero_repeated_slice() {
    let policy = ConstructionPolicy::frozen_default();
    let mut base = MemoryStore::new();
    let mut payload = vec![0; 32_768];
    payload[16_384..].fill(0x39);
    let mut builder = ExtentBuilder::new(&policy.capacities());
    let chunk = builder
        .push_repeated_chunk(&payload, 128, &mut base)
        .unwrap()
        .unwrap();
    let mapping = builder.finish(&mut base).unwrap().root.unwrap();
    let root = emit_file_state(&mut base, mapping).unwrap();
    let provider = Counted::new(&base);
    let source = Sparse {
        parts: vec![vec![Piece::Zero(1024)], vec![Piece::Zero(1)]],
        ..Sparse::default()
    };
    let edits = Edits::new(
        128 * 32_768,
        vec![
            Edit::overwrite(0, 1024),
            Edit::overwrite(32_768 + 16_384, 32_768 + 16_385),
        ],
    )
    .unwrap();
    let mut output = MemoryStore::new();
    let result = run(&provider, root, &edits, &source, &mut output, None).unwrap();
    assert_ne!(result.root, root);
    assert_eq!(
        provider
            .requests
            .borrow()
            .iter()
            .filter(|id| **id == chunk)
            .count(),
        2,
        "the first partial zero comparison supplies no whole-payload witness"
    );
    let mut merged = base.merged_clone();
    merged.absorb(&output);
    assert_eq!(
        range(&merged, result.root, 32_768 + 16_383, 32_768 + 16_386),
        vec![0, 0, 0x39]
    );
}

#[test]
fn zero_evidence_eviction_handles_more_than_sixty_four_distinct_payloads() {
    let mut store = MemoryStore::new();
    let mut extents = Vec::new();
    let mut bytes = 0u64;
    let mut payload_ids = Vec::new();
    for length in 8_192..8_320usize {
        let chunk = FinalizedObject::new(
            ObjectRole::Chunk,
            encode_chunk_object(&vec![0; length]).unwrap(),
        )
        .unwrap();
        let id = chunk.id();
        payload_ids.push(id);
        store.accept(chunk).unwrap();
        extents.push(ExtentSlice::new(id, 0, length as u32).unwrap());
        bytes += length as u64;
    }
    let leaf = FinalizedObject::new(
        ObjectRole::ExtentLeaf,
        encode_node(
            &ExtentNode::Leaf {
                subtree_logical_bytes: bytes,
                extents,
            },
            false,
        )
        .unwrap(),
    )
    .unwrap();
    let leaf_id = leaf.id();
    store.accept(leaf).unwrap();
    let children = (1..=3)
        .map(|index| ChildDescriptor {
            cumulative_logical_end: bytes * index,
            cumulative_extent_end: 128 * index,
            child_object_id: leaf_id,
        })
        .collect();
    let branch = FinalizedObject::new(
        ObjectRole::ExtentBranch,
        encode_node(
            &ExtentNode::Branch {
                level: 1,
                subtree_logical_bytes: bytes * 3,
                subtree_extent_count: 384,
                children,
            },
            true,
        )
        .unwrap(),
    )
    .unwrap();
    let id = branch.id();
    store.accept(branch).unwrap();
    let root = emit_file_state(
        &mut store,
        NodeSummary {
            id,
            bytes: bytes * 3,
            extents: 384,
            level: 1,
        },
    )
    .unwrap();
    let provider = Counted::new(&store);
    let source = Sparse::one(vec![Piece::Zero(bytes * 3)]);
    let edits = Edits::new(bytes * 3, vec![Edit::overwrite(0, bytes * 3)]).unwrap();
    let mut output = MemoryStore::new();
    assert_eq!(
        run(&provider, root, &edits, &source, &mut output, None)
            .unwrap()
            .root,
        root
    );
    let requests = provider.requests.borrow();
    assert_eq!(
        requests
            .iter()
            .filter(|id| payload_ids.contains(id))
            .count(),
        128
    );
    assert_eq!(
        requests.iter().filter(|id| **id == leaf_id).count(),
        1,
        "the fully validated leaf can be reused after bounded payload-memo eviction"
    );
    assert!(output.is_empty());
}

#[test]
fn zero_witness_reuse_rejects_changed_summary_and_underfilled_child_context() {
    for underfilled in [false, true] {
        let mut store = MemoryStore::new();
        let chunk = FinalizedObject::new(
            ObjectRole::Chunk,
            encode_chunk_object(&[0; 32_768]).unwrap(),
        )
        .unwrap();
        let chunk_id = chunk.id();
        store.accept(chunk).unwrap();
        let count = if underfilled { 32 } else { 64 };
        let bytes = count as u64 * 32_768;
        let leaf = FinalizedObject::new(
            ObjectRole::ExtentLeaf,
            encode_node(
                &ExtentNode::Leaf {
                    subtree_logical_bytes: bytes,
                    extents: vec![ExtentSlice::new(chunk_id, 0, 32_768).unwrap(); count],
                },
                true,
            )
            .unwrap(),
        )
        .unwrap();
        let leaf_id = leaf.id();
        store.accept(leaf).unwrap();
        let mut children = Vec::new();
        for index in 1..=3 {
            children.push(ChildDescriptor {
                cumulative_logical_end: bytes * index + u64::from(!underfilled && index == 2),
                cumulative_extent_end: count as u64 * index,
                child_object_id: leaf_id,
            });
        }
        let node = ExtentNode::Branch {
            level: 1,
            subtree_logical_bytes: bytes * 3,
            subtree_extent_count: count as u64 * 3,
            children,
        };
        // Canonical individual bodies, deliberately invalid referenced context.
        let branch =
            FinalizedObject::new(ObjectRole::ExtentBranch, encode_node(&node, true).unwrap())
                .unwrap();
        let id = branch.id();
        store.accept(branch).unwrap();
        let root = emit_file_state(
            &mut store,
            NodeSummary {
                id,
                bytes: bytes * 3,
                extents: count as u64 * 3,
                level: 1,
            },
        )
        .unwrap();
        let length = bytes * 3;
        let source = Sparse::one(vec![Piece::Zero(length)]);
        let edits = Edits::new(length, vec![Edit::overwrite(0, length)]).unwrap();
        let mut output = MemoryStore::new();
        let mut backing = Backing {
            forbidden: true,
            ..Backing::default()
        };
        let error = run(
            &store,
            root,
            &edits,
            &source,
            &mut output,
            Some(&mut backing),
        )
        .unwrap_err();
        assert_eq!(
            error,
            if underfilled {
                ContentError::NonCanonicalPagePartition
            } else {
                ContentError::InvalidRecord("zero mapping witness")
            }
        );
        assert!(output.is_empty());
        assert_eq!(backing.calls, 0);
    }
}

#[test]
fn original_zero_evidence_refusal_and_absence_end_without_construction_or_fallback() {
    let (store, root, length) = zero_file(256);
    let chunk = store
        .order()
        .iter()
        .find(|(_, role)| *role == ObjectRole::Chunk)
        .unwrap()
        .0;
    for absent in [false, true] {
        let provider = Counted::new(&store);
        *provider.deny.borrow_mut() = Some((
            chunk,
            if absent {
                ContentError::MissingObject
            } else {
                ContentError::ProviderFailure {
                    what: "original zero evidence denial",
                }
            },
        ));
        let source = Sparse::one(vec![Piece::Zero(length)]);
        let edits = Edits::new(length, vec![Edit::overwrite(0, length)]).unwrap();
        let mut output = MemoryStore::new();
        let mut backing = Backing {
            forbidden: true,
            ..Backing::default()
        };
        assert_eq!(
            run(
                &provider,
                root,
                &edits,
                &source,
                &mut output,
                Some(&mut backing)
            )
            .unwrap_err(),
            if absent {
                ContentError::MissingObject
            } else {
                ContentError::ProviderFailure {
                    what: "original zero evidence denial",
                }
            }
        );
        assert_eq!(provider.denied.get(), 1);
        assert_eq!(provider.after_error.get(), 0);
        assert!(output.is_empty());
    }
}

#[test]
fn malformed_runs_and_original_late_source_failure_preserve_one_attempt() {
    struct Bad {
        kind: usize,
        calls: Cell<usize>,
    }
    impl EditSource for Bad {
        fn replacement_len(&self, _: usize) -> u64 {
            600_000
        }
        fn read_at(&self, _: usize, _: u64, _: &mut [u8]) -> ContentResult<usize> {
            panic!("run override")
        }
        fn read_run_at(&self, _: usize, _: u64, output: &mut [u8]) -> ContentResult<FileRun> {
            self.calls.set(self.calls.get() + 1);
            Ok(match self.kind {
                0 => FileRun::Zero(0),
                1 => FileRun::Zero(600_001),
                2 => FileRun::Data(0),
                3 => FileRun::Data(output.len() + 1),
                _ => FileRun::End,
            })
        }
    }
    let base = noise(512 * 1024);
    let (store, built) = build_file(&base);
    let edits = Edits::new(base.len() as u64, vec![Edit::insert(0, 600_000)]).unwrap();
    for kind in 0..5 {
        let source = Bad {
            kind,
            calls: Cell::new(0),
        };
        let mut output = MemoryStore::new();
        assert_eq!(
            run(&store, built.root, &edits, &source, &mut output, None).unwrap_err(),
            if kind == 4 {
                ContentError::Io
            } else {
                ContentError::InvalidRecord("edit run window")
            }
        );
        assert_eq!(source.calls.get(), 1);
        assert!(output.is_empty());
    }
    struct Late {
        data: Vec<u8>,
        calls: Cell<usize>,
    }
    impl EditSource for Late {
        fn replacement_len(&self, _: usize) -> u64 {
            600_000
        }
        fn read_at(&self, _: usize, _: u64, _: &mut [u8]) -> ContentResult<usize> {
            panic!("run override")
        }
        fn read_run_at(&self, _: usize, offset: u64, output: &mut [u8]) -> ContentResult<FileRun> {
            self.calls.set(self.calls.get() + 1);
            if offset != 0 {
                return Err(ContentError::ProviderFailure {
                    what: "original late run failure",
                });
            }
            output.copy_from_slice(&self.data);
            Ok(FileRun::Data(output.len()))
        }
    }
    let source = Late {
        data: noise(32_768),
        calls: Cell::new(0),
    };
    let mut output = MemoryStore::new();
    assert_eq!(
        run(&store, built.root, &edits, &source, &mut output, None).unwrap_err(),
        ContentError::ProviderFailure {
            what: "original late run failure"
        }
    );
    assert_eq!(source.calls.get(), 2);
    assert!(output.roles().contains(&ObjectRole::Chunk));
    assert!(!output.roles().contains(&ObjectRole::FileState));
}

#[test]
fn sparse_emission_preserves_predecessor_and_refuses_occupied_scope_before_acceptance() {
    struct Seen {
        store: MemoryStore,
        hints: usize,
        reject: bool,
        attempted: usize,
        refused: Option<FinalizedObject>,
    }
    impl FinalizedConsumer for Seen {
        fn accept(&mut self, object: FinalizedObject) -> ContentResult<()> {
            self.attempted += 1;
            if self.reject {
                self.refused = Some(object);
                return Err(ContentError::OutputRejected);
            }
            if object.role() == ObjectRole::Chunk {
                assert!(object
                    .predecessors()
                    .entries()
                    .iter()
                    .all(|hint| hint.provenance() == PredecessorProvenance::UnchangedPrefix));
                assert_eq!(object.predecessors().len(), 1);
                self.hints += 1;
            }
            self.store.accept(object)
        }
    }
    let base = noise(512 * 1024);
    let (store, built) = build_file(&base);
    let source = Sparse::one(vec![Piece::Zero(2 * 1024 * 1024)]);
    let edits = Edits::new(
        base.len() as u64,
        vec![Edit::insert(19, source.replacement_len(0))],
    )
    .unwrap();
    let mut seen = Seen {
        store: MemoryStore::new(),
        hints: 0,
        reject: false,
        attempted: 0,
        refused: None,
    };
    run(&store, built.root, &edits, &source, &mut seen, None).unwrap();
    assert!(
        seen.hints >= 2,
        "head and repeated zero chunks keep the declared predecessor"
    );
    let mut rejected = Seen {
        store: MemoryStore::new(),
        hints: 0,
        reject: true,
        attempted: 0,
        refused: None,
    };
    assert_eq!(
        run(&store, built.root, &edits, &source, &mut rejected, None).unwrap_err(),
        ContentError::OutputRejected
    );
    assert_eq!(rejected.attempted, 1);
    assert_eq!(rejected.refused.as_ref().unwrap().role(), ObjectRole::Chunk);
    let mut backing = Backing::default();
    backing.values.insert(
        EditRecordKey {
            kind: 10,
            key: [0; 32],
        },
        vec![0],
    );
    let mut output = MemoryStore::new();
    assert_eq!(
        run(
            &store,
            built.root,
            &edits,
            &source,
            &mut output,
            Some(&mut backing)
        )
        .unwrap_err(),
        ContentError::ProviderFailure {
            what: "edit backing precondition"
        }
    );
    assert!(output.is_empty());
    assert_eq!(backing.values.len(), 1, "no preguard backing mutation");
}

#[test]
fn equal_length_comparison_end_is_one_invalid_replacement_without_output_or_backing() {
    struct End {
        calls: Cell<usize>,
    }
    impl EditSource for End {
        fn replacement_len(&self, _: usize) -> u64 {
            10
        }
        fn read_at(&self, _: usize, _: u64, _: &mut [u8]) -> ContentResult<usize> {
            panic!("comparison must consume the run port")
        }
        fn read_run_at(&self, _: usize, _: u64, _: &mut [u8]) -> ContentResult<FileRun> {
            self.calls.set(self.calls.get() + 1);
            Ok(FileRun::End)
        }
    }
    let base = noise(512 * 1024);
    let (store, built) = build_file(&base);
    let edits = Edits::new(base.len() as u64, vec![Edit::overwrite(1_000, 1_010)]).unwrap();
    let source = End {
        calls: Cell::new(0),
    };
    let mut output = MemoryStore::new();
    let mut backing = Backing {
        forbidden: true,
        ..Backing::default()
    };
    assert_eq!(
        run(
            &store,
            built.root,
            &edits,
            &source,
            &mut output,
            Some(&mut backing)
        )
        .unwrap_err(),
        ContentError::InvalidEdit {
            what: "replacement bytes"
        }
    );
    assert_eq!(source.calls.get(), 1);
    assert!(output.is_empty());
    assert_eq!(backing.calls, 0);
}

#[test]
fn zero_payload_identity_role_and_shape_failures_end_on_the_original_demand() {
    for mode in 0..3 {
        let canonical = match mode {
            0 => encode_chunk_object(&[0; 32_768]).unwrap(),
            1 => layerfs_content::encode_whole_file_payload(&[0; 31]).unwrap(),
            _ => {
                let mut value = layerfs_content::file::mapping::CHUNK_MAGIC.to_vec();
                value.resize(value.len() + 32_769, 0);
                layerfs_content::object::encode_bytes_object(&value).unwrap()
            }
        };
        let mut store = MemoryStore::new();
        let payload = FinalizedObject::new(
            if mode == 1 {
                ObjectRole::WholeFile
            } else {
                ObjectRole::Chunk
            },
            canonical,
        )
        .unwrap();
        let payload_id = payload.id();
        store.accept(payload).unwrap();
        // The wrong-role/malformed bodies have their real canonical identity.
        // IdentityMismatch is instead the original provider error in mode0.
        assert_eq!(
            ObjectId::for_bytes(store.canonical(payload_id).unwrap()),
            payload_id
        );
        let length = 128 * 32_768u64;
        let leaf = FinalizedObject::new(
            ObjectRole::ExtentLeaf,
            encode_node(
                &ExtentNode::Leaf {
                    subtree_logical_bytes: length,
                    extents: vec![ExtentSlice::new(payload_id, 0, 32_768).unwrap(); 128],
                },
                true,
            )
            .unwrap(),
        )
        .unwrap()
        .with_references(vec![payload_id]);
        let mapping_id = leaf.id();
        store.accept(leaf).unwrap();
        let root = emit_file_state(
            &mut store,
            NodeSummary {
                id: mapping_id,
                bytes: length,
                extents: 128,
                level: 0,
            },
        )
        .unwrap();
        let provider = Counted::new(&store);
        if mode == 0 {
            *provider.deny.borrow_mut() = Some((payload_id, ContentError::IdentityMismatch));
        }
        let edits = Edits::new(length, vec![Edit::overwrite(0, length)]).unwrap();
        let source = Sparse::one(vec![Piece::Zero(length)]);
        let mut output = MemoryStore::new();
        let mut backing = Backing {
            forbidden: true,
            ..Backing::default()
        };
        let expected = match mode {
            0 => ContentError::IdentityMismatch,
            1 => ContentError::WrongLogicalRole,
            _ => ContentError::ObjectLimitExceeded {
                limit: 32_768,
                actual: 32_769,
            },
        };
        assert_eq!(
            run(
                &provider,
                root,
                &edits,
                &source,
                &mut output,
                Some(&mut backing)
            )
            .unwrap_err(),
            expected
        );
        let requests = provider.requests.borrow();
        assert_eq!(
            requests.last(),
            Some(&payload_id),
            "no demand follows the deciding payload error"
        );
        assert_eq!(requests.iter().filter(|id| **id == payload_id).count(), 1);
        assert_eq!(provider.denied.get(), usize::from(mode == 0));
        assert_eq!(provider.after_error.get(), 0);
        assert_eq!(source.calls.get(), 1);
        assert!(output.is_empty());
        assert_eq!(backing.calls, 0);
    }
}
